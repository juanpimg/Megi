//! Round 35: WPT-like DOM/JS conformance audit (regression tests for found gaps).

use js::runtime::JsRuntime;

fn runtime() -> JsRuntime {
    JsRuntime::new().expect("runtime")
}
fn eval(runtime: &mut JsRuntime, script: &str) -> String {
    runtime.eval(script, "conformance.js").expect("eval")
}

#[test]
fn fragment_append_moves_all_children() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var frag = document.createDocumentFragment();
      var a = document.createElement('a'); var b = document.createElement('b');
      frag.appendChild(a); frag.appendChild(b);
      document.body.appendChild(frag);
      document.body.children.length + ':' + frag.children.length + ':' + document.body.children[0].tagName + ':' + document.body.children[1].tagName
    "#,
    );
    assert_eq!(value, "2:0:A:B");
}

#[test]
fn fragment_insert_before_keeps_order() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var x = document.createElement('x'); document.body.appendChild(x);
      var frag = document.createDocumentFragment();
      var a = document.createElement('a'); var b = document.createElement('b');
      frag.appendChild(a); frag.appendChild(b);
      document.body.insertBefore(frag, x);
      var out = []; for (var i = 0; i < document.body.children.length; i++) { out.push(document.body.children[i].tagName); }
      out.join(',')
    "#,
    );
    assert_eq!(value, "A,B,X");
}

#[test]
fn clone_node_deep_and_shallow() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var p = document.createElement('p'); p.setAttribute('id', 'orig');
      var child = document.createElement('span'); child.textContent = 'kid'; p.appendChild(child);
      var deep = p.cloneNode(true); var shallow = p.cloneNode(false);
      deep.setAttribute('id', 'clone'); deep.children[0].textContent = 'changed';
      deep.children.length + ':' + shallow.children.length + ':' + p.children[0].textContent + ':' + p.getAttribute('id')
    "#,
    );
    assert_eq!(value, "1:0:kid:orig");
}

#[test]
fn attributes_are_case_insensitive_in_html() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var el = document.createElement('div');
      el.setAttribute('DATA-X', '1');
      el.getAttribute('data-x') + ':' + el.hasAttribute('Data-X') + ':' + el.attributes.length
    "#,
    );
    assert_eq!(value, "1:true:1");
}

#[test]
fn class_list_multi_arg_and_force() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var el = document.createElement('div');
      el.classList.add('a', 'b'); el.classList.remove('a'); el.classList.toggle('c'); el.classList.toggle('b');
      var before = el.className + ':' + el.classList.length + ':' + el.classList.contains('c');
      el.classList.toggle('d', true); el.classList.toggle('c', false);
      before + ':' + el.className + ':' + el.classList.contains('c')
    "#,
    );
    assert_eq!(value, "c:1:true:d:false");
}

#[test]
fn dataset_and_inner_text() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var el = document.createElement('div'); el.setAttribute('data-foo-bar', 'x');
      el.textContent = 'hello world';
      el.dataset.fooBar + ':' + el.innerText
    "#,
    );
    assert_eq!(value, "x:hello world");
}

#[test]
fn child_nodes_are_a_documented_snapshot() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var el = document.createElement('div'); el.appendChild(document.createElement('a'));
      var list = el.childNodes;
      el.appendChild(document.createElement('b'));
      list.length + ':' + el.childNodes.length
    "#,
    );
    assert_eq!(value, "1:2");
}

#[test]
fn capture_bubble_order_and_once() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      window.__order = [];
      var outer = document.createElement('div'); var inner = document.createElement('span');
      outer.appendChild(inner); document.body.appendChild(outer);
      outer.addEventListener('x', function () { __order.push('outer-capture'); }, true);
      inner.addEventListener('x', function () { __order.push('inner'); });
      outer.addEventListener('x', function () { __order.push('outer-bubble'); });
      inner.addEventListener('x', function () { __order.push('once'); }, { once: true });
      inner.dispatchEvent(new Event('x', { bubbles: true }));
      inner.dispatchEvent(new Event('x', { bubbles: true }));
      __order.join(',')
    "#,
    );
    assert_eq!(
        value,
        "outer-capture,inner,once,outer-bubble,outer-capture,inner,outer-bubble"
    );
}

#[test]
fn prevent_default_and_stop_immediate() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var a = document.createElement('a'); var b = document.createElement('b'); a.appendChild(b);
      window.__seen = 0;
      b.addEventListener('y', function (e) { e.preventDefault(); e.stopImmediatePropagation(); __seen += 1; });
      b.addEventListener('y', function () { __seen += 1; });
      a.addEventListener('y', function () { __seen += 1; });
      var result = b.dispatchEvent(new Event('y', { bubbles: true, cancelable: true }));
      __seen + ':' + result
    "#,
    );
    assert_eq!(value, "1:false");
}

#[test]
fn event_target_document_and_window() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      window.__doc = 0; window.__win = 0;
      document.addEventListener('z', function () { __doc += 1; });
      window.addEventListener('z', function () { __win += 1; });
      document.dispatchEvent(new Event('z'));
      window.dispatchEvent(new Event('z'));
      __doc + ':' + __win
    "#,
    );
    assert_eq!(value, "1:1");
}

#[test]
fn mouse_event_related_target() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var a = document.createElement('a'); var b = document.createElement('b');
      window.__rel = null;
      a.addEventListener('click', function (e) { __rel = e.relatedTarget; });
      a.dispatchEvent(new MouseEvent('click', { relatedTarget: b }));
      __rel === b
    "#,
    );
    assert_eq!(value, "true");
}

#[test]
fn reflected_form_properties_and_select_options() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var input = document.createElement('input'); input.value = 'v'; input.checked = true; input.disabled = true;
      var select = document.createElement('select');
      var o1 = document.createElement('option'); o1.value = 'a'; var o2 = document.createElement('option'); o2.value = 'b';
      select.appendChild(o1); select.appendChild(o2);
      o2.selected = true;
      input.value + ':' + input.checked + ':' + input.disabled + ':' + select.options.length + ':' + o2.selected
    "#,
    );
    assert_eq!(value, "v:true:true:2:true");
}

#[test]
fn select_selected_index_is_a_documented_gap() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var select = document.createElement('select'); select.appendChild(document.createElement('option'));
      typeof select.selectedIndex
    "#,
    );
    assert_eq!(value, "undefined");
}

#[test]
fn computed_style_and_bounding_rect_provider() {
    use js::host::LayoutRect;
    let mut rt = runtime();
    rt.set_layout_provider(Box::new(|_node| {
        Some(LayoutRect {
            border_box: (1.0, 2.0, 10.0, 20.0),
            content_box: (1.0, 2.0, 8.0, 18.0),
        })
    }));
    let value = eval(
        &mut rt,
        r#"
      var el = document.createElement('div'); document.body.appendChild(el); el.style.color = 'red';
      var style = getComputedStyle(el);
      style.color + ':' + el.getBoundingClientRect().width + ':' + el.offsetWidth
    "#,
    );
    assert_eq!(value, "red:10:10");
}

#[test]
fn timers_raf_microtask_and_promise() {
    let mut rt = runtime();
    rt.eval("window.__log = []; setTimeout(function(){ __log.push('timeout'); }, 0); requestAnimationFrame(function(){ __log.push('raf'); }); queueMicrotask(function(){ __log.push('micro'); }); Promise.resolve().then(function(){ __log.push('promise'); });", "t").expect("schedule");
    rt.run_microtasks().expect("microtasks");
    rt.advance(std::time::Duration::from_millis(20))
        .expect("advance");
    rt.run_tasks(64).expect("tasks");
    let log = eval(&mut rt, "__log.join(',')");
    assert!(
        log.contains("micro")
            && log.contains("promise")
            && log.contains("timeout")
            && log.contains("raf"),
        "{log}"
    );
}

#[test]
fn url_search_params_and_structured_clone() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var p = new URLSearchParams('a=1&b=2'); p.set('a', '3');
      var o = { a: 1, b: [2, 3] }; var c = structuredClone(o); c.b[0] = 9;
      p.toString() + ':' + o.b[0] + ':' + c.b[0]
    "#,
    );
    assert_eq!(value, "a=3&b=2:2:9");
}

#[test]
fn normalize_and_compare_document_position() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var p = document.createElement('p');
      p.appendChild(document.createTextNode('a')); p.appendChild(document.createTextNode('b'));
      var a = document.createElement('a'); var b = document.createElement('b');
      p.appendChild(a); p.appendChild(b);
      var removed = p.normalize();
      p.childNodes.length + ':' + p.firstChild.textContent + ':' + a.compareDocumentPosition(b) + ':' + b.compareDocumentPosition(a)
    "#,
    );
    assert_eq!(value, "3:ab:4:2");
}

#[test]
fn fragment_with_text_nodes_and_append_child_move() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var frag = document.createDocumentFragment();
      frag.appendChild(document.createTextNode('a')); frag.appendChild(document.createTextNode('b'));
      document.body.appendChild(frag);
      var x = document.createElement('x'); document.body.appendChild(x); document.body.appendChild(x);
      document.body.childNodes.length + ':' + document.body.children.length + ':' + document.body.lastChild.tagName
    "#,
    );
    assert_eq!(value, "3:1:X");
}

#[test]
fn insert_before_fragment_null_appends_all() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      document.body.appendChild(document.createElement('x'));
      var frag = document.createDocumentFragment(); frag.appendChild(document.createElement('a')); frag.appendChild(document.createElement('b'));
      document.body.insertBefore(frag, null);
      var out = []; for (var i = 0; i < document.body.children.length; i++) out.push(document.body.children[i].tagName);
      out.join(',')
    "#,
    );
    assert_eq!(value, "X,A,B");
}

#[test]
fn remove_attribute_is_case_insensitive() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var el = document.createElement('div'); el.setAttribute('Data-X', '1'); el.removeAttribute('data-x');
      el.hasAttribute('DATA-X') + ':' + el.attributes.length
    "#,
    );
    assert_eq!(value, "false:0");
}

#[test]
fn class_list_remove_multiple_and_duplicates() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var el = document.createElement('div'); el.className = 'a b c';
      el.classList.add('a'); el.classList.remove('a', 'c');
      el.className + ':' + el.classList.length
    "#,
    );
    assert_eq!(value, "b:1");
}

#[test]
fn text_content_vs_inner_text_nested() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var p = document.createElement('p');
      var span = document.createElement('span'); span.textContent = 'a';
      p.appendChild(span); p.appendChild(document.createTextNode('b'));
      p.textContent + ':' + p.innerText
    "#,
    );
    assert_eq!(value, "ab:ab");
}

#[test]
fn passive_listener_cannot_prevent_default() {
    let mut rt = runtime();
    let value = eval(
        &mut rt,
        r#"
      var el = document.createElement('div'); document.body.appendChild(el);
      var seen = false;
      el.addEventListener('p', function (e) { e.preventDefault(); }, { passive: true });
      var result = el.dispatchEvent(new Event('p', { cancelable: true }));
      seen = result;
      seen
    "#,
    );
    // If passive is honored, preventDefault is ignored and dispatchEvent returns true.
    assert_eq!(value, "true");
}
