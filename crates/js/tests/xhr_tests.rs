//! Integration tests for `XMLHttpRequest`, `XMLHttpRequestUpload`, and `ProgressEvent` (#1532).

#![cfg(feature = "js")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use megi_js::error::JsError;
use megi_js::host::{FetchProvider, FetchRequest, FetchResult};
use megi_js::runtime::JsRuntime;

fn mock_runtime(result: FetchResult) -> (JsRuntime, Arc<Mutex<Vec<FetchRequest>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&calls);
    let mut runtime = JsRuntime::new().expect("runtime");
    let provider: FetchProvider = Arc::new(move |request| {
        seen.lock().expect("lock").push(request);
        Ok(result.clone())
    });
    runtime.set_fetch_provider(provider);
    (runtime, calls)
}

#[test]
fn xhr_constants_and_initial_state() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            r#"
            var xhr = new XMLHttpRequest();
            [
              XMLHttpRequest.UNSENT, XMLHttpRequest.OPENED,
              XMLHttpRequest.HEADERS_RECEIVED, XMLHttpRequest.LOADING, XMLHttpRequest.DONE,
              xhr.UNSENT, xhr.DONE, xhr.readyState, xhr.status, xhr.statusText,
              xhr.responseType, xhr.responseText, xhr.response, xhr.timeout,
              xhr.withCredentials, typeof xhr.upload
            ].join('|');
            "#,
            "xhr",
        )
        .expect("eval");
    assert_eq!(out, "0|1|2|3|4|0|4|0|0|||||0|false|object");
}

#[test]
fn xhr_async_get_lifecycle_headers_and_progress_events() {
    let (mut runtime, calls) = mock_runtime(FetchResult {
        status: 200,
        status_text: "OK".into(),
        url: "https://example.test/api/data.json".into(),
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("X-Multi".into(), "one".into()),
            ("X-Multi".into(), "two".into()),
        ],
        body: br#"{"ok":true,"n":7}"#.to_vec(),
    });
    runtime
        .eval(
            r#"
            window.__states = [];
            window.__events = [];
            window.__xhr = new XMLHttpRequest();
            __xhr.onreadystatechange = function () { __states.push(__xhr.readyState); };
            __xhr.addEventListener('loadstart', function (e) { __events.push('loadstart:' + (e instanceof ProgressEvent)); });
            __xhr.addEventListener('progress', function (e) { __events.push('progress:' + e.lengthComputable + ':' + e.loaded + '/' + e.total); });
            __xhr.onload = function (e) { __events.push('load:' + e.loaded); };
            __xhr.addEventListener('loadend', function () { __events.push('loadend'); });
            __xhr.open('get', 'https://example.test/api/data.json');
            __xhr.setRequestHeader('X-Custom', 'alpha');
            __xhr.setRequestHeader('X-Custom', 'beta');
            // Forbidden headers must be ignored rather than crashing send().
            __xhr.setRequestHeader('User-Agent', 'Forbidden');
            __xhr.send();
            "#,
            "xhr",
        )
        .expect("eval");
    runtime.run_microtasks().expect("microtasks");
    let summary = runtime
        .eval(
            r#"
            __states.join(',') + '#' + __events.join(',') + '#' +
            __xhr.status + ':' + __xhr.statusText + ':' + __xhr.responseURL + ':' +
            __xhr.getResponseHeader('content-type') + ':' + __xhr.getResponseHeader('x-multi') + ':' +
            JSON.parse(__xhr.responseText).n
            "#,
            "xhr",
        )
        .expect("summary");
    assert_eq!(
        summary,
        "1,2,3,4#loadstart:true,progress:true:17/17,load:17,loadend#200:OK:https://example.test/api/data.json:application/json:one, two:7"
    );
    let all_headers = runtime
        .eval("__xhr.getAllResponseHeaders()", "xhr")
        .expect("allHeaders");
    assert_eq!(
        all_headers,
        "content-type: application/json\r\nx-multi: one, two\r\n"
    );
    let recorded = calls.lock().expect("lock");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].method, "GET");
    assert!(
        recorded[0]
            .headers
            .iter()
            .any(|(k, v)| k == "X-Custom" && v == "alpha, beta")
    );
}

#[test]
fn xhr_post_response_types_json_and_arraybuffer() {
    let (mut runtime, calls) = mock_runtime(FetchResult {
        status: 201,
        status_text: "Created".into(),
        url: "https://example.test/items".into(),
        headers: vec![("content-type".into(), "application/json".into())],
        body: br#"{"id":99}"#.to_vec(),
    });
    runtime
        .eval(
            r#"
            window.__uploadEvents = [];
            window.__a = new XMLHttpRequest();
            __a.open('POST', 'https://example.test/items');
            __a.responseType = 'json';
            __a.upload.onloadstart = function (e) { __uploadEvents.push('up_start:' + e.total); };
            __a.upload.onload = function (e) { __uploadEvents.push('up_load:' + e.loaded); };
            __a.send('{"name":"megi"}');

            window.__b = new XMLHttpRequest();
            __b.open('POST', 'https://example.test/items');
            __b.responseType = 'arraybuffer';
            __b.send(new Uint8Array([65, 66, 67]));
            "#,
            "xhr",
        )
        .expect("eval");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime
            .eval(
                "__a.status + ':' + __a.response.id + ':' + __uploadEvents.join(',')",
                "xhr"
            )
            .unwrap(),
        "201:99:up_start:15,up_load:15"
    );
    assert_eq!(
        runtime
            .eval(
                "(__b.response instanceof ArrayBuffer) + ':' + __b.response.byteLength + ':' + new Uint8Array(__b.response)[1]",
                "xhr"
            )
            .unwrap(),
        "true:9:34"
    );
    let recorded = calls.lock().expect("lock");
    assert_eq!(recorded.len(), 2);
    assert_eq!(
        recorded[0].body.as_deref(),
        Some(br#"{"name":"megi"}"#.as_slice())
    );
    assert_eq!(recorded[1].body.as_deref(), Some(b"ABC".as_slice()));
}

#[test]
fn xhr_abort_before_microtask_cancels_request_and_resets_to_unsent() {
    let (mut runtime, calls) = mock_runtime(FetchResult {
        status: 200,
        status_text: "OK".into(),
        url: "https://example.test/slow".into(),
        headers: vec![],
        body: b"ok".to_vec(),
    });
    runtime
        .eval(
            r#"
            window.__log = [];
            var x = new XMLHttpRequest();
            x.onreadystatechange = function () { __log.push('rs:' + x.readyState); };
            x.onabort = function () { __log.push('abort'); };
            x.onloadend = function () { __log.push('end'); };
            x.open('GET', 'https://example.test/slow');
            x.send();
            x.abort();
            window.__afterAbortState = x.readyState;
            "#,
            "xhr",
        )
        .expect("eval");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime
            .eval("__log.join(',') + '|' + __afterAbortState", "xhr")
            .unwrap(),
        "rs:1,rs:4,abort,end|0"
    );
    assert_eq!(calls.lock().expect("lock").len(), 0);
}

#[test]
fn xhr_network_error_and_timeout_fire_expected_events() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_fetch_provider(Arc::new(|request| {
        if request.url.contains("timeout") {
            std::thread::sleep(Duration::from_millis(25));
            return Ok(FetchResult {
                status: 200,
                status_text: "OK".into(),
                url: request.url,
                headers: vec![],
                body: b"late".to_vec(),
            });
        }
        Err(JsError::Runtime("connection refused".into()))
    }));
    runtime
        .eval(
            r#"
            window.__errLog = [];
            var errXhr = new XMLHttpRequest();
            errXhr.onerror = function () { __errLog.push('error:' + errXhr.readyState + ':' + errXhr.status); };
            errXhr.onloadend = function () { __errLog.push('loadend'); };
            errXhr.open('GET', 'https://example.test/fail');
            errXhr.send();

            window.__toLog = [];
            var toXhr = new XMLHttpRequest();
            toXhr.timeout = 5;
            toXhr.ontimeout = function () { __toLog.push('timeout:' + toXhr.readyState + ':' + toXhr.status); };
            toXhr.onloadend = function () { __toLog.push('loadend'); };
            toXhr.open('GET', 'https://example.test/timeout');
            toXhr.send();
            "#,
            "xhr",
        )
        .expect("eval");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime
            .eval("__errLog.join(',') + '|' + __toLog.join(',')", "xhr")
            .unwrap(),
        "error:4:0,loadend|timeout:4:0,loadend"
    );
}

#[test]
fn xhr_rejects_synchronous_and_invalid_state_operations() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            r#"
            var xhr = new XMLHttpRequest();
            var syncName = '';
            try { xhr.open('GET', 'https://example.test/sync', false); } catch (e) { syncName = e.name; }
            var hdrErr = '';
            try { xhr.setRequestHeader('X-A', '1'); } catch (e) { hdrErr = e.name; }
            xhr.open('GET', 'https://example.test/ok');
            xhr.responseType = 'json';
            var rtErr = '';
            try { var _t = xhr.responseText; } catch (e) { rtErr = e.name; }
            xhr.responseType = 'invalid-type';
            syncName + '|' + hdrErr + '|' + rtErr + '|' + xhr.responseType;
            "#,
            "xhr",
        )
        .expect("eval");
    assert_eq!(
        out,
        // Synchronous XHR now completes with a deterministic empty response
        // instead of rejecting (#1904); only the invalid-state checks throw.
        "||InvalidStateError|json"
    );
}

#[test]
fn xhr_open_validates_method_token_and_forbidden_methods() {
    // #1854: WHATWG XHR §3.5.1 steps 2-3. A method must be an HTTP token
    // (RFC 9110 §5.6.2). Tokens are NOT trimmed: surrounding whitespace makes
    // them invalid. CONNECT/TRACE/TRACK are forbidden (SecurityError).
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            r#"
            function openResult(method) {
              var x = new XMLHttpRequest();
              try { x.open(method, 'https://example.test/'); return 'ok:' + x._method; }
              catch (e) { return e.name; }
            }
            [
              openResult('get'),
              openResult('BAD METHOD'),
              openResult(' GET'),
              openResult('GET '),
              openResult(''),
              openResult('TRaCe'),
              openResult('connect'),
              openResult('TRACK'),
              openResult('X-CUSTOM')
            ].join('|');
            "#,
            "xhr",
        )
        .expect("eval");
    assert_eq!(
        out,
        "ok:GET|SyntaxError|SyntaxError|SyntaxError|SyntaxError|SecurityError|SecurityError|SecurityError|ok:X-CUSTOM"
    );
}
