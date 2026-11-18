//! In-memory [`ResourceLoader`] used by the document/resource pipeline.
//!
//! The loader dispatches by URL scheme:
//!
//! - `http:`/`https:` go through [`crate::http::fetch`] (TLS with the `tls` feature);
//! - `file:` reads the local file (see [`crate::file`]);
//! - `data:` decodes RFC 2397 inline payloads (see [`crate::data`]).
//!
//! The resulting bytes are paired with a resolved content type (HTTP `Content-Type`
//! header when present, otherwise MIME sniffing).
//!
//! # HTTP cache (RFC 9111 subset)
//!
//! The loader implements a real, single-user HTTP cache rather than plain memoization:
//!
//! - **Freshness**: `Cache-Control: max-age=N` and `Expires` (IMF-fixdate) decide when an
//!   entry is fresh. Responses with no explicit freshness but a successful `200` get a
//!   documented heuristic lifetime ([`HEURISTIC_FRESHNESS`] = 5 minutes) so plain servers
//!   are not re-fetched on every resource. `file://` resources use the same heuristic.
//! - **Revalidation**: stale entries with `ETag`/`Last-Modified` are revalidated with
//!   `If-None-Match`/`If-Modified-Since`; a `304 Not Modified` refreshes the stored
//!   metadata and serves the cached body. A `200` replaces the entry.
//! - **`no-store`** responses are never stored; **`no-cache`/`must-revalidate`** entries
//!   are stored but always revalidated before reuse.
//! - **`Vary`**: `Accept-Encoding` and `User-Agent` are tracked; `Vary: *` and unknown
//!   vary headers make the response uncacheable. Only **one variant per URL** is stored,
//!   so a different `User-Agent` is a cache miss (documented limitation; the loader
//!   never sends `Accept-Encoding` because there is no compression support).
//! - Only `200` responses are cached; `3xx` are followed by the HTTP client, and
//!   `4xx`/`5xx` are not stored. A `304` without a cached entry is
//!   [`FetchError::UnconditionalNotModified`].
//!
//! `data:` URLs are decoded on every load (they are trivially cheap and immutable).
//!
//! ## Referrer policy and byte ranges
//!
//! [`ResourceLoader::set_referrer`] installs a [`ReferrerConfig`] used for **every**
//! subsequent HTTP(S) fetch, so subresources send a policy-computed `Referer` header
//! (the render pipeline sets it once the document URL/policy is known; see
//! [`crate::ReferrerPolicy`]). [`ResourceLoader::load_range`] performs a one-shot partial
//! fetch (`Range: bytes=...`): HTTP(S) responses may be `206` (the raw status is only
//! visible through [`crate::http::fetch`]), `file:`/`data:` resources are sliced locally.
//! Range loads bypass the cache and do not implement `If-Range`, resume or streaming.
//!
//! [`Resource`] is returned behind [`std::sync::Arc`] so sharing it with the DOM/CSS
//! pipeline does not copy the payload.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::cache::{self, CachePolicy};
use crate::data::decode_data_url;
use crate::error::{FetchError, LoadError};
use crate::file::load_file_url_with_limit;
use crate::http::{FetchOptions, Headers, Response, fetch, fetch_with_headers};
use crate::mime::{self, MimeHint};
use crate::range::ByteRange;
use crate::referrer::ReferrerConfig;
use crate::url::{Scheme, Url};

/// Heuristic freshness lifetime for responses without explicit cache metadata
/// (`Cache-Control`/`Expires`) and for `file://` resources.
///
/// Browsers use a fraction of the time since `Last-Modified`; this loader keeps the
/// previous memoization semantics with a conservative fixed five-minute window.
pub const HEURISTIC_FRESHNESS: Duration = Duration::from_secs(300);
const MAX_CACHE_ENTRIES: usize = 256;
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;

/// A successfully loaded resource.
#[derive(Clone, Debug)]
pub struct Resource {
    /// Final URL (after redirects for `http:`; the requested URL otherwise).
    pub url: Url,
    /// Raw resource bytes (identity-encoded: no decompression is applied).
    pub bytes: Vec<u8>,
    /// Resolved content type (e.g. `text/html`, `image/png`).
    pub content_type: String,
}

impl Resource {
    /// Interprets the payload as UTF-8 text, if it is valid UTF-8.
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }

    /// Returns the MIME hint implied by the payload bytes.
    pub fn sniffed(&self) -> MimeHint {
        mime::sniff(&self.bytes)
    }
}

/// One stored HTTP cache entry.
#[derive(Clone, Debug)]
struct CachedEntry {
    resource: Arc<Resource>,
    stored_at: Instant,
    freshness: Duration,
    /// `no-cache`: always revalidate before reuse.
    no_cache: bool,
    etag: Option<String>,
    last_modified: Option<String>,
    /// `Vary: User-Agent` value at store time (None when the response does not vary).
    vary_user_agent: Option<String>,
    /// Request `Origin` at store time (`None` for no-CORS/same-origin requests) so a cached
    /// entry validated for one origin or in `no-cors` mode is never served to another (#99).
    origin: Option<String>,
}

impl CachedEntry {
    fn is_fresh(&self) -> bool {
        // `must-revalidate` does not disable freshness (RFC 9111 §5.2.2.2):
        // a response inside its freshness lifetime is still a cache hit (#1344).
        !self.no_cache && self.stored_at.elapsed() < self.freshness
    }

    fn matches_request(&self, user_agent: &str, origin: Option<&str>) -> bool {
        self.origin.as_deref() == origin
            && self
                .vary_user_agent
                .as_deref()
                .is_none_or(|stored| stored == user_agent)
    }

    fn can_revalidate(&self) -> bool {
        self.etag.is_some() || self.last_modified.is_some()
    }

    fn conditional_headers(&self) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        if let Some(etag) = &self.etag {
            headers.push(("If-None-Match".to_owned(), etag.clone()));
        }
        if let Some(last_modified) = &self.last_modified {
            headers.push(("If-Modified-Since".to_owned(), last_modified.clone()));
        }
        headers
    }
}

/// Loads resources by URL with a single-user HTTP cache.
///
/// ```no_run
/// use megi_net::{ResourceLoader, Url};
///
/// let mut loader = ResourceLoader::new();
/// let url = Url::parse("http://127.0.0.1:8080/index.html")?;
/// let resource = loader.load(&url)?;
/// println!("{} ({} bytes)", resource.content_type, resource.bytes.len());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct ResourceLoader {
    options: FetchOptions,
    cache: Option<HashMap<String, CachedEntry>>,
    cache_bytes: usize,
    cache_hits: u64,
    cache_misses: u64,
    cache_revalidations: u64,
    /// Server-pushed resources preloaded for this load session (consumed by `load`).
    push_cache: HashMap<String, Arc<Resource>>,
    /// Pushed resources actually consumed by `load`.
    pushed_used: usize,
    /// Pushed resources dropped (cap/count/body limits).
    pushed_discarded: usize,
    /// Encoded bytes currently held in the push cache.
    pushed_bytes: usize,
    /// Per-origin idle HTTP/1.1 keep-alive connections (R185).
    h1_pool: crate::http::H1Pool,
    /// Whether `file:` URLs may be loaded (default `true`; the renderer disables it for
    /// documents that are not themselves `file:`).
    allow_file: bool,
    /// Scheme of the document this loader serves (#1528). When `https:`, `http:` subresources
    /// are blocked (active content) or upgraded (passive content) by [`Self::load_checked`].
    mixed_content_context: Option<Scheme>,
}

impl Default for ResourceLoader {
    fn default() -> Self {
        Self::new()
    }
}

mod core;
mod fetch;
mod helpers;
mod load;

#[cfg(test)]
mod tests;
