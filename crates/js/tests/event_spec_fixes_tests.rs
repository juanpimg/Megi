//! Regression tests for JS event/document specs (#1619, #1620, #1621).

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

#[test]
fn custom_event_is_not_cancelable_by_default() {
    // #1619: CustomEventInit.cancelable defaults to false (DOM spec).
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var e = new CustomEvent('x'); String([e.cancelable, e.bubbles].join(','));",
            "t",
        )
        .expect("CustomEvent");
    assert_eq!(out, "false,false", "custom event defaults: {out}");
}

#[test]
fn writeln_appends_a_newline() {
    // #1620: document.writeln must append a newline, unlike write.
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "document.body.innerHTML=''; document.writeln('<b>X</b>'); String(document.body.innerHTML);",
            "t",
        )
        .expect("writeln");
    assert!(out.contains('\n'), "writeln must append a newline: {out:?}");
}

#[test]
fn event_without_type_throws_type_error() {
    // #1621: new Event() with no type must throw TypeError, not coerce to "undefined".
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var threw = false; try { new Event(); } catch (e) { threw = e instanceof TypeError; } String(threw);",
            "t",
        )
        .expect("Event()");
    assert_eq!(
        out, "true",
        "Event() without a type must throw TypeError: {out}"
    );
}

#[test]
fn document_title_setter_updates_dom_title() {
    // #1645: assigning document.title must update the DOM <title> element so the window/tab
    // title (composed from the DOM) reflects it.
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "document.title = 'Updated Title'; String(document.title);",
            "t",
        )
        .expect("title setter");
    let doc = runtime.document();
    let title = doc
        .get_elements_by_tag_name("title")
        .first()
        .map(|node| doc.text_content(*node))
        .unwrap_or_default();
    assert_eq!(
        title, "Updated Title",
        "DOM <title> must reflect the setter: {title:?}"
    );
}

#[test]
fn worker_error_does_not_leak_local_paths() {
    // #1646: errors delivered to page scripts must not echo absolute file:// paths.
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var msg = ''; \
             try { new Worker('file:///tmp/sito/secreto/worker.js'); } \
             catch (e) { msg = e.message; } String(msg);",
            "t",
        )
        .expect("worker");
    assert!(
        !out.contains("/tmp") && !out.contains("secreto") && !out.contains("file:///"),
        "worker error leaked a local path: {out}"
    );
}

#[test]
fn only_one_xmlhttprequest_definition_is_installed() {
    // #1624: the PRELUDE used to define XMLHttpRequest and xhr.rs overwrote it — two installs
    // per realm with a divergence risk. Only xhr.rs must define it now.
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime.rs"),
    )
    .expect("runtime.rs source");
    assert!(
        !source.contains("define(\"XMLHttpRequest\""),
        "runtime PRELUDE must not define XMLHttpRequest (xhr.rs owns it)"
    );
}

#[test]
fn docs_no_longer_list_fetch_xhr_as_gaps() {
    // #1623: fetch/XHR are implemented; the crate docs must not call them gaps.
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
    )
    .expect("lib.rs source");
    assert!(
        !source.contains("`fetch`/XHR,"),
        "docs must not list fetch/XHR as remaining gaps"
    );
    assert!(
        source.contains("XMLHttpRequest"),
        "docs should mention XHR as implemented"
    );
}

#[test]
fn event_constructors_have_prototypes_and_instanceof_works() {
    // #1740: ECMA-262 §7.3.7 requires a `prototype` object on the RHS of instanceof.
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var e = new Event('x'); var c = new CustomEvent('y'); var m = new MessageEvent('z'); \
             String(typeof Event.prototype) + ':' + (e instanceof Event) + ':' + \
             (c instanceof CustomEvent) + ':' + (c instanceof Event) + ':' + \
             (m instanceof MessageEvent);",
            "t",
        )
        .expect("eval");
    assert_eq!(out, "object:true:true:true:true");
}

#[test]
fn debug_event_properties() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var k = new KeyboardEvent('keydown', { key: 'a', code: 'KeyA', ctrlKey: true, shiftKey: true, bubbles: true }); var i = new InputEvent('input', { data: 'x', inputType: 'insertText' }); var f = new FocusEvent('focus', { relatedTarget: null }); String(k.ctrlKey) + ':' + String(k.shiftKey) + ':' + String(k.code) + ':' + String(i.data) + ':' + String(i.inputType) + ':' + String(f.type) + ':' + String(f.relatedTarget);",
            "t",
        )
        .expect("eval");
    assert_eq!(out, "true:true:KeyA:x:insertText:focus:null");
}

#[test]
fn new_event_subclasses_keep_the_internal_event_marker() {
    // Regression after #1806: `Base.call(this, ...)` discarded the object
    // returned by the native constructor, leaving subclasses without the
    // `__megi_event` marker required by dispatchEvent's type check.
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var e = new KeyboardEvent('keydown', { key: 'A' });
             String([e.__megi_event, e.type, e.key, e instanceof Event,
                     typeof e.preventDefault].join(','))",
            "t",
        )
        .expect("keyboard event marker");
    assert_eq!(
        out, "true,keydown,A,true,function",
        "subclass lost Event state: {out}"
    );
}

#[test]
fn detached_node_events_do_not_reach_document_or_window() {
    // #1743: DOM §2.9 — the event path of a detached node ends at the node itself.
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "window.__log = ''; \
             var d = document.createElement('div'); \
             d.addEventListener('click', function () { __log += 'd,'; }); \
             document.addEventListener('click', function () { __log += 'doc,'; }); \
             window.addEventListener('click', function () { __log += 'win,'; }); \
             d.dispatchEvent(new Event('click', { bubbles: true })); \
             __log;",
            "t",
        )
        .expect("eval");
    assert_eq!(out, "d,");
}
