#![cfg(feature = "js")]

//! jQuery 3.5.1 compatibility (#1880).

use megi_js::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::default());
    let _ = runtime.eval("document.body.innerHTML='';", "setup.js");
    runtime
}

/// jQuery's support-detection calls `createElement('fieldset')` on `document`.
#[test]
fn jquery_style_create_element_fieldset() {
    let mut runtime = runtime();
    let out = runtime.eval(
        "var d=document; var t=d.createElement('fieldset'); typeof t;",
        "jquery.js",
    );
    assert!(
        out.is_ok(),
        "createElement(fieldset) failed: {:?}",
        out.err()
    );
    assert_eq!(out.unwrap(), "object");
}

/// jQuery's support-detection compares `compareDocumentPosition` on a created div.
#[test]
fn jquery_style_compare_document_position() {
    let mut runtime = runtime();
    let out = runtime.eval(
        "var d=document; var a=d.createElement('div'), b=d.createElement('div'); \
         d.body.appendChild(a); d.body.appendChild(b); \
         a.compareDocumentPosition(b).toString();",
        "jquery.js",
    );
    assert!(
        out.is_ok(),
        "compareDocumentPosition failed: {:?}",
        out.err()
    );
}

/// Full jQuery 3.5.1 must initialize without throwing (#1880).
#[test]
fn jquery_351_initializes() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let mut runtime = runtime();
            let source = include_str!("fixtures/jquery-3.5.1.min.js");
            let out = runtime.eval(source, "jquery-3.5.1.min.js");
            assert!(out.is_ok(), "jQuery 3.5.1 failed: {:?}", out.err());
            let has_jquery = runtime.eval("typeof jQuery", "check.js").unwrap();
            assert_eq!(has_jquery, "function");
        })
        .unwrap()
        .join()
        .unwrap();
}

/// jQuery's support.sortDetached: compareDocumentPosition with a fieldset argument.
#[test]
fn jquery_sort_detached_pattern() {
    let mut runtime = runtime();
    let out = runtime.eval(
        "var d=document; var el=d.createElement('div'); d.body.appendChild(el); \
         (el.compareDocumentPosition(d.createElement('fieldset')) & 1).toString();",
        "jquery.js",
    );
    assert!(out.is_ok(), "sortDetached pattern failed: {:?}", out.err());
}

/// jQuery captures `slice.call(document.childNodes)` during Sizzle init (#1880).
#[test]
fn document_child_nodes_is_array_like() {
    let mut runtime = runtime();
    let out = runtime.eval(
        "var cn = document.childNodes; [typeof cn, cn && cn.length, \
         Array.prototype.slice.call(cn).length].join('|');",
        "jquery.js",
    );
    assert!(out.is_ok(), "childNodes access failed: {:?}", out.err());
    assert_eq!(out.unwrap(), "object|1|1");
}

/// jQuery's UMD wrapper reads `window.document` (not the bare `document` global).
#[test]
fn window_document_is_the_document() {
    let mut runtime = runtime();
    let out = runtime.eval(
        "[typeof window.document, window.document === document].join('|')",
        "t",
    );
    assert!(out.is_ok(), "{:?}", out.err());
    assert_eq!(out.unwrap(), "object|true");
}

/// jQuery's setDocument requires `document.nodeType === 9` and a documentElement.
#[test]
fn document_node_type_and_document_element() {
    let mut runtime = runtime();
    let out = runtime.eval(
        "[document.nodeType, document.documentElement && document.documentElement.tagName].join('|')",
        "t",
    );
    assert!(out.is_ok(), "{:?}", out.err());
    assert_eq!(out.unwrap(), "9|HTML");
}

/// jQuery's support detection calls document.implementation.createHTMLDocument().
#[test]
fn document_implementation_exists() {
    let mut runtime = runtime();
    let out = runtime.eval(
        "[typeof document.implementation, document.implementation && typeof document.implementation.createHTMLDocument].join('|')",
        "t",
    );
    assert!(out.is_ok(), "{:?}", out.err());
    assert_eq!(out.unwrap(), "object|function");
}
