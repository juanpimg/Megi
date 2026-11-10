//! Offline integration tests for the embedded Boa runtime and the DOM bindings.
//!
//! These tests never touch the network or filesystem; each test builds its own runtime
//! (one realm per thread) and asserts both JS behaviour and the resulting DOM state.

#![cfg(feature = "js")]

use js::{Budget, JsError, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

fn body_tag(runtime: &JsRuntime) -> String {
    let document = runtime.document();
    let body = document.body().expect("body");
    document
        .try_node(body)
        .and_then(|node| node.as_element().map(|element| element.local_name.clone()))
        .unwrap_or_default()
}

#[test]
fn evaluates_expressions_strings_objects_and_functions() {
    let mut runtime = runtime();
    assert_eq!(runtime.eval("1 + 2 * 3", "test.js").expect("eval"), "7");
    assert_eq!(
        runtime
            .eval("var s = 'megi'; s + '!';", "test.js")
            .expect("eval"),
        "megi!"
    );
    assert_eq!(
        runtime
            .eval("({a: 1}).a + [10, 20][1]", "test.js")
            .expect("eval"),
        "21"
    );
    assert_eq!(
        runtime
            .eval("(function (x) { return x * 2; })(21)", "test.js")
            .expect("eval"),
        "42"
    );
    // Globals persist across eval calls (single realm).
    runtime
        .eval("globalThis.counter = 40", "test.js")
        .expect("eval");
    assert_eq!(
        runtime
            .eval("counter + 2", "test.js")
            .expect("persistent globals"),
        "42"
    );
}

#[test]
fn console_is_captured() {
    let mut runtime = runtime();
    runtime
        .eval(
            "console.log('hello', 42); console.warn('warned'); console.error('boom');",
            "console.js",
        )
        .expect("console eval");
    let entries = runtime.take_console();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].message, "hello 42");
    assert_eq!(entries[1].message, "warned");
    assert_eq!(entries[2].message, "boom");
    assert!(runtime.take_console().is_empty(), "buffer is drained");
}

#[test]
fn mutates_dom_with_get_element_by_id_and_attributes() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var root = document.createElement('div');
             root.id = 'root';
             root.className = 'box';
             root.setAttribute('data-kind', 'demo');
             document.body.appendChild(root);
             var child = document.createElement('span');
             child.textContent = 'hello';
             root.appendChild(child);",
            "dom.js",
        )
        .expect("dom script");
    let document = runtime.document();
    let root = document.get_element_by_id("root").expect("root exists");
    let node = document.node(root);
    assert_eq!(body_tag(&runtime), "body");
    assert!(document.try_node(node.parent.expect("parent")).is_some());
    let element = node.as_element().expect("element");
    assert_eq!(element.local_name, "div");
    assert_eq!(element.get_attribute("id"), Some("root"));
    assert_eq!(element.get_attribute("class"), Some("box"));
    assert_eq!(element.get_attribute("data-kind"), Some("demo"));
    assert_eq!(document.text_content(root), "hello");
}

#[test]
fn query_selector_finds_elements_and_returns_null_for_misses() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var card = document.createElement('div');
             card.className = 'card active';
             card.id = 'card';
             document.body.appendChild(card);",
            "query.js",
        )
        .expect("setup");
    runtime
        .eval(
            "var found = document.querySelector('#card.card.active');
             var missing = document.querySelector('.nope');
             var all = document.querySelectorAll('div');",
            "query.js",
        )
        .expect("query");
    assert_eq!(
        runtime.eval("found.id", "query.js").expect("found id"),
        "card"
    );
    assert_eq!(
        runtime
            .eval("missing === null", "query.js")
            .expect("missing"),
        "true"
    );
    assert_eq!(
        runtime.eval("all.length >= 1", "query.js").expect("all"),
        "true"
    );
}

#[test]
fn inner_html_set_and_get_round_trip() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var host = document.createElement('div');
             host.innerHTML = '<b>bold</b><i>italic</i>';
             document.body.appendChild(host);",
            "html.js",
        )
        .expect("innerHTML set");
    assert_eq!(
        runtime
            .eval("host.innerHTML", "html.js")
            .expect("innerHTML get"),
        "<b>bold</b><i>italic</i>"
    );
    runtime
        .eval("host.innerHTML = '<p class=\"x\">new</p>'", "html.js")
        .expect("innerHTML replace");
    let document = runtime.document();
    let host_id = document.get_element_by_id("host").or_else(|| {
        // The element has no id: find the div whose text is "new".
        document
            .get_elements_by_tag_name("div")
            .into_iter()
            .find(|node| document.text_content(*node) == "new")
    });
    assert!(host_id.is_some(), "replaced content must exist in DOM");
    assert_eq!(document.text_content(host_id.unwrap()), "new");
}

#[test]
fn style_property_updates_inline_attribute_and_css_parses_it() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var box = document.createElement('div');
             document.body.appendChild(box);
             box.style.color = 'red';
             box.style.width = '120px';",
            "cssom.js",
        )
        .expect("style script");
    let style_color = runtime
        .eval("box.style.color", "cssom.js")
        .expect("style get");
    assert_eq!(style_color, "red");
    let document = runtime.document();
    let box_id = document
        .get_elements_by_tag_name("div")
        .into_iter()
        .find(|node| {
            document
                .try_node(*node)
                .and_then(|node| node.as_element())
                .and_then(|element| element.get_attribute("style"))
                .is_some()
        })
        .expect("styled box");
    let style = document
        .try_node(box_id)
        .and_then(|node| node.as_element())
        .and_then(|element| element.get_attribute("style"))
        .expect("style attribute");
    assert!(style.contains("color: red"), "style attribute: {style}");
    assert!(style.contains("width: 120px"), "style attribute: {style}");
    let declarations = css::parser::parse_list_of_declarations(style);
    assert!(
        declarations
            .iter()
            .any(|d| d.name == "color" && d.raw_value == "red")
    );
    assert!(
        declarations
            .iter()
            .any(|d| d.name == "width" && d.raw_value == "120px")
    );
}

#[test]
fn document_write_appends_html_to_body() {
    let mut runtime = runtime();
    runtime
        .eval(
            "document.write('<p class=\"written\">from js</p>')",
            "write.js",
        )
        .expect("document.write");
    let document = runtime.document();
    let written = document.get_elements_by_class_name("written");
    assert_eq!(written.len(), 1);
    assert_eq!(document.text_content(written[0]), "from js");
}

#[test]
fn class_list_add_remove_toggle_contains() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var el = document.createElement('div');
             document.body.appendChild(el);
             el.classList.add('a');
             el.classList.add('b');
             el.classList.remove('a');
             el.classList.toggle('c');",
            "classlist.js",
        )
        .expect("classList");
    assert_eq!(
        runtime
            .eval("el.classList.contains('b')", "classlist.js")
            .expect("contains"),
        "true"
    );
    assert_eq!(
        runtime
            .eval("el.classList.contains('a')", "classlist.js")
            .expect("removed"),
        "false"
    );
    assert_eq!(
        runtime
            .eval("el.className", "classlist.js")
            .expect("className"),
        "b c"
    );
}

#[test]
fn infinite_loop_is_stopped_by_the_budget() {
    let mut runtime = runtime();
    runtime.set_budget(Budget {
        loop_iterations: 10_000,
        ..Budget::SMALL
    });
    let error = runtime
        .eval("while (true) { }", "loop.js")
        .expect_err("budget must stop the loop");
    assert!(matches!(error, JsError::Budget(_)), "{error:?}");
}

#[test]
fn syntax_error_is_typed() {
    let mut runtime = runtime();
    let error = runtime
        .eval("function (", "syntax.js")
        .expect_err("syntax error");
    assert!(matches!(error, JsError::Syntax(_)), "{error:?}");
}

#[test]
fn stale_node_handle_is_a_typed_js_error() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var doomed = document.createElement('div');
             document.body.appendChild(doomed);",
            "stale.js",
        )
        .expect("setup");
    // Replacing body's innerHTML frees the previous subtree (arena slots become stale).
    runtime
        .eval("document.body.innerHTML = ''", "stale.js")
        .expect("clear body");
    let error = runtime
        .eval("doomed.textContent", "stale.js")
        .expect_err("stale wrapper must throw");
    let message = error.to_string();
    assert!(
        message.contains("stale")
            || message.contains("DOM error")
            || matches!(error, JsError::Runtime(_)),
        "unexpected error: {message}"
    );
}

#[test]
fn timers_are_queued_and_pumped() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.ticks = 0;
             setTimeout(function () { globalThis.ticks += 1; }, 0);
             setInterval(function () { globalThis.ticks += 1; }, 10);",
            "timers.js",
        )
        .expect("timers");
    assert!(runtime.pending_timers() >= 2);
    let ran = runtime.run_tasks(8).expect("run tasks");
    assert!(ran >= 2, "expected timers to run, got {ran}");
    let ticks = runtime.eval("ticks", "timers.js").expect("ticks");
    assert!(ticks.parse::<u32>().unwrap_or(0) >= 2, "ticks={ticks}");
}
