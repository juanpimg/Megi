use std::sync::{Arc, Mutex};

use megi_core::Document;
use megi_js::host::{HistoryAction, LocationAction};
use megi_js::runtime::JsRuntime;

#[test]
fn history_provider_push_replace_back_forward_go() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let actions = Arc::new(Mutex::new(Vec::<HistoryAction>::new()));
    let seen = Arc::clone(&actions);
    runtime.set_history_provider(Arc::new(move |action| {
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
    runtime.set_location_provider(Arc::new(move |action| {
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
fn history_replace_keeps_length_and_updates_state() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_history_provider(Arc::new(|_| {}));
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
    runtime.set_location_provider(Arc::new(|_| {}));
    runtime
        .eval("location.assign('other?q=1#x')", "t")
        .expect("assign");
    assert_eq!(
        runtime.eval("location.pathname", "t").unwrap(),
        "/docs/other"
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
fn history_state_urls_must_be_same_origin() {
    // #2339 / WHATWG HTML §7.8.1: pushState/replaceState throw SecurityError for
    // a URL whose origin differs from the document's, and must not mutate the URL.
    let mut runtime = JsRuntime::with_document(
        Document::new(),
        "https://example.com/a/b?x=1#frag".to_string(),
    )
    .expect("runtime");
    let actions = Arc::new(Mutex::new(Vec::<HistoryAction>::new()));
    let seen = Arc::clone(&actions);
    runtime.set_history_provider(Arc::new(move |action| {
        seen.lock().expect("lock").push(action)
    }));

    let script = r#"
        window.__crossOriginError = '';
        try { history.pushState({}, '', 'https://other.example/x'); }
        catch (error) { __crossOriginError = String(error); }
        window.__urlAfterPush = location.href;
        try { history.replaceState({}, '', 'https://other.example/y'); }
        catch (error) { __crossOriginError += '|' + String(error); }
        [__crossOriginError, __urlAfterPush, location.href].join('~')
    "#;
    let outcome = runtime.eval(script, "t").expect("eval");
    let parts: Vec<&str> = outcome.split('~').collect();
    assert_eq!(parts.len(), 3, "outcome = {outcome}");
    assert!(
        parts[0].contains("SecurityError") && parts[0].contains('|'),
        "both calls must throw SecurityError: {}",
        parts[0]
    );
    assert_eq!(
        parts[1], "https://example.com/a/b?x=1#frag",
        "pushState must not mutate the URL"
    );
    assert_eq!(
        parts[2], "https://example.com/a/b?x=1#frag",
        "replaceState must not mutate the URL"
    );

    // A same-origin URL still succeeds and updates the document URL.
    runtime
        .eval("history.pushState({}, '', '/c')", "t")
        .expect("same-origin push");
    assert_eq!(
        runtime.eval("location.href", "t").unwrap(),
        "https://example.com/c"
    );

    // No cross-origin navigation reached the provider.
    let actions = actions.lock().expect("lock");
    assert!(
        actions.iter().all(|action| matches!(
            action,
            HistoryAction::Push { url, .. } if url.as_deref() == Some("https://example.com/c")
        )),
        "only the same-origin push may reach the provider: {actions:?}"
    );
}
