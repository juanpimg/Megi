//! WPT-like regression tests for the R167 DOM/JS bug-bash.
//!
//! Each test names the bug it pins down (WPT-inspired, offline, one realm per test).
#![cfg(feature = "js")]

use js::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

fn eval(runtime: &mut JsRuntime, source: &str) -> String {
    runtime.eval(source, "r167.js").expect("eval")
}

#[test]
fn child_nodes_includes_text_nodes_and_elements() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var parent = document.createElement('div');
        parent.appendChild(document.createTextNode('hello'));
        parent.appendChild(document.createElement('span'));
        parent.appendChild(document.createTextNode('!'));
        parent.childNodes.length + ':' + parent.childNodes[0].textContent + ':' +
            parent.childNodes.item(1).tagName;
        "#,
    );
    assert_eq!(value, "3:hello:SPAN");
}

#[test]
fn append_child_with_document_fragment_moves_children() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var parent = document.createElement('div');
        var fragment = document.createDocumentFragment();
        fragment.appendChild(document.createElement('a'));
        fragment.appendChild(document.createElement('b'));
        var returned = parent.appendChild(fragment);
        parent.children.length + ':' + parent.children[0].tagName +
            parent.children[1].tagName + ':' + fragment.childNodes.length + ':' +
            (returned === fragment ? 'fragment' : 'other');
        "#,
    );
    assert_eq!(
        value, "2:AB:0:fragment",
        "fragment expands and returns the fragment"
    );
}

#[test]
fn insert_before_null_appends_and_replace_child_keeps_position() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var parent = document.createElement('ul');
        var a = document.createElement('li'); a.textContent = 'a';
        var b = document.createElement('li'); b.textContent = 'b';
        var c = document.createElement('li'); c.textContent = 'c';
        parent.appendChild(a);
        parent.insertBefore(b, null);       // DOM: appends
        parent.insertBefore(c, a);          // c, a, b
        var replacement = document.createElement('li'); replacement.textContent = 'x';
        parent.replaceChild(replacement, a); // c, x, b
        var names = [];
        for (var i = 0; i < parent.children.length; i++) names.push(parent.children[i].textContent);
        names.join('');
        "#,
    );
    assert_eq!(value, "cxb");
}

#[test]
fn html_attribute_names_are_case_insensitive() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var div = document.createElement('div');
        div.setAttribute('DATA-Test', 'one');
        var got = div.getAttribute('data-test');
        var has = div.hasAttribute('Data-TEST');
        div.removeAttribute('DATA-test');
        got + ':' + has + ':' + div.hasAttribute('data-test');
        "#,
    );
    assert_eq!(value, "one:true:false");
}

#[test]
fn reflected_value_checked_disabled_properties() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var input = document.createElement('input');
        input.value = 'typed';
        input.checked = true;
        input.disabled = true;
        var ok = input.value === 'typed' && input.checked === true && input.disabled === true;
        ok = ok && input.getAttribute('value') === 'typed';
        ok = ok && input.hasAttribute('checked') && input.hasAttribute('disabled');
        input.checked = false;
        ok = ok && !input.hasAttribute('checked');
        var textarea = document.createElement('textarea');
        textarea.value = 'multi';
        ok = ok && textarea.value === 'multi' && textarea.textContent === 'multi';
        ok ? 'ok' : 'fail';
        "#,
    );
    assert_eq!(value, "ok");
}

#[test]
fn select_options_and_selected_reflection() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var select = document.createElement('select');
        var one = document.createElement('option'); one.value = '1'; one.textContent = 'One';
        var two = document.createElement('option'); two.value = '2'; two.textContent = 'Two';
        select.appendChild(one);
        select.appendChild(two);
        two.selected = true;
        select.options.length + ':' + select.options.item(1).textContent + ':' +
            two.selected + ':' + select.value;
        "#,
    );
    assert_eq!(value, "2:Two:true:2");
}

#[test]
fn class_list_and_text_content_updates_children() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var div = document.createElement('div');
        div.classList.add('a', 'b');
        div.classList.toggle('c', true);
        div.textContent = 'text';
        var ok = div.classList.contains('a') && div.classList.contains('c');
        ok = ok && div.className.indexOf('a') !== -1 && div.childNodes.length === 1;
        ok = ok && div.textContent === 'text';
        div.textContent = '';
        ok = ok && div.childNodes.length === 0;
        ok ? 'ok' : 'fail';
        "#,
    );
    assert_eq!(value, "ok");
}

#[test]
fn clone_node_deep_copies_descendants() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var source = document.createElement('div');
        source.setAttribute('data-x', '1');
        var child = document.createElement('span'); child.textContent = 'child';
        source.appendChild(child);
        var shallow = source.cloneNode(false);
        var deep = source.cloneNode(true);
        shallow.children.length + ':' + deep.children.length + ':' +
            deep.getAttribute('data-x') + ':' + deep.children[0].textContent + ':' +
            (shallow === source ? 'same' : 'different');
        "#,
    );
    assert_eq!(value, "0:1:1:child:different");
}

#[test]
fn event_once_listener_is_removed_after_first_dispatch() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var element = document.createElement('div');
        var calls = 0;
        element.addEventListener('ping', function () { calls++; }, { once: true });
        element.dispatchEvent(new Event('ping'));
        element.dispatchEvent(new Event('ping'));
        String(calls);
        "#,
    );
    assert_eq!(value, "1");
}

#[test]
fn event_stop_immediate_propagation_skips_later_listeners() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var element = document.createElement('div');
        var order = [];
        element.addEventListener('ping', function (event) {
            order.push('first');
            event.stopImmediatePropagation();
        });
        element.addEventListener('ping', function () { order.push('second'); });
        var parent = document.createElement('section');
        parent.appendChild(element);
        parent.addEventListener('ping', function () { order.push('parent'); });
        element.dispatchEvent(new Event('ping', { bubbles: true }));
        order.join(',');
        "#,
    );
    assert_eq!(value, "first");
}

#[test]
fn passive_listener_cannot_prevent_the_default_action() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var element = document.createElement('div');
        element.addEventListener('ping', function (event) {
            event.preventDefault();
        }, { passive: true });
        var event = new Event('ping', { cancelable: true });
        element.dispatchEvent(event);
        String(event.defaultPrevented);
        "#,
    );
    assert_eq!(value, "false");
}

#[test]
fn capture_and_bubble_phases_visit_document_then_element() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var element = document.createElement('div');
        document.body.appendChild(element);
        var order = [];
        document.addEventListener('ping', function () { order.push('doc-capture'); }, true);
        document.addEventListener('ping', function () { order.push('doc-bubble'); }, false);
        element.addEventListener('ping', function () { order.push('target'); }, false);
        element.dispatchEvent(new Event('ping', { bubbles: true }));
        order.join(',');
        "#,
    );
    assert_eq!(value, "doc-capture,target,doc-bubble");
}

#[test]
fn timers_and_request_animation_frame_are_runnable() {
    let mut runtime = runtime();
    eval(
        &mut runtime,
        r#"
        window.__r167 = [];
        setTimeout(function () { window.__r167.push('timeout'); }, 10);
        requestAnimationFrame(function (timestamp) { window.__r167.push('raf'); });
        "#,
    );
    let _ = runtime.run_tasks(64);
    let _ = runtime.advance(std::time::Duration::from_millis(20));
    let _ = runtime.run_tasks(64);
    let value = eval(&mut runtime, "window.__r167.join(',')");
    assert_eq!(value, "timeout,raf");
}

#[test]
fn get_computed_style_falls_back_to_inline_declarations() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var div = document.createElement('div');
        div.style.color = 'red';
        var style = getComputedStyle(div);
        style.color + ':' + style.getPropertyValue('color');
        "#,
    );
    assert_eq!(value, "red:red");
}

/// Documented gaps of this runtime (see `crates/js/src/lib.rs`). Kept as a test so the list
/// cannot silently drift: unsupported APIs stay `undefined` instead of pretending to work.
#[test]
fn documented_unsupported_apis_are_absent() {
    let mut runtime = runtime();
    let value = eval(
        &mut runtime,
        r#"
        var div = document.createElement('div');
        var gaps = [typeof fetch, typeof crypto, typeof Range];
        var supported = [
          typeof URLSearchParams, typeof queueMicrotask, typeof MouseEvent,
          typeof structuredClone, typeof atob, typeof btoa,
          typeof div.getBoundingClientRect
        ];
        gaps.every(function (value) { return value === 'undefined'; }) &&
          supported.every(function (value) { return value === 'function'; });
        "#,
    );
    assert_eq!(
        value, "true",
        "unsupported APIs must remain explicitly absent"
    );
}
