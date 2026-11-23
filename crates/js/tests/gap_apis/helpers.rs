//! Shared runtime helper.

#![cfg(feature = "js")]

pub(super) use megi_js::runtime::{Budget, JsRuntime};
pub(super) use std::sync::Arc;

pub(super) fn runtime() -> JsRuntime {
    JsRuntime::new().expect("runtime")
}
