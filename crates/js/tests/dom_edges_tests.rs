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

#[test]
fn insert_adjacent_rejects_unknown_positions_with_syntax_error() {
    // #653: an invalid position used to be a silent no-op that discarded the parsed nodes;
    // the DOM requires a SyntaxError before anything is detached or created.
    let mut runtime = runtime();
    runtime
        .eval(
            "var host = document.createElement('div');
             host.innerHTML = '<p>keep</p>';
             document.body.appendChild(host);
             globalThis.results = [];
             var positions = ['sideways', '', 'BEFOREEND ', 'before-end'];
             for (var i = 0; i < positions.length; i++) {
               var htmlError = null, textError = null;
               try { host.insertAdjacentHTML(positions[i], '<b>new</b>'); }
               catch (error) { htmlError = error.name + ':' + String(error); }
               try { host.insertAdjacentText(positions[i], 'new'); }
               catch (error) { textError = error.name + ':' + String(error); }
               results.push([positions[i], htmlError, textError].join('|'));
             }
             globalThis.html = host.innerHTML;
             globalThis.childCount = host.childNodes.length;",
            "positions.js",
        )
        .expect("insertAdjacent positions");
    let results = runtime
        .eval("results.join(';')", "positions.js")
        .expect("results");
    for entry in results.split(';') {
        assert!(
            entry.contains("SyntaxError:"),
            "every invalid position must throw SyntaxError: {entry}"
        );
    }
    // Nothing was detached, created or moved by the failing calls.
    assert_eq!(
        runtime.eval("childCount", "positions.js").expect("count"),
        "1",
        "a rejected position must leave the element untouched"
    );
    assert_eq!(
        runtime.eval("html", "positions.js").expect("html"),
        "<p>keep</p>"
    );
}

#[test]
fn insert_adjacent_accepts_positions_case_insensitively() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var host = document.createElement('div');
             host.innerHTML = '<i>mid</i>';
             document.body.appendChild(host);
             host.insertAdjacentHTML('BeforeEnd', '<b>end</b>');
             host.insertAdjacentText('AFTERBEGIN', 'start');
             globalThis.out = host.innerHTML;",
            "case.js",
        )
        .expect("mixed case positions");
    let out = runtime.eval("out", "case.js").expect("html");
    assert!(out.contains("end"), "beforeend must append: {out}");
    assert!(out.contains("start"), "afterbegin must prepend: {out}");
}
