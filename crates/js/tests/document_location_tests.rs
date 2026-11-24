//! #2245: document.location must be available (paywall scripts read .protocol).

#![cfg(feature = "js")]

use megi_js::JsRuntime;

#[test]
fn document_location_exposes_protocol() {
    let mut rt = JsRuntime::new().expect("runtime");
    let out = rt.eval("typeof document.location", "t.js");
    assert!(out.is_ok(), "{:?}", out.err());
    assert_eq!(out.unwrap(), "object");
}

#[test]
fn location_hostname_is_exposed() {
    let mut rt = JsRuntime::new().expect("runtime");
    let out = rt.eval("typeof location.hostname", "t.js");
    assert!(out.is_ok(), "{:?}", out.err());
    assert_eq!(out.unwrap(), "string");
}
