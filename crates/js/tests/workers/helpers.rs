//! Shared helpers for the Web Worker integration tests.

#![cfg(feature = "js")]

pub(super) use megi_js::runtime::JsRuntime;
pub(super) use std::sync::Arc;

pub(super) fn runtime_with_worker(source: &'static str) -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_worker_loader(Arc::new(move |url| {
        if url.ends_with("worker.js") {
            Ok(source.to_string())
        } else {
            Err(megi_js::JsError::Runtime(format!("not found: {url}")))
        }
    }));
    runtime
}

pub(super) const SUM: &str = "onmessage = function (e) { postMessage(e.data.a + e.data.b); };";
