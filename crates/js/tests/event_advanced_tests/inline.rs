use crate::runtime;

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
            .dispatch_event(megi_js::JsEventTarget::Window, "load", false)
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
fn inline_onclick_is_skipped_after_stop_immediate_propagation() {
    // #2297: DOM §2.9 — `stopImmediatePropagation()` must prevent every later
    // listener on the same target, including the inline `on<event>` handler.
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.order = [];
             var el = document.createElement('button');
             document.body.appendChild(el);
             el.onclick = function () { order.push('inline'); };
             el.addEventListener('click', function (event) {
                 order.push('listener');
                 event.stopImmediatePropagation();
             });
             el.dispatchEvent(new Event('click'));",
            "inline-stop-immediate.js",
        )
        .expect("inline handler dispatch");
    assert_eq!(
        runtime
            .eval("order.join(',')", "inline-stop-immediate.js")
            .expect("order"),
        "listener",
        "inline onclick must not run after stopImmediatePropagation()"
    );
}

#[test]
fn inline_onclick_still_runs_after_stop_propagation_only() {
    // #2297: DOM §2.9 — `stopPropagation()` alone stops propagation to ancestor
    // nodes but must NOT suppress listeners on the current target, including the
    // inline `on<event>` handler. Guards the stopImmediatePropagation fix from
    // overreaching into the plain propagation flag.
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.order2 = [];
             var el = document.createElement('button');
             document.body.appendChild(el);
             el.onclick = function () { order2.push('inline'); };
             el.addEventListener('click', function (event) {
                 order2.push('listener');
                 event.stopPropagation();
             });
             el.dispatchEvent(new Event('click'));",
            "inline-stop-propagation.js",
        )
        .expect("inline handler dispatch");
    assert_eq!(
        runtime
            .eval("order2.join(',')", "inline-stop-propagation.js")
            .expect("order2"),
        "listener,inline",
        "inline onclick must still run after stopPropagation() alone"
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
            .dispatch_event(megi_js::JsEventTarget::Window, "click", false)
            .expect("dispatch window click")
    );
    assert_eq!(
        runtime
            .eval("windowClicks", "window-onclick.js")
            .expect("window clicks"),
        "1"
    );
}
