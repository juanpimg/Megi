//! Round 33: AbortController/AbortSignal, extended fetch methods and Response.body stream.

use js::host::{FetchProvider, FetchRequest, FetchResult};
use js::runtime::JsRuntime;
use std::sync::{Arc, Mutex};

fn mock_runtime(result: FetchResult) -> (JsRuntime, Arc<Mutex<Vec<FetchRequest>>>) {
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
        headers: vec![("content-type".into(), "application/json".into())],
        body: br#"{"answer":42,"text":"hello world"}"#.to_vec(),
    }
}

#[test]
fn abort_controller_signal_basics() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.eval("var c = new AbortController(); var seen = 0; c.signal.addEventListener('abort', function () { seen += 1; });", "t").expect("controller");
    assert_eq!(runtime.eval("c.signal.aborted", "t").unwrap(), "false");
    runtime.eval("c.abort('why')", "t").expect("abort");
    assert_eq!(
        runtime
            .eval("c.signal.aborted + ':' + c.signal.reason + ':' + seen", "t")
            .unwrap(),
        "true:why:1"
    );
    assert_eq!(
        runtime
            .eval(
                "try { c.signal.throwIfAborted(); 'no' } catch (e) { e.name }",
                "t"
            )
            .unwrap(),
        "AbortError"
    );
}

#[test]
fn pre_aborted_signal_rejects_typed() {
    let (mut runtime, calls) = mock_runtime(ok_result());
    runtime.eval("var c = new AbortController(); c.abort(); window.__err = ''; fetch('/x', {signal: c.signal}).catch(function (e) { __err = e.name; });", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__err", "t").unwrap(), "AbortError");
    assert!(
        calls.lock().expect("lock").is_empty(),
        "pre-aborted fetch must not call the provider"
    );
}

#[test]
fn abort_in_flight_rejects_before_microtask_drain() {
    let (mut runtime, calls) = mock_runtime(ok_result());
    runtime.eval("var c = new AbortController(); window.__err = ''; fetch('/x', {signal: c.signal}).catch(function (e) { __err = e.name; }); c.abort();", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__err", "t").unwrap(), "AbortError");
    assert_eq!(calls.lock().expect("lock").len(), 1);
}

#[test]
fn fetch_methods_are_forwarded() {
    let (mut runtime, calls) = mock_runtime(ok_result());
    for method in ["PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"] {
        runtime
            .eval(
                &format!("fetch('/x', {{method: '{method}'}}).catch(function () {{}});"),
                "t",
            )
            .expect("fetch");
    }
    runtime.run_microtasks().expect("microtasks");
    let methods: Vec<String> = calls
        .lock()
        .expect("lock")
        .iter()
        .map(|request| request.method.clone())
        .collect();
    assert_eq!(methods, vec!["PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"]);
}

#[test]
fn fetch_method_get_and_head_have_no_body() {
    let (mut runtime, calls) = mock_runtime(ok_result());
    runtime.eval("fetch('/x', {method: 'GET', body: 'ignored'}).catch(function () {}); fetch('/x', {method: 'HEAD', body: 'ignored'}).catch(function () {});", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    let calls = calls.lock().expect("lock");
    assert!(
        calls.iter().all(|request| request.body.is_none()),
        "{calls:?}"
    );
}

#[test]
fn response_body_stream_reads_chunks() {
    let (mut runtime, _calls) = mock_runtime(ok_result());
    runtime.eval("window.__chunks = 0; window.__text = ''; fetch('/x').then(function (r) { var reader = r.body.getReader(); function pump() { return reader.read().then(function (step) { if (step.done) { return; } __chunks += 1; for (var i = 0; i < step.value.length; i++) { __text += String.fromCharCode(step.value[i]); } return pump(); }); } return pump(); });", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert!(
        runtime
            .eval("__chunks", "t")
            .unwrap()
            .parse::<u32>()
            .unwrap()
            >= 2
    );
    assert!(
        runtime
            .eval("__text", "t")
            .unwrap()
            .contains("\"answer\":42")
    );
}

#[test]
fn response_body_cancel_marks_done() {
    let (mut runtime, _calls) = mock_runtime(ok_result());
    runtime.eval("window.__done = false; fetch('/x').then(function (r) { var reader = r.body.getReader(); return reader.cancel('stop').then(function () { return reader.read(); }).then(function (step) { __done = step.done; }); });", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__done", "t").unwrap(), "true");
}

#[test]
fn head_response_has_empty_body() {
    let (mut runtime, _calls) = mock_runtime(ok_result());
    runtime.eval("window.__len = -1; fetch('/x', {method: 'HEAD'}).then(function (r) { return r.arrayBuffer(); }).then(function (b) { __len = b.length; });", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__len", "t").unwrap(), "0");
}

#[test]
fn network_error_is_type_error_like() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_fetch_provider(Box::new(|_| {
        Err(js::JsError::Runtime("network down".into()))
    }));
    runtime.eval("window.__name = ''; window.__msg = ''; fetch('/x').catch(function (e) { __name = e.name; __msg = String(e); });", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__name", "t").unwrap(), "TypeError");
    assert!(runtime.eval("__msg", "t").unwrap().contains("fetch failed"));
}

#[test]
fn stream_consumption_marks_body_used() {
    let (mut runtime, _calls) = mock_runtime(ok_result());
    runtime.eval("window.__used = null; fetch('/x').then(function (r) { var reader = r.body.getReader(); return reader.read().then(function () { __used = r.bodyUsed; return r.text(); }); }).catch(function () { __used = 'rejected'; });", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__used", "t").unwrap(), "rejected");
}
