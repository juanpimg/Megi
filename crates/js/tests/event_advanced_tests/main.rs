//! Advanced event tests (task-80): real `currentTarget`, `once`/`passive`,
//! inline `onclick`/`onload`/`onerror`, `document.close`, memory estimate and
//! dispatch robustness.

#![cfg(feature = "js")]

use megi_js::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

mod animations;
mod dispatch;
mod inline;
mod ui_events;
