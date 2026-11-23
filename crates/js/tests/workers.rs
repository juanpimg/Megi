//! Round 30: deterministic cooperative Web Workers (no threads).

#![cfg(feature = "js")]

use std::sync::Arc;

use megi_js::runtime::JsRuntime;

fn runtime_with_worker(source: &'static str) -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_worker_loader(Arc::new(move |url| {
        if url.ends_with("worker.js") {
            Ok(source.to_string())
        } else {
            Err(megi_js::JsError::Runtime(format!("not found: {url}")))
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
fn worker_has_no_direct_dom_globals() {
    // F6-WORKERS-DOM: `document` is now a message-based proxy (not the real DOM), while
    // `window` stays absent and `self` is still the worker global.
    let mut runtime = runtime_with_worker(
        "postMessage(typeof document + ':' + typeof document.getElementById + ':' + typeof window + ':' + (self === globalThis));",
    );
    runtime.eval("window.__info = ''; var w = new Worker('worker.js'); w.onmessage = function (e) { __info = e.data; };", "t").expect("script");
    runtime.run_worker_tasks(16).expect("tasks");
    assert_eq!(
        runtime.eval("__info", "t").unwrap(),
        "object:function:undefined:true"
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

#[test]
fn forged_worker_handle_cannot_abort_the_process() {
    let mut runtime = runtime_with_worker(SUM);
    // The internal handle is read-only and termination ignores unknown ids. Neither
    // a forged assignment nor a borrowed `terminate` may index past the table (#1465).
    runtime
        .eval(
            "window.__ok = 0; var w = new Worker('worker.js');
             try { w.__megi_worker = 1000; } catch (e) {}
             try { w.terminate.call({ __megi_worker: 1000 }); } catch (e) {}
             window.__ok = 1;",
            "t",
        )
        .expect("forged handle script must not panic");
    assert_eq!(runtime.eval("__ok", "t").unwrap(), "1");
}

#[test]
fn worker_reply_handler_exception_is_reported_to_the_page() {
    let mut runtime = runtime_with_worker(SUM);
    runtime
        .eval(
            "window.__reported = ''; var w = new Worker('worker.js');
             window.addEventListener('error', function (e) { __reported = e.message; });
             w.onmessage = function () { throw new Error('reply boom'); };
             w.postMessage({a: 1, b: 2});",
            "t",
        )
        .expect("setup");
    runtime.run_worker_tasks(16).expect("tasks");
    let console = runtime.take_console();
    assert!(
        console
            .iter()
            .any(|entry| entry.level == "error" && entry.message.contains("reply boom")),
        "the reply handler exception must be reported: {console:?}"
    );
    let _ = runtime.eval("__reported", "t").unwrap();
}

#[test]
fn worker_onerror_getter_reflects_assigned_handler() {
    // #1746: WebIDL §3.7.4 — an attribute must reflect its assigned value.
    let mut runtime = runtime_with_worker("");
    let out = runtime
        .eval(
            "var w = new Worker('worker.js'); \
             var f = function () {}; w.onerror = f; \
             String(w.onerror === f);",
            "t",
        )
        .expect("script");
    assert_eq!(out, "true");
}

#[test]
fn worker_remove_event_listener_stops_delivery() {
    // #1745: removeEventListener must unregister by callback identity.
    let mut runtime = runtime_with_worker("onmessage = function (e) { postMessage(e.data); };");
    runtime
        .eval(
            "window.__n = 0; var w = new Worker('worker.js'); \
             var f = function () { __n += 1; }; \
             w.addEventListener('message', f); w.postMessage(1);",
            "t",
        )
        .expect("script");
    runtime.run_worker_tasks(16).expect("tasks");
    assert_eq!(runtime.eval("__n", "t").unwrap(), "1");
    runtime
        .eval(
            "w.removeEventListener('message', f); w.postMessage(2);",
            "t",
        )
        .expect("script");
    runtime.run_worker_tasks(16).expect("tasks");
    assert_eq!(runtime.eval("__n", "t").unwrap(), "1");
}

#[test]
fn terminated_worker_id_is_not_reused_by_a_new_worker() {
    // #1742: a terminated Worker keeps its id so the old JS object cannot address a new one.
    let mut runtime = runtime_with_worker("onmessage = function (e) { postMessage(e.data); };");
    runtime
        .eval(
            "window.__info = ''; var w = new Worker('worker.js'); \
             var id0 = w.__megi_worker; w.terminate(); \
             var w2 = new Worker('worker.js'); var id2 = w2.__megi_worker; \
             try { w.postMessage(99); } catch (e) {} \
             w2.postMessage(1); \
             __info = String(id0) + ':' + String(id2);",
            "t",
        )
        .expect("script");
    runtime.run_worker_tasks(16).expect("tasks");
    assert_eq!(runtime.eval("__info", "t").unwrap(), "0:1");
}

#[test]
fn worker_reads_dom_through_a_message_proxy() {
    // F6-WORKERS-DOM: workers have no direct DOM, but a proxy forwards queries to the
    // owning document over the existing deterministic postMessage channel.
    let mut runtime = runtime_with_worker(
        "document.getElementById('x').then(function (el) { \
           el.textContent.then(function (text) { postMessage(text); }); \
         });",
    );
    runtime
        .eval(
            "document.body.innerHTML = '<span id=x>hello dom</span>';",
            "page.js",
        )
        .expect("setup");
    runtime
        .eval(
            "window.__seen = ''; var w = new Worker('worker.js'); \
             w.onmessage = function (e) { __seen = e.data; };",
            "page.js",
        )
        .expect("worker");
    runtime.run_worker_tasks(64).expect("tasks");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__seen", "page.js").expect("seen"),
        "hello dom"
    );
}

#[test]
fn worker_writes_dom_through_the_proxy() {
    // F6-WORKERS-DOM: setAttribute / setTextContent are forwarded through __megi_dom_call
    // and applied to the owning Document on the main side.
    let mut runtime = runtime_with_worker(
        "document.getElementById('x').then(function (el) { \
           el.setAttribute('data-from-worker', 'yes').then(function () { \
             el.setTextContent('changed').then(function () { postMessage('done'); }); \
           }); \
         });",
    );
    runtime
        .eval(
            "document.body.innerHTML = '<span id=x>original</span>';",
            "page.js",
        )
        .expect("setup");
    runtime
        .eval(
            "window.__seen = ''; var w = new Worker('worker.js'); \
             w.onmessage = function (e) { __seen = e.data; };",
            "page.js",
        )
        .expect("worker");
    runtime.run_worker_tasks(128).expect("tasks");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__seen", "page.js").expect("seen"), "done");
    assert_eq!(
        runtime
            .eval(
                "document.getElementById('x').getAttribute('data-from-worker')",
                "page.js",
            )
            .expect("attribute"),
        "yes"
    );
    assert_eq!(
        runtime
            .eval("document.getElementById('x').textContent", "page.js")
            .expect("text"),
        "changed"
    );
}

#[test]
fn worker_can_query_computed_style_through_the_proxy() {
    // F6-WORKERS-DOM: proxy supports getComputedStyle(property) against the owning document.
    let mut runtime = runtime_with_worker(
        "document.getElementById('x').then(function (el) { \
           el.getComputedStyle('color').then(function (value) { postMessage(value); }); \
         });",
    );
    runtime
        .eval(
            "document.body.innerHTML = '<span id=x style=\"color:red\">x</span>';",
            "page.js",
        )
        .expect("setup");
    runtime
        .eval(
            "window.__seen = ''; var w = new Worker('worker.js'); \
             w.onmessage = function (e) { __seen = e.data; };",
            "page.js",
        )
        .expect("worker");
    runtime.run_worker_tasks(128).expect("tasks");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__seen", "page.js").expect("seen"), "red");
}

#[test]
fn worker_can_traverse_elements_by_tag_name() {
    // F6-WORKERS-DOM: proxy supports document.getElementsByTagName() returning element proxies.
    let mut runtime = runtime_with_worker(
        "document.getElementsByTagName('p').then(function (els) { \
           postMessage(els.length); \
         });",
    );
    runtime
        .eval(
            "document.body.innerHTML = '<p id=a>one</p><div>x</div><p id=b>two</p>';",
            "page.js",
        )
        .expect("setup");
    runtime
        .eval(
            "window.__seen = ''; var w = new Worker('worker.js'); \
             w.onmessage = function (e) { __seen = e.data; };",
            "page.js",
        )
        .expect("worker");
    runtime.run_worker_tasks(128).expect("tasks");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__seen", "page.js").expect("seen"), "2");
}

#[test]
fn worker_can_read_and_mutate_classlist_through_the_proxy() {
    // F6-WORKERS-DOM: proxy element exposes classList with Promise-returning operations.
    let mut runtime = runtime_with_worker(
        "document.getElementById('x').then(function (el) { \
           return el.classList.contains('a').then(function (before) { \
             return el.classList.add('b').then(function () { \
               return el.classList.remove('a').then(function () { \
                 return el.classList.contains('b').then(function (after) { \
                   postMessage(before + ':' + after); \
                 }); \
               }); \
             }); \
           }); \
         });",
    );
    runtime
        .eval(
            "document.body.innerHTML = '<div id=x class=\"a\">x</div>';",
            "page.js",
        )
        .expect("setup");
    runtime
        .eval(
            "window.__seen = ''; var w = new Worker('worker.js'); \
             w.onmessage = function (e) { __seen = e.data; };",
            "page.js",
        )
        .expect("worker");
    runtime.run_worker_tasks(256).expect("tasks");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__seen", "page.js").expect("seen"),
        "true:true"
    );
    let classes = runtime
        .eval("document.getElementById('x').className", "page.js")
        .expect("class");
    assert_eq!(
        classes, "b",
        "class mutations must reach the owning document"
    );
}

#[test]
fn worker_receives_dom_events_from_a_proxied_element() {
    // F6-WORKERS-DOM: a worker can listen for events on a proxied element and receives them
    // through the deterministic message pump when the page dispatches them.
    let mut runtime = runtime_with_worker(
        "document.getElementById('x').then(function (el) { \
           el.addEventListener('click', function (event) { postMessage('clicked:' + event.type); }); \
           postMessage('listening'); \
         });",
    );
    runtime
        .eval(
            "document.body.innerHTML = '<button id=x>x</button>';",
            "page.js",
        )
        .expect("setup");
    runtime
        .eval(
            "window.__log = ''; var w = new Worker('worker.js'); \
             w.onmessage = function (e) { __log += e.data + ';'; };",
            "page.js",
        )
        .expect("worker");
    runtime.run_worker_tasks(128).expect("setup tasks");
    runtime
        .eval(
            "document.getElementById('x').dispatchEvent(new Event('click'));",
            "page.js",
        )
        .expect("dispatch click");
    runtime.run_worker_tasks(128).expect("delivery tasks");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__log", "page.js").expect("log"),
        "listening;clicked:click;"
    );
}

#[test]
fn worker_onmessage_setter_preserves_add_event_listener() {
    // #2156: Assigning `worker.onmessage` must not clear `addEventListener('message', ...)` listeners.
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_worker_loader(Arc::new(|_url| {
        Ok("self.postMessage('hi');".to_string())
    }));
    runtime
        .eval(
            "window.__log = []; \
             var w = new Worker('worker.js'); \
             w.addEventListener('message', function(e) { __log.push('listener:' + e.data); }); \
             w.onmessage = function(e) { __log.push('prop:' + e.data); };",
            "page.js",
        )
        .expect("worker");
    runtime.run_worker_tasks(64).expect("tasks");
    runtime.run_microtasks().expect("microtasks");
    let out = runtime.eval("__log.join('|')", "page.js").expect("log");
    assert_eq!(out, "prop:hi|listener:hi");
}
