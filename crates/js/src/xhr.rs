//! `XMLHttpRequest` and `ProgressEvent` built on top of `__megi_fetch_native` (#1532).
//!
//! Covers the standard asynchronous `XMLHttpRequest` lifecycle so bundled libraries and scripts
//! that probe or use XHR before `fetch()` work cleanly against the host `FetchProvider`:
//! - States `UNSENT (0)`, `OPENED (1)`, `HEADERS_RECEIVED (2)`, `LOADING (3)`, `DONE (4)` on both
//!   the constructor and prototype
//! - `open(method, url, async?, user?, password?)` (rejects synchronous requests with
//!   `NotSupportedError` `DOMException`; resolves relative URLs against `location.href`)
//! - `setRequestHeader(name, value)` with RFC 9110 validation and forbidden-header filtering
//! - `overrideMimeType`, `withCredentials`, `timeout`, `upload` EventTarget, `abort()`
//! - `send(body?)` dispatching `loadstart`, `readystatechange` (`HEADERS_RECEIVED`, `LOADING`,
//!   `DONE`), `progress`, `load` / `error` / `abort` / `timeout`, and `loadend` asynchronously via
//!   microtask scheduling so `abort()` right after `send()` works deterministically
//! - `responseType` (`""`, `"text"`, `"json"`, `"arraybuffer"`), `response`, `responseText`,
//!   `status`, `statusText`, `responseURL`, `getResponseHeader`, `getAllResponseHeaders`

use boa_engine::{Context, Source};

use crate::error::JsError;

/// XMLHttpRequest JS shim injected verbatim as data files.
const XHR_PRELUDE: &str = concat!(
    include_str!("xhr/prelude_a.js"),
    include_str!("xhr/prelude_b.js"),
);

pub(crate) fn install_xhr(context: &mut Context) -> Result<(), JsError> {
    context
        .eval(Source::from_bytes(XHR_PRELUDE))
        .map(|_| ())
        .map_err(|error| JsError::Internal(error.to_string()))
}
