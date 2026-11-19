//! #1939: `document.createElementNS` for SVG/MathML element creation.

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

#[test]
fn create_element_ns_exposes_namespace_local_name_and_prefix() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg'); \
             var circle = document.createElementNS('http://www.w3.org/2000/svg', 'svg:circle'); \
             var div = document.createElementNS(null, 'div'); \
             [svg.namespaceURI, svg.localName, svg.tagName, \
              circle.prefix, circle.localName, \
              String(div.namespaceURI), div.localName].join('|');",
            "ns.js",
        )
        .expect("eval");
    assert_eq!(
        out,
        "http://www.w3.org/2000/svg|svg|SVG|svg|circle|null|div"
    );
}
