//! #1541: custom elements registry and upgrade on createElement.

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

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
fn registry_define_checks_constructor_before_name() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var name = ''; \
             try { customElements.define('badname', 1); } catch (e) { name = e.name; } \
             String(name);",
            "t",
        )
        .expect("validation order");
    assert_eq!(out, "TypeError", "IsConstructor precedes valid name: {out}");
}

#[test]
fn registry_define_reads_observed_attributes_from_constructor() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var calls = []; \
             function Observed() {} \
             Observed.prototype.connectedCallback = function () {}; \
             Observed.prototype.attributeChangedCallback = function () {}; \
             Observed.observedAttributes = ['data-a']; \
             Observed.prototype = new Proxy(Observed.prototype, { \
               get: function (target, key) { calls.push('p:' + String(key)); return target[key]; } \
             }); \
             var constructor = new Proxy(Observed, { \
               get: function (target, key) { calls.push('c:' + String(key)); return target[key]; } \
             }); \
             customElements.define('x-observed', constructor); \
             String(calls.join(','));",
            "t",
        )
        .expect("callback access order");
    assert_eq!(
        out,
        "c:prototype,p:connectedCallback,p:disconnectedCallback,p:adoptedCallback,p:attributeChangedCallback,c:observedAttributes",
        "WHATWG Custom Elements §4.2 callback/observedAttributes access: {out}"
    );
}

#[test]
fn registry_define_rejects_nested_definition() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var calls = []; \
             var Inner = class extends HTMLElement {}; \
             var Outer = new Proxy(class extends HTMLElement {}, { \
               get: function (target, key) { \
                 calls.push(String(key)); \
                 if (key === 'prototype') { \
                   try { customElements.define('x-inner', Inner); } catch (e) { calls.push(e.name); } \
                 } \
                 return target[key]; \
               } \
             }); \
             customElements.define('x-outer', Outer); \
             String(calls.join(','));",
            "t",
        )
        .expect("definition running flag");
    assert_eq!(
        out, "prototype,NotSupportedError",
        "WHATWG Custom Elements §4.2 definition running flag: {out}"
    );
}

#[test]
fn define_upgrades_existing_elements_in_shadow_including_tree_order() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var before = document.createElement('x-order'); \
             var after = document.createElement('x-order'); \
             var inShadow = document.createElement('x-order'); \
             var light = document.createElement('x-order'); \
             var host = document.createElement('div'); \
             var container = document.createElement('div'); \
             container.appendChild(before); \
             container.appendChild(host); \
             container.appendChild(after); \
             host.appendChild(light); \
             var root = host.attachShadow({mode: 'closed'}); \
             root.appendChild(inShadow); \
             document.body.appendChild(container); \
             var calls = []; \
             class XOrder extends HTMLElement { constructor() { super(); calls.push(this); } } \
             customElements.define('x-order', XOrder); \
             String(calls.map(function (node) { \
               if (node === before) return 'before'; \
               if (node === after) return 'after'; \
               if (node === inShadow) return 'inShadow'; \
               if (node === light) return 'light'; \
               return 'other'; \
             }).join(','));",
            "t",
        )
        .expect("upgrade order");
    assert_eq!(
        out, "before,inShadow,light,after",
        "WHATWG Custom Elements §4.2: shadow-including tree order: {out}"
    );
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
