//! Tests for `document.cookie` backed by an injectable `CookieProvider` and fallback store.

#![cfg(feature = "js")]

use std::sync::{Arc, Mutex};

use js::{CookieProvider, JsRuntime};

#[test]
fn document_cookie_delegates_to_cookie_provider() {
    let mut runtime = JsRuntime::with_document(
        dom::Document::new(),
        "https://app.example.test/checkout".to_owned(),
    )
    .expect("runtime");

    let seen_reads = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen_writes = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let store = Arc::new(Mutex::new(String::from("prefs=dark")));

    let reads_clone = Arc::clone(&seen_reads);
    let writes_clone = Arc::clone(&seen_writes);
    let store_get = Arc::clone(&store);
    let store_set = Arc::clone(&store);

    runtime.set_cookie_provider(CookieProvider {
        get: Box::new(move |url| {
            reads_clone.lock().unwrap().push(url.to_owned());
            store_get.lock().unwrap().clone()
        }),
        set: Box::new(move |url, value| {
            writes_clone
                .lock()
                .unwrap()
                .push((url.to_owned(), value.to_owned()));
            let pair = value.split(';').next().unwrap_or("").trim();
            let mut s = store_set.lock().unwrap();
            if !s.is_empty() {
                s.push_str("; ");
            }
            s.push_str(pair);
        }),
    });

    let result = runtime
        .eval(
            r#"
            const initial = document.cookie;
            document.cookie = "consent=true; Path=/; SameSite=Lax";
            initial + "|" + document.cookie;
            "#,
            "cookie.js",
        )
        .expect("eval");

    assert_eq!(result, "prefs=dark|prefs=dark; consent=true");
    assert_eq!(seen_reads.lock().unwrap().len(), 2);
    assert_eq!(
        seen_writes.lock().unwrap().as_slice(),
        &[(
            "https://app.example.test/checkout".to_owned(),
            "consent=true; Path=/; SameSite=Lax".to_owned()
        )]
    );
}

#[test]
fn document_cookie_standalone_fallback_updates_and_ignores_http_only() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let result = runtime
        .eval(
            r#"
            document.cookie = "a=1";
            document.cookie = "b=2; HttpOnly";
            document.cookie = "a=updated";
            document.cookie = "temp=x";
            document.cookie = "temp=; Max-Age=0";
            document.cookie;
            "#,
            "fallback.js",
        )
        .expect("eval");

    assert_eq!(result, "a=updated");
}
