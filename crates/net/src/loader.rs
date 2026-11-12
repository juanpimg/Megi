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
use crate::file::load_file_url;
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
    /// `must-revalidate`: once stale, revalidate before reuse (RFC 9111 §5.2.2.2).
    must_revalidate: bool,
    etag: Option<String>,
    last_modified: Option<String>,
    /// `Vary: User-Agent` value at store time (None when the response does not vary).
    vary_user_agent: Option<String>,
}

impl CachedEntry {
    fn is_fresh(&self) -> bool {
        !self.no_cache && !self.must_revalidate && self.stored_at.elapsed() < self.freshness
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

    /// Installs a proxy configuration for every subsequent HTTP(S) fetch.
    ///
    /// Equivalent to setting [`FetchOptions::proxy`] before constructing the loader;
    /// `no_proxy` matches per target URL/redirect hop and the HTTP cache/cookies/CORS
    /// are unchanged. Returns `&mut Self` for chaining.
    pub fn set_proxy(&mut self, proxy: crate::proxy::ProxyConfig) -> &mut Self {
        self.options.proxy = Some(Arc::new(proxy));
        self
    }

    /// Builder variant of [`Self::set_proxy`].
    pub fn with_proxy(mut self, proxy: crate::proxy::ProxyConfig) -> Self {
        self.set_proxy(proxy);
        self
    }

    /// Creates a loader with custom [`FetchOptions`] and the cache enabled.
    pub fn with_options(options: FetchOptions) -> Self {
        let h1_pool = crate::http::H1Pool::new(
            options.max_connections_per_origin,
            options.connection_idle_timeout,
        );
        Self {
            options,
            cache: Some(HashMap::new()),
            cache_hits: 0,
            cache_misses: 0,
            cache_revalidations: 0,
            push_cache: HashMap::new(),
            pushed_used: 0,
            pushed_discarded: 0,
            pushed_bytes: 0,
            h1_pool,
            allow_file: true,
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
            push_cache: HashMap::new(),
            pushed_used: 0,
            pushed_discarded: 0,
            pushed_bytes: 0,
            h1_pool: crate::http::H1Pool::new(4, std::time::Duration::from_secs(30)),
            allow_file: true,
        }
    }

    /// Allows or forbids loading `file:` URLs (allowed by default).
    ///
    /// The renderer disables it for documents that are not themselves `file:`, so a remote
    /// page cannot read the local filesystem through an image, script, stylesheet or fetch.
    pub fn set_allow_file(&mut self, allow: bool) -> &mut Self {
        self.allow_file = allow;
        self
    }

    /// Builder variant of [`Self::set_allow_file`].
    pub fn with_allow_file(mut self, allow: bool) -> Self {
        self.set_allow_file(allow);
        self
    }

    /// Rejects `file:` URLs when local-file access is disabled.
    fn check_file_access(&self, url: &Url) -> Result<(), LoadError> {
        if url.scheme() == Scheme::File && !self.allow_file {
            return Err(FetchError::FileAccessBlocked.into());
        }
        Ok(())
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

    /// Number of server-pushed resources consumed by [`Self::load`] in this load session.
    pub const fn pushed_used(&self) -> usize {
        self.pushed_used
    }

    /// Transfers the server-pushed preloads of `other` into this loader.
    ///
    /// Render uses two loaders per render (document + subresources) that share the same
    /// load session: pushes received while fetching the document must be visible to the
    /// subresource loader. Caps of `self` are applied while transferring.
    pub fn absorb_pushes_from(&mut self, other: &mut ResourceLoader) {
        for (key, resource) in other.push_cache.drain() {
            let bytes = resource.bytes.len();
            if self.push_cache.len() >= self.options.max_pushed_resources
                || self.pushed_bytes.saturating_add(bytes) > self.options.max_pushed_total
            {
                self.pushed_discarded += 1;
                continue;
            }
            self.pushed_bytes = self.pushed_bytes.saturating_add(bytes);
            self.push_cache.insert(key, resource);
        }
        other.pushed_bytes = 0;
    }

    /// Returns `true` when `url` has a server-pushed preload waiting in this load session.
    pub fn cache_key_is_preloaded(&self, url: &Url) -> bool {
        self.push_cache.contains_key(&url.cache_key())
    }

    /// Number of server-pushed resources dropped because a push cap was exceeded.
    pub const fn pushed_discarded(&self) -> usize {
        self.pushed_discarded
    }

    /// Stores server-pushed responses fetched by a caller that used [`crate::http::fetch`]
    /// directly (the render document loader) so subresource loads can consume them.
    pub fn absorb_pushed(&mut self, pushed: Vec<crate::http::PushedResponse>) {
        self.store_pushed(pushed);
    }

    /// Stores server-pushed responses for later [`Self::load`] calls.
    ///
    /// Only resources from HTTP/2 connections of the fetch that produced them are stored
    /// (the loader is a per-load-session object in render), with the
    /// `FetchOptions::{max_pushed_*,accept_push}` caps. Entries are consumed once so a
    /// later real request still goes to the network.
    fn store_pushed(&mut self, pushed: Vec<crate::http::PushedResponse>) {
        for pushed in pushed {
            let bytes = pushed.body.len();
            if pushed.body.len() > self.options.max_pushed_body
                || self.push_cache.len() >= self.options.max_pushed_resources
                || self.pushed_bytes.saturating_add(bytes) > self.options.max_pushed_total
            {
                self.pushed_discarded += 1;
                continue;
            }
            let content_type = pushed
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .map(|(_, value)| value.split(';').next().unwrap_or("").trim().to_owned())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| mime::sniff(&pushed.body).mime_type().to_owned());
            let key = pushed.url.cache_key();
            if self
                .push_cache
                .insert(
                    key,
                    Arc::new(Resource {
                        url: pushed.url,
                        content_type,
                        bytes: pushed.body,
                    }),
                )
                .is_none()
            {
                self.pushed_bytes = self.pushed_bytes.saturating_add(bytes);
            }
        }
    }

    /// Loads `url`, using the HTTP cache (fresh entry, revalidation or full fetch).
    pub fn load(&mut self, url: &Url) -> Result<Arc<Resource>, LoadError> {
        self.check_file_access(url)?;
        // `data:` URLs are immutable inline payloads: decode on demand, no cache.
        if url.scheme() == Scheme::Data {
            let data = decode_data_url(url)?;
            if data.bytes.len() > self.options.max_response_bytes {
                return Err(LoadError::Fetch(FetchError::ResponseTooLarge));
            }
            return Ok(Arc::new(Resource {
                url: url.clone(),
                bytes: data.bytes,
                content_type: data.mime_type,
            }));
        }

        // Server-pushed preloads are consumed before any cache/network work (per session).
        if let Some(resource) = self.push_cache.remove(&url.cache_key()) {
            self.pushed_used += 1;
            return Ok(resource);
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

    /// Loads `url` in `no-cors` mode (without sending `Origin` or enforcing CORS).
    pub fn load_no_cors(&mut self, url: &Url) -> Result<Arc<Resource>, LoadError> {
        let saved_origin = self.options.origin.take();
        let result = self.load(url);
        self.options.origin = saved_origin;
        result
    }

    /// Loads `url` with a streaming callback.
    ///
    /// MVP fallback documented: for `http(s)` the response is fetched with the regular
    /// buffered [`crate::http::fetch`] (redirects, cookie jar, CORS and compression are
    /// respected) and its body is delivered to `on_data` in 16 KiB chunks; the resource is
    /// returned with an empty body. The HTTP/preload caches are bypassed by design for this
    /// streaming entry point. `file:`/`data:` bodies are read normally and chunked.
    pub fn load_streaming(
        &mut self,
        url: &Url,
        on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
    ) -> Result<Resource, LoadError> {
        self.check_file_access(url)?;
        match url.scheme() {
            Scheme::Http | Scheme::Https => {
                let response = crate::http::fetch_streaming(url, &self.options, on_data)?;
                Ok(Resource {
                    url: response.final_url,
                    content_type: response
                        .headers
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                        .map(|(_, value)| value.split(';').next().unwrap_or("").trim().to_owned())
                        .unwrap_or_default(),
                    bytes: response.body,
                })
            }
            Scheme::File | Scheme::Data => {
                let resource = self.load(url)?;
                for chunk in resource.bytes.chunks(16 * 1024) {
                    on_data(chunk)?;
                }
                Ok(Resource {
                    url: resource.url.clone(),
                    content_type: resource.content_type.clone(),
                    bytes: Vec::new(),
                })
            }
        }
    }

    /// Number of idle HTTP/1.1 connections currently pooled across origins.
    pub fn idle_connections(&self) -> usize {
        self.h1_pool.idle_connections()
    }

    /// Loads multiple URLs, fetching uncached HTTP/HTTPS targets concurrently (bounded to
    /// 6 worker threads) while preserving cache/push/CORS semantics per request.
    pub fn load_many(
        &mut self,
        requests: &[(&Url, bool)],
    ) -> Vec<Result<Arc<Resource>, LoadError>> {
        let mut results: Vec<Option<Result<Arc<Resource>, LoadError>>> =
            (0..requests.len()).map(|_| None).collect();
        let mut pending_indices: HashMap<String, (Url, bool, Vec<usize>)> = HashMap::new();

        for (idx, &(url, use_cors)) in requests.iter().enumerate() {
            if !matches!(url.scheme(), Scheme::Http | Scheme::Https) {
                results[idx] = Some(if use_cors {
                    self.load(url)
                } else {
                    self.load_no_cors(url)
                });
                continue;
            }
            let key = url.cache_key();
            if self.push_cache.contains_key(&key)
                || self.cache.as_ref().is_some_and(|c| c.contains_key(&key))
            {
                results[idx] = Some(if use_cors {
                    self.load(url)
                } else {
                    self.load_no_cors(url)
                });
                continue;
            }
            let entry = pending_indices
                .entry(format!("{use_cors}:{key}"))
                .or_insert_with(|| (url.clone(), use_cors, Vec::new()));
            entry.2.push(idx);
        }

        if pending_indices.len() <= 1 {
            for (_, (url, use_cors, indices)) in pending_indices {
                for idx in indices {
                    results[idx] = Some(if use_cors {
                        self.load(&url)
                    } else {
                        self.load_no_cors(&url)
                    });
                }
            }
            return results.into_iter().map(|r| r.expect("filled")).collect();
        }

        let work: Vec<(Url, bool, Vec<usize>)> = pending_indices.into_values().collect();
        let base_options = &self.options;
        let concurrency = work.len().min(6);
        let chunk_size = work.len().div_ceil(concurrency);
        let fetched: Vec<(
            String,
            Vec<usize>,
            Result<crate::http::Response, FetchError>,
        )> = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(concurrency);
            for chunk in work.chunks(chunk_size) {
                handles.push(scope.spawn(move || {
                    let mut out = Vec::with_capacity(chunk.len());
                    for (url, use_cors, indices) in chunk {
                        let mut opts = base_options.clone();
                        if !*use_cors {
                            opts.origin = None;
                        }
                        let resp = fetch(url, &opts);
                        out.push((url.cache_key(), indices.clone(), resp));
                    }
                    out
                }));
            }
            let mut all = Vec::with_capacity(work.len());
            for h in handles {
                all.extend(h.join().expect("fetch worker panicked"));
            }
            all
        });

        for (cache_key, indices, resp) in fetched {
            let first_url = &requests[indices[0]].0;
            match resp {
                Ok(mut response) => {
                    if response.status == 304 {
                        for idx in indices {
                            results[idx] =
                                Some(Err(LoadError::from(FetchError::UnconditionalNotModified)));
                        }
                    } else {
                        self.store_pushed(std::mem::take(&mut response.pushed));
                        let headers = response.headers.clone();
                        let resource = Arc::new(resource_from_response(response));
                        self.cache_misses += 1;
                        self.store(first_url, cache_key, Arc::clone(&resource), Some(&headers));
                        for idx in indices {
                            results[idx] = Some(Ok(Arc::clone(&resource)));
                        }
                    }
                }
                Err(err) => {
                    let msg = err.to_string();
                    let mut iter = indices.into_iter();
                    if let Some(first_idx) = iter.next() {
                        results[first_idx] = Some(Err(LoadError::from(err)));
                    }
                    for idx in iter {
                        results[idx] = Some(Err(LoadError::from(FetchError::InvalidResponse(
                            msg.clone(),
                        ))));
                    }
                }
            }
        }

        results.into_iter().map(|r| r.expect("filled")).collect()
    }

    /// Fetches without consulting the cache (still stores the result when allowed).
    ///
    /// Returns the response headers (when fetched over HTTP) so the caller can apply
    /// `Cache-Control`/validators to the stored entry.
    fn fetch_resource(&mut self, url: &Url) -> Result<(Resource, Option<Headers>), FetchError> {
        match url.scheme() {
            Scheme::Http | Scheme::Https => {
                let mut response =
                    crate::http::fetch_pooled(url, &self.options, &mut self.h1_pool)?;
                if response.status == 304 {
                    // Reached only when no conditional request was sent (no cached
                    // representation): a server bug must be a typed error.
                    return Err(FetchError::UnconditionalNotModified);
                }
                self.store_pushed(std::mem::take(&mut response.pushed));
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
        let mut response = response;
        self.store_pushed(std::mem::take(&mut response.pushed));
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
                let freshness = policy
                    .freshness_lifetime(now)
                    .unwrap_or(HEURISTIC_FRESHNESS);
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
        // Evict oldest entries when the cap is reached.
        if cache.len() >= MAX_CACHE_ENTRIES {
            // Remove the first entry (oldest insertion order in HashMap is arbitrary,
            // but this bounds memory).
            if let Some(oldest_key) = cache.keys().next().cloned() {
                cache.remove(&oldest_key);
            }
        }
        cache.insert(
            cache_key,
            CachedEntry {
                resource,
                stored_at: Instant::now(),
                freshness,
                no_cache: policy.no_cache,
                must_revalidate: policy.must_revalidate,
                etag: policy.etag,
                last_modified: policy.last_modified,
                vary_user_agent,
            },
        );
    }

    /// Fetches `url` with an arbitrary method and a streaming [`crate::RequestBody`].
    ///
    /// Streaming bodies are single-shot (no cache); byte bodies delegate to
    /// [`Self::fetch_with_method`] so the GET cache path is unchanged. Redirects that
    /// would replay a stream return a typed error.
    pub fn fetch_with_request_body(
        &mut self,
        url: &Url,
        method: &str,
        extra_headers: &[(&str, &str)],
        body: Option<crate::body::RequestBody>,
    ) -> Result<Arc<Resource>, LoadError> {
        self.check_file_access(url)?;
        let streaming = matches!(body, Some(crate::body::RequestBody::Stream(_)));
        if method.eq_ignore_ascii_case("GET") && !streaming && body.is_none() {
            return self.load(url);
        }
        let response =
            crate::http::fetch_with_request_body(url, &self.options, method, extra_headers, body)?;
        Ok(Arc::new(resource_from_response(response)))
    }

    /// Parses `url` and loads it; convenience wrapper for callers holding strings.
    pub fn load_str(&mut self, url: &str) -> Result<Arc<Resource>, LoadError> {
        let parsed = Url::parse(url)?;
        self.load(&parsed)
    }

    /// Fetches `url` with an arbitrary method and optional request body.
    ///
    /// Non-`GET` requests bypass the HTTP cache (POST/PUT are not cacheable here) and
    /// return the response bytes/content type. `GET` without a body delegates to
    /// [`Self::load`] so the cache/cookies/CORS behaviour is unchanged. Redirects follow
    /// the [`crate::http::fetch_with_method`] rules; `--no-network` is enforced by the
    /// caller (the loader never runs when networking is disabled).
    pub fn fetch_with_method(
        &mut self,
        url: &Url,
        method: &str,
        extra_headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<Arc<Resource>, LoadError> {
        self.check_file_access(url)?;
        if method.eq_ignore_ascii_case("GET") && body.is_none() {
            return self.load(url);
        }
        let response =
            crate::http::fetch_with_method(url, &self.options, method, extra_headers, body)?;
        Ok(Arc::new(resource_from_response(response)))
    }

    /// Installs the referrer configuration applied to every subsequent HTTP(S) fetch.
    ///
    /// Pass `Some(ReferrerConfig::with_policy(document_url, policy))` once the document
    /// URL and its `Referrer-Policy` are known; `None` disables the `Referer` header.
    pub fn set_referrer(&mut self, referrer: Option<ReferrerConfig>) {
        self.options.referrer = referrer;
    }

    /// Returns the currently installed referrer configuration, if any.
    pub fn referrer(&self) -> Option<&ReferrerConfig> {
        self.options.referrer.as_ref()
    }

    /// Performs a one-shot byte-range fetch of `url` (not cached).
    ///
    /// For `http(s)` the request carries `Range: bytes=...` and the returned bytes are the
    /// server's partial body (the status/`Content-Range` are available through
    /// [`crate::http::fetch`] when needed). `file:`/`data:` payloads are sliced locally.
    /// Invalid/unsatisfiable ranges are [`crate::FetchError::InvalidRange`].
    pub fn load_range(&mut self, url: &Url, range: ByteRange) -> Result<Resource, LoadError> {
        self.check_file_access(url)?;
        match url.scheme() {
            Scheme::Http | Scheme::Https => {
                let mut options = self.options.clone();
                options.range = Some(range);
                let response = fetch(url, &options)?;
                if response.status == 304 {
                    return Err(FetchError::UnconditionalNotModified.into());
                }
                Ok(Resource {
                    url: response.final_url,
                    content_type: mime::resolve_content_type(&response.headers, &response.body),
                    bytes: response.body,
                })
            }
            Scheme::File => {
                let bytes = load_file_url(url)?;
                let slice = slice_bytes(&bytes, range)?;
                Ok(Resource {
                    url: url.clone(),
                    content_type: mime::sniff(&slice).mime_type().to_owned(),
                    bytes: slice,
                })
            }
            Scheme::Data => {
                let data = decode_data_url(url)?;
                let slice = slice_bytes(&data.bytes, range)?;
                Ok(Resource {
                    url: url.clone(),
                    content_type: data.mime_type,
                    bytes: slice,
                })
            }
        }
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

/// Slices `bytes` with `range`, mapping unsatisfiable ranges to [`FetchError::InvalidRange`].
fn slice_bytes(bytes: &[u8], range: ByteRange) -> Result<Vec<u8>, FetchError> {
    let Some((start, end)) = range.resolve(bytes.len() as u64) else {
        return Err(FetchError::InvalidRange(format!(
            "range {range} is unsatisfiable for {} bytes",
            bytes.len()
        )));
    };
    Ok(bytes[start as usize..=end as usize].to_vec())
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
        entry.must_revalidate = true;
    } else {
        let freshness = policy
            .freshness_lifetime(SystemTime::now())
            .unwrap_or(HEURISTIC_FRESHNESS);
        entry.freshness = freshness;
        entry.no_cache = policy.no_cache;
        entry.must_revalidate = policy.must_revalidate;
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
