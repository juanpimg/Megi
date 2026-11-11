//! Round 30: deterministic cooperative Web Workers (no threads).

use js::runtime::JsRuntime;

fn runtime_with_worker(source: &'static str) -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_worker_loader(Box::new(move |url| {
        if url.ends_with("worker.js") {
            Ok(source.to_string())
        } else {
            Err(js::JsError::Runtime(format!("not found: {url}")))
        }
    }));
    runtime
}

const SUM: &str = "onmessage = function (e) { postMessage(e.data.a + e.data.b); };";

#[test]
fn worker_sum_replies_onmessage() {
    let mut runtime = runtime_with_worker(SUM);
    runtime.eval("window.__sum = 0; var w = new Worker('worker.js'); w.onmessage = function (e) { __sum = e.data; }; w.postMessage({a: 20, b: 22});", "t").expect("script");
    assert_eq!(runtime.eval("__sum", "t").unwrap(), "0");
    runtime.run_worker_tasks(16).expect("tasks");
    assert_eq!(runtime.eval("__sum", "t").unwrap(), "42");
}

#[test]
fn worker_add_event_listener() {
    let mut runtime = runtime_with_worker(SUM);
    runtime.eval("window.__got = 0; var w = new Worker('worker.js'); w.addEventListener('message', function (e) { __got = e.data; }); w.postMessage({a: 2, b: 3});", "t").expect("script");
    runtime.run_worker_tasks(16).expect("tasks");
    assert_eq!(runtime.eval("__got", "t").unwrap(), "5");
}

#[test]
fn worker_messages_are_fifo() {
    let mut runtime = runtime_with_worker("onmessage = function (e) { postMessage(e.data); };");
    runtime.eval("window.__order = []; var w = new Worker('worker.js'); w.onmessage = function (e) { __order.push(e.data); }; w.postMessage(1); w.postMessage(2); w.postMessage(3);", "t").expect("script");
    runtime.run_worker_tasks(16).expect("tasks");
    assert_eq!(runtime.eval("__order.join(',')", "t").unwrap(), "1,2,3");
}

#[test]
fn worker_terminate_discards_pending() {
    let mut runtime = runtime_with_worker(SUM);
    runtime.eval("window.__n = 0; var w = new Worker('worker.js'); w.onmessage = function () { __n += 1; }; w.postMessage({a: 1, b: 2}); w.terminate();", "t").expect("script");
    runtime.run_worker_tasks(16).expect("tasks");
    assert_eq!(runtime.eval("__n", "t").unwrap(), "0");
}

#[test]
fn worker_script_error_calls_onerror() {
    let mut runtime = runtime_with_worker("throw new Error('boom');");
    runtime.eval("window.__err = ''; var w = new Worker('worker.js'); w.onerror = function (e) { __err = e.message; };", "t").expect("script");
    runtime.run_worker_tasks(16).expect("tasks");
    assert!(runtime.eval("__err", "t").unwrap().contains("boom"));
    assert_eq!(runtime.eval("1 + 1", "t").unwrap(), "2");
}

#[test]
fn worker_message_exception_calls_onerror_and_page_continues() {
    let mut runtime =
        runtime_with_worker("onmessage = function () { throw new Error('handler boom'); };");
    runtime.eval("window.__err = ''; var w = new Worker('worker.js'); w.onerror = function (e) { __err = e.message; }; w.postMessage(1);", "t").expect("script");
    runtime.run_worker_tasks(16).expect("tasks");
    assert!(runtime.eval("__err", "t").unwrap().contains("boom"));
}

#[test]
fn worker_without_loader_throws_typed_error() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let error = runtime
        .eval("new Worker('worker.js')", "t")
        .expect_err("no loader");
    assert!(error.to_string().contains("Worker scripts"), "{error}");
}

#[test]
fn worker_has_no_dom_globals() {
    let mut runtime = runtime_with_worker(
        "postMessage(typeof document + ':' + typeof window + ':' + (self === globalThis));",
    );
    runtime.eval("window.__info = ''; var w = new Worker('worker.js'); w.onmessage = function (e) { __info = e.data; };", "t").expect("script");
    runtime.run_worker_tasks(16).expect("tasks");
    assert_eq!(
        runtime.eval("__info", "t").unwrap(),
        "undefined:undefined:true"
    );
}

#[test]
fn worker_cap_is_enforced() {
    let mut runtime = runtime_with_worker(SUM);
    runtime.eval("window.__workers = []; for (var i = 0; i < 8; i++) { __workers.push(new Worker('worker.js')); }", "t").expect("8 workers");
    let error = runtime
        .eval("new Worker('worker.js')", "t")
        .expect_err("cap");
    assert!(error.to_string().contains("too many"), "{error}");
}

#[test]
fn worker_reply_delivered_by_run_microtasks() {
    let mut runtime = runtime_with_worker(SUM);
    runtime.eval("window.__sum = 0; var w = new Worker('worker.js'); w.onmessage = function (e) { __sum = e.data; }; w.postMessage({a: 1, b: 41});", "t").expect("script");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__sum", "t").unwrap(), "42");
}
