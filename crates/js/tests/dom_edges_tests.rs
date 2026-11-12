#![cfg(feature = "js")]

//! DOM edge cases surfaced by the audit: `createElement` casing and
//! `insertAdjacent*` on non-element nodes.

use js::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

#[test]
fn create_element_lowercases_html_names() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var template = document.createElement('TEMPLATE');
             template.innerHTML = '<p>x</p>';
             globalThis.templateChildren = template.childNodes.length;",
            "create.js",
        )
        .expect("createElement");
    // Lowercase `template` gets its inert content fragment, so the element itself has no
    // children (an uppercase name would have kept the parsed <p> as a direct child).
    assert_eq!(
        runtime
            .eval("templateChildren", "create.js")
            .expect("children"),
        "0"
    );
}

#[test]
fn insert_adjacent_is_element_only() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.textError = null;
             var text = document.createTextNode('x');
             try { text.insertAdjacentHTML('beforeend', '<p>y</p>'); }
             catch (error) { textError = String(error); }
             globalThis.textChildren = text.childNodes.length;
             globalThis.htmlError = null;
             try { text.insertAdjacentText('beforeend', 'y'); }
             catch (error) { htmlError = String(error); }",
            "insert.js",
        )
        .expect("insertAdjacent");
    assert_eq!(
        runtime
            .eval("textError !== null", "insert.js")
            .expect("html error"),
        "true"
    );
    assert_eq!(
        runtime
            .eval("htmlError !== null", "insert.js")
            .expect("text error"),
        "true"
    );
    assert_eq!(
        runtime.eval("textChildren", "insert.js").expect("children"),
        "0"
    );
}
