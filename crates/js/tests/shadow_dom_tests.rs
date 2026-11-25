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
fn flattened_slot_distributes_fallback_children() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var host = document.createElement('div'); \
             document.body.appendChild(host); \
             var root = host.attachShadow({mode: 'open'}); \
             var slot = document.createElement('slot'); \
             root.appendChild(slot); \
             var fallback = document.createElement('b'); \
             slot.appendChild(fallback); \
             var nodes = slot.assignedNodes({flatten: true}); \
             String(nodes.length === 1 && nodes[0] === fallback);",
            "t",
        )
        .expect("flatten fallback");
    assert_eq!(out, "true");
}

#[test]
fn dom_node_type_constants_are_exposed() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "String(Node.ELEMENT_NODE === 1 && Node.TEXT_NODE === 3 && \
             Node.DOCUMENT_FRAGMENT_NODE === 11 && \
             document.createElement('div').nodeType === 1);",
            "t",
        )
        .expect("node constants");
    assert_eq!(out, "true");
}

#[test]
fn parsed_template_content_survives_import_node() {
    let html = "<div id=test1><div id=host><template data-mode=open><slot id=s1 name=slot1><div id=f1></div></slot></template></div></div>";
    let document = megi_html::parse_document(html, false);
    let mut runtime = JsRuntime::with_document(document, "http://web-platform.test/".into())
        .expect("runtime with fixture");
    let out = runtime
        .eval(
            "var n = {}; \
             for (var e of document.querySelectorAll('[id]')) n[e.id] = e; \
             var t = document.querySelector('template'); \
             n.host = t.parentNode; \
             n.host.removeChild(t); \
             var root = n.host.attachShadow({mode:'open'}); \
             root.appendChild(document.importNode(t.content, true)); \
             n.s1 = root.querySelector('slot'); \
             n.f1 = root.querySelector('div'); \
             String(root.childNodes.length + ',' + n.s1.childNodes.length + ',' + \
                    n.s1.assignedNodes({flatten:true}).length);",
            "t",
        )
        .expect("probe");
    assert_eq!(out, "1,1,1");
}

#[test]
fn slot_reflection_and_processing_instruction_nodes() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var light = document.createElement('span'); \
             light.slot = 'named'; \
             var host = document.createElement('div'); \
             var pi = document.createProcessingInstruction('target', 'data'); \
             host.appendChild(pi); \
             host.appendChild(light); \
             document.body.appendChild(host); \
             var root = host.attachShadow({mode: 'open'}); \
             var slot = document.createElement('slot'); \
             slot.setAttribute('name', 'named'); \
             root.appendChild(slot); \
             String(light.getAttribute('slot') === 'named' && light.slot === 'named' \
                    && pi.nodeType === 7 && pi.data === 'data' \
                    && light.assignedSlot === slot && slot.assignedNodes()[0] === light \
                    && slot.assignedNodes().indexOf(pi) === -1);",
            "t",
        )
        .expect("slot reflection");
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
