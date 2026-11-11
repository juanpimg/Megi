//! Round 27: DOM/JS gap APIs (URL/SearchParams, dataset, innerText, attributes,
//! normalize/compareDocumentPosition, queueMicrotask/idle, MouseEvent, base64, clone).

use js::runtime::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    JsRuntime::new().expect("runtime")
}

#[test]
fn url_and_search_params_subset() {
    let mut rt = runtime();
    assert_eq!(
        rt.eval("new URL('https://example.com/a/b?x=1#h').pathname", "t")
            .unwrap(),
        "/a/b"
    );
    assert_eq!(
        rt.eval("new URL('https://example.com/a/b?x=1#h').origin", "t")
            .unwrap(),
        "https://example.com"
    );
    assert_eq!(
        rt.eval("new URL('/c', 'https://example.com/a/b').pathname", "t")
            .unwrap(),
        "/c"
    );
    let params = rt
        .eval(
            "var p = new URLSearchParams('a=1&b=2'); p.append('a','3'); p.set('b','4'); p.delete('x'); p.has('a') + ':' + p.get('a') + ':' + p.toString()",
            "t",
        )
        .unwrap();
    assert_eq!(params, "true:1:a=1&b=4&a=3");
}

#[test]
fn dataset_reads_data_attributes() {
    let mut rt = runtime();
    rt.eval("var el = document.createElement('div'); el.setAttribute('data-foo-bar','x'); el.setAttribute('data-id','7')", "t").unwrap();
    assert_eq!(rt.eval("el.dataset.fooBar", "t").unwrap(), "x");
    assert_eq!(rt.eval("el.dataset.id", "t").unwrap(), "7");
}

#[test]
fn inner_text_approximation() {
    let mut rt = runtime();
    rt.eval(
        "var el = document.createElement('div'); el.textContent = 'hello world'",
        "t",
    )
    .unwrap();
    assert_eq!(rt.eval("el.innerText", "t").unwrap(), "hello world");
}

#[test]
fn attributes_named_node_map() {
    let mut rt = runtime();
    rt.eval("var el = document.createElement('div'); el.setAttribute('id','a'); el.setAttribute('class','b')", "t").unwrap();
    assert_eq!(rt.eval("el.attributes.length", "t").unwrap(), "2");
    assert_eq!(
        rt.eval("el.attributes.getNamedItem('id').value", "t")
            .unwrap(),
        "a"
    );
    assert_eq!(rt.eval("el.attributes.item(1).name", "t").unwrap(), "class");
    assert_eq!(
        rt.eval("el.attributes.removeNamedItem('class').value", "t")
            .unwrap(),
        "b"
    );
    assert_eq!(rt.eval("el.attributes.length", "t").unwrap(), "1");
}

#[test]
fn normalize_merges_text_nodes() {
    let mut rt = runtime();
    rt.eval("var el = document.createElement('p'); el.appendChild(document.createTextNode('a')); el.appendChild(document.createTextNode('b')); el.appendChild(document.createTextNode(''))", "t").unwrap();
    assert_eq!(rt.eval("el.childNodes.length", "t").unwrap(), "3");
    assert_eq!(rt.eval("el.normalize()", "t").unwrap(), "2");
    assert_eq!(rt.eval("el.childNodes.length", "t").unwrap(), "1");
    assert_eq!(rt.eval("el.textContent", "t").unwrap(), "ab");
}

#[test]
fn compare_document_position_bits() {
    let mut rt = runtime();
    rt.eval("var a = document.createElement('div'); var b = document.createElement('span'); a.appendChild(b)", "t").unwrap();
    assert_eq!(rt.eval("a.compareDocumentPosition(b)", "t").unwrap(), "20"); // CONTAINED_BY|FOLLOWING
    assert_eq!(rt.eval("b.compareDocumentPosition(a)", "t").unwrap(), "10"); // CONTAINS|PRECEDING
    assert_eq!(rt.eval("a.compareDocumentPosition(a)", "t").unwrap(), "0");
    assert_eq!(
        rt.eval("Node.DOCUMENT_POSITION_FOLLOWING", "t").unwrap(),
        "4"
    );
}

#[test]
fn queue_microtask_runs_on_flush() {
    let mut rt = runtime();
    rt.eval(
        "window.__micro = 0; queueMicrotask(function(){ __micro = 1; })",
        "t",
    )
    .unwrap();
    assert_eq!(rt.eval("__micro", "t").unwrap(), "0");
    rt.run_microtasks().expect("microtasks");
    assert_eq!(rt.eval("__micro", "t").unwrap(), "1");
}

#[test]
fn request_idle_callback_is_deterministic() {
    let mut rt = runtime();
    rt.eval(
        "window.__idle = 0; requestIdleCallback(function(d){ __idle = d.timeRemaining(); })",
        "t",
    )
    .unwrap();
    rt.run_tasks(64).expect("tasks");
    assert_eq!(rt.eval("__idle", "t").unwrap(), "50");
}

#[test]
fn mouse_and_pointer_events_related_target() {
    let mut rt = runtime();
    let result = rt
        .eval(
            "var a = document.createElement('a'); var b = document.createElement('b'); var seen = null; a.addEventListener('click', function(e){ seen = e.relatedTarget; }); var ev = new MouseEvent('click', {relatedTarget: b}); a.dispatchEvent(ev); seen === b",
            "t",
        )
        .unwrap();
    assert_eq!(result, "true");
    assert_eq!(
        rt.eval("new PointerEvent('click', {pointerId: 3}).pointerId", "t")
            .unwrap(),
        "3"
    );
}

#[test]
fn focus_and_blur_bookkeeping() {
    let mut rt = runtime();
    rt.eval(
        "var el = document.createElement('input'); document.body.appendChild(el); el.focus()",
        "t",
    )
    .unwrap();
    assert_eq!(
        rt.eval("document.activeElement === el", "t").unwrap(),
        "true"
    );
    rt.eval("el.blur()", "t").unwrap();
    assert_eq!(rt.eval("document.activeElement", "t").unwrap(), "null");
}

#[test]
fn scroll_into_view_is_noop() {
    let mut rt = runtime();
    rt.eval(
        "var el = document.createElement('div'); el.scrollIntoView()",
        "t",
    )
    .unwrap();
    assert_eq!(rt.eval("1", "t").unwrap(), "1");
}

#[test]
fn structured_clone_subset() {
    let mut rt = runtime();
    assert_eq!(rt.eval("var o = {a:1,b:[2,3],c:{d:4}}; var c = structuredClone(o); c.a = 9; o.a + ':' + c.b[1]", "t").unwrap(), "1:3");
    assert_eq!(
        rt.eval(
            "var m = new Map([['k',1]]); structuredClone(m).get('k')",
            "t"
        )
        .unwrap(),
        "1"
    );
}

#[test]
fn base64_roundtrip() {
    let mut rt = runtime();
    assert_eq!(rt.eval("btoa('Megi')", "t").unwrap(), "TWVnaQ==");
    assert_eq!(rt.eval("atob('TWVnaQ==')", "t").unwrap(), "Megi");
}

#[test]
fn no_default_stub_still_compiles() {
    let _ = Budget::default();
}

#[test]
fn url_to_string_and_params_iteration() {
    let mut rt = runtime();
    assert_eq!(
        rt.eval("new URL('https://e.com/x?a=1').toString()", "t")
            .unwrap(),
        "https://e.com/x?a=1"
    );
    assert_eq!(rt.eval("var p = new URLSearchParams('a=1&b=2'); var out = []; p.forEach(function(v,k){ out.push(k + '=' + v); }); out.join(',')", "t").unwrap(), "a=1,b=2");
    assert_eq!(
        rt.eval("new URLSearchParams('a=1&a=2').getAll('a').join(',')", "t")
            .unwrap(),
        "1,2"
    );
}

#[test]
fn document_normalize_and_disconnected_bits() {
    let mut rt = runtime();
    rt.eval("var p = document.createElement('p'); p.appendChild(document.createTextNode('x')); p.appendChild(document.createTextNode('y')); document.body.appendChild(p); document.body.normalize()", "t").unwrap();
    assert_eq!(rt.eval("p.childNodes.length", "t").unwrap(), "1");
    assert_eq!(
        rt.eval(
            "document.createElement('i').compareDocumentPosition(document.createElement('b'))",
            "t"
        )
        .unwrap(),
        "33"
    );
}

#[test]
fn idle_callback_can_be_cancelled() {
    let mut rt = runtime();
    rt.eval("window.__idle2 = 0; var id = requestIdleCallback(function(){ __idle2 = 1; }); cancelIdleCallback(id)", "t").unwrap();
    rt.run_tasks(64).expect("tasks");
    assert_eq!(rt.eval("__idle2", "t").unwrap(), "0");
}

#[test]
fn layout_provider_geometry() {
    use js::host::{LayoutProvider, LayoutRect, ViewportInfo};
    let mut rt = runtime();
    let provider: LayoutProvider = Box::new(|_node| {
        Some(LayoutRect {
            border_box: (10.0, 20.0, 100.0, 50.0),
            content_box: (12.0, 22.0, 96.0, 46.0),
        })
    });
    rt.set_layout_provider(provider);
    rt.set_viewport_provider(Box::new(|| ViewportInfo {
        width: 800,
        height: 600,
        device_pixel_ratio: 2.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    }));
    rt.eval(
        "var el = document.createElement('div'); document.body.appendChild(el)",
        "t",
    )
    .unwrap();
    assert_eq!(rt.eval("el.getBoundingClientRect().x + ',' + el.getBoundingClientRect().width + ',' + el.getBoundingClientRect().right", "t").unwrap(), "10,100,110");
    assert_eq!(
        rt.eval("el.offsetWidth + ',' + el.offsetHeight", "t")
            .unwrap(),
        "100,50"
    );
    assert_eq!(
        rt.eval("el.clientWidth + ',' + el.clientHeight", "t")
            .unwrap(),
        "96,46"
    );
    assert_eq!(
        rt.eval(
            "window.innerWidth + ',' + window.innerHeight + ',' + window.devicePixelRatio",
            "t"
        )
        .unwrap(),
        "800,600,2"
    );
}

#[test]
fn geometry_without_provider_is_zero() {
    let mut rt = runtime();
    rt.eval("var el = document.createElement('div')", "t")
        .unwrap();
    assert_eq!(
        rt.eval(
            "el.getBoundingClientRect().width + ':' + el.offsetWidth + ':' + el.clientWidth",
            "t"
        )
        .unwrap(),
        "0:0:0"
    );
    assert_eq!(rt.eval("window.innerWidth", "t").unwrap(), "1280"); // documented default without a provider
}
