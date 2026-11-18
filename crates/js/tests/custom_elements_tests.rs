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

#[test]
fn custom_element_class_extends_html_element_and_lifecycle_callbacks() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var log = []; \
             class XCounter extends HTMLElement { \
               static get observedAttributes() { return ['tone', 'count']; } \
               constructor() { \
                 super(); \
                 log.push('ctor'); \
                 this.attachShadow({ mode: 'open' }); \
               } \
               connectedCallback() { log.push('connected:' + this.isConnected); } \
               disconnectedCallback() { log.push('disconnected:' + this.isConnected); } \
               attributeChangedCallback(name, oldVal, newVal) { \
                 log.push('attr:' + name + ':' + oldVal + '->' + newVal); \
               } \
             } \
             customElements.define('x-counter', XCounter); \
             var pre = document.createElement('div'); \
             var el = document.createElement('x-counter'); \
             el.setAttribute('tone', 'warm'); \
             document.body.appendChild(el); \
             el.setAttribute('tone', 'cool'); \
             el.removeAttribute('tone'); \
             document.body.removeChild(el); \
             String(log.join(','));",
            "t",
        )
        .expect("lifecycle");
    assert_eq!(
        out,
        "ctor,attr:tone:null->warm,connected:true,attr:tone:warm->cool,attr:tone:cool->null,disconnected:false",
        "lifecycle log: {out}"
    );
}

#[test]
fn custom_elements_retroactive_upgrade_and_when_defined() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "document.body.innerHTML = '<x-banner label=\"hi\"></x-banner>'; \
             var preEl = document.body.querySelector('x-banner'); \
             var events = []; \
             customElements.whenDefined('x-banner').then(function (ctor) { \
               events.push('defined:' + (ctor === XBanner)); \
             }); \
             class XBanner extends HTMLElement { \
               static observedAttributes = ['label']; \
               constructor() { super(); events.push('ctor'); } \
               attributeChangedCallback(n, o, v) { events.push('attr:' + n + '=' + v); } \
               connectedCallback() { events.push('conn:' + this.getAttribute('label')); } \
             } \
             customElements.define('x-banner', XBanner); \
             var dup = false; \
             try { customElements.define('x-banner', XBanner); } catch (e) { dup = true; }",
            "t",
        )
        .expect("upgrade + whenDefined setup");
    runtime.run_microtasks().expect("microtasks");
    let out = runtime
        .eval(
            "String(events.join('|') + '|dup=' + dup + '|proto=' + (preEl instanceof XBanner));",
            "t2",
        )
        .expect("upgrade + whenDefined");
    assert_eq!(
        out, "ctor|attr:label=hi|conn:hi|defined:true|dup=true|proto=true",
        "events: {out}"
    );
}

#[test]
fn custom_elements_upgrade_explicit_detached_subtree() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var wrapper = document.createElement('div'); \
             var late = document.createElement('x-late'); \
             wrapper.appendChild(late); \
             var upgraded = 0; \
             function XLate() { upgraded++; } \
             customElements.define('x-late', XLate); \
             customElements.upgrade(wrapper); \
             String(upgraded);",
            "t",
        )
        .expect("explicit upgrade");
    assert_eq!(out, "1");
}
