#![cfg(feature = "js")]

//! `normalize()` must keep removed text nodes usable for existing JS references.

use js::{Budget, JsRuntime};

#[test]
fn normalize_keeps_js_handles_usable() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
        .eval(
            "var paragraph = document.createElement('p');
             var first = document.createTextNode('a');
             var second = document.createTextNode('b');
             paragraph.appendChild(first);
             paragraph.appendChild(second);
             paragraph.normalize();
             globalThis.mergedText = second.textContent;
             globalThis.mergedParent = second.parentNode === null;
             globalThis.paragraphText = paragraph.textContent;",
            "normalize.js",
        )
        .expect("normalize");
    assert_eq!(
        runtime
            .eval("mergedText", "normalize.js")
            .expect("merged text"),
        "b",
        "the merged text node keeps its data"
    );
    assert_eq!(
        runtime
            .eval("mergedParent", "normalize.js")
            .expect("parent"),
        "true"
    );
    assert_eq!(
        runtime
            .eval("paragraphText", "normalize.js")
            .expect("paragraph text"),
        "ab"
    );
}
