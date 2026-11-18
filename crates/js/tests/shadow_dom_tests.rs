//! #1541: lightweight open Shadow DOM — `attachShadow`, `shadowRoot`, `getRootNode`.

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

fn runtime() -> JsRuntime {
    JsRuntime::new().expect("runtime")
}

#[test]
fn attach_shadow_creates_a_root_and_shadow_root_getter() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var host = document.createElement('div'); \
             document.body.appendChild(host); \
             var before = host.shadowRoot; \
             var root = host.attachShadow({ mode: 'open' }); \
             String(before === null && root !== null && root === host.shadowRoot \
                    && root.nodeType === 11 && host.attachShadow({ mode: 'open' }) === root);",
            "t",
        )
        .expect("attachShadow");
    assert_eq!(out, "true");
}

#[test]
fn shadow_root_children_and_get_root_node() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var host = document.createElement('div'); \
             document.body.appendChild(host); \
             var root = host.attachShadow({ mode: 'open' }); \
             var child = document.createElement('span'); \
             root.appendChild(child); \
             String(root.firstChild === child && child.getRootNode() === root \
                    && host.getRootNode() === root && root.querySelector('span') === child);",
            "t",
        )
        .expect("shadow children");
    assert_eq!(out, "true");
}

#[test]
fn element_without_shadow_has_null_shadow_root() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var el = document.createElement('p'); String(el.shadowRoot === null);",
            "t",
        )
        .expect("no shadow");
    assert_eq!(out, "true");
}
