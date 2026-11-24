//! Scheme dispatch, cache-aware loads and batched loading.

use super::helpers::resource_from_response;
use super::*;

impl ResourceLoader {
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
                status: 200,
                url: url.clone(),
                bytes: data.bytes,
                content_type: data.mime_type,
            }));
        }

        // Server-pushed preloads are consumed before any cache/network work (per session).
        if let Some(resource) = self.push_cache.remove(&url.cache_key()) {
            // The pushed entry is now consumed: release its bytes so a later push is not
            // rejected by a stale accounting total (#640).
            self.pushed_bytes = self.pushed_bytes.saturating_sub(resource.bytes.len());
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
            if !entry.matches_request(&self.options.user_agent, self.options.origin.as_deref()) {
                // Another `Vary` or `Origin` variant: treat as a miss and refetch.
            } else if entry.is_fresh() {
                self.cache_hits += 1;
                return Ok(entry.resource);
            } else if entry.can_revalidate() {
                return self.revalidate(url, cache_key, entry);
            }
        }

        self.cache_misses += 1;
        let (resource, headers, status) = self.fetch_resource(url)?;
        let resource = Arc::new(resource);
        if status.is_none_or(|code| code == 200) {
            self.store(
                url,
                cache_key,
                Arc::clone(&resource),
                headers.as_ref(),
                self.options.origin.clone(),
            );
        }
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
                    status: response.status,
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
                    status: resource.status,
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
            let (first_url, use_cors) = requests[indices[0]];
            let effective_origin = if use_cors {
                self.options.origin.clone()
            } else {
                None
            };
            match resp {
                Ok(mut response) => {
                    if response.status == 304 {
                        for idx in indices {
                            results[idx] =
                                Some(Err(LoadError::from(FetchError::UnconditionalNotModified)));
                        }
                    } else {
                        let status = response.status;
                        self.store_pushed(std::mem::take(&mut response.pushed));
                        let headers = response.headers.clone();
                        let resource = Arc::new(resource_from_response(response));
                        self.cache_misses += 1;
                        if status == 200 {
                            self.store(
                                first_url,
                                cache_key,
                                Arc::clone(&resource),
                                Some(&headers),
                                effective_origin,
                            );
                        }
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
}
