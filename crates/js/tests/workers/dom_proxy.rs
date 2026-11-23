//! Web Workers reading/writing the DOM through the message proxy.

use super::helpers::*;

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
    runtime.set_worker_loader(Arc::new(|_url| Ok("self.postMessage('hi');".to_string())));
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

#[test]
fn worker_dom_event_is_delivered_only_to_subscribed_workers() {
    // #2148: forward_dom_event_to_workers must route each subscription only to its
    // owning worker_id (and deduplicate per worker), never broadcast to every worker.
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_worker_loader(Arc::new(|url| {
        if url.ends_with("w1.js") {
            Ok("document.getElementById('x').then(function (el) { \
                   el.addEventListener('click', function (e) { postMessage('W1:' + e.type); }); \
                   postMessage('W1:ready'); \
                 });"
            .to_string())
        } else if url.ends_with("w2.js") {
            Ok("document.getElementById('x').then(function (el) { \
                   el.addEventListener('click', function (e) { postMessage('W2:' + e.type); }); \
                   postMessage('W2:ready'); \
                 });"
            .to_string())
        } else {
            Ok("document.getElementById('y').then(function (el) { \
                   el.addEventListener('click', function (e) { postMessage('W3:' + e.type); }); \
                   postMessage('W3:ready'); \
                 });"
            .to_string())
        }
    }));
    runtime
        .eval(
            "document.body.innerHTML = '<button id=x>x</button><div id=y>y</div>'; \
             window.__log = []; \
             var w1 = new Worker('w1.js'); w1.onmessage = function (e) { __log.push(e.data); }; \
             var w2 = new Worker('w2.js'); w2.onmessage = function (e) { __log.push(e.data); }; \
             var w3 = new Worker('w3.js'); w3.onmessage = function (e) { __log.push(e.data); };",
            "page.js",
        )
        .expect("setup");
    runtime.run_worker_tasks(64).expect("register tasks");
    runtime
        .eval(
            "document.getElementById('x').dispatchEvent(new Event('click'));",
            "page.js",
        )
        .expect("dispatch");
    runtime.run_worker_tasks(64).expect("delivery tasks");
    runtime.run_microtasks().expect("microtasks");
    let log = runtime.eval("__log.join(',')", "page.js").expect("log");
    let w1_clicks = log.split(',').filter(|s| *s == "W1:click").count();
    let w2_clicks = log.split(',').filter(|s| *s == "W2:click").count();
    let w3_clicks = log.split(',').filter(|s| *s == "W3:click").count();
    assert_eq!(w1_clicks, 1, "W1 must receive 1 click: {log}");
    assert_eq!(w2_clicks, 1, "W2 must receive 1 click: {log}");
    assert_eq!(w3_clicks, 0, "unsubscribed W3 must receive 0 clicks: {log}");
}

#[test]
fn worker_dom_event_escapes_control_characters_in_event_type() {
    // #2179: RFC 8259 §7 — control characters (`\n`, `\r`, `\t`, etc.) in the event type
    // must be JSON-escaped before queuing the forwarded event message into the worker.
    let mut runtime = runtime_with_worker(
        "document.getElementById('x').then(function (el) { \
           el.addEventListener('a\\nb\\r\\t\\u0007\"\\\\c', function (e) { \
             postMessage('got:' + JSON.stringify(e.type)); \
           }); \
           postMessage('ready'); \
         });",
    );
    runtime
        .eval(
            "document.body.innerHTML = '<button id=x>x</button>'; \
             window.__log = []; window.__err = ''; \
             var w = new Worker('worker.js'); \
             w.onmessage = function (e) { __log.push(e.data); }; \
             w.onerror = function (e) { __err = e.message; };",
            "page.js",
        )
        .expect("setup");
    runtime.run_worker_tasks(64).expect("setup tasks");
    runtime
        .eval(
            "document.getElementById('x').dispatchEvent(new Event('a\\nb\\r\\t\\u0007\"\\\\c'));",
            "page.js",
        )
        .expect("dispatch");
    runtime.run_worker_tasks(64).expect("delivery tasks");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__err", "page.js").unwrap(),
        "",
        "worker must not hit SyntaxError"
    );
    assert_eq!(
        runtime.eval("__log.join('|')", "page.js").unwrap(),
        r#"ready|got:"a\nb\r\t\u0007\"\\c""#
    );
}

#[test]
fn worker_dom_event_forwarding_respects_max_worker_messages() {
    // #2157: forward_dom_event_to_workers and dom_proxy must enforce MAX_WORKER_MESSAGES
    // (256) on the worker inbox instead of letting unbounded events overflow the outbox.
    let mut runtime = runtime_with_worker(
        "document.getElementById('x').then(function (el) { \
           el.addEventListener('click', function (e) { postMessage(e.type); }); \
           postMessage('ready'); \
         });",
    );
    runtime
        .eval(
            "document.body.innerHTML = '<button id=x>x</button>'; \
             window.__delivered = 0; window.__errors = []; \
             var w = new Worker('worker.js'); \
             w.onmessage = function (e) { if (e.data === 'click') { __delivered += 1; } }; \
             w.onerror = function (e) { __errors.push(e.message); };",
            "page.js",
        )
        .expect("setup");
    runtime.run_worker_tasks(64).expect("setup tasks");
    runtime
        .eval(
            "var btn = document.getElementById('x'); \
             for (var i = 0; i < 400; i++) { btn.dispatchEvent(new Event('click')); }",
            "page.js",
        )
        .expect("dispatch burst");
    for _ in 0..10 {
        runtime.run_worker_tasks(64).expect("pump");
    }
    runtime.run_microtasks().expect("microtasks");
    let delivered: usize = runtime
        .eval("__delivered", "page.js")
        .unwrap()
        .parse()
        .unwrap();
    let errors = runtime.eval("__errors.join('|')", "page.js").unwrap();
    assert_eq!(
        delivered, 256,
        "inbox must cap pending DOM events at MAX_WORKER_MESSAGES (256)"
    );
    assert_eq!(errors, "", "worker must not fail with queue full: {errors}");
}

#[test]
fn worker_get_elements_by_tag_name_returns_valid_node_tokens() {
    // #2155: getElementsByTagName must register matched nodes in worker_nodes and
    // return those proxy tokens, matching getElementById so element operations work.
    let mut runtime = runtime_with_worker(
        "Promise.all([document.getElementById('a'), document.getElementsByTagName('p')]).then(function (pair) { \
           var byId = pair[0]; \
           var byTag = pair[1]; \
           Promise.all([byId.textContent, byTag[0].textContent, byTag[1].textContent, byTag[1].setAttribute('data-tag', 'ok')]).then(function (vals) { \
             postMessage(vals[0] + '|' + vals[1] + '|' + vals[2] + '|' + (byId.__megi_node_id === byTag[0].__megi_node_id)); \
           }); \
         });",
    );
    runtime
        .eval(
            "document.body.innerHTML = '<p id=a>hello</p><div>skip</div><p id=b>world</p>'; \
             window.__seen = ''; \
             var w = new Worker('worker.js'); \
             w.onmessage = function (e) { __seen = e.data; };",
            "page.js",
        )
        .expect("setup");
    runtime.run_worker_tasks(128).expect("tasks");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__seen", "page.js").expect("seen"),
        "hello|hello|world|true"
    );
    assert_eq!(
        runtime
            .eval(
                "document.getElementById('b').getAttribute('data-tag')",
                "page.js"
            )
            .expect("attr"),
        "ok"
    );
}
