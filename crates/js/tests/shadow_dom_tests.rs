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
             var second = null; \
             try { host.attachShadow({ mode: 'open' }); } catch (e) { second = e.name; } \
             String(before === null && root !== null && root === host.shadowRoot \
                    && root.nodeType === 11 && second === 'InvalidStateError');",
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

#[test]
fn closed_shadow_root_is_hidden_but_exposes_its_host() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var host = document.createElement('div'); \
             var root = host.attachShadow({ mode: 'closed' }); \
             String(host.shadowRoot === null && root.host === host && \
                    root instanceof ShadowRoot && \
                    ShadowRoot.prototype.__proto__ === DocumentFragment.prototype);",
            "t",
        )
        .expect("closed shadow root");
    assert_eq!(out, "true");
}

#[test]
fn assigned_slot_tracks_light_children_and_closed_trees() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var host = document.createElement('div'); \
             document.body.appendChild(host); \
             var root = host.attachShadow({mode: 'open'}); \
             var slot = document.createElement('slot'); \
             slot.setAttribute('name', 'one'); \
             root.appendChild(slot); \
             var light = document.createElement('span'); \
             light.setAttribute('slot', 'one'); \
             host.appendChild(light); \
             var open = light.assignedSlot === slot \
                     && slot.assignedNodes()[0] === light; \
             var closedHost = document.createElement('div'); \
             document.body.appendChild(closedHost); \
             var closed = closedHost.attachShadow({mode: 'closed'}); \
             var closedSlot = document.createElement('slot'); \
             closed.appendChild(closedSlot); \
             var closedLight = document.createElement('span'); \
             closedHost.appendChild(closedLight); \
             String(open && closedLight.assignedSlot === null \
                    && closedSlot.assignedNodes()[0] === closedLight);",
            "t",
        )
        .expect("assignedSlot");
    assert_eq!(out, "true");
}

#[test]
fn attach_shadow_validates_mode_and_safelisted_elements() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var host = document.createElement('div'); \
             var missing = null, bad = null, safelist = null; \
             try { host.attachShadow({}); } catch (e) { missing = e.name; } \
             try { host.attachShadow({ mode: 1 }); } catch (e) { bad = e.name; } \
             globalThis.ATTACHSHADOW_DISALLOWED_ELEMENTS = ['a']; \
             try { document.createElement('a').attachShadow({ mode: 'open' }); } \
             catch (e) { safelist = e.name; } \
             String(missing === 'TypeError' && bad === 'TypeError' && \
                    safelist === 'NotSupportedError');",
            "t",
        )
        .expect("attachShadow validation");
    assert_eq!(out, "true");
}
