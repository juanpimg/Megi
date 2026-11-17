//! #1532: XMLHttpRequest subset over the host fetch provider.

#![cfg(feature = "js")]

use std::sync::{Arc, Mutex};

use js::host::{FetchProvider, FetchRequest, FetchResult};
use js::runtime::JsRuntime;

fn runtime_with(result: FetchResult) -> (JsRuntime, Arc<Mutex<Vec<FetchRequest>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&calls);
    let mut runtime = JsRuntime::new().expect("runtime");
    let provider: FetchProvider = Box::new(move |request| {
        seen.lock().expect("lock").push(request);
        Ok(result.clone())
    });
    runtime.set_fetch_provider(provider);
    (runtime, calls)
}

fn ok_result() -> FetchResult {
    FetchResult {
        status: 200,
        status_text: "OK".into(),
        url: "https://example.test/data".into(),
        headers: vec![("content-type".into(), "text/plain".into())],
        body: b"hello xhr".to_vec(),
    }
}

#[test]
fn xhr_constructor_constants_and_initial_state() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var x = new XMLHttpRequest(); String([x.readyState === XMLHttpRequest.UNSENT, \
             XMLHttpRequest.DONE === 4, typeof x.open === 'function', typeof x.send === 'function']);",
            "t",
        )
        .expect("xhr");
    assert_eq!(out, "true,true,true,true");
}

#[test]
fn xhr_get_completes_with_status_and_response_text() {
    let (mut runtime, calls) = runtime_with(ok_result());
    runtime
        .eval(
            "globalThis.__states = []; globalThis.__loaded = false; \
             var x = new XMLHttpRequest(); \
             x.onreadystatechange = function () { __states.push(x.readyState); }; \
             x.onload = function () { __loaded = true; }; \
             x.open('GET', '/data'); x.send();",
            "t",
        )
        .expect("xhr");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("String(__loaded)", "t").unwrap(),
        "true",
        "load event must fire"
    );
    assert_eq!(runtime.eval("String(x.status)", "t").unwrap(), "200");
    assert_eq!(runtime.eval("x.responseText", "t").unwrap(), "hello xhr");
    assert_eq!(runtime.eval("String(x.readyState)", "t").unwrap(), "4");
    let seen = calls.lock().expect("lock");
    assert_eq!(seen.len(), 1, "one request through the host provider");
    assert_eq!(seen[0].method, "GET");
    assert!(seen[0].url.ends_with("/data"), "url={}", seen[0].url);
}

#[test]
fn xhr_send_post_carries_body_and_headers() {
    let (mut runtime, calls) = runtime_with(ok_result());
    runtime
        .eval(
            "var x = new XMLHttpRequest(); x.open('POST', '/submit'); \
             x.setRequestHeader('Content-Type', 'text/plain'); x.send('payload');",
            "t",
        )
        .expect("xhr");
    runtime.run_microtasks().expect("microtasks");
    let seen = calls.lock().expect("lock");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].body.as_deref(), Some(b"payload".as_slice()));
    assert!(
        seen[0]
            .headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("content-type") && value == "text/plain")
    );
}

#[test]
fn xhr_sync_open_is_rejected() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var x = new XMLHttpRequest(); var rejected = false; \
             try { x.open('GET', '/x', false); } catch (e) { rejected = true; } String(rejected);",
            "t",
        )
        .expect("xhr");
    assert_eq!(out, "true");
}
