use crate::runtime;

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
                megi_js::JsEventTarget::Node(input_node),
                "focus",
                false,
                false,
                &megi_js::JsUiEventKind::Focus,
            )
            .expect("focus")
    );
    assert!(
        runtime
            .dispatch_ui_event(
                megi_js::JsEventTarget::Node(input_node),
                "keydown",
                true,
                true,
                &megi_js::JsUiEventKind::Keyboard {
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
                megi_js::JsEventTarget::Node(input_node),
                "keydown",
                true,
                true,
                &megi_js::JsUiEventKind::Keyboard {
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
                megi_js::JsEventTarget::Node(input_node),
                "input",
                true,
                false,
                &megi_js::JsUiEventKind::Input {
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
                megi_js::JsEventTarget::Node(input_node),
                "blur",
                false,
                false,
                &megi_js::JsUiEventKind::Focus,
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
