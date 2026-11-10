//! In-memory [`ResourceLoader`] used by the document/resource pipeline.
//!
//! The loader dispatches by URL scheme: `http:`/`https:` go through [`crate::http::fetch`]
//! (`https` currently fails with [`FetchError::TlsUnsupported`]), `file:` reads the local
//! file, and the resulting bytes are paired with a resolved content type (HTTP
//! `Content-Type` header when present, otherwise MIME sniffing).
//!
//! ## Cache
//!
//! The in-memory cache is enabled by default and keyed by the URL without its fragment
//! ([`Url::cache_key`]). It caches successful loads only; errors are never cached. The
//! loader therefore makes **no** freshness/revalidation guarantees — it is a per-run
//! memoization mechanism, not an HTTP cache (no `Cache-Control`, `ETag` or `Vary`
//! handling; that arrives with the Fetch/CORS work).
//!
//! [`Resource`] is returned behind [`std::sync::Arc`] so sharing it with the DOM/CSS
//! pipeline does not copy the payload.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{FetchError, LoadError};
use crate::file::load_file_url;
use crate::http::{FetchOptions, fetch};
use crate::mime::{self, MimeHint};
use crate::url::{Scheme, Url};

/// A successfully loaded resource.
#[derive(Clone, Debug)]
pub struct Resource {
    /// Final URL (after redirects for `http:`; the requested URL for `file:`).
    pub url: Url,
    /// Raw resource bytes.
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

/// Loads resources by URL, optionally memoizing them in memory.
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
    cache: Option<HashMap<String, Arc<Resource>>>,
    cache_hits: u64,
    cache_misses: u64,
}

impl Default for ResourceLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceLoader {
    /// Creates a loader with default [`FetchOptions`] and the in-memory cache enabled.
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
        }
    }

    /// Creates a loader without a cache (every load hits the network/filesystem).
    pub fn without_cache() -> Self {
        Self {
            options: FetchOptions::default(),
            cache: None,
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    /// Enables or disables the in-memory cache; disabling it drops the cached entries.
    pub fn set_cache_enabled(&mut self, enabled: bool) {
        if enabled {
            if self.cache.is_none() {
                self.cache = Some(HashMap::new());
            }
        } else {
            self.cache = None;
        }
    }

    /// Returns `true` when the in-memory cache is enabled.
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

    /// Loads `url`, returning a cached [`Resource`] when possible.
    pub fn load(&mut self, url: &Url) -> Result<Arc<Resource>, LoadError> {
        let cache_key = url.cache_key();
        if let Some(cache) = &self.cache
            && let Some(cached) = cache.get(&cache_key)
        {
            self.cache_hits += 1;
            return Ok(Arc::clone(cached));
        }
        self.cache_misses += 1;

        let resource = match url.scheme() {
            Scheme::Http => {
                let response = fetch(url, &self.options)?;
                let content_type = mime::resolve_content_type(&response.headers, &response.body);
                Resource {
                    url: response.final_url,
                    bytes: response.body,
                    content_type,
                }
            }
            Scheme::Https => return Err(FetchError::TlsUnsupported.into()),
            Scheme::File => {
                let bytes = load_file_url(url)?;
                Resource {
                    url: url.clone(),
                    content_type: mime::sniff(&bytes).mime_type().to_owned(),
                    bytes,
                }
            }
        };
        let resource = Arc::new(resource);
        if let Some(cache) = &mut self.cache {
            cache.insert(cache_key, Arc::clone(&resource));
        }
        Ok(resource)
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

    /// Number of cached resources.
    pub fn cache_len(&self) -> usize {
        self.cache.as_ref().map_or(0, HashMap::len)
    }

    /// Number of loads served from the cache.
    pub const fn cache_hits(&self) -> u64 {
        self.cache_hits
    }

    /// Number of loads that had to hit the network/filesystem.
    pub const fn cache_misses(&self) -> u64 {
        self.cache_misses
    }
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
