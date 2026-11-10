//! Rust-facing APIs (block 0): executing scripts over a borrowed `&mut Document` and
//! dispatching DOM events from the host without going through `eval`.

#![cfg(feature = "js")]

use dom::{Document, Namespace};
use js::{Budget, JsError, JsRuntime};

fn document_with_target() -> (Document, dom::NodeId) {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let target = document.create_element(Namespace::Html, "div", vec![]);
    document.append_child(document.root(), html);
    document.append_child(html, body);
    document.append_child(body, target);
    document.append_text(target, "original");
    (document, target)
}

#[test]
fn eval_on_document_mutates_the_borrowed_document() {
    let (mut document, target) = document_with_target();
    JsRuntime::eval_on_document(
        &mut document,
        "document.getElementById('missing');
         document.body.firstChild.textContent = 'from script';
         var added = document.createElement('span');
         added.textContent = 'added';
         document.body.appendChild(added);",
        "borrowed.js",
        &Budget::SMALL,
    )
    .expect("script runs");
    assert_eq!(document.text_content(target), "from script");
    let body = document.body().expect("body");
    let text: String = document
        .children(body)
        .map(|child| document.text_content(child))
        .collect::<Vec<_>>()
        .join("|");
    assert!(text.contains("added"), "unexpected body text: {text}");
}

#[test]
fn eval_on_document_restores_the_document_on_syntax_error() {
    let (mut document, target) = document_with_target();
    let error = JsRuntime::eval_on_document(&mut document, "function (", "bad.js", &Budget::SMALL)
        .expect_err("syntax error");
    assert!(matches!(error, JsError::Syntax(_)), "{error:?}");
    // The document was written back intact.
    assert_eq!(document.text_content(target), "original");
    assert_eq!(document.children(document.root()).count(), 1);
}

#[test]
fn eval_on_document_enforces_the_budget() {
    let (mut document, _) = document_with_target();
    let budget = Budget {
        loop_iterations: 5_000,
        ..Budget::SMALL
    };
    let error = JsRuntime::eval_on_document(&mut document, "while (true) {}", "loop.js", &budget)
        .expect_err("budget");
    assert!(matches!(error, JsError::Budget(_)), "{error:?}");
}

#[test]
fn take_document_round_trips_the_arena() {
    let (document, target) = document_with_target();
    let mut runtime =
        JsRuntime::with_document(document, "about:blank".to_string()).expect("runtime");
    runtime
        .eval("document.body.firstChild.textContent = 'mutated';", "rt.js")
        .expect("eval");
    let document = runtime.take_document();
    assert_eq!(document.text_content(target), "mutated");
}

#[test]
fn dispatch_simple_event_reaches_node_listeners() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "globalThis.clicks = 0;
             var button = document.createElement('button');
             button.id = 'button';
             document.body.appendChild(button);
             button.addEventListener('click', function () { globalThis.clicks += 1; });",
            "dispatch.js",
        )
        .expect("setup");
    let button = runtime
        .document()
        .get_element_by_id("button")
        .expect("button node");
    assert!(
        runtime
            .dispatch_simple_event(Some(button), "click", true)
            .expect("dispatch")
    );
    assert_eq!(runtime.eval("clicks", "dispatch.js").expect("clicks"), "1");
}

#[test]
fn dispatch_simple_event_bubbles_to_ancestors() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "globalThis.path = [];
             var parent = document.createElement('div');
             var child = document.createElement('span');
             parent.id = 'parent';
             child.id = 'child';
             document.body.appendChild(parent);
             parent.appendChild(child);
             parent.addEventListener('ping', function () { globalThis.path.push('parent'); });
             child.addEventListener('ping', function () { globalThis.path.push('child'); });",
            "bubble.js",
        )
        .expect("setup");
    let child = runtime
        .document()
        .get_element_by_id("child")
        .expect("child node");
    runtime
        .dispatch_simple_event(Some(child), "ping", true)
        .expect("dispatch");
    assert_eq!(
        runtime.eval("path.join(',')", "bubble.js").expect("path"),
        "child,parent"
    );
}

#[test]
fn dispatch_simple_event_to_window_and_prevented_default() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "globalThis.loaded = false;
             window.addEventListener('DOMContentLoaded', function () { globalThis.loaded = true; });
             window.addEventListener('blocked', function (event) { event.preventDefault(); });",
            "window.js",
        )
        .expect("setup");
    assert!(
        runtime
            .dispatch_simple_event(None, "DOMContentLoaded", false)
            .expect("dispatch window")
    );
    assert_eq!(runtime.eval("loaded", "window.js").expect("loaded"), "true");
    let not_prevented = runtime
        .dispatch_simple_event(None, "blocked", false)
        .expect("prevented dispatch");
    assert!(!not_prevented, "preventDefault must be reported");
}

#[test]
fn document_is_an_event_target_in_js() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "globalThis.seen = [];
             document.addEventListener('DOMContentLoaded', function () { globalThis.seen.push('document'); });
             window.addEventListener('DOMContentLoaded', function () { globalThis.seen.push('window'); });
             document.dispatchEvent(new Event('DOMContentLoaded', { bubbles: true }));",
            "document-events.js",
        )
        .expect("document.dispatchEvent");
    assert_eq!(
        runtime
            .eval("seen.join(',')", "document-events.js")
            .expect("seen"),
        "document,window"
    );
}

#[test]
fn document_event_target_remove_and_prevent_default() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "globalThis.hits = 0;
             function onLoaded() { globalThis.hits += 1; }
             document.addEventListener('DOMContentLoaded', onLoaded);
             document.dispatchEvent(new Event('DOMContentLoaded'));
             document.removeEventListener('DOMContentLoaded', onLoaded);
             document.dispatchEvent(new Event('DOMContentLoaded'));
             globalThis.prevented = -1;
             document.addEventListener('blocked', function (event) {
                 event.preventDefault();
             });
             globalThis.prevented = document.dispatchEvent(new Event('blocked'));",
            "document-remove.js",
        )
        .expect("document events");
    assert_eq!(
        runtime.eval("hits", "document-remove.js").expect("hits"),
        "1"
    );
    assert_eq!(
        runtime
            .eval("prevented", "document-remove.js")
            .expect("prevented"),
        "false"
    );
}

#[test]
fn rust_can_dispatch_directly_to_the_document() {
    use js::JsEventTarget;
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "globalThis.log = [];
             document.addEventListener('DOMContentLoaded', function () { globalThis.log.push('document'); });
             window.addEventListener('DOMContentLoaded', function () { globalThis.log.push('window'); });",
            "document-rust.js",
        )
        .expect("setup");
    assert!(
        runtime
            .dispatch_event(JsEventTarget::Document, "DOMContentLoaded", true)
            .expect("dispatch to document")
    );
    assert_eq!(
        runtime
            .eval("log.join(',')", "document-rust.js")
            .expect("log"),
        "document,window"
    );
    // `None` targets window: only window listeners run (document listeners fire in the
    // capture phase only for capture=true listeners, and the document bubble leg is not
    // part of a window-target dispatch).
    assert!(
        runtime
            .dispatch_simple_event(None, "DOMContentLoaded", true)
            .expect("dispatch to window")
    );
    assert_eq!(
        runtime
            .eval("log.join(',')", "document-rust.js")
            .expect("log"),
        "document,window,window"
    );
}
