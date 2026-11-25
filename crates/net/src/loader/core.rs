//! Inherent methods: construction, configuration, cache/push management.

use super::*;

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

    /// Installs a session-scoped HSTS store for every subsequent HTTP(S) fetch (#1529).
    ///
    /// When unset, fetches fall back to the process-global HSTS cache. Returns `&mut Self`
    /// for chaining.
    pub fn set_hsts_store(&mut self, store: crate::hsts::SharedHstsStore) -> &mut Self {
        self.options.hsts_store = Some(store);
        self
    }

    /// Builder variant of [`Self::set_hsts_store`].
    pub fn with_hsts_store(mut self, store: crate::hsts::SharedHstsStore) -> Self {
        self.set_hsts_store(store);
        self
    }

    /// Installs a session-scoped credential store for Basic auth challenge retries (#1533).
    pub fn set_credential_store(&mut self, store: crate::auth::SharedCredentialStore) -> &mut Self {
        self.options.credential_store = Some(store);
        self
    }

    /// Builder variant of [`Self::set_credential_store`].
    pub fn with_credential_store(mut self, store: crate::auth::SharedCredentialStore) -> Self {
        self.set_credential_store(store);
        self
    }

    /// Lowers the per-response body cap for the next loads and returns the previous value.
    /// Used by the render image loader so the image byte budget is enforced while fetching,
    /// not after buffering every response (#1213).
    pub fn set_max_response_bytes(&mut self, limit: usize) -> usize {
        let previous = self.options.max_response_bytes;
        self.options.max_response_bytes = limit;
        previous
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
            cache_bytes: 0,
            cache_hits: 0,
            cache_misses: 0,
            cache_revalidations: 0,
            push_cache: HashMap::new(),
            pushed_used: 0,
            pushed_discarded: 0,
            pushed_bytes: 0,
            h1_pool,
            allow_file: true,
            mixed_content_context: None,
            events: Vec::new(),
        }
    }

    /// Creates a loader without a cache (every load hits the network/filesystem).
    pub fn without_cache() -> Self {
        Self {
            options: FetchOptions::default(),
            cache: None,
            cache_bytes: 0,
            cache_hits: 0,
            cache_misses: 0,
            cache_revalidations: 0,
            push_cache: HashMap::new(),
            pushed_used: 0,
            pushed_discarded: 0,
            pushed_bytes: 0,
            h1_pool: crate::http::H1Pool::new(4, std::time::Duration::from_secs(30)),
            allow_file: true,
            mixed_content_context: None,
            events: Vec::new(),
        }
    }

    /// Appends the real request/response observations to an owned sink (M7 Network domain).
    ///
    /// The loader keeps ownership of its log; callers receive copies in their own buffer and
    /// never hold a borrow into loader state.
    pub fn append_network_events_to(&self, sink: &mut Vec<NetworkEvent>) {
        sink.extend_from_slice(&self.events);
    }

    /// Records one observation, keeping the log bounded (oldest events are dropped first).
    pub(super) fn record_network_event(&mut self, event: NetworkEvent) {
        if self.events.len() == MAX_NETWORK_EVENTS {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    /// Merges the navigation loader's observations into the subresource loader that
    /// becomes the render session's event log.
    pub fn absorb_network_events_from(&mut self, other: &mut Self) {
        self.events.append(&mut other.events);
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

    /// Sets the document scheme used for mixed-content decisions (#1528). `None` (default)
    /// disables the policy. The renderer sets this to the loaded document's scheme.
    pub fn set_mixed_content_context(&mut self, scheme: Option<Scheme>) -> &mut Self {
        self.mixed_content_context = scheme;
        self
    }

    /// Loads a subresource in CORS mode while enforcing the mixed-content policy (#1528).
    pub fn load_checked(&mut self, url: &Url, active: bool) -> Result<Arc<Resource>, LoadError> {
        self.load_with_mode_checked(url, active, true)
    }

    /// Loads a subresource in `no-cors` mode while enforcing the mixed-content policy
    /// (#1528, #1537). Used for style sheets, backgrounds and classic scripts.
    pub fn load_no_cors_checked(
        &mut self,
        url: &Url,
        active: bool,
    ) -> Result<Arc<Resource>, LoadError> {
        self.load_with_mode_checked(url, active, false)
    }

    /// Loads a subresource, enforcing the mixed-content policy (#1528) and selecting CORS
    /// (`use_cors = true`) or `no-cors` (`use_cors = false`) mode (#1537).
    ///
    /// The policy is installed on [`FetchOptions::mixed_content`] for the duration of the
    /// fetch so every HTTP redirect hop is checked/upgraded before a plaintext socket opens.
    pub fn load_with_mode_checked(
        &mut self,
        url: &Url,
        active: bool,
        use_cors: bool,
    ) -> Result<Arc<Resource>, LoadError> {
        let Some(doc_scheme) = self.mixed_content_context else {
            return if use_cors {
                self.load(url)
            } else {
                self.load_no_cors(url)
            };
        };
        let policy = crate::mixed_content::MixedContentPolicy::new(doc_scheme, active);
        let checked_url = policy.apply(url)?;
        let saved = self.options.mixed_content.replace(policy);
        let result = if use_cors {
            self.load(&checked_url)
        } else {
            self.load_no_cors(&checked_url)
        };
        self.options.mixed_content = saved;
        let resource = result?;
        // Also guard against a cached resource whose final URL was plaintext HTTP.
        if policy.decide(&resource.url) != crate::mixed_content::MixedContentAction::Allow {
            return Err(FetchError::MixedContentBlocked(resource.url.as_str().to_owned()).into());
        }
        Ok(resource)
    }

    /// Batch variant of [`Self::load_checked`]: loads `(url, use_cors, active)` entries while
    /// enforcing the installed mixed-content policy on initial URLs and redirect hops (#1528).
    pub fn load_many_checked(
        &mut self,
        requests: &[(&Url, bool, bool)],
    ) -> Vec<Result<Arc<Resource>, LoadError>> {
        let Some(doc_scheme) = self.mixed_content_context else {
            let plain: Vec<(&Url, bool)> =
                requests.iter().map(|&(url, cors, _)| (url, cors)).collect();
            return self.load_many(&plain);
        };
        let mut results: Vec<Option<Result<Arc<Resource>, LoadError>>> =
            (0..requests.len()).map(|_| None).collect();
        let mut forwarded_urls: Vec<Url> = Vec::with_capacity(requests.len());
        let mut forwarded_flags: Vec<(usize, bool, bool)> = Vec::with_capacity(requests.len());
        for (idx, &(url, use_cors, active)) in requests.iter().enumerate() {
            let policy = crate::mixed_content::MixedContentPolicy::new(doc_scheme, active);
            match policy.apply(url) {
                Ok(checked) => {
                    forwarded_urls.push(checked);
                    forwarded_flags.push((idx, use_cors, active));
                }
                Err(error) => {
                    results[idx] = Some(Err(error.into()));
                }
            }
        }
        // Only passive (active = false) or active (active = true) policies need redirect enforcement;
        // if all forwarded requests share `active`, run `load_many` once; otherwise split.
        let mut run_batch = |target_active: bool| {
            let subset_indices: Vec<usize> = forwarded_flags
                .iter()
                .enumerate()
                .filter_map(|(pos, &(_, _, active))| (active == target_active).then_some(pos))
                .collect();
            if subset_indices.is_empty() {
                return;
            }
            let policy = crate::mixed_content::MixedContentPolicy::new(doc_scheme, target_active);
            let sub_reqs: Vec<(&Url, bool)> = subset_indices
                .iter()
                .map(|&pos| (&forwarded_urls[pos], forwarded_flags[pos].1))
                .collect();
            let saved = self.options.mixed_content.replace(policy);
            let sub_results = self.load_many(&sub_reqs);
            self.options.mixed_content = saved;
            for (pos, res) in subset_indices.into_iter().zip(sub_results) {
                let orig_idx = forwarded_flags[pos].0;
                let checked = res.and_then(|resource| {
                    if policy.decide(&resource.url)
                        != crate::mixed_content::MixedContentAction::Allow
                    {
                        Err(
                            FetchError::MixedContentBlocked(resource.url.as_str().to_owned())
                                .into(),
                        )
                    } else {
                        Ok(resource)
                    }
                });
                results[orig_idx] = Some(checked);
            }
        };
        run_batch(false);
        run_batch(true);
        results.into_iter().map(|r| r.expect("filled")).collect()
    }

    /// Rejects `file:` URLs when local-file access is disabled.
    pub(super) fn check_file_access(&self, url: &Url) -> Result<(), LoadError> {
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
            self.cache_bytes = 0;
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

    /// Fetches a navigation GET (following redirects) with pooled connections.
    ///
    /// The final hop's keep-alive connection stays in this loader's pool, so a caller can
    /// transfer the warm pool to the loader that serves the page's subresources. Keeping the
    /// pool behind this method avoids exposing the connection store as a field.
    pub fn fetch_navigation_pooled(&mut self, url: &Url) -> Result<Response, FetchError> {
        // M7 Network domain: a navigation is a real request of this load session.
        let started = Instant::now();
        let result =
            crate::http::fetch_pooled_following_redirects(url, &self.options, &mut self.h1_pool);
        match &result {
            Ok(response) => {
                let resource = Resource {
                    status: response.status,
                    url: response.final_url.clone(),
                    bytes: response.body.clone(),
                    content_type: mime::resolve_content_type(&response.headers, &response.body),
                };
                self.record_network_event(NetworkEvent::from_resource(
                    &resource,
                    "GET",
                    false,
                    started.elapsed(),
                ));
            }
            Err(error) => self.record_network_event(NetworkEvent {
                url: url.as_str().to_owned(),
                method: "GET".to_owned(),
                status: 0,
                content_type: String::new(),
                encoded_bytes: 0,
                from_cache: false,
                duration: started.elapsed(),
                error: Some(error.to_string()),
            }),
        }
        result
    }

    /// Transfers the warmed keep-alive pool of `other` into this loader. Both loaders belong
    /// to one load session (document + subresources), so a connection established by the
    /// navigation request is reusable by the CSS/script/image fetches that follow it.
    pub fn absorb_pool_from(&mut self, other: &mut ResourceLoader) {
        self.h1_pool = std::mem::take(&mut other.h1_pool);
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
    pub(super) fn store_pushed(&mut self, pushed: Vec<crate::http::PushedResponse>) {
        for pushed in pushed {
            // Pushed responses are real responses: a cross-origin push must pass the same
            // CORS check as a normal fetch BEFORE its `Set-Cookie` lands in the jar —
            // otherwise a hostile h2 server plants cookies via rejected pushes (w2-net-001).
            // Only CORS-approved pushes reach `push_cache` served by `load()` (#652).
            if let Some(origin) = self.options.origin.as_deref()
                // A push is a preload candidate, not a credentialed response: it is only served
                // to a same-origin/CORS-approved consumer, so the credential flag stays false.
                && crate::cors::validate_response(&pushed.url, origin, &pushed.headers, false)
                    .is_err()
            {
                self.pushed_discarded += 1;
                continue;
            }
            if let Some(jar) = &self.options.cookie_jar {
                jar.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .store_from_response(&pushed.url, &pushed.headers);
            }
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
            // Subtract the replaced entry's bytes before adding the new one (#1258).
            if let Some(old) = self.push_cache.get(&key) {
                self.pushed_bytes = self.pushed_bytes.saturating_sub(old.bytes.len());
            }
            self.push_cache.insert(
                key,
                Arc::new(Resource {
                    status: 200,
                    url: pushed.url,
                    content_type,
                    bytes: pushed.body,
                }),
            );
            self.pushed_bytes = self.pushed_bytes.saturating_add(bytes);
        }
    }
}
