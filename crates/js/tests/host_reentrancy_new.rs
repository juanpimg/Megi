//! TDD regression specifications for the JS host-bridge rewrite (plan Task 4).
//!
//! Every test here is `#[ignore]`d until the migration completes, so the current
//! suite stays green while these pin the intended re-entrancy contract:
//!
//! - a provider may call back into JavaScript without hitting `already borrowed`;
//! - a nested (re-entrant) event listener sees DOM nodes created by its caller;
//! - a history provider may dispatch an event;
//! - a fetch provider may pump microtasks, delivering a promise exactly once.
//!
//! Re-entry seam: providers are `Send + Sync`, while `JsRuntime` owns a
//! `Rc<RefCell<Host>>` and a Boa `Context`, so it is `!Send`. A provider cannot
//! capture the very runtime that is calling it through today's public API. Each
//! provider below therefore re-enters a short-lived runtime created *inside* the
//! closure (no captured non-`Send` state). When the bridge lands, replace that
//! probe with the new same-realm re-entry handle and assert the JS-visible globals
//! on the original runtime — the surrounding assertions already pin that outcome.

#![cfg(feature = "js")]

use std::sync::{Arc, Mutex};

use megi_js::host::FetchResult;
use megi_js::JsRuntime;

/// Counts provider invocations across a `Send + Sync` closure.
fn call_counter() -> (Arc<Mutex<usize>>, impl Fn() -> usize) {
    let calls = Arc::new(Mutex::new(0usize));
    let reader = Arc::clone(&calls);
    (calls, move || *reader.lock().expect("counter lock"))
}

/// A provider that re-enters JavaScript: it evaluates a script in a fresh
/// `JsRuntime` and returns the completion value. This is the seam to replace
/// with the migrated bridge's same-realm handle.
fn reentrant_eval(script: &str, source_name: &str) -> String {
    let mut nested = JsRuntime::new().expect("reentrant runtime");
    nested.eval(script, source_name).expect("reentrant eval")
}

#[test]
#[ignore = "enabled when host bridge migration completes"]
fn provider_can_call_back_into_javascript() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let (calls, call_count) = call_counter();

    runtime.set_computed_style_provider(Box::new(move |_document, _node| {
        // A provider is allowed to enter JS while the outer `getComputedStyle`
        // binding is on the stack. With a borrowed host this panics with
        // `already borrowed: BorrowMutError`; the migrated bridge parks the host
        // in the Boa Context so the nested call runs cleanly.
        let value = reentrant_eval("window.__called = 1; __called", "reentry.js");
        assert_eq!(value, "1", "the re-entered realm must observe its own global");
        *calls.lock().expect("counter lock") += 1;
        vec![("color".to_string(), "rgb(1, 2, 3)".to_string())]
    }));

    runtime
        .eval(
            "var el = document.createElement('div');
             document.body.appendChild(el);
             var computed = getComputedStyle(el);
             window.__out = computed.getPropertyValue('color');",
            "computed.js",
        )
        .expect("getComputedStyle with reentrant provider");

    assert_eq!(
        runtime.eval("__out", "computed.js").expect("out"),
        "rgb(1, 2, 3)",
        "the provider's value must still reach the original realm"
    );
    assert_eq!(call_count(), 1, "one getComputedStyle call ⇒ one provider call");
}

#[test]
#[ignore = "enabled when host bridge migration completes"]
fn nested_callback_sees_nodes_created_by_outer() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "window.__order = [];
             var a = document.createElement('div');
             var b = document.createElement('div');
             document.body.appendChild(a);
             document.body.appendChild(b);
             a.addEventListener('click', function () {
                 __order.push('outer');
                 var span = document.createElement('span');
                 document.body.appendChild(span);
                 b.dispatchEvent(new Event('ping'));
             });
             b.addEventListener('ping', function () {
                 __order.push('inner');
                 window.__tag = document.body.lastChild.tagName;
             });
             a.dispatchEvent(new Event('click'));",
            "nested.js",
        )
        .expect("nested dispatch");

    assert_eq!(
        runtime.eval("__order.join(',')", "nested.js").expect("order"),
        "outer,inner",
        "both listeners must run even though the outer one is still on the stack"
    );
    assert_eq!(
        runtime.eval("__tag", "nested.js").expect("tag"),
        "SPAN",
        "the inner listener must see the node appended by the outer listener"
    );
}

#[test]
#[ignore = "enabled when host bridge migration completes"]
fn history_provider_reentry_does_not_panic() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let (calls, call_count) = call_counter();

    runtime.set_history_provider(Box::new(move |_action| {
        // Dispatching an event from a provider is the re-entrant host effect the
        // old `Rc<RefCell<Host>>` bridge could not express safely.
        let log = reentrant_eval(
            "window.__log = [];
             var n = document.createElement('div');
             document.body.appendChild(n);
             n.addEventListener('from-history', function () { __log.push('seen'); });
             n.dispatchEvent(new Event('from-history'));
             __log.join(',')",
            "history-reentry.js",
        );
        assert_eq!(log, "seen", "the provider must dispatch without panicking");
        *calls.lock().expect("counter lock") += 1;
    }));

    runtime
        .eval(
            "history.pushState({n: 1}, '', '/reentry')",
            "history.js",
        )
        .expect("history.pushState");

    assert_eq!(call_count(), 1, "one pushState ⇒ one provider call");
    assert_eq!(
        runtime.eval("history.length", "history.js").expect("length"),
        "2",
        "the outer realm keeps its own history state"
    );
}

#[test]
#[ignore = "enabled when host bridge migration completes"]
fn fetch_provider_can_pump_microtasks() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let (calls, call_count) = call_counter();

    runtime.set_fetch_provider(Box::new(move |request| {
        assert_eq!(request.url, "/data", "provider receives the normalized request");
        // Pumping from inside a provider must deliver each queued job exactly
        // once. A real same-realm handle would pump the runtime being called;
        // the probe verifies the same contract on its own microtask queue.
        let mut nested = JsRuntime::new().expect("reentrant runtime");
        nested
            .eval(
                "window.__done = 0;
                 Promise.resolve().then(function () { __done += 1; });",
                "fetch-pump.js",
            )
            .expect("queue job");
        nested.run_microtasks().expect("reentrant pump");
        assert_eq!(
            nested.eval("__done", "fetch-pump.js").expect("done"),
            "1",
            "a single pump must settle the promise exactly once"
        );
        *calls.lock().expect("counter lock") += 1;
        Ok(FetchResult {
            status: 200,
            status_text: "OK".to_string(),
            url: request.url.clone(),
            headers: Vec::new(),
            body: br#"{"answer":42}"#.to_vec(),
        })
    }));

    runtime
        .eval(
            "window.__settled = 0;
             fetch('/data').then(function () { __settled += 1; });",
            "fetch.js",
        )
        .expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    runtime.run_microtasks().expect("second pump is a no-op");

    assert_eq!(call_count(), 1, "one fetch ⇒ one provider call");
    assert_eq!(
        runtime.eval("__settled", "fetch.js").expect("settled"),
        "1",
        "the continuation must run exactly once across both pumps"
    );
}
