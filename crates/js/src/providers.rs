//! Immutable host providers stored in the Boa [`boa_engine::Context`].
//!
//! The bridge keeps the mutable [`crate::host::Host`] state and the *providers* in two
//! separate slots. Providers are `Arc<dyn Fn…>` closures injected by the embedding host
//! (navigation, storage persistence, module loading, layout, …). They are:
//!
//! - **Immutable**: installed once with [`install_providers`] and only ever read through
//!   [`providers`]; the closures may be shared (`Arc`) because they are never mutated.
//! - **Called outside any host borrow**: a provider may re-enter JavaScript (for example a
//!   storage `save` callback that dispatches an event). Invoking it while a `Ref`/`RefMut` of
//!   the host is alive would risk a double borrow, so bindings must drop their host borrow
//!   before calling a provider. The provider slot makes it possible to obtain `&HostProviders`
//!   from a shared `&Context`, independently of the host cell.
//!
//! Each alias mirrors the `Box<dyn Fn…>` alias of the same name in [`crate::host`], with
//! `Arc` in place of `Box`. The one exception is [`WebSocketProvider`]: its *return* value is
//! a `Box<dyn WebSocketTransport>` because a transport needs `&mut self` for `send`/`poll`/
//! `close` and therefore cannot be shared through an `Arc`.

use std::sync::Arc;

use megi_core::{Document, NodeId};

use crate::host::{
    FetchRequest, FetchResult, HistoryAction, LayoutRect, LocationAction, StorageKind,
    ViewportInfo, WebSocketTransport,
};

/// Evaluates a CSS media query string (`window.matchMedia`).
pub type MediaProvider = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Reports inner viewport dimensions / DPR / scroll base.
pub type ViewportProvider = Arc<dyn Fn() -> ViewportInfo + Send + Sync>;

/// Supplies computed-style declarations for a node (`window.getComputedStyle`).
pub type ComputedStyleProvider =
    Arc<dyn Fn(&Document, NodeId) -> Vec<(String, String)> + Send + Sync>;

/// Forwards a History API action to the host.
pub type HistoryProvider = Arc<dyn Fn(HistoryAction) + Send + Sync>;

/// Forwards a `location.*` navigation action to the host.
pub type LocationProvider = Arc<dyn Fn(LocationAction) + Send + Sync>;

/// Executes one `fetch()` request on the host side.
pub type FetchProvider =
    Arc<dyn Fn(FetchRequest) -> Result<FetchResult, crate::error::JsError> + Send + Sync>;

/// Creates a WebSocket transport for a URL.
///
/// The transport itself stays boxed: it is owned by one JS handle and needs `&mut self`.
pub type WebSocketProvider =
    Arc<dyn Fn(&str) -> Result<Box<dyn WebSocketTransport>, crate::error::JsError> + Send + Sync>;

/// Loads the source text of a worker script URL.
pub type WorkerLoader = Arc<dyn Fn(&str) -> Result<String, crate::error::JsError> + Send + Sync>;

/// Loads HTML content for an external `<iframe src="...">` (`resolved_url`, `html`).
pub type FrameLoader = Arc<dyn Fn(&str) -> Result<(String, String), String> + Send + Sync>;

/// Reports the viewport-space rectangles of a node (`getBoundingClientRect` et al.).
pub type LayoutProvider = Arc<dyn Fn(NodeId) -> Option<LayoutRect> + Send + Sync>;

/// Resolves an ES module specifier against an importer URL.
pub type ModuleResolver =
    Arc<dyn Fn(&str, &str) -> Result<Option<String>, crate::error::JsError> + Send + Sync>;

/// Loads the source text of an already-resolved module URL.
pub type ModuleLoader = Arc<dyn Fn(&str) -> Result<String, crate::error::JsError> + Send + Sync>;

/// Loads the persisted entries of one Web Storage area.
pub type StorageLoadFn = Arc<dyn Fn(StorageKind) -> Vec<(String, String)> + Send + Sync>;

/// Saves the current entries of one Web Storage area (called after writes).
pub type StorageSaveFn = Arc<dyn Fn(StorageKind, &[(String, String)]) + Send + Sync>;

/// Reads the `document.cookie` header string for a document URL.
pub type CookieGetFn = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// Writes one `Set-Cookie`-style string for a document URL.
pub type CookieSetFn = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// The full set of host-provided callbacks, stored in the [`boa_engine::Context`] next to the host cell.
///
/// Every field is optional: a missing provider leaves the corresponding web API at its
/// documented fallback/no-op behaviour. All callbacks are immutable `Arc` closures so they
/// can be read from a shared [`providers`] borrow and safely re-entered outside a host borrow.
#[derive(Default)]
pub(crate) struct HostProviders {
    pub(crate) computed_style: Option<ComputedStyleProvider>,
    pub(crate) media: Option<MediaProvider>,
    pub(crate) viewport: Option<ViewportProvider>,
    pub(crate) history: Option<HistoryProvider>,
    pub(crate) location: Option<LocationProvider>,
    pub(crate) fetch: Option<FetchProvider>,
    pub(crate) websocket: Option<WebSocketProvider>,
    pub(crate) worker_loader: Option<WorkerLoader>,
    pub(crate) frame_loader: Option<FrameLoader>,
    pub(crate) layout: Option<LayoutProvider>,
    pub(crate) module_resolver: Option<ModuleResolver>,
    pub(crate) module_loader: Option<ModuleLoader>,
    pub(crate) storage_load: Option<StorageLoadFn>,
    pub(crate) storage_save: Option<StorageSaveFn>,
    pub(crate) cookie_get: Option<CookieGetFn>,
    pub(crate) cookie_set: Option<CookieSetFn>,
}

/// Installs the provider slot into `context`.
///
/// Called once while setting up a realm, before any script runs. Installing again replaces
/// the previous slot; the old providers are dropped.
pub(crate) fn install_providers(context: &mut boa_engine::Context, providers: HostProviders) {
    let _ = context.insert_data(providers);
}

/// Borrows the installed providers from `context`, if a slot was installed.
///
/// The returned reference is independent of the mutable host cell, so a binding can read a
/// provider, drop any host borrow it holds, and only then invoke the callback.
pub(crate) fn providers(context: &boa_engine::Context) -> Option<&HostProviders> {
    context.get_data::<HostProviders>()
}
