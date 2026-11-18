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

#[test]
fn keyboard_input_focus_event_constructors_expose_init_fields() {
    // #1504: KeyboardEvent/InputEvent/FocusEvent constructors must exist and expose their
    // standard init fields, so scripts can build and inspect them like MouseEvent.
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var k = new KeyboardEvent('keydown', { key: 'a', code: 'KeyA', ctrlKey: true, shiftKey: true, bubbles: true });
             var i = new InputEvent('input', { data: 'x', inputType: 'insertText' });
             var f = new FocusEvent('focus', { relatedTarget: null });
             var ok = typeof KeyboardEvent === 'function'
               && typeof InputEvent === 'function'
               && typeof FocusEvent === 'function'
               && k.key === 'a' && k.code === 'KeyA' && k.ctrlKey === true && k.shiftKey === true
               && k.bubbles === true && k.__megi_event === true
               && i.data === 'x' && i.inputType === 'insertText'
               && f.type === 'focus' && f.relatedTarget === null;
             String(ok);",
            "events_ctor.js",
        )
        .expect("evaluate event constructors");
    assert_eq!(
        out, "true",
        "all three constructors must expose their init fields"
    );
}

#[test]
fn inline_input_focus_keydown_handlers_dispatch() {
    // #1504: oninput/onfocus/onblur/onkeydown/onsubmit/onchange are inline event handlers;
    // assigning them and dispatching the matching event must invoke the callback.
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "globalThis.log = [];
             var input = document.createElement('input');
             document.body.appendChild(input);
             input.oninput = function () { log.push('input'); };
             input.onfocus = function () { log.push('focus'); };
             input.onblur = function () { log.push('blur'); };
             input.onkeydown = function (e) { log.push('key:' + e.key); };
             var form = document.createElement('form');
             document.body.appendChild(form);
             form.onsubmit = function () { log.push('submit'); };
             form.onchange = function () { log.push('change'); };
             input.dispatchEvent(new Event('input'));
             input.dispatchEvent(new Event('focus'));
             input.dispatchEvent(new Event('blur'));
             input.dispatchEvent(new KeyboardEvent('keydown', { key: 'A' }));
             form.dispatchEvent(new Event('submit'));
             form.dispatchEvent(new Event('change'));
             String(log.length === 6 && log[0] === 'input' && log[1] === 'focus' && log[2] === 'blur' && log[3] === 'key:A' && log[4] === 'submit' && log[5] === 'change');",
            "inline_handlers.js",
        )
        .expect("evaluate inline handlers");
    assert_eq!(out, "true", "all inline handlers must dispatch: {out}");
}

#[test]
fn form_validity_and_submit_methods() {
    // #1505: controls expose validity/checkValidity/setCustomValidity and forms requestSubmit.
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "globalThis.submitted = 0;
             var form = document.createElement('form');
             document.body.appendChild(form);
             var input = document.createElement('input');
             input.setAttribute('required', '');
             form.appendChild(input);
             var beforeValid = input.checkValidity();
             var beforeValidity = input.validity.valueMissing;
             var r1 = form.requestSubmit(); // invalid -> blocked
             input.value = 'x';
             var afterValid = input.checkValidity();
             input.setCustomValidity('bad');
             var customInvalid = !input.checkValidity() && input.validity.customError;
             input.setCustomValidity('');
             form.addEventListener('submit', function () { submitted += 1; });
             var r2 = form.requestSubmit(); // valid -> fires submit
             String([beforeValid, beforeValidity, r1, afterValid, customInvalid, r2, submitted].join(','));",
            "validity.js",
        )
        .expect("evaluate validity");
    assert_eq!(
        out, "false,true,false,true,true,true,1",
        "validity and submit semantics: {out}"
    );
}

#[test]
fn dispatch_ui_event_builds_typed_keyboard_input_focus_events() {
    // #1504: Rust-initiated UI events construct real KeyboardEvent/InputEvent/FocusEvent instances,
    // honor instanceof, expose event-specific properties, update document.activeElement, and
    // respect preventDefault().
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.events = [];
             var form = document.createElement('form');
             var input = document.createElement('input');
             input.id = 'q';
             form.appendChild(input);
             document.body.appendChild(form);
             input.onfocus = function (e) {
                 events.push('focus:' + (e instanceof FocusEvent) + ':' + e.bubbles + ':' + (document.activeElement === input));
             };
             input.onblur = function (e) {
                 events.push('blur:' + (e instanceof FocusEvent) + ':' + e.bubbles + ':' + (document.activeElement === null));
             };
             input.onkeydown = function (e) {
                 events.push('keydown:' + (e instanceof KeyboardEvent) + ':' + e.key + ':' + e.code + ':' + e.ctrlKey + ':' + e.shiftKey);
                 if (e.key === 'Escape') { e.preventDefault(); }
             };
             input.oninput = function (e) {
                 events.push('input:' + (e instanceof InputEvent) + ':' + e.data + ':' + e.inputType);
             };
             form.addEventListener('keydown', function (e) {
                 events.push('bubble-keydown:' + e.key);
             });",
            "host_ui_events.js",
        )
        .expect("setup listeners");

    let input_node = {
        let doc = runtime.document();
        doc.descendants(doc.root())
            .find(|&id| {
                doc.node(id)
                    .as_element()
                    .and_then(|el| el.get_attribute("id"))
                    == Some("q")
            })
            .expect("input node")
    };

    runtime.set_active_element(Some(input_node));
    assert!(
        runtime
            .dispatch_ui_event(
                js::JsEventTarget::Node(input_node),
                "focus",
                false,
                false,
                &js::JsUiEventKind::Focus,
            )
            .expect("focus")
    );
    assert!(
        runtime
            .dispatch_ui_event(
                js::JsEventTarget::Node(input_node),
                "keydown",
                true,
                true,
                &js::JsUiEventKind::Keyboard {
                    key: "a".to_owned(),
                    code: "KeyA".to_owned(),
                    ctrl_key: true,
                    shift_key: false,
                    alt_key: false,
                    meta_key: false,
                    repeat: false,
                },
            )
            .expect("keydown a")
    );
    assert!(
        !runtime
            .dispatch_ui_event(
                js::JsEventTarget::Node(input_node),
                "keydown",
                true,
                true,
                &js::JsUiEventKind::Keyboard {
                    key: "Escape".to_owned(),
                    code: "Escape".to_owned(),
                    ctrl_key: false,
                    shift_key: false,
                    alt_key: false,
                    meta_key: false,
                    repeat: false,
                },
            )
            .expect("keydown Escape"),
        "preventDefault() on host KeyboardEvent must return false"
    );
    assert!(
        runtime
            .dispatch_ui_event(
                js::JsEventTarget::Node(input_node),
                "input",
                true,
                false,
                &js::JsUiEventKind::Input {
                    data: Some("a".to_owned()),
                    input_type: "insertText".to_owned(),
                },
            )
            .expect("input")
    );
    runtime.set_active_element(None);
    assert!(
        runtime
            .dispatch_ui_event(
                js::JsEventTarget::Node(input_node),
                "blur",
                false,
                false,
                &js::JsUiEventKind::Focus,
            )
            .expect("blur")
    );

    let log = runtime
        .eval("events.join('|')", "host_ui_events.js")
        .expect("read events");
    assert_eq!(
        log,
        "focus:true:false:true|keydown:true:a:KeyA:true:false|bubble-keydown:a|keydown:true:Escape:Escape:false:false|bubble-keydown:Escape|input:true:a:insertText|blur:true:false:true"
    );
}
