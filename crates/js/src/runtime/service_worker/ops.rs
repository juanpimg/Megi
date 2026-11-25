//! Lower capability layer for Service Workers phase 1 (M4).
//!
//! Follows the `CanvasOps` pattern (ADR-006): a `Copy` slot of non-capturing
//! function pointers installed in Boa's `HostDefined` data. `Host` remains the
//! single owner of mutable state; the code that reaches it lives in closures
//! installed by `runtime::globals::install`, so this module and the bindings
//! that call it never name `Host`, `host_cell` or `JsRuntime`. That keeps them
//! out of the pre-existing `events → host → runtime` cycle.
//!
//! Spec anchors: WHATWG Service Workers §4/§5 and W3C Cache API §5/§5.4.

use std::collections::HashMap;

use boa_engine::Context;

/// Same-origin GET request passed to the active worker's `fetch` event (SW §5.2).
#[derive(Clone, Debug)]
pub(crate) struct SwRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
}

/// One cached response, flattened from the JS `Response` shim (Cache API §5.4).
/// Phase 1 stores UTF-8 text bodies; binary bodies are normalized through text.
#[derive(Clone, Debug)]
pub(crate) struct CachedResponse {
    pub status: u16,
    pub status_text: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl CachedResponse {
    /// Adapts a stored response to the shape `new Response(raw)` expects.
    pub(crate) fn into_raw(self, context: &mut Context) -> boa_engine::JsValue {
        use boa_engine::js_string;
        use boa_engine::object::ObjectInitializer;
        use boa_engine::object::builtins::JsArray;
        use boa_engine::property::Attribute;

        let pairs: Vec<boa_engine::JsValue> = self
            .headers
            .iter()
            .map(|(name, value)| {
                boa_engine::JsValue::from(JsArray::from_iter(
                    [
                        boa_engine::JsValue::from(js_string!(name.clone())),
                        boa_engine::JsValue::from(js_string!(value.clone())),
                    ],
                    context,
                ))
            })
            .collect();
        let headers = boa_engine::JsValue::from(JsArray::from_iter(pairs, context));
        let mut init = ObjectInitializer::new(context);
        init.property(
            js_string!("status"),
            boa_engine::JsValue::from(f64::from(self.status)),
            Attribute::all(),
        )
        .property(
            js_string!("statusText"),
            boa_engine::JsValue::from(js_string!(self.status_text)),
            Attribute::all(),
        )
        .property(
            js_string!("url"),
            boa_engine::JsValue::from(js_string!(self.url)),
            Attribute::all(),
        )
        .property(js_string!("headers"), headers, Attribute::all())
        .property(
            js_string!("bodyText"),
            boa_engine::JsValue::from(js_string!(String::from_utf8_lossy(&self.body).to_string())),
            Attribute::all(),
        )
        .property(
            js_string!("bodyLength"),
            boa_engine::JsValue::from(self.body.len() as f64),
            Attribute::all(),
        );
        boa_engine::JsValue::from(init.build())
    }
}

/// WHATWG SW §4.2: the two life-cycle states modelled in phase 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwState {
    Installing,
    Activated,
}

impl SwState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Installing => "installing",
            Self::Activated => "activated",
        }
    }
}

/// Data-only registration; the realm itself lives in the host's worker table.
#[derive(Clone, Debug)]
pub(crate) struct SwRegistration {
    pub script_url: String,
    pub scope: String,
    pub state: SwState,
    pub worker_id: u32,
}

/// In-memory CacheStorage: cache name → insertion-ordered `(key, response)`.
pub(crate) type CacheStore = HashMap<String, Vec<(String, CachedResponse)>>;

/// Bounded caches for untrusted scripts (W3C Cache API §5).
pub(crate) const MAX_CACHE_ENTRIES: usize = 512;

/// Narrow capability surface installed once per document realm.
///
/// Every callback is a plain `fn` pointer: no captures, no borrows, `Copy`.
#[derive(Clone, Copy)]
pub(crate) struct SwOps {
    /// `(document URL, opaque origin)` for same-origin checks (SW §4.1.1).
    pub document: fn(&Context) -> (String, bool),
    /// Resolves a URL against the document URL.
    pub resolve_url: fn(&Context, &str) -> String,
    /// Loads the worker script source (host owns all I/O).
    pub load_script: fn(&mut Context, &str) -> Result<String, String>,
    /// Creates a fresh worker realm, returning its id in the host worker table.
    pub create_realm: fn(&mut Context) -> Result<u32, String>,
    /// Installs fetch/Response + SW natives/prelude inside a worker realm.
    pub prepare_realm: fn(&mut Context, u32, &str) -> Result<(), String>,
    /// Evaluates source inside a worker realm.
    pub eval_realm: fn(&mut Context, u32, &str, &str) -> Result<(), String>,
    /// Runs pending promise jobs inside a worker realm.
    pub pump_realm: fn(&mut Context, u32) -> Result<(), String>,
    /// Fires `install`/`activate` in a worker realm.
    pub fire_lifecycle: fn(&mut Context, u32, &str) -> Result<(), String>,
    /// Fires `fetch` with the given request in a worker realm.
    pub fire_fetch: fn(&mut Context, u32, &SwRequest) -> Result<(), String>,
    /// Takes the response captured by `respondWith` (SW §5.2).
    pub take_captured: fn(&mut Context, u32) -> Option<CachedResponse>,
    /// Reads the current registration, if any.
    pub registration: fn(&Context) -> Option<SwRegistration>,
    /// Takes the registration out of the host for the duration of a dispatch.
    pub take_registration: fn(&mut Context) -> Option<SwRegistration>,
    /// Puts the registration back into the host.
    pub put_registration: fn(&mut Context, SwRegistration),
    /// Writes one entry into a cache in the current (worker) realm.
    pub cache_put: fn(&mut Context, &str, &str, CachedResponse) -> bool,
    /// Reads one entry from a cache in the current (worker) realm.
    pub cache_match: fn(&Context, &str, &str) -> Option<CachedResponse>,
    /// Script URL of the current worker realm.
    pub realm_url: fn(&Context) -> String,
    /// Stores the response captured by `respondWith` in the worker realm.
    pub set_captured: fn(&mut Context, CachedResponse),
}

impl SwOps {
    /// Installs the capability slot in `context` (once, during realm setup).
    pub(crate) fn install(context: &mut Context, ops: Self) {
        let _ = context.insert_data(ops);
    }

    /// Copies the installed capabilities out of `context`.
    pub(crate) fn from_context(context: &Context) -> Option<Self> {
        context.get_data::<Self>().copied()
    }
}

/// Copies the installed capabilities or raises a typed Boa error.
pub(crate) fn require_ops(context: &Context) -> boa_engine::JsResult<SwOps> {
    SwOps::from_context(context).ok_or_else(|| {
        boa_engine::JsNativeError::error()
            .with_message("service worker capability slot is missing")
            .into()
    })
}

/// RFC-like origin comparison for same-origin checks (WHATWG SW §4.1.1).
pub(crate) fn is_same_origin(a: &str, b: &str) -> bool {
    let origin = |url: &str| {
        let (scheme, rest) = url.split_once("://")?;
        let authority = rest.split(['/', '?', '#']).next()?;
        if authority.is_empty() {
            return None;
        }
        Some((scheme.to_ascii_lowercase(), authority.to_ascii_lowercase()))
    };
    match (origin(a), origin(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    }
}

/// Typed JS error for invalid binding arguments (kept local: no runtime helpers).
pub(crate) fn js_error(message: impl Into<String>) -> boa_engine::JsError {
    boa_engine::JsNativeError::typ()
        .with_message(message.into())
        .into()
}

/// Reads one string argument (kept local: no runtime helpers).
pub(crate) fn arg_string(
    args: &[boa_engine::JsValue],
    index: usize,
    context: &mut Context,
) -> boa_engine::JsResult<String> {
    let value = args
        .get(index)
        .cloned()
        .unwrap_or(boa_engine::JsValue::undefined());
    value
        .to_string(context)
        .map(|text| text.to_std_string_escaped())
        .map_err(|error| js_error(error.to_string()))
}
