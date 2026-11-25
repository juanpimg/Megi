//! JS prelude for the `ServiceWorkerGlobalScope` realm (WHATWG SW §5).

/// The worker-scope shim evaluated after the fetch/Response prelude.
pub(crate) const SW_PRELUDE: &str = include_str!("sw_prelude.js");
