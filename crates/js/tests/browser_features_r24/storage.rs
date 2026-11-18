use std::sync::{Arc, Mutex};

use megi_js::host::{StorageKind, StorageProvider};
use megi_js::runtime::JsRuntime;

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
        load: Arc::new(|kind| {
            if kind == StorageKind::Local {
                vec![("persisted".to_string(), "yes".to_string())]
            } else {
                Vec::new()
            }
        }),
        save: Arc::new(move |kind, entries| {
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
