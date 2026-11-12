//! Round 31: fetch() subset with a host provider (Headers/Request/Response).

#![cfg(feature = "js")]

use std::sync::{Arc, Mutex};

use js::host::{FetchProvider, FetchRequest, FetchResult};
use js::runtime::JsRuntime;

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
        url: "https://example.test/data.json".into(),
        headers: vec![("content-type".into(), "application/json".into())],
        body: br#"{"answer":42}"#.to_vec(),
    }
}

#[test]
fn headers_are_case_insensitive() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.eval("var h = new Headers({'Content-Type': 'application/json'}); h.set('X-A', '1'); h.append('X-A', '2');", "t").expect("headers");
    assert_eq!(
        runtime.eval("h.get('content-type')", "t").unwrap(),
        "application/json"
    );
    assert_eq!(runtime.eval("h.has('x-a')", "t").unwrap(), "true");
    // Fetch: `append` combines values with a comma-space separator (unlike `set`).
    assert_eq!(runtime.eval("h.get('X-A')", "t").unwrap(), "1, 2");
}

#[test]
fn fetch_json_text_and_headers_after_microtasks() {
    let (mut runtime, _calls) = mock_runtime(ok_result());
    runtime.eval("window.__done = false; window.__status = 0; window.__type = ''; fetch('data.json').then(function (r) { __status = r.status; __type = r.headers.get('content-type'); return r.json(); }).then(function (v) { __done = v.answer === 42; });", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime
            .eval("__done + ':' + __status + ':' + __type", "t")
            .unwrap(),
        "true:200:application/json"
    );
}

#[test]
fn fetch_get_request_is_normalized() {
    let (mut runtime, calls) = mock_runtime(ok_result());
    runtime
        .eval(
            "fetch('data.json', {headers: {'X-Test': 'yes'}}).then(function () {});",
            "t",
        )
        .expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    let calls = calls.lock().expect("lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "GET");
    assert!(calls[0].url.contains("data.json"), "{}", calls[0].url);
    assert!(
        calls[0]
            .headers
            .iter()
            .any(|(name, value)| name == "X-Test" && value == "yes")
    );
}

#[test]
fn fetch_post_body_is_forwarded() {
    let (mut runtime, calls) = mock_runtime(ok_result());
    runtime.eval("fetch('/api', {method: 'POST', headers: {'Content-Type': 'text/plain'}, body: 'hola'}).then(function () {});", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    let calls = calls.lock().expect("lock");
    assert_eq!(calls[0].method, "POST");
    assert_eq!(calls[0].body.as_deref(), Some(b"hola".as_slice()));
}

#[test]
fn fetch_404_sets_ok_false() {
    let (mut runtime, _calls) = mock_runtime(FetchResult {
        status: 404,
        status_text: "Not Found".into(),
        url: "https://example.test/missing".into(),
        headers: vec![],
        body: vec![],
    });
    runtime
        .eval(
            "window.__ok = null; fetch('/missing').then(function (r) { __ok = r.ok; });",
            "t",
        )
        .expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__ok", "t").unwrap(), "false");
}

#[test]
fn fetch_without_provider_rejects_typed() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "window.__err = ''; fetch('/x').catch(function (e) { __err = String(e); });",
            "t",
        )
        .expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    let error = runtime.eval("__err", "t").unwrap();
    assert!(error.contains("fetch is not available"), "{error}");
}

#[test]
fn fetch_provider_error_rejects() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_fetch_provider(Box::new(|_| {
        Err(js::JsError::Runtime("network down".into()))
    }));
    runtime
        .eval(
            "window.__err = ''; fetch('/x').catch(function (e) { __err = String(e); });",
            "t",
        )
        .expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert!(runtime.eval("__err", "t").unwrap().contains("network down"));
}

#[test]
fn request_and_response_basics() {
    let (mut runtime, _calls) = mock_runtime(ok_result());
    runtime.eval("window.__url = ''; window.__method = ''; var req = new Request('/a', {method: 'POST', body: 'x'}); fetch(req).then(function () {});", "t").expect("request");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("req.method", "t").unwrap(), "POST");
    assert_eq!(runtime.eval("new Response({status: 201, statusText: 'Created', url: '/x', headers: [], bodyText: 'hi', bodyLength: 2}).ok", "t").unwrap(), "true");
}

#[test]
fn response_body_can_be_used_once() {
    let (mut runtime, _calls) = mock_runtime(ok_result());
    runtime.eval("window.__second = ''; fetch('/a').then(function (r) { return r.text().then(function () { return r.text(); }); }).catch(function (e) { __second = String(e); });", "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert!(
        runtime
            .eval("__second", "t")
            .unwrap()
            .contains("body already used")
    );
}

#[test]
fn fetch_caps_headers() {
    let (mut runtime, _calls) = mock_runtime(ok_result());
    let mut script = String::from("window.__err = ''; var h = {}; ");
    for index in 0..70 {
        script.push_str(&format!("h['x-{index}'] = 'v'; "));
    }
    script.push_str("fetch('/x', {headers: h}).catch(function (e) { __err = String(e); });");
    runtime.eval(&script, "t").expect("fetch");
    runtime.run_microtasks().expect("microtasks");
    assert!(
        runtime
            .eval("__err", "t")
            .unwrap()
            .contains("headers limit"),
        "{}",
        runtime.eval("__err", "t").unwrap()
    );
}

#[test]
fn fetch_file_base_resolves_json_chain() {
    use dom::Document;
    let mut runtime = JsRuntime::with_document(
        Document::new(),
        "file:///tmp/js_fetch_json.html".to_string(),
    )
    .expect("runtime");
    runtime.set_fetch_provider(Box::new(|request| {
        assert!(request.url.contains("fetch_data.json"), "{}", request.url);
        Ok(ok_result())
    }));
    runtime.eval("window.__out = 'pending'; fetch('fetch_data.json').then(function (r) { return r.json(); }).then(function (data) { __out = 'value=' + data.answer; }).catch(function (error) { __out = 'error:' + error; });", "t").expect("script");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__out", "t").unwrap(), "value=42");
}
