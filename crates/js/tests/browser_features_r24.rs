//! Round 24: History API, location, Web Storage and MutationObserver.

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
