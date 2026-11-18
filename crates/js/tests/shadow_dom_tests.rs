//! #1541: lightweight open Shadow DOM — `attachShadow`, `shadowRoot`, `getRootNode`.

#![cfg(feature = "js")]

use js::runtime::JsRuntime;

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

#[test]
fn closed_shadow_root_hides_shadow_root_and_assigned_slot() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var host = document.createElement('div'); \
             document.body.appendChild(host); \
             var closed = host.attachShadow({ mode: 'closed' }); \
             closed.innerHTML = '<slot name=\"x\"></slot>'; \
             var slotted = document.createElement('span'); \
             slotted.slot = 'x'; \
             host.appendChild(slotted); \
             var conflict = false; \
             try { host.attachShadow({ mode: 'open' }); } catch (e) { conflict = true; } \
             String(host.shadowRoot === null && closed.mode === 'closed' && closed.host === host \
                    && slotted.assignedSlot === null && conflict);",
            "t",
        )
        .expect("closed mode");
    assert_eq!(out, "true");
}

#[test]
fn slot_assignment_flatten_and_slotchange_event() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var host = document.createElement('div'); \
             document.body.appendChild(host); \
             var outer = host.attachShadow({ mode: 'open' }); \
             outer.innerHTML = '<inner-comp><slot name=\"item\"></slot></inner-comp>'; \
             var outerSlot = outer.querySelector('slot'); \
             var innerHost = outer.querySelector('inner-comp'); \
             var inner = innerHost.attachShadow({ mode: 'open' }); \
             inner.innerHTML = '<slot id=\"leaf\"></slot>'; \
             var leafSlot = inner.querySelector('#leaf'); \
             var changes = 0; \
             outerSlot.addEventListener('slotchange', function () { changes++; }); \
             var a = document.createElement('b'); \
             a.slot = 'item'; \
             host.appendChild(a); \
             var unslotted = document.createElement('i'); \
             host.appendChild(unslotted); \
             var beforeRetarget = outerSlot.assignedElements().length; \
             var flattened = leafSlot.assignedNodes({ flatten: true }); \
             a.slot = 'other'; \
             String(changes + '|' + beforeRetarget + '|' + (flattened.length === 1 && flattened[0] === a) \
                    + '|' + (unslotted.assignedSlot === null) + '|' + (leafSlot.getRootNode({ composed: true }) === document));",
            "t",
        )
        .expect("slot assignment");
    assert_eq!(out, "2|1|true|true|true", "slot output: {out}");
}

#[test]
fn node_is_connected_reflects_shadow_dom_membership() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var host = document.createElement('div'); \
             var child = document.createElement('span'); \
             var sr = host.attachShadow({ mode: 'open' }); \
             sr.appendChild(child); \
             var detachedConnected = child.isConnected; \
             document.body.appendChild(host); \
             var attachedConnected = child.isConnected; \
             document.body.removeChild(host); \
             String(detachedConnected + '|' + attachedConnected + '|' + child.isConnected);",
            "t",
        )
        .expect("isConnected");
    assert_eq!(out, "false|true|false");
}
