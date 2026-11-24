//! #2245: Bootstrap's `Element.prototype.querySelectorAll.call(...)` must work.

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

#[test]
fn element_prototype_exposes_query_selector_all() {
    let mut rt = JsRuntime::new().expect("runtime");
    let out = rt.eval("typeof Element.prototype.querySelectorAll", "t.js");
    assert!(out.is_ok(), "{:?}", out.err());
    assert_eq!(out.unwrap(), "function");
}

#[test]
fn element_prototype_query_selector_all_call_works() {
    let mut rt = JsRuntime::new().expect("runtime");
    rt.eval(
        "document.body.innerHTML='<p class=x>a</p><p>b</p>'; \
         globalThis.__n = Element.prototype.querySelectorAll.call(document.body, 'p.x').length;",
        "t.js",
    )
    .expect("setup + call");
    let n = rt.eval("globalThis.__n", "t.js").expect("read");
    assert_eq!(n, "1");
}
