//! #1541: Shadow DOM slot assignment — light-DOM children map to default/named slots.

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

#[test]
fn default_slot_receives_unnamed_light_children() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var host = document.createElement('my-card'); document.body.appendChild(host); \
             var light = document.createElement('span'); light.textContent = 'content'; host.appendChild(light); \
             var root = host.attachShadow({ mode: 'open' }); \
             var slot = document.createElement('slot'); root.appendChild(slot); \
             var assigned = slot.assignedNodes(); \
             String(assigned.length + ':' + (assigned[0] === light) + ':' + slot.assignedElements().length);",
            "t",
        )
        .expect("slots");
    assert_eq!(out, "1:true:1", "default slot assignment: {out}");
}

#[test]
fn named_slots_route_by_slot_attribute() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var host = document.createElement('x-widget'); document.body.appendChild(host); \
             var a = document.createElement('b'); a.setAttribute('slot', 'title'); host.appendChild(a); \
             var b = document.createElement('i'); host.appendChild(b); \
             var root = host.attachShadow({ mode: 'open' }); \
             var titleSlot = document.createElement('slot'); titleSlot.setAttribute('name', 'title'); root.appendChild(titleSlot); \
             var defaultSlot = document.createElement('slot'); root.appendChild(defaultSlot); \
             String(titleSlot.assignedNodes()[0] === a && defaultSlot.assignedNodes()[0] === b && titleSlot.assignedNodes().length === 1);",
            "t",
        )
        .expect("named slots");
    assert_eq!(out, "true", "named slot routing: {out}");
}

#[test]
fn slot_outside_shadow_has_no_assignments() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var slot = document.createElement('slot'); document.body.appendChild(slot); \
             String(slot.assignedNodes().length);",
            "t",
        )
        .expect("orphan slot");
    assert_eq!(out, "0");
}
