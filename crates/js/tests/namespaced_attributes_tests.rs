//! #2317: the JS DOM bridge must expose namespaced attributes parsed from SVG markup.

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

#[test]
fn svg_xlink_href_is_visible_through_the_dom_bridge() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "document.body.innerHTML = '<svg><use xlink:href=\"#icon\"></use></svg>'; \
             var use = document.querySelector('use'); \
             var got = use.getAttribute('xlink:href'); \
             var has = use.hasAttribute('xlink:href'); \
             var removed = use.removeAttribute('xlink:href'); \
             [got, has, removed, use.hasAttribute('xlink:href')].join('|');",
            "ns-attrs.js",
        )
        .expect("eval");
    // removeAttribute returns undefined in this engine (its JS return is not the old
    // value); what matters is that the namespaced attribute stops being visible.
    assert_eq!(out, "#icon|true||false");
}
