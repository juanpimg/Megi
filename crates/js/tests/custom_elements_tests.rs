//! #1541: custom elements registry and upgrade on createElement.

#![cfg(feature = "js")]

use js::runtime::JsRuntime;

#[test]
fn custom_element_define_get_and_upgrade() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var upgraded = false; \
             function MyWidget() { upgraded = true; this.__setup = 42; } \
             customElements.define('my-widget', MyWidget); \
             var el = document.createElement('my-widget'); \
             String(typeof customElements.get('my-widget') + '|' + upgraded + '|' + el.__setup + '|' + el.tagName.toLowerCase());",
            "t",
        )
        .expect("custom element");
    assert_eq!(out, "function|true|42|my-widget", "upgrade: {out}");
}

#[test]
fn custom_element_name_validation() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var bad = false; \
             try { customElements.define('nohyphen', function () {}); } catch (e) { bad = true; } \
             String(bad);",
            "t",
        )
        .expect("validation");
    assert_eq!(out, "true", "names without a hyphen must be rejected");
}

#[test]
fn custom_elements_compose_with_shadow_dom() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "function Card() { this._root = this.attachShadow({ mode: 'open' }); } \
             customElements.define('x-card', Card); \
             var el = document.createElement('x-card'); \
             el._root.appendChild(document.createElement('span')); \
             String(el.shadowRoot !== null && el.shadowRoot.firstChild !== null && el.getRootNode() === el.shadowRoot);",
            "t",
        )
        .expect("shadow composition");
    assert_eq!(
        out, "true",
        "custom element should own a usable shadow root"
    );
}
