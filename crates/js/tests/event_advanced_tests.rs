//! Advanced event tests (task-80): real `currentTarget`, `once`/`passive`,
//! inline `onclick`/`onload`/`onerror`, `document.close`, memory estimate and
//! dispatch robustness.

#![cfg(feature = "js")]

use js::{Budget, JsError, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

#[test]
fn current_target_tracks_capture_and_bubble_visits() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.log = [];
             var parent = document.createElement('div');
             var child = document.createElement('span');
             parent.id = 'parent';
             child.id = 'child';
             document.body.appendChild(parent);
             parent.appendChild(child);
             parent.addEventListener('ping', function (event) {
                 log.push('parent:' + event.eventPhase + ':' +
                     (event.currentTarget === parent) + ':' +
                     (event.target === child) + ':' + (this === parent));
             }, true);
             parent.addEventListener('ping', function (event) {
                 log.push('parent-bubble:' + (event.currentTarget === parent));
             });
             child.addEventListener('ping', function (event) {
                 log.push('child:' + (event.currentTarget === child) + ':' +
                     (event.target === child) + ':' + (this === child));
             });
             var event = new Event('ping', { bubbles: true });
             child.dispatchEvent(event);
             globalThis.afterTarget = event.currentTarget;
             globalThis.afterPhase = event.eventPhase;",
            "currenttarget.js",
        )
        .expect("dispatch");
    let log = runtime
        .eval("log.join(',')", "currenttarget.js")
        .expect("log");
    assert!(log.contains("child:true:true:true"), "{log}");
    assert!(log.contains("parent:1:true:true:true"), "{log}");
    assert!(log.contains("parent-bubble:true"), "{log}");
    assert_eq!(
        runtime
            .eval("afterTarget === null", "currenttarget.js")
            .expect("after target"),
        "true"
    );
    assert_eq!(
        runtime
            .eval("afterPhase", "currenttarget.js")
            .expect("phase"),
        "0"
    );
}

#[test]
fn once_listener_is_removed_after_first_dispatch() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.hits = 0;
             var el = document.createElement('div');
             document.body.appendChild(el);
             el.addEventListener('tick', function () { hits += 1; }, { once: true });
             el.dispatchEvent(new Event('tick'));
             el.dispatchEvent(new Event('tick'));
             el.dispatchEvent(new Event('tick'));",
            "once.js",
        )
        .expect("once");
    assert_eq!(runtime.eval("hits", "once.js").expect("hits"), "1");
}

#[test]
fn passive_listener_cannot_prevent_default() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var el = document.createElement('div');
             document.body.appendChild(el);
             el.addEventListener('scroll', function (event) { event.preventDefault(); }, { passive: true });
             globalThis.passiveResult = el.dispatchEvent(new Event('scroll', { cancelable: true }));
             globalThis.normalResult = (function () {
                 var other = document.createElement('div');
                 other.addEventListener('scroll', function (event) { event.preventDefault(); });
                 return other.dispatchEvent(new Event('scroll', { cancelable: true }));
             })();",
            "passive.js",
        )
        .expect("passive");
    assert_eq!(
        runtime
            .eval("passiveResult", "passive.js")
            .expect("passive result"),
        "true",
        "passive listeners must not cancel the default"
    );
    assert_eq!(
        runtime
            .eval("normalResult", "passive.js")
            .expect("normal result"),
        "false"
    );
}

#[test]
fn inline_onclick_on_element_and_document() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.clicks = 0;
             var el = document.createElement('button');
             el.id = 'btn';
             document.body.appendChild(el);
             el.onclick = function (event) { clicks += 1; globalThis.thisIsEl = (this === el); };
             globalThis.getterType = typeof el.onclick;
             el.dispatchEvent(new Event('click'));
             el.dispatchEvent(new Event('click'));
             document.onclick = function () { clicks += 10; };
             document.dispatchEvent(new Event('click'));
             el.onclick = null;
             globalThis.removedGetter = el.onclick;
             el.dispatchEvent(new Event('click'));",
            "inline.js",
        )
        .expect("inline handlers");
    assert_eq!(runtime.eval("clicks", "inline.js").expect("clicks"), "12");
    assert_eq!(
        runtime
            .eval("getterType", "inline.js")
            .expect("getter typeof"),
        "function"
    );
    assert_eq!(runtime.eval("thisIsEl", "inline.js").expect("this"), "true");
    assert_eq!(
        runtime
            .eval("removedGetter === null", "inline.js")
            .expect("removed"),
        "true"
    );
}

#[test]
fn inline_onload_on_window() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.loaded = 0;
             window.onload = function () { loaded += 1; };",
            "onload.js",
        )
        .expect("window onload setter");
    assert!(
        runtime
            .dispatch_event(js::JsEventTarget::Window, "load", false)
            .expect("dispatch load")
    );
    assert_eq!(runtime.eval("loaded", "onload.js").expect("loaded"), "1");
}

#[test]
fn document_close_is_a_noop_and_document_element_is_coherent() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.closeResult = document.close();
             globalThis.htmlTag = document.documentElement.tagName;
             globalThis.bodyTag = document.body.tagName;
             globalThis.bodyParentIsHtml = document.body.parentNode === document.documentElement;
             globalThis.documentElementParent = document.documentElement.parentNode;",
            "close.js",
        )
        .expect("document.close");
    assert_eq!(
        runtime.eval("closeResult", "close.js").expect("close"),
        "undefined"
    );
    assert_eq!(runtime.eval("htmlTag", "close.js").expect("html"), "HTML");
    assert_eq!(runtime.eval("bodyTag", "close.js").expect("body"), "BODY");
    assert_eq!(
        runtime
            .eval("bodyParentIsHtml", "close.js")
            .expect("coherence"),
        "true"
    );
    assert_eq!(
        runtime
            .eval("documentElementParent !== undefined", "close.js")
            .expect("root parent"),
        "true"
    );
}

#[test]
fn estimated_heap_bytes_grows_with_dom_and_listeners() {
    let mut runtime = runtime();
    let before = runtime.estimated_heap_bytes().expect("estimate");
    assert!(before > 0);
    runtime
        .eval(
            "for (var i = 0; i < 50; i++) {
                 var el = document.createElement('div');
                 el.addEventListener('x', function () {});
                 document.body.appendChild(el);
             }",
            "heap.js",
        )
        .expect("allocate");
    let after = runtime.estimated_heap_bytes().expect("estimate");
    assert!(after > before, "estimate should grow: {before} -> {after}");
}

#[test]
fn dispatch_rejects_non_events() {
    let mut runtime = runtime();
    let error = runtime
        .eval(
            "var el = document.createElement('div');
             el.dispatchEvent({});",
            "non-event.js",
        )
        .expect_err("plain object must be rejected");
    match error {
        JsError::Runtime(message) => assert!(
            message.contains("TypeError") && message.contains("Event"),
            "{message}"
        ),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn throwing_listener_does_not_interrupt_the_rest() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.ran = [];
             var el = document.createElement('div');
             document.body.appendChild(el);
             el.addEventListener('boom', function () { ran.push('first'); throw new Error('listener boom'); });
             el.addEventListener('boom', function () { ran.push('second'); });",
            "throws.js",
        )
        .expect("setup");
    runtime
        .eval("el.dispatchEvent(new Event('boom'))", "throws.js")
        .expect("dispatch continues");
    assert_eq!(
        runtime.eval("ran.join(',')", "throws.js").expect("ran"),
        "first,second"
    );
    let last_error = runtime.last_event_error().expect("recorded error");
    assert!(last_error.contains("listener boom"), "{last_error}");
    let console = runtime.take_console();
    assert!(
        console
            .iter()
            .any(|entry| entry.level == "error" && entry.message.contains("listener boom")),
        "listener exception must be reported to console"
    );
}

#[test]
fn budgets_still_stop_loops_inside_listeners() {
    let mut runtime = runtime();
    let error = runtime
        .eval(
            "var el = document.createElement('div');
             el.addEventListener('spin', function () { while (true) {} });
             el.dispatchEvent(new Event('spin'));",
            "budget-listener.js",
        )
        .expect_err("budget must stop the listener");
    assert!(matches!(error, JsError::Budget(_)), "{error:?}");
}

#[test]
fn remove_listener_matches_callback_identity() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.ran = [];
             var el = document.createElement('div');
             document.body.appendChild(el);
             function first() { ran.push('first'); }
             function second() { ran.push('second'); }
             el.addEventListener('tick', first);
             el.addEventListener('tick', second);
             el.removeEventListener('tick', first);
             el.dispatchEvent(new Event('tick'));",
            "identity-remove.js",
        )
        .expect("remove by identity");
    assert_eq!(
        runtime
            .eval("ran.join(',')", "identity-remove.js")
            .expect("ran"),
        "second"
    );
}

#[test]
fn inline_onclick_on_window() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.windowClicks = 0;
             window.onclick = function () { windowClicks += 1; };",
            "window-onclick.js",
        )
        .expect("window onclick setter");
    assert!(
        runtime
            .dispatch_event(js::JsEventTarget::Window, "click", false)
            .expect("dispatch window click")
    );
    assert_eq!(
        runtime
            .eval("windowClicks", "window-onclick.js")
            .expect("window clicks"),
        "1"
    );
}

#[test]
fn duplicate_listener_registration_is_a_no_op() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.count = 0;
             var el = document.createElement('div');
             document.body.appendChild(el);
             var cb = function () { count += 1; };
             el.addEventListener('dup', cb);
             el.addEventListener('dup', cb);
             el.dispatchEvent(new Event('dup'));
             el.addEventListener('dup', function () { count += 10; });
             el.dispatchEvent(new Event('dup'));",
            "dup.js",
        )
        .expect("duplicate listeners");
    // First dispatch: one deduplicated callback (+1). Second: same callback plus the
    // distinct one (+1 +10).
    assert_eq!(runtime.eval("count", "dup.js").expect("count"), "12");
}

#[test]
fn listener_removed_during_dispatch_does_not_run() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.hits = 0;
             var el = document.createElement('div');
             document.body.appendChild(el);
             var second = function () { hits += 100; };
             el.addEventListener('rm', function () { el.removeEventListener('rm', second); });
             el.addEventListener('rm', second);
             el.dispatchEvent(new Event('rm'));",
            "rm.js",
        )
        .expect("removal during dispatch");
    assert_eq!(runtime.eval("hits", "rm.js").expect("hits"), "0");
}

#[test]
fn at_target_runs_capture_then_bubble_listeners_in_phase_two() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.phases = [];
             var el = document.createElement('div');
             document.body.appendChild(el);
             el.addEventListener('phase', function (event) { phases.push(event.eventPhase); }, true);
             el.addEventListener('phase', function (event) { phases.push(event.eventPhase); });
             el.dispatchEvent(new Event('phase', { bubbles: true }));

             globalThis.hits = [];
             var inner = document.createElement('span');
             document.body.appendChild(inner);
             inner.addEventListener('stop', function (event) {
                 event.stopPropagation();
                 hits.push('cap');
             }, true);
             inner.addEventListener('stop', function () { hits.push('bubble'); });
             document.body.addEventListener('stop', function () { hits.push('ancestor'); }, true);
             inner.dispatchEvent(new Event('stop', { bubbles: true }));",
            "attarget.js",
        )
        .expect("at-target phases");
    assert_eq!(
        runtime
            .eval("phases.join(',')", "attarget.js")
            .expect("phases"),
        "2,2"
    );
    assert_eq!(
        runtime.eval("hits.join(',')", "attarget.js").expect("hits"),
        "ancestor,cap,bubble",
        "stopPropagation at the target does not skip the target's own bubble listeners"
    );
}

#[test]
fn window_and_document_targets_use_the_at_target_phase() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.windowPhases = [];
             window.addEventListener('w', function (event) { windowPhases.push(event.eventPhase); }, true);
             window.addEventListener('w', function (event) { windowPhases.push(event.eventPhase); });
             window.dispatchEvent(new Event('w'));

             globalThis.documentPhases = [];
             document.addEventListener('d', function (event) { documentPhases.push(event.eventPhase); }, true);
             document.addEventListener('d', function (event) { documentPhases.push(event.eventPhase); });
             document.dispatchEvent(new Event('d'));",
            "targets.js",
        )
        .expect("window/document targets");
    assert_eq!(
        runtime
            .eval("windowPhases.join(',')", "targets.js")
            .expect("window phases"),
        "2,2"
    );
    assert_eq!(
        runtime
            .eval("documentPhases.join(',')", "targets.js")
            .expect("document phases"),
        "2,2"
    );
}
