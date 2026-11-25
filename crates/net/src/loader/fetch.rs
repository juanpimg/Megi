//! Network fetch, revalidation, cache storage and range loads.

use super::helpers::{resource_from_response, slice_bytes, updated_entry};
use super::*;

impl ResourceLoader {
    /// Fetches without consulting the cache (still stores the result when allowed).
    ///
    /// Returns the response headers and status code (when fetched over HTTP) so the caller
    /// can apply `Cache-Control`/validators and only cache `200` responses (#91).
    pub(super) fn fetch_resource(
        &mut self,
        url: &Url,
    ) -> Result<(Resource, Option<Headers>, Option<u16>), FetchError> {
        // M7 Network domain: record the loader's own observation of every real fetch.
        let started = Instant::now();
        match url.scheme() {
            Scheme::Http | Scheme::Https => {
                let mut response =
                    crate::http::fetch_pooled(url, &self.options, &mut self.h1_pool)?;
                if response.status == 304 {
                    // Reached only when no conditional request was sent (no cached
                    // representation): a server bug must be a typed error.
                    return Err(FetchError::UnconditionalNotModified);
                }
                let status = response.status;
                self.store_pushed(std::mem::take(&mut response.pushed));
                let headers = response.headers.clone();
                let resource = resource_from_response(response);
                self.record_network_event(NetworkEvent::from_resource(
                    &resource,
                    "GET",
                    false,
                    started.elapsed(),
                ));
                Ok((resource, Some(headers), Some(status)))
            }
            Scheme::File => {
                let bytes = load_file_url_with_limit(url, self.options.max_response_bytes)?;
                let resource = Resource {
                    status: 200,
                    url: url.clone(),
                    content_type: mime::sniff(&bytes).mime_type().to_owned(),
                    bytes,
                };
                self.record_network_event(NetworkEvent::from_resource(
                    &resource,
                    "GET",
                    false,
                    started.elapsed(),
                ));
                Ok((resource, None, None))
            }
            Scheme::Data => {
                // Handled before the cache lookup in `load`; defensive fallback.
                let data = decode_data_url(url)?;
                let resource = Resource {
                    status: 200,
                    url: url.clone(),
                    bytes: data.bytes,
                    content_type: data.mime_type,
                };
                self.record_network_event(NetworkEvent::from_resource(
                    &resource,
                    "GET",
                    false,
                    started.elapsed(),
                ));
                Ok((resource, None, None))
            }
        }
    }

    /// Conditional revalidation of a stale entry with `If-None-Match`/`If-Modified-Since`.
    pub(super) fn revalidate(
        &mut self,
        url: &Url,
        cache_key: String,
        entry: CachedEntry,
    ) -> Result<Arc<Resource>, LoadError> {
        if !matches!(url.scheme(), Scheme::Http | Scheme::Https) {
            // `file://` has no conditional requests: refetch normally.
            self.cache_misses += 1;
            let (resource, headers, _) = self.fetch_resource(url)?;
            let resource = Arc::new(resource);
            self.store(
                url,
                cache_key,
                Arc::clone(&resource),
                headers.as_ref(),
                self.options.origin.clone(),
            );
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
        let status = response.status;
        self.store_pushed(std::mem::take(&mut response.pushed));
        let headers = response.headers.clone();
        let resource = Arc::new(resource_from_response(response));
        if status == 200 {
            self.store(
                url,
                cache_key,
                Arc::clone(&resource),
                Some(&headers),
                self.options.origin.clone(),
            );
        } else if let Some(cache) = &mut self.cache
            && let Some(removed) = cache.remove(&cache_key)
        {
            self.cache_bytes = self
                .cache_bytes
                .saturating_sub(removed.resource.bytes.len());
        }
        Ok(resource)
    }

    /// Stores a freshly fetched success response according to its cache policy.
    pub(super) fn store(
        &mut self,
        _url: &Url,
        cache_key: String,
        resource: Arc<Resource>,
        response_headers: Option<&Headers>,
        origin: Option<String>,
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
            // Refuse to cache a body above the response cap so a single large file cannot
            // pin an unbounded amount of memory in the cache HashMap (#637).
            None => {
                if resource.bytes.len() > self.options.max_response_bytes {
                    return;
                }
                (CachePolicy::default(), HEURISTIC_FRESHNESS)
            }
        };
        let vary_user_agent = if policy.vary.user_agent {
            Some(self.options.user_agent.clone())
        } else {
            None
        };
        let entry_bytes = resource.bytes.len();
        if entry_bytes > MAX_CACHE_BYTES {
            return;
        }
        if let Some(previous) = cache.remove(&cache_key) {
            self.cache_bytes = self
                .cache_bytes
                .saturating_sub(previous.resource.bytes.len());
        }
        // Evict entries when the entry count or aggregate byte cap is reached (#455).
        while cache.len() >= MAX_CACHE_ENTRIES
            || self.cache_bytes.saturating_add(entry_bytes) > MAX_CACHE_BYTES
        {
            let Some(oldest_key) = cache.keys().next().cloned() else {
                break;
            };
            if let Some(removed) = cache.remove(&oldest_key) {
                self.cache_bytes = self
                    .cache_bytes
                    .saturating_sub(removed.resource.bytes.len());
            }
        }
        self.cache_bytes = self.cache_bytes.saturating_add(entry_bytes);
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
                origin,
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

    /// Like [`Self::load_str`] but applies the mixed-content policy (#1528).
    pub fn load_str_checked(
        &mut self,
        url: &str,
        active: bool,
    ) -> Result<Arc<Resource>, LoadError> {
        let parsed = Url::parse(url)?;
        self.load_checked(&parsed, active)
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
                    status: response.status,
                    url: response.final_url,
                    content_type: mime::resolve_content_type(&response.headers, &response.body),
                    bytes: response.body,
                })
            }
            Scheme::File => {
                let bytes = load_file_url_with_limit(url, self.options.max_response_bytes)?;
                let slice = slice_bytes(&bytes, range)?;
                Ok(Resource {
                    status: 200,
                    url: url.clone(),
                    content_type: mime::sniff(&slice).mime_type().to_owned(),
                    bytes: slice,
                })
            }
            Scheme::Data => {
                let data = decode_data_url(url)?;
                let slice = slice_bytes(&data.bytes, range)?;
                Ok(Resource {
                    status: 200,
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

    /// Drops every cached resource, including PAC decisions (#1475).
    pub fn clear_cache(&mut self) {
        if let Some(cache) = &mut self.cache {
            cache.clear();
        }
        self.cache_bytes = 0;
        #[cfg(feature = "pac")]
        crate::pac::clear_cache();
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
