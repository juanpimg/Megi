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
//! [`Resource`] is returned behind [`std::sync::Arc`] so sharing it with the DOM/CSS
//! pipeline does not copy the payload.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::cache::{self, CachePolicy};
use crate::data::decode_data_url;
use crate::error::{FetchError, LoadError};
use crate::file::load_file_url;
use crate::http::{FetchOptions, Headers, Response, fetch, fetch_with_headers};
use crate::mime::{self, MimeHint};
use crate::url::{Scheme, Url};

/// Heuristic freshness lifetime for responses without explicit cache metadata
/// (`Cache-Control`/`Expires`) and for `file://` resources.
///
/// Browsers use a fraction of the time since `Last-Modified`; this loader keeps the
/// previous memoization semantics with a conservative fixed five-minute window.
pub const HEURISTIC_FRESHNESS: Duration = Duration::from_secs(300);

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
    /// `no-cache`/`must-revalidate`: always revalidate before reuse.
    no_cache: bool,
    etag: Option<String>,
    last_modified: Option<String>,
    /// `Vary: User-Agent` value at store time (None when the response does not vary).
    vary_user_agent: Option<String>,
}

impl CachedEntry {
    fn is_fresh(&self) -> bool {
        !self.no_cache && self.stored_at.elapsed() < self.freshness
    }

    fn matches_request(&self, user_agent: &str) -> bool {
        self.vary_user_agent
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
/// use net::{ResourceLoader, Url};
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
    cache_hits: u64,
    cache_misses: u64,
    cache_revalidations: u64,
}

impl Default for ResourceLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceLoader {
    /// Creates a loader with default [`FetchOptions`] and the HTTP cache enabled.
    pub fn new() -> Self {
        Self::with_options(FetchOptions::default())
    }

    /// Creates a loader with custom [`FetchOptions`] and the cache enabled.
    pub fn with_options(options: FetchOptions) -> Self {
        Self {
            options,
            cache: Some(HashMap::new()),
            cache_hits: 0,
            cache_misses: 0,
            cache_revalidations: 0,
        }
    }

    /// Creates a loader without a cache (every load hits the network/filesystem).
    pub fn without_cache() -> Self {
        Self {
            options: FetchOptions::default(),
            cache: None,
            cache_hits: 0,
            cache_misses: 0,
            cache_revalidations: 0,
        }
    }

    /// Enables or disables the cache; disabling it drops the cached entries.
    pub fn set_cache_enabled(&mut self, enabled: bool) {
        if enabled {
            if self.cache.is_none() {
                self.cache = Some(HashMap::new());
            }
        } else {
            self.cache = None;
        }
    }

    /// Returns `true` when the cache is enabled.
    pub const fn is_cache_enabled(&self) -> bool {
        self.cache.is_some()
    }

    /// Returns the fetch configuration used by this loader.
    pub fn fetch_options(&self) -> &FetchOptions {
        &self.options
    }

    /// Returns a mutable reference to the fetch configuration (e.g. to raise a cap).
    pub fn fetch_options_mut(&mut self) -> &mut FetchOptions {
        &mut self.options
    }

    /// Loads `url`, using the HTTP cache (fresh entry, revalidation or full fetch).
    pub fn load(&mut self, url: &Url) -> Result<Arc<Resource>, LoadError> {
        // `data:` URLs are immutable inline payloads: decode on demand, no cache.
        if url.scheme() == Scheme::Data {
            let data = decode_data_url(url)?;
            return Ok(Arc::new(Resource {
                url: url.clone(),
                bytes: data.bytes,
                content_type: data.mime_type,
            }));
        }

        let cache_key = url.cache_key();
        let stored = self
            .cache
            .as_ref()
            .and_then(|cache| cache.get(&cache_key))
            .cloned();

        if let Some(entry) = stored {
            if !entry.matches_request(&self.options.user_agent) {
                // Another `Vary` variant: treat as a miss and refetch.
            } else if entry.is_fresh() {
                self.cache_hits += 1;
                return Ok(entry.resource);
            } else if entry.can_revalidate() {
                return self.revalidate(url, cache_key, entry);
            }
        }

        self.cache_misses += 1;
        let (resource, headers) = self.fetch_resource(url)?;
        let resource = Arc::new(resource);
        self.store(url, cache_key, Arc::clone(&resource), headers.as_ref());
        Ok(resource)
    }

    /// Fetches without consulting the cache (still stores the result when allowed).
    ///
    /// Returns the response headers (when fetched over HTTP) so the caller can apply
    /// `Cache-Control`/validators to the stored entry.
    fn fetch_resource(&self, url: &Url) -> Result<(Resource, Option<Headers>), FetchError> {
        match url.scheme() {
            Scheme::Http | Scheme::Https => {
                let response = fetch(url, &self.options)?;
                if response.status == 304 {
                    // Reached only when no conditional request was sent (no cached
                    // representation): a server bug must be a typed error.
                    return Err(FetchError::UnconditionalNotModified);
                }
                let headers = response.headers.clone();
                Ok((resource_from_response(response), Some(headers)))
            }
            Scheme::File => {
                let bytes = load_file_url(url)?;
                Ok((
                    Resource {
                        url: url.clone(),
                        content_type: mime::sniff(&bytes).mime_type().to_owned(),
                        bytes,
                    },
                    None,
                ))
            }
            Scheme::Data => {
                // Handled before the cache lookup in `load`; defensive fallback.
                let data = decode_data_url(url)?;
                Ok((
                    Resource {
                        url: url.clone(),
                        bytes: data.bytes,
                        content_type: data.mime_type,
                    },
                    None,
                ))
            }
        }
    }

    /// Conditional revalidation of a stale entry with `If-None-Match`/`If-Modified-Since`.
    fn revalidate(
        &mut self,
        url: &Url,
        cache_key: String,
        entry: CachedEntry,
    ) -> Result<Arc<Resource>, LoadError> {
        if !matches!(url.scheme(), Scheme::Http | Scheme::Https) {
            // `file://` has no conditional requests: refetch normally.
            self.cache_misses += 1;
            let (resource, headers) = self.fetch_resource(url)?;
            let resource = Arc::new(resource);
            self.store(url, cache_key, Arc::clone(&resource), headers.as_ref());
            return Ok(resource);
        }
        let headers = entry.conditional_headers();
        let header_refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let response = fetch_with_headers(url, &self.options, &header_refs)?;
        if response.status == 304 {
            self.cache_hits += 1;
            self.cache_revalidations += 1;
            let policy = cache::parse_cache_policy(&response.headers, SystemTime::now());
            let updated = updated_entry(entry, &policy, self.options.user_agent.clone());
            if let Some(cache) = &mut self.cache {
                cache.insert(cache_key, updated.clone());
            }
            return Ok(updated.resource);
        }
        self.cache_misses += 1;
        let headers = response.headers.clone();
        let resource = Arc::new(resource_from_response(response));
        self.store(url, cache_key, Arc::clone(&resource), Some(&headers));
        Ok(resource)
    }

    /// Stores a freshly fetched success response according to its cache policy.
    fn store(
        &mut self,
        _url: &Url,
        cache_key: String,
        resource: Arc<Resource>,
        response_headers: Option<&Headers>,
    ) {
        let Some(cache) = &mut self.cache else {
            return;
        };
        let now = SystemTime::now();
        let (policy, freshness) = match response_headers {
            Some(headers) => {
                let policy = cache::parse_cache_policy(headers, now);
                if policy.uncacheable() {
                    return;
                }
                let freshness = policy.explicit_lifetime(now).unwrap_or(HEURISTIC_FRESHNESS);
                (policy, freshness)
            }
            // `file://` (and synthetic) resources: heuristic freshness, no validators.
            None => (CachePolicy::default(), HEURISTIC_FRESHNESS),
        };
        let vary_user_agent = if policy.vary.user_agent {
            Some(self.options.user_agent.clone())
        } else {
            None
        };
        cache.insert(
            cache_key,
            CachedEntry {
                resource,
                stored_at: Instant::now(),
                freshness,
                no_cache: policy.no_cache,
                etag: policy.etag,
                last_modified: policy.last_modified,
                vary_user_agent,
            },
        );
    }

    /// Parses `url` and loads it; convenience wrapper for callers holding strings.
    pub fn load_str(&mut self, url: &str) -> Result<Arc<Resource>, LoadError> {
        let parsed = Url::parse(url)?;
        self.load(&parsed)
    }

    /// Returns `true` when a successful load for `url` (without fragment) is cached.
    pub fn is_cached(&self, url: &Url) -> bool {
        self.cache
            .as_ref()
            .is_some_and(|cache| cache.contains_key(&url.cache_key()))
    }

    /// Drops every cached resource.
    pub fn clear_cache(&mut self) {
        if let Some(cache) = &mut self.cache {
            cache.clear();
        }
    }

    /// Number of cached resources (one per URL).
    pub fn cache_len(&self) -> usize {
        self.cache.as_ref().map_or(0, HashMap::len)
    }

    /// Number of loads served from the cache (fresh hit or successful revalidation).
    pub const fn cache_hits(&self) -> u64 {
        self.cache_hits
    }

    /// Number of loads that had to hit the network/filesystem for a new body.
    pub const fn cache_misses(&self) -> u64 {
        self.cache_misses
    }

    /// Number of stale entries successfully revalidated with `304 Not Modified`.
    pub const fn cache_revalidations(&self) -> u64 {
        self.cache_revalidations
    }
}

/// Builds a [`Resource`] from an HTTP response, resolving its content type.
fn resource_from_response(response: Response) -> Resource {
    let content_type = mime::resolve_content_type(&response.headers, &response.body);
    Resource {
        url: response.final_url,
        bytes: response.body,
        content_type,
    }
}

/// Recomputes a cache entry after a `304 Not Modified`, keeping the stored body.
fn updated_entry(mut entry: CachedEntry, policy: &CachePolicy, user_agent: String) -> CachedEntry {
    if policy.no_store {
        // A `304` with `no-store` invalidates the stored representation.
        entry.freshness = Duration::ZERO;
        entry.no_cache = true;
    } else {
        let freshness = policy
            .explicit_lifetime(SystemTime::now())
            .unwrap_or(HEURISTIC_FRESHNESS);
        entry.freshness = freshness;
        entry.no_cache = policy.no_cache;
        if policy.etag.is_some() {
            entry.etag = policy.etag.clone();
        }
        if policy.last_modified.is_some() {
            entry.last_modified = policy.last_modified.clone();
        }
    }
    entry.stored_at = Instant::now();
    entry.vary_user_agent = if policy.vary.user_agent {
        Some(user_agent)
    } else {
        None
    };
    entry
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn loader_starts_with_empty_cache() {
        let loader = ResourceLoader::new();
        assert!(loader.is_cache_enabled());
        assert_eq!(loader.cache_len(), 0);
        assert_eq!(loader.cache_hits(), 0);
        assert_eq!(loader.cache_misses(), 0);
        assert_eq!(loader.cache_revalidations(), 0);
    }

    #[test]
    fn disabling_cache_drops_entries() {
        let mut loader = ResourceLoader::new();
        loader.set_cache_enabled(false);
        assert!(!loader.is_cache_enabled());
        assert_eq!(loader.cache_len(), 0);
        loader.set_cache_enabled(true);
        assert!(loader.is_cache_enabled());
    }
}
