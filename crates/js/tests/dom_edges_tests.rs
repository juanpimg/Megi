#![cfg(feature = "js")]

//! DOM edge cases surfaced by the audit: `createElement` casing and
//! `insertAdjacent*` on non-element nodes.

use megi_js::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

#[test]
fn create_element_lowercases_html_names() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var template = document.createElement('TEMPLATE');
             template.innerHTML = '<p>x</p>';
             globalThis.templateChildren = template.childNodes.length;",
            "create.js",
        )
        .expect("createElement");
    // Lowercase `template` gets its inert content fragment, so the element itself has no
    // children (an uppercase name would have kept the parsed <p> as a direct child).
    assert_eq!(
        runtime
            .eval("templateChildren", "create.js")
            .expect("children"),
        "0"
    );
}

#[test]
fn insert_adjacent_is_element_only() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.textError = null;
             var text = document.createTextNode('x');
             try { text.insertAdjacentHTML('beforeend', '<p>y</p>'); }
             catch (error) { textError = String(error); }
             globalThis.textChildren = text.childNodes.length;
             globalThis.htmlError = null;
             try { text.insertAdjacentText('beforeend', 'y'); }
             catch (error) { htmlError = String(error); }",
            "insert.js",
        )
        .expect("insertAdjacent");
    assert_eq!(
        runtime
            .eval("textError !== null", "insert.js")
            .expect("html error"),
        "true"
    );
    assert_eq!(
        runtime
            .eval("htmlError !== null", "insert.js")
            .expect("text error"),
        "true"
    );
    assert_eq!(
        runtime.eval("textChildren", "insert.js").expect("children"),
        "0"
    );
}

#[test]
fn insert_adjacent_rejects_unknown_positions_with_syntax_error() {
    // #653: an invalid position used to be a silent no-op that discarded the parsed nodes;
    // the DOM requires a SyntaxError before anything is detached or created.
    let mut runtime = runtime();
    runtime
        .eval(
            "var host = document.createElement('div');
             host.innerHTML = '<p>keep</p>';
             document.body.appendChild(host);
             globalThis.results = [];
             var positions = ['sideways', '', 'BEFOREEND ', 'before-end'];
             for (var i = 0; i < positions.length; i++) {
               var htmlError = null, textError = null;
               try { host.insertAdjacentHTML(positions[i], '<b>new</b>'); }
               catch (error) { htmlError = error.name + ':' + String(error); }
               try { host.insertAdjacentText(positions[i], 'new'); }
               catch (error) { textError = error.name + ':' + String(error); }
               results.push([positions[i], htmlError, textError].join('|'));
             }
             globalThis.html = host.innerHTML;
             globalThis.childCount = host.childNodes.length;",
            "positions.js",
        )
        .expect("insertAdjacent positions");
    let results = runtime
        .eval("results.join(';')", "positions.js")
        .expect("results");
    for entry in results.split(';') {
        assert!(
            entry.contains("SyntaxError:"),
            "every invalid position must throw SyntaxError: {entry}"
        );
    }
    // Nothing was detached, created or moved by the failing calls.
    assert_eq!(
        runtime.eval("childCount", "positions.js").expect("count"),
        "1",
        "a rejected position must leave the element untouched"
    );
    assert_eq!(
        runtime.eval("html", "positions.js").expect("html"),
        "<p>keep</p>"
    );
}

#[test]
fn insert_adjacent_accepts_positions_case_insensitively() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var host = document.createElement('div');
             host.innerHTML = '<i>mid</i>';
             document.body.appendChild(host);
             host.insertAdjacentHTML('BeforeEnd', '<b>end</b>');
             host.insertAdjacentText('AFTERBEGIN', 'start');
             globalThis.out = host.innerHTML;",
            "case.js",
        )
        .expect("mixed case positions");
    let out = runtime.eval("out", "case.js").expect("html");
    assert!(out.contains("end"), "beforeend must append: {out}");
    assert!(out.contains("start"), "afterbegin must prepend: {out}");
}

#[test]
fn clone_node_respects_the_dom_node_budget() {
    // #657: `cloneNode(true)` doubled the arena without any accounting; the per-eval loop
    // budget does not bound allocation.
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_max_dom_nodes(64);

    runtime
        .eval(
            "window.__err = '';\n\
             var root = document.documentElement;\n\
             try {\n\
               for (var i = 0; i < 20; i++) { root.appendChild(root.cloneNode(true)); }\n\
             } catch (error) { __err = String(error); }\n\
             __err",
            "t",
        )
        .map(|_| ())
        .expect("the script itself must not fail the eval after catching");
    let error = runtime.eval("__err", "t").expect("eval");
    assert!(
        error.contains("node budget"),
        "expected a node-budget rejection, got {error:?}"
    );
}

#[test]
fn listener_slot_reuse_bounds_internal_array_across_churn() {
    let mut runtime = runtime();
    let len = runtime
        .eval(
            "var div = document.createElement('div');
             for (var i = 0; i < 200; i++) {
               var fn = function() {};
               div.addEventListener('click', fn);
               div.removeEventListener('click', fn);
             }
             for (var j = 0; j < 50; j++) {
               div.addEventListener('ping', function() {}, { once: true });
               div.dispatchEvent(new Event('ping'));
             }
             __megi_listeners.length",
            "listeners.js",
        )
        .expect("eval");
    assert_eq!(
        len, "1",
        "freed listener slots must be reused, got len={len}"
    );
}

#[test]
fn insert_adjacent_html_frees_temp_and_propagates_hierarchy_error() {
    let mut runtime = runtime();
    let before = runtime.document().len();
    runtime
        .eval(
            "var host = document.createElement('div');
             document.body.appendChild(host);
             for (var i = 0; i < 20; i++) {
               host.insertAdjacentHTML('beforeend', '');
             }
             var err = '';
             try {
               document.body.appendChild(document.documentElement);
             } catch (e) { err = String(e); }
             globalThis.__err = err;",
            "adj.js",
        )
        .expect("eval");
    let err = runtime
        .eval("globalThis.__err", "adj.js")
        .expect("read captured error");
    assert!(
        err.contains("hierarchy") || err.contains("unknown node"),
        "hierarchy failure must be reported, got: {err}"
    );
    let after = runtime.document().len();
    assert!(
        after <= before + 4,
        "temp containers must not leak in arena: before={before}, after={after}"
    );
}

#[test]
fn class_list_rejects_empty_and_whitespace_tokens() {
    let mut runtime = runtime();
    let res = runtime
        .eval(
            "var d = document.createElement('div');
             var errs = [];
             try { d.classList.add(''); } catch (e) { errs.push(e.name || String(e)); }
             try { d.classList.add('a b'); } catch (e) { errs.push(String(e)); }
             try { d.classList.remove(''); } catch (e) { errs.push(e.name || String(e)); }
             try { d.classList.toggle('a\\tb'); } catch (e) { errs.push(String(e)); }
             errs.length + ':' + errs[0].indexOf('SyntaxError') + ':' + (errs[1].indexOf('InvalidCharacterError') >= 0)",
            "classlist.js",
        )
        .expect("eval");
    assert_eq!(res, "4:0:true", "unexpected classList error output: {res}");
}

#[test]
fn get_computed_style_does_not_leak_prototype_and_handles_negative_item() {
    let mut runtime = runtime();
    let res = runtime
        .eval(
            "var d = document.createElement('div');
             d.setAttribute('style', 'color: red;');
             var cs = getComputedStyle(d);
             [
               cs.getPropertyValue('color'),
               cs.getPropertyValue('toString'),
               cs.getPropertyValue('getPropertyValue'),
               cs.getPropertyValue('length'),
               cs.item(-1),
               cs.item(0),
               cs.item(99)
             ].join('|')",
            "cs.js",
        )
        .expect("eval");
    assert_eq!(res, "red|||||color|", "unexpected getComputedStyle: {res}");
}

#[test]
fn create_element_rejects_invalid_tag_names() {
    let mut runtime = runtime();
    let res = runtime
        .eval(
            "var errs = 0;
             for (var name of ['', '1div', '<bad>', 'a b']) {
               try { document.createElement(name); } catch (e) { errs++; }
             }
             errs + ':' + document.createElement('DIV').tagName",
            "create.js",
        )
        .expect("eval");
    assert_eq!(res, "4:DIV");
}

#[test]
fn select_value_returns_selected_option_text_only() {
    let mut runtime = runtime();
    let res = runtime
        .eval(
            "var s = document.createElement('select');
             s.innerHTML = '<option>One</option><option selected>Two</option><option>Three</option>';
             s.value",
            "select.js",
        )
        .expect("eval");
    assert_eq!(res, "Two");
}

#[test]
fn webidl_coercions_and_storage_key_order() {
    let mut runtime = runtime();
    let res = runtime
        .eval(
            "var cb = document.createElement('input');
             cb.checked = 1;
             var c1 = cb.checked;
             cb.checked = 0;
             var c0 = cb.checked;
             var d = document.createElement('div');
             var t1 = d.classList.toggle('x', 1);
             var t0 = d.classList.toggle('x', 0);
             var ev = new Event('x', { bubbles: 1, cancelable: 0 });
             localStorage.setItem('b', '2');
             localStorage.setItem('a', '1');
             localStorage.setItem('c', '3');
             var keys = [localStorage.key(0), localStorage.key(1), localStorage.key(2), String(localStorage.key(-1))].join(',');
             var nl = document.body.childNodes.item(-1);
             var st = d.style.item(-1);
             var at = d.attributes.item(-1);
             [c1, c0, t1, t0, ev.bubbles, ev.cancelable, keys, String(nl), st, String(at)].join('|')",
            "webidl.js",
        )
        .expect("eval");
    assert_eq!(
        res,
        "true|false|true|false|true|false|b,a,c,null|null||null"
    );
}

#[test]
fn element_get_elements_by_tag_name_is_available() {
    // #1875: jQuery calls `element.getElementsByTagName("*")` during capability detection.
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var d = document.createElement('div'); \
             var a = document.createElement('span'); d.appendChild(a); \
             String(typeof d.getElementsByTagName) + ':' + \
             d.getElementsByTagName('span').length + ':' + \
             d.getElementsByTagName('*').length;",
            "t",
        )
        .expect("eval");
    assert_eq!(out, "function:1:1");
}

#[test]
fn element_constructor_and_instanceof_are_available() {
    // #1877: `Element is not defined` breaks Wikipedia and python.org scripts.
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var d = document.createElement('div'); \
             String(typeof Element) + ':' + String(d instanceof Element) + ':' + \
             String(typeof Element.prototype);",
            "t",
        )
        .expect("eval");
    assert_eq!(out, "function:true:object");
}

#[test]
fn standard_dom_globals_are_exposed() {
    // #1882: `self`, `EventTarget`, `Image` and `HTMLLinkElement` are used by real sites.
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var img = new Image('x.png'); \
             String(typeof self) + ':' + String(typeof EventTarget) + ':' + \
             String(typeof HTMLLinkElement) + ':' + String(img.tagName) + ':' + \
             String(img instanceof Element);",
            "t",
        )
        .expect("eval");
    assert_eq!(out, "object:function:function:IMG:true");
}

#[test]
fn accessibility_snapshot_exposes_named_nodes() {
    // F7-A11Y (partial): deterministic accessibility snapshot for embedders.
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "document.body.innerHTML = '<h1>Hi</h1><img alt=\"Logo\"><button aria-label=\"Go\">x</button>'; \
             __megiAccessibilityTree().map(function (n) { return n.role + ':' + n.name; }).join(',');",
            "t",
        )
        .expect("eval");
    assert_eq!(out, "heading:Hi,img:Logo,button:Go");
}
