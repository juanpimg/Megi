//! M4 phase 1: Service Worker register → install (cache) → activate → fetch interception.
//!
//! Spec anchors: WHATWG Service Workers §4 (Service Worker Life Cycle) and §5
//! (Service Worker Global Scope); W3C Cache API §5 (CacheStorage) / §5.4 (Cache).

#![cfg(feature = "js")]

use std::sync::{Arc, Mutex};

use megi_core::Document;
use megi_js::host::FetchResult;
use megi_js::runtime::JsRuntime;

const SW_SOURCE: &str = r#"
self.addEventListener('install', function (event) {
  event.waitUntil(caches.open('v1').then(function (cache) {
    return cache.put('/data.txt', new Response({
      status: 200,
      statusText: 'OK',
      url: '/data.txt',
      headers: [['content-type', 'text/plain']],
      bodyText: 'FROM-SW-CACHE',
      bodyLength: 13
    }));
  }));
});
self.addEventListener('fetch', function (event) {
  event.respondWith(caches.match(event.request));
});
"#;

#[test]
fn sw_install_caches_and_serves_same_origin_fetch() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::clone(&seen);
    let mut runtime =
        JsRuntime::with_document(Document::new(), "https://example.test/".into()).expect("runtime");

    // The page's network provider would answer with a different body; the
    // assertion below proves the response came from the worker's cache.
    runtime.set_fetch_provider(Arc::new(move |request| {
        calls.lock().expect("lock").push(request.url.clone());
        Ok(FetchResult {
            status: 200,
            status_text: "OK".into(),
            url: request.url.clone(),
            headers: vec![],
            body: b"FROM-NETWORK".to_vec(),
        })
    }));
    runtime.set_worker_loader(Arc::new(|_url| Ok(SW_SOURCE.to_string())));

    runtime
        .eval(
            "navigator.serviceWorker.register('/sw.js', {scope: '/'}).then(function (r) { window.__scope = r.scope; });",
            "sw-test",
        )
        .expect("register");
    runtime.run_microtasks().expect("register microtasks");

    assert_eq!(
        runtime
            .eval(
                "navigator.serviceWorker.controller ? navigator.serviceWorker.controller.state : 'none'",
                "sw-test"
            )
            .expect("controller"),
        "activated"
    );

    runtime
        .eval(
            "window.__body = ''; fetch('/data.txt').then(function (r) { return r.text(); }).then(function (t) { __body = t; });",
            "sw-test",
        )
        .expect("fetch");
    runtime.run_microtasks().expect("fetch microtasks");

    assert_eq!(
        runtime.eval("__body", "sw-test").expect("body"),
        "FROM-SW-CACHE"
    );
    assert_eq!(runtime.eval("__scope", "sw-test").expect("scope"), "/");
    assert!(
        seen.lock().expect("lock").is_empty(),
        "an intercepted same-origin GET must not reach the network provider"
    );
}
