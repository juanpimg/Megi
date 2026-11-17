//! Round 24: History API, location, Web Storage and MutationObserver.

#![cfg(feature = "js")]

use std::sync::{Arc, Mutex};

use dom::Document;
use js::host::{HistoryAction, LocationAction, StorageKind, StorageProvider};
use js::runtime::JsRuntime;

#[test]
fn history_provider_push_replace_back_forward_go() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let actions = Arc::new(Mutex::new(Vec::<HistoryAction>::new()));
    let seen = Arc::clone(&actions);
    runtime.set_history_provider(Box::new(move |action| {
        seen.lock().expect("lock").push(action)
    }));
    runtime
        .eval("history.pushState({a:1}, '', '/one')", "t")
        .expect("push");
    runtime
        .eval("history.replaceState({b:2}, '', '/two?x=1')", "t")
        .expect("replace");
    runtime
        .eval("history.back(); history.forward(); history.go(-2)", "t")
        .expect("travel");
    assert_eq!(runtime.eval("history.length", "t").expect("length"), "2");
    let state = runtime
        .eval("JSON.stringify(history.state)", "t")
        .expect("state");
    assert!(state.contains("\"b\":2"), "state = {state}");
    let actions = actions.lock().expect("lock");
    assert!(
        matches!(&actions[0], HistoryAction::Push { state, url } if state.contains("\"a\":1") && url.as_deref() == Some("/one")),
        "{actions:?}"
    );
    assert!(
        matches!(&actions[1], HistoryAction::Replace { state, .. } if state.contains("\"b\":2"))
    );
    assert!(matches!(actions[2], HistoryAction::Back));
    assert!(matches!(actions[3], HistoryAction::Forward));
    assert!(matches!(actions[4], HistoryAction::Go(-2)));
}

#[test]
fn history_without_provider_is_noop() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval("history.pushState({x:1}, '', '/ignored')", "t")
        .expect("push");
    assert_eq!(runtime.eval("history.length", "t").expect("length"), "1");
    assert_eq!(runtime.eval("history.state", "t").expect("state"), "null");
}

#[test]
fn location_properties_and_navigation_provider() {
    let mut runtime = JsRuntime::with_document(
        Document::new(),
        "https://example.com/a/b?x=1#frag".to_string(),
    )
    .expect("runtime");
    assert_eq!(runtime.eval("location.protocol", "t").unwrap(), "https:");
    assert_eq!(runtime.eval("location.host", "t").unwrap(), "example.com");
    assert_eq!(
        runtime.eval("location.origin", "t").unwrap(),
        "https://example.com"
    );
    assert_eq!(runtime.eval("location.pathname", "t").unwrap(), "/a/b");
    assert_eq!(runtime.eval("location.search", "t").unwrap(), "?x=1");
    assert_eq!(runtime.eval("location.hash", "t").unwrap(), "#frag");
    let actions = Arc::new(Mutex::new(Vec::<LocationAction>::new()));
    let seen = Arc::clone(&actions);
    runtime.set_location_provider(Box::new(move |action| {
        seen.lock().expect("lock").push(action)
    }));
    runtime
        .eval("location.assign('/next'); location.reload()", "t")
        .expect("navigate");
    assert_eq!(runtime.eval("location.pathname", "t").unwrap(), "/next");
    let actions = actions.lock().expect("lock");
    assert!(
        matches!(&actions[0], LocationAction::Assign(url) if url.ends_with("/next")),
        "{actions:?}"
    );
    assert!(matches!(actions[1], LocationAction::Reload));
}

#[test]
fn web_storage_api_and_isolation() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "localStorage.setItem('a','1'); localStorage.setItem('b','two')",
            "t",
        )
        .expect("set");
    runtime
        .eval("sessionStorage.setItem('a','session')", "t")
        .expect("session");
    assert_eq!(runtime.eval("localStorage.length", "t").unwrap(), "2");
    assert_eq!(
        runtime.eval("localStorage.getItem('b')", "t").unwrap(),
        "two"
    );
    assert_eq!(
        runtime.eval("sessionStorage.getItem('a')", "t").unwrap(),
        "session"
    );
    assert_eq!(
        runtime
            .eval("localStorage.getItem('missing')", "t")
            .unwrap(),
        "null"
    );
    assert!(!runtime.eval("localStorage.key(0)", "t").unwrap().is_empty());
    runtime
        .eval("localStorage.removeItem('a'); sessionStorage.clear()", "t")
        .expect("remove");
    assert_eq!(runtime.eval("localStorage.length", "t").unwrap(), "1");
    assert_eq!(runtime.eval("sessionStorage.length", "t").unwrap(), "0");
}

#[test]
fn web_storage_quota_exceeded() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let error = runtime
        .eval(
            "localStorage.setItem('big', 'x'.repeat(1024 * 1024 + 1))",
            "t",
        )
        .expect_err("quota");
    assert!(error.to_string().contains("QuotaExceededError"), "{error}");
    assert_eq!(runtime.eval("localStorage.length", "t").unwrap(), "0");
}

#[test]
fn storage_provider_loads_and_saves() {
    let saved = Arc::new(Mutex::new(
        Vec::<(StorageKind, Vec<(String, String)>)>::new(),
    ));
    let saved_clone = Arc::clone(&saved);
    let provider = StorageProvider {
        load: Box::new(|kind| {
            if kind == StorageKind::Local {
                vec![("persisted".to_string(), "yes".to_string())]
            } else {
                Vec::new()
            }
        }),
        save: Box::new(move |kind, entries| {
            saved_clone
                .lock()
                .expect("lock")
                .push((kind, entries.to_vec()));
        }),
    };
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_storage_provider(provider);
    assert_eq!(
        runtime
            .eval("localStorage.getItem('persisted')", "t")
            .unwrap(),
        "yes"
    );
    runtime
        .eval("localStorage.setItem('new', 'value')", "t")
        .expect("set");
    let saved = saved.lock().expect("lock");
    assert!(!saved.is_empty());
    assert!(
        saved
            .iter()
            .any(|(kind, entries)| *kind == StorageKind::Local
                && entries.iter().any(|(k, v)| k == "new" && v == "value"))
    );
}

#[test]
fn mutation_observer_child_list_delivers_on_microtasks() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "window.__records = [];
             var obs = new MutationObserver(function (records) {
                 for (var i = 0; i < records.length; i++) __records.push(records[i].type);
             });
             obs.observe(document.body, { childList: true });
             var p = document.createElement('p');
             document.body.appendChild(p);",
            "t",
        )
        .expect("observe");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__records.join(',')", "t").unwrap(),
        "childList"
    );
}

#[test]
fn mutation_observer_take_records_and_disconnect() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "window.__records = [];
             var obs2 = new MutationObserver(function (records) { __records.push(records.length); });
             obs2.observe(document.body, { attributes: true, subtree: true });
             document.body.setAttribute('data-x', '1');",
            "t",
        )
        .expect("observe");
    assert_eq!(runtime.eval("obs2.takeRecords().length", "t").unwrap(), "1");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__records.length", "t").unwrap(),
        "0",
        "takeRecords drained the queue"
    );
    runtime
        .eval(
            "obs2.disconnect(); document.body.setAttribute('data-x', '2')",
            "t",
        )
        .expect("disconnect");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("__records.length", "t").unwrap(), "0");
}

#[test]
fn mutation_observer_subtree_attributes() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "window.__types = [];
             var obs3 = new MutationObserver(function (records) { for (var i = 0; i < records.length; i++) __types.push(records[i].type + ':' + records[i].attributeName); });
             var child = document.createElement('span');
             document.body.appendChild(child);
             obs3.observe(document.body, { attributes: true, subtree: true });
             child.setAttribute('data-k', 'v');",
            "t",
        )
        .expect("observe");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__types.join('|')", "t").unwrap(),
        "attributes:data-k"
    );
}

#[test]
fn mutation_observer_character_data() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "window.__cd = [];
             var obs4 = new MutationObserver(function (records) { __cd.push(records[0].type); });
             var p = document.createElement('p');
             document.body.appendChild(p);
             p.textContent = 'hello';
             obs4.observe(p, { characterData: true, subtree: true });",
            "t",
        )
        .expect("observe");
    // Observe before the mutation so the record is queued.
    runtime
        .eval("p.textContent = 'world'", "t")
        .expect("mutate");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__cd.join(',')", "t").unwrap(),
        "characterData"
    );
}

#[test]
fn mutation_observer_observe_replaces_options_for_the_same_node() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "window.__types = [];
             var obs = new MutationObserver(function (records) {
                 for (var i = 0; i < records.length; i++) __types.push(records[i].type);
             });
             // Re-observing the same node updates its options instead of adding a second
             // registration (DOM spec, `observe()`). With two entries the childList mutation
             // below would be delivered as well (#437).
             obs.observe(document.body, { childList: true });
             obs.observe(document.body, { attributes: true });
             document.body.appendChild(document.createElement('p'));
             document.body.setAttribute('data-x', '1');",
            "t",
        )
        .expect("observe");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__types.join(',')", "t").unwrap(),
        "attributes"
    );
}

#[test]
fn mutation_observer_target_limit_is_enforced() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let error = runtime
        .eval(
            "var obs = new MutationObserver(function () {});
             for (var i = 0; i < 2048; i++) {
                 obs.observe(document.createElement('div'), { attributes: true });
             }",
            "t",
        )
        .expect_err("observer target cap");
    let message = error.to_string();
    assert!(
        message.contains("MutationObserver target limit"),
        "unexpected error: {message}"
    );
}

#[test]
fn history_replace_keeps_length_and_updates_state() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_history_provider(Box::new(|_| {}));
    runtime
        .eval("history.pushState({n:1}, '', '/a')", "t")
        .expect("push");
    runtime
        .eval("history.replaceState({n:2}, '', '/b')", "t")
        .expect("replace");
    assert_eq!(runtime.eval("history.length", "t").unwrap(), "2");
    assert!(
        runtime
            .eval("JSON.stringify(history.state)", "t")
            .unwrap()
            .contains("\"n\":2")
    );
}

#[test]
fn location_relative_url_and_hash_navigation() {
    let mut runtime =
        JsRuntime::with_document(Document::new(), "https://example.com/docs/page".to_string())
            .expect("runtime");
    runtime.set_location_provider(Box::new(|_| {}));
    runtime
        .eval("location.assign('other?q=1#x')", "t")
        .expect("assign");
    assert_eq!(
        runtime.eval("location.pathname", "t").unwrap(),
        "/docs/other"
    );
}

#[test]
fn mutation_observer_ignores_a_target_freed_before_delivery() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "window.__delivered = 0;
             var obs = new MutationObserver(function (records) { __delivered += records.length; });
             obs.observe(document.body, { attributes: true, subtree: true });
             var d = document.createElement('div');
             document.body.appendChild(d);
             d.setAttribute('x', '1');
             d.outerHTML = '<span></span>';   // frees d; the record target is now stale
             __delivered += obs.takeRecords().length;
             document.body.setAttribute('y', '2');",
            "t",
        )
        .expect("mutations");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("__delivered", "t").expect("delivered"),
        "1",
        "only the live mutation is delivered"
    );
}

#[test]
fn history_state_is_capped_like_storage() {
    // #663: `history.pushState` retained an arbitrarily large state in Rust, rooted it as a JS
    // value and copied it to the provider, with no quota at all.
    let mut runtime = JsRuntime::new().expect("runtime");
    let error = runtime
        .eval(
            "window.__err = ''; try { history.pushState({big: 'x'.repeat(2 * 1024 * 1024)}, '', '/big'); } catch (error) { __err = String(error); } __err",
            "t",
        )
        .expect("eval");
    assert!(
        error.contains("QuotaExceededError"),
        "an oversized state must be refused: {error}"
    );
    // The rejected call must not change the current entry.
    assert_eq!(runtime.eval("history.state", "t").unwrap(), "null");
    // A state under the limit still works.
    runtime
        .eval(
            "history.pushState({ok: 'x'.repeat(1024 * 1024 - 64)}, '', '/ok')",
            "t",
        )
        .expect("a state under the quota is accepted");
}

#[test]
fn mutation_observer_cap_throws_range_error_above_limit() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var count = 0;
             var lastErr = '';
             for (var i = 0; i < 1030; i++) {
                 try {
                     new MutationObserver(function () {});
                     count++;
                 } catch (err) {
                     lastErr = err.name + ':' + err.message;
                     break;
                 }
             }
             count + '|' + lastErr",
            "t",
        )
        .expect("eval");
    assert!(
        out.starts_with("1024|RangeError:"),
        "expected cap at 1024 with RangeError: {out}"
    );
}

#[test]
fn mutation_observer_scales_with_depth_not_unrelated_observers() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "window.__unrelated = 0;
             window.__matched = 0;
             for (var i = 0; i < 1023; i++) {
                 var detached = document.createElement('div');
                 var o = new MutationObserver(function (records) { window.__unrelated += records.length; });
                 o.observe(detached, { childList: true, attributes: true, subtree: true });
             }
             var target = document.createElement('div');
             var cur = document.body;
             for (var d = 0; d < 32; d++) {
                 var next = document.createElement('div');
                 cur.appendChild(next);
                 cur = next;
             }
             cur.appendChild(target);
             var live = new MutationObserver(function (records) { window.__matched += records.length; });
             live.observe(target, { attributes: true, attributeOldValue: true });",
            "t",
        )
        .expect("setup 1024 observers");
    let started = std::time::Instant::now();
    runtime
        .eval(
            "for (var m = 0; m < 500; m++) {
                 target.setAttribute('data-n', String(m));
             }",
            "t",
        )
        .expect("500 mutations");
    let elapsed = started.elapsed();
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(runtime.eval("window.__unrelated", "t").unwrap(), "0");
    assert_eq!(runtime.eval("window.__matched", "t").unwrap(), "500");
    // Even on heavily loaded CI runners, 500 indexed mutations take a few milliseconds;
    // keep a very generous ceiling (scaled via MEGI_PERF_BUDGET_SCALE) to avoid flakiness.
    let scale = std::env::var("MEGI_PERF_BUDGET_SCALE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(1.0)
        .clamp(0.1, 1000.0);
    let max_budget = std::time::Duration::from_secs_f64(2.0 * scale);
    assert!(
        elapsed < max_budget,
        "500 mutations with 1023 unrelated subtree observers took {elapsed:?} (budget {max_budget:?})"
    );
}

#[test]
fn mutation_observer_remove_attribute_node_records_old_value() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime
        .eval(
            "window.__records = [];
             var el = document.createElement('div');
             el.setAttribute('data-role', 'initial');
             var obs = new MutationObserver(function (records) {
                 for (var i = 0; i < records.length; i++) {
                     window.__records.push(records[i].type + ':' + records[i].attributeName + ':' + records[i].oldValue);
                 }
             });
             obs.observe(el, { attributes: true, attributeOldValue: true });
             var attr = el.getAttributeNode('data-role');
             var removed = el.removeAttributeNode(attr);
             window.__removed = removed ? removed.name + '=' + removed.value : 'none';",
            "t",
        )
        .expect("eval");
    runtime.run_microtasks().expect("microtasks");
    assert_eq!(
        runtime.eval("window.__removed", "t").unwrap(),
        "data-role=initial"
    );
    assert_eq!(
        runtime.eval("window.__records.join('|')", "t").unwrap(),
        "attributes:data-role:initial"
    );
}
