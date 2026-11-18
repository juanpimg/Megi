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
