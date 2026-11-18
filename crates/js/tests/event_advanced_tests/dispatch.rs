use megi_js::JsError;

use crate::runtime;

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
fn budgets_stop_loops_inside_listeners_and_report_to_console_without_propagating_in_boa() {
    let mut runtime = runtime();
    let result = runtime.eval(
        "var el = document.createElement('div');
         el.addEventListener('spin', function () { while (true) {} });
         el.dispatchEvent(new Event('spin'));",
        "budget-listener.js",
    );
    // Boa 0.22 reports the listener budget error to the console instead of propagating
    // it out of dispatchEvent, so eval succeeds but the console records the budget error.
    assert!(
        result.is_ok(),
        "Boa 0.22 reports listener budget errors to the console instead of propagating them: {result:?}"
    );
    let console = runtime.take_console();
    assert!(
        console.iter().any(|entry| {
            entry.level == "error"
                && (entry.message.to_ascii_lowercase().contains("runtimelimit")
                    || entry
                        .message
                        .to_ascii_lowercase()
                        .contains("iteration loops"))
        }),
        "budget must stop the listener and be reported to console: {console:?}"
    );
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
