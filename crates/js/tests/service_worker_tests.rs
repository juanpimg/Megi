//! M4 phase 1: Service Worker register → install → activate → fetch interception.
//!
//! Spec anchors: WHATWG Service Workers §2.3 / Appendix A «Match Service Worker
//! Registration» (scope matching), §4.5 (activation), §5.2 (fetch event) and
//! §6.3 (origin restriction); W3C Cache API §5 (CacheStorage) / §5.4 (Cache).

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

const SW_SOURCE_ACTIVATE: &str = r#"
self.addEventListener('activate', function (event) {
  event.waitUntil(caches.open('v2').then(function (cache) {
    return cache.put('/activate.txt', new Response({
      status: 200,
      statusText: 'OK',
      url: '/activate.txt',
      headers: [],
      bodyText: 'FROM-ACTIVATE',
      bodyLength: 13
    }));
  }));
});
self.addEventListener('fetch', function (event) {
  event.respondWith(caches.match(event.request));
});
"#;

const SW_SOURCE_ANY: &str = r#"
function swResponse(body) {
  return new Response({
    status: 200,
    statusText: 'OK',
    url: '',
    headers: [],
    bodyText: body,
    bodyLength: body.length
  });
}
self.addEventListener('fetch', function (event) {
  event.respondWith(swResponse('FROM-SW-CACHE'));
});
"#;

const SW_SOURCE_QUOTA: &str = r#"
function quotaResponse(body) {
  return new Response({
    status: 200,
    statusText: 'OK',
    url: '',
    headers: [],
    bodyText: body,
    bodyLength: body.length
  });
}
self.addEventListener('install', function (event) {
  event.waitUntil(caches.open('quota').then(function (cache) {
    var jobs = [];
    for (var i = 0; i < 512; i++) { jobs.push(cache.put('/key-' + i, quotaResponse('x'))); }
    return Promise.all(jobs).then(function () {
      return cache.put('/overflow', quotaResponse('x')).then(
        function () { self.__quota = 'none'; },
        function (error) { self.__quota = error.name; }
      );
    });
  }));
});
self.addEventListener('fetch', function (event) {
  event.respondWith(quotaResponse(String(self.__quota || 'none')));
});
"#;

fn runtime_with_sw(source: &'static str, seen: Arc<Mutex<Vec<String>>>) -> JsRuntime {
    let calls = Arc::clone(&seen);
    let mut runtime =
        JsRuntime::with_document(Document::new(), "https://example.test/".into()).expect("runtime");
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
    runtime.set_worker_loader(Arc::new(move |_url| Ok(source.to_string())));
    runtime
}

fn register(runtime: &mut JsRuntime, scope: &str) {
    let script = format!(
        "navigator.serviceWorker.register('/sw.js', {{scope: '{}'}}).then(function (r) {{ window.__scope = r.scope; }});",
        scope
    );
    runtime.eval(&script, "sw-test").expect("register");
    runtime.run_microtasks().expect("register microtasks");
}

fn fetch_text(runtime: &mut JsRuntime, url: &str) {
    let script = format!(
        "window.__body = ''; fetch('{}').then(function (r) {{ return r.text(); }}).then(function (t) {{ __body = t; }});",
        url
    );
    runtime.eval(&script, "sw-test").expect("fetch");
    runtime.run_microtasks().expect("fetch microtasks");
}

fn body(runtime: &mut JsRuntime) -> String {
    runtime.eval("__body", "sw-test").expect("body").to_string()
}

#[test]
fn sw_install_caches_and_serves_same_origin_fetch() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = runtime_with_sw(SW_SOURCE, Arc::clone(&seen));

    register(&mut runtime, "/");

    assert_eq!(
        runtime
            .eval(
                "navigator.serviceWorker.controller ? navigator.serviceWorker.controller.state : 'none'",
                "sw-test"
            )
            .expect("controller"),
        "activated"
    );

    fetch_text(&mut runtime, "/data.txt");

    assert_eq!(body(&mut runtime), "FROM-SW-CACHE");
    assert_eq!(runtime.eval("__scope", "sw-test").expect("scope"), "/");
    assert!(
        seen.lock().expect("lock").is_empty(),
        "an intercepted same-origin GET must not reach the network provider"
    );
}

#[test]
fn sw_activate_caches_and_serves_same_origin_fetch() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = runtime_with_sw(SW_SOURCE_ACTIVATE, Arc::clone(&seen));

    register(&mut runtime, "/");
    fetch_text(&mut runtime, "/activate.txt");

    assert_eq!(
        body(&mut runtime),
        "FROM-ACTIVATE",
        "the activate handler must run before the worker controls fetches"
    );
    assert!(
        seen.lock().expect("lock").is_empty(),
        "a response cached during activate must not reach the network provider"
    );
}

#[test]
fn sw_does_not_intercept_cross_origin_fetch() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = runtime_with_sw(SW_SOURCE_ANY, Arc::clone(&seen));

    register(&mut runtime, "/");
    fetch_text(&mut runtime, "https://other.test/data.txt");

    assert_eq!(
        body(&mut runtime),
        "FROM-NETWORK",
        "a cross-origin GET must bypass the active worker"
    );
    assert_eq!(
        seen.lock().expect("lock").as_slice(),
        ["https://other.test/data.txt"]
    );
}

#[test]
fn sw_scope_boundary_does_not_match_sibling_path() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = runtime_with_sw(SW_SOURCE_ANY, Arc::clone(&seen));

    register(&mut runtime, "/app");
    fetch_text(&mut runtime, "/application");

    assert_eq!(
        body(&mut runtime),
        "FROM-NETWORK",
        "/application must not match a worker registered for /app"
    );
    assert_eq!(
        seen.lock().expect("lock").as_slice(),
        ["https://example.test/application"]
    );
}

#[test]
fn sw_cache_quota_raises_typed_quota_error() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = runtime_with_sw(SW_SOURCE_QUOTA, Arc::clone(&seen));

    register(&mut runtime, "/");
    fetch_text(&mut runtime, "/quota-status");

    assert_eq!(
        body(&mut runtime),
        "QuotaExceededError",
        "the phase-1 cache cap must surface as a typed DOMException"
    );
    assert!(seen.lock().expect("lock").is_empty());
}

#[test]
fn sw_aborted_fetch_skips_worker_and_network() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = runtime_with_sw(SW_SOURCE_ANY, Arc::clone(&seen));

    register(&mut runtime, "/");
    runtime
        .eval(
            "window.__abort = 'none'; var controller = new AbortController(); controller.abort(); fetch('/data.txt', {signal: controller.signal}).then(function () { __abort = 'resolved'; }, function (error) { __abort = error.name; });",
            "sw-test",
        )
        .expect("aborted fetch");
    runtime.run_microtasks().expect("aborted fetch microtasks");

    assert_eq!(
        runtime.eval("__abort", "sw-test").expect("abort"),
        "AbortError"
    );
    assert!(
        seen.lock().expect("lock").is_empty(),
        "an already-aborted fetch must reach neither the worker nor the network"
    );
}
