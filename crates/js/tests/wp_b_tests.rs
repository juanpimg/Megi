//! WP-B tests: CSSStyleDeclaration, getComputedStyle provider, NodeList,
//! matches/closest, insertAdjacent* and wrapper identity cache.

#![cfg(feature = "js")]

use js::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

// ---------------------------------------------------------------------------
// B1: CSSStyleDeclaration
// ---------------------------------------------------------------------------

#[test]
fn style_common_properties_round_trip_through_the_attribute() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var el = document.createElement('div');
             document.body.appendChild(el);
             el.style.color = 'red';
             el.style.width = '120px';",
            "style.js",
        )
        .expect("style set");
    let attribute = runtime
        .eval("el.getAttribute('style')", "style.js")
        .expect("attribute");
    assert!(attribute.contains("color: red"), "{attribute}");
    assert!(attribute.contains("width: 120px"), "{attribute}");
    assert_eq!(
        runtime.eval("el.style.color", "style.js").expect("color"),
        "red"
    );
    assert_eq!(
        runtime.eval("el.style.width", "style.js").expect("width"),
        "120px"
    );
}

#[test]
fn style_get_set_remove_property_supports_any_property() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var el = document.createElement('div');
             document.body.appendChild(el);
             el.style.setProperty('margin-top', '7px');
             el.style.setProperty('border-color', 'blue');",
            "props.js",
        )
        .expect("setProperty");
    assert_eq!(
        runtime
            .eval("el.style.getPropertyValue('margin-top')", "props.js")
            .expect("margin"),
        "7px"
    );
    assert_eq!(
        runtime
            .eval("el.style.getPropertyValue('border-color')", "props.js")
            .expect("border"),
        "blue"
    );
    assert_eq!(
        runtime.eval("el.style.length", "props.js").expect("length"),
        "2"
    );
    assert_eq!(
        runtime.eval("el.style.item(0)", "props.js").expect("item"),
        "margin-top"
    );
    assert_eq!(
        runtime
            .eval("el.style.removeProperty('margin-top')", "props.js")
            .expect("removed value"),
        "7px"
    );
    assert_eq!(
        runtime
            .eval("el.style.getPropertyValue('margin-top')", "props.js")
            .expect("gone"),
        ""
    );
    let attribute = runtime
        .eval("el.getAttribute('style')", "props.js")
        .expect("attribute");
    assert!(!attribute.contains("margin-top"), "{attribute}");
    assert!(attribute.contains("border-color"), "{attribute}");
}

#[test]
fn style_css_text_replaces_and_serializes_declarations() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var el = document.createElement('div');
             document.body.appendChild(el);
             el.style.cssText = 'color: green; padding: 4px';",
            "csstext.js",
        )
        .expect("cssText set");
    assert_eq!(
        runtime
            .eval("el.style.length", "csstext.js")
            .expect("length"),
        "2"
    );
    let css_text = runtime
        .eval("el.style.cssText", "csstext.js")
        .expect("cssText");
    assert!(css_text.contains("color: green"), "{css_text}");
    assert!(css_text.contains("padding: 4px"), "{css_text}");
    let attribute = runtime
        .eval("el.getAttribute('style')", "csstext.js")
        .expect("attribute");
    assert_eq!(
        css_text, attribute,
        "cssText and style attribute are coherent"
    );
    // Replacing cssText drops the previous declarations.
    runtime
        .eval("el.style.cssText = 'opacity: 0.5'", "csstext.js")
        .expect("cssText replace");
    assert_eq!(
        runtime.eval("el.style.color", "csstext.js").expect("color"),
        ""
    );
    assert!(
        runtime
            .eval("el.style.opacity !== undefined", "csstext.js")
            .is_ok(),
        "arbitrary CSS property reads must not throw"
    );
}

#[test]
fn style_object_is_stable_for_reads_inside_one_eval() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var el = document.createElement('div');
             document.body.appendChild(el);
             el.setAttribute('style', 'color: teal');
             globalThis.read = el.style.color;",
            "stable.js",
        )
        .expect("read");
    assert_eq!(runtime.eval("read", "stable.js").expect("read"), "teal");
}

// ---------------------------------------------------------------------------
// B2: getComputedStyle + provider
// ---------------------------------------------------------------------------

#[test]
fn get_computed_style_uses_the_injected_provider() {
    let mut runtime = runtime();
    runtime.set_computed_style_provider(Box::new(|_document, _node| {
        vec![
            ("color".to_string(), "rgb(1, 2, 3)".to_string()),
            ("width".to_string(), "100px".to_string()),
        ]
    }));
    runtime
        .eval(
            "var el = document.createElement('div');
             document.body.appendChild(el);
             var computed = window.getComputedStyle(el);",
            "computed.js",
        )
        .expect("getComputedStyle");
    assert_eq!(
        runtime
            .eval("computed.getPropertyValue('width')", "computed.js")
            .expect("width"),
        "100px"
    );
    assert_eq!(
        runtime
            .eval("computed.getPropertyValue('color')", "computed.js")
            .expect("color"),
        "rgb(1, 2, 3)"
    );
    assert_eq!(
        runtime
            .eval("computed.length", "computed.js")
            .expect("length"),
        "2"
    );
    assert_eq!(
        runtime
            .eval("computed.item(0)", "computed.js")
            .expect("item"),
        "color"
    );
    let css_text = runtime
        .eval("computed.cssText", "computed.js")
        .expect("cssText");
    assert!(css_text.contains("width: 100px"), "{css_text}");
    assert_eq!(
        runtime
            .eval(
                "try { computed.setProperty('color', 'red'); 'no-throw' } catch (e) { 'read-only' }",
                "computed.js",
            )
            .expect("read-only"),
        "read-only"
    );
}

#[test]
fn get_computed_style_without_provider_falls_back_to_inline() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var el = document.createElement('div');
             document.body.appendChild(el);
             el.style.cssText = 'color: purple; height: 12px';
             var computed = window.getComputedStyle(el);",
            "inline-computed.js",
        )
        .expect("inline fallback");
    assert_eq!(
        runtime
            .eval("computed.getPropertyValue('color')", "inline-computed.js")
            .expect("color"),
        "purple"
    );
    assert_eq!(
        runtime
            .eval("computed.getPropertyValue('height')", "inline-computed.js")
            .expect("height"),
        "12px"
    );
}

// ---------------------------------------------------------------------------
// B3: NodeList
// ---------------------------------------------------------------------------

#[test]
fn children_and_query_selector_all_return_node_lists() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var host = document.createElement('div');
             host.id = 'host';
             for (var i = 0; i < 3; i++) {
                 var child = document.createElement('span');
                 child.className = 'item';
                 child.textContent = 'n' + i;
                 host.appendChild(child);
             }
             document.body.appendChild(host);",
            "nodelist.js",
        )
        .expect("setup");
    assert_eq!(
        runtime
            .eval("host.children.length", "nodelist.js")
            .expect("length"),
        "3"
    );
    assert_eq!(
        runtime
            .eval("host.children.item(1).textContent", "nodelist.js")
            .expect("item"),
        "n1"
    );
    assert_eq!(
        runtime
            .eval("host.children[2].textContent", "nodelist.js")
            .expect("index"),
        "n2"
    );
    assert_eq!(
        runtime
            .eval(
                "var count = 0; host.children.forEach(function (el) { count += 1; }); count;",
                "nodelist.js",
            )
            .expect("forEach"),
        "3"
    );
    assert_eq!(
        runtime
            .eval("document.querySelectorAll('.item').length", "nodelist.js")
            .expect("qsall length"),
        "3"
    );
    assert_eq!(
        runtime
            .eval(
                "document.querySelectorAll('.item')[0].textContent",
                "nodelist.js"
            )
            .expect("qsall index"),
        "n0"
    );
}

// ---------------------------------------------------------------------------
// B4: matches / closest
// ---------------------------------------------------------------------------

#[test]
fn matches_and_closest_use_css_selectors() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var card = document.createElement('div');
             card.className = 'card active';
             card.id = 'card';
             var label = document.createElement('span');
             label.className = 'label';
             card.appendChild(label);
             document.body.appendChild(card);",
            "matches.js",
        )
        .expect("setup");
    assert_eq!(
        runtime
            .eval("label.matches('.label')", "matches.js")
            .expect("matches"),
        "true"
    );
    assert_eq!(
        runtime
            .eval("label.matches('.nope')", "matches.js")
            .expect("matches false"),
        "false"
    );
    assert_eq!(
        runtime
            .eval("label.closest('#card').id", "matches.js")
            .expect("closest"),
        "card"
    );
    assert_eq!(
        runtime
            .eval("label.closest('.active') === card", "matches.js")
            .expect("closest identity"),
        "true"
    );
    assert_eq!(
        runtime
            .eval("label.closest('.missing') === null", "matches.js")
            .expect("closest null"),
        "true"
    );
}

// ---------------------------------------------------------------------------
// B5: insertAdjacentHTML / insertAdjacentText
// ---------------------------------------------------------------------------

#[test]
fn insert_adjacent_html_and_text_positions() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var host = document.createElement('div');
             host.id = 'host';
             var inner = document.createElement('b');
             inner.id = 'inner';
             host.appendChild(inner);
             document.body.appendChild(host);",
            "adjacent.js",
        )
        .expect("setup");
    runtime
        .eval(
            "host.insertAdjacentHTML('afterbegin', '<i>first</i>');
             host.insertAdjacentHTML('beforeend', '<i>last</i>');
             inner.insertAdjacentText('beforebegin', 'before-inner');
             inner.insertAdjacentText('afterend', 'after-inner');",
            "adjacent.js",
        )
        .expect("insertAdjacent*");
    let html = runtime
        .eval("host.innerHTML", "adjacent.js")
        .expect("innerHTML");
    assert!(html.contains("<i>first</i>"), "{html}");
    assert!(html.contains("<i>last</i>"), "{html}");
    assert!(html.contains("before-inner"), "{html}");
    assert!(html.contains("after-inner"), "{html}");
    assert_eq!(
        runtime
            .eval("host.firstChild.textContent", "adjacent.js")
            .expect("first"),
        "first"
    );
}

// ---------------------------------------------------------------------------
// B6: wrapper identity cache
// ---------------------------------------------------------------------------

#[test]
fn wrapper_identity_is_cached_per_realm() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var el = document.createElement('div');
             el.id = 'cached';
             document.body.appendChild(el);",
            "identity.js",
        )
        .expect("setup");
    assert_eq!(
        runtime
            .eval("document.getElementById('cached') === el", "identity.js")
            .expect("getElementById identity"),
        "true"
    );
    assert_eq!(
        runtime
            .eval("document.querySelector('#cached') === el", "identity.js")
            .expect("querySelector identity"),
        "true"
    );
    assert_eq!(
        runtime
            .eval(
                "document.getElementsByTagName('div')[0] === el",
                "identity.js"
            )
            .expect("tag identity"),
        "true"
    );
    assert_eq!(
        runtime
            .eval("el.parentNode.children[0] === el", "identity.js")
            .expect("parent children identity"),
        "true"
    );
}
