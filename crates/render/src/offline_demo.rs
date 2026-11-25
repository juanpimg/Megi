//! M4#6: reload a document from a service worker cache with the network cut.
//!
//! WHATWG Service Workers §5.2 (`fetch` event) and Appendix A «Match Service
//! Worker Registration»; W3C Cache API §5.4. The first load registers a worker
//! that caches one UTF-8 HTML document. The reload is a **real document
//! navigation** through [`render_url`] with `no_network: true` and a provider
//! that fails; no HTML/CSS/layout/paint code is special-cased.
//!
//! Scope: phase-1 in-memory worker store, UTF-8 bodies, no cache versioning.
//! Only the cached document is served offline; subresources still need network.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use megi_core::Document;
use megi_js::host::{FetchRequest, FetchResult};
use megi_js::{JsError, JsRuntime};

use crate::{CachedDocument, RenderError, RenderOptions, Viewport, render_url};

const DOCUMENT_URL: &str = "https://offline.test/sw_offline.html";
const CACHE_NAME: &str = "sw-offline-demo-v1";

/// Evidence produced by one first-load → network-cut → reload cycle.
#[derive(Debug)]
pub struct OfflineReload {
    /// Deterministic PNG of the reloaded document.
    pub png: Vec<u8>,
    /// Response the worker served for the document navigation.
    pub cached: CachedDocument,
    /// Provider invocations for the reload itself (must be zero).
    pub provider_calls_after_cut: usize,
    /// Whether a request outside the cache failed after the cut.
    pub network_cut_verified: bool,
}

/// Registers the SW, cuts the network and renders a cached document reload.
pub fn render_sw_offline(html: &str, viewport: Viewport) -> Result<OfflineReload, RenderError> {
    let mut runtime = JsRuntime::with_document(Document::new(), DOCUMENT_URL.to_owned())
        .map_err(|_| failure())?;

    let calls = Arc::new(AtomicUsize::new(0));
    let cut = Arc::new(AtomicBool::new(false));
    let body = html.as_bytes().to_vec();
    let provider_calls = Arc::clone(&calls);
    let provider_cut = Arc::clone(&cut);
    runtime.set_fetch_provider(Arc::new(move |request: FetchRequest| {
        provider_calls.fetch_add(1, Ordering::SeqCst);
        if provider_cut.load(Ordering::SeqCst) {
            return Err(JsError::Runtime(format!("network cut: {}", request.url)));
        }
        Ok(FetchResult {
            status: 200,
            status_text: "OK".to_owned(),
            url: request.url.clone(),
            headers: vec![(
                "content-type".to_owned(),
                "text/html; charset=utf-8".to_owned(),
            )],
            body: body.clone(),
        })
    }));

    let source = worker_source(html);
    runtime.set_worker_loader(Arc::new(move |_url| Ok(source.clone())));
    runtime
        .eval(
            "navigator.serviceWorker.register('/sw.js', {scope: '/'}).then(function (r) { window.__sw = r.state; });",
            "sw-offline-register",
        )
        .map_err(|_| failure())?;
    runtime.run_microtasks().map_err(|_| failure())?;
    let state = runtime
        .eval("String(window.__sw || '')", "sw-offline-state")
        .map_err(|_| failure())?;
    if state != "activated" {
        return Err(failure());
    }

    // Cut the provider and prove it: a request outside the cache must fail.
    cut.store(true, Ordering::SeqCst);
    runtime
        .eval(
            "fetch('/not-cached.html').then(function () { window.__cut = 'ok'; }, function () { window.__cut = 'failed'; });",
            "sw-offline-cut",
        )
        .map_err(|_| failure())?;
    runtime.run_microtasks().map_err(|_| failure())?;
    let cut_state = runtime
        .eval("String(window.__cut || '')", "sw-offline-cut-state")
        .map_err(|_| failure())?;
    let network_cut_verified = cut_state == "failed";

    // The document reload itself must be answered by the worker alone.
    calls.store(0, Ordering::SeqCst);
    let response = runtime
        .service_worker_navigation(DOCUMENT_URL)
        .ok_or_else(failure)?;
    let provider_calls_after_cut = calls.load(Ordering::SeqCst);
    let cached = CachedDocument {
        status: response.status,
        headers: response.headers,
        body: response.body,
    };

    let options = RenderOptions {
        viewport,
        no_network: true,
        offline_document: Some(cached.clone()),
        ..RenderOptions::default()
    };
    let output = render_url(DOCUMENT_URL, &options)?;
    Ok(OfflineReload {
        png: output.png,
        cached,
        provider_calls_after_cut,
        network_cut_verified,
    })
}

fn failure() -> RenderError {
    RenderError::NetworkDisabled {
        url: DOCUMENT_URL.to_owned(),
    }
}

/// Worker source: cache the document during install, serve it on every GET.
fn worker_source(html: &str) -> String {
    format!(
        r#"
var DOC_URL = {url};
var DOC_HTML = {html};
self.addEventListener('install', function (event) {{
  event.waitUntil(caches.open('{cache}').then(function (cache) {{
    return cache.put(DOC_URL, new Response({{
      status: 200,
      statusText: 'OK',
      url: DOC_URL,
      headers: [['content-type', 'text/html; charset=utf-8']],
      bodyText: DOC_HTML
    }}));
  }}));
}});
self.addEventListener('fetch', function (event) {{
  event.respondWith(caches.match(event.request).then(function (response) {{
    if (!response) {{ throw new Error('offline: document not cached'); }}
    return response;
  }}));
}});
"#,
        url = js_literal(DOCUMENT_URL),
        html = js_literal(html),
        cache = CACHE_NAME,
    )
}

/// Escapes a Rust string as a JavaScript double-quoted literal.
fn js_literal(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::js_literal;

    #[test]
    fn js_literal_escapes_string_terminators() {
        assert_eq!(js_literal("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }
}
