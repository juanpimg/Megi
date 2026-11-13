//! Host state shared between the Rust runtime and the Boa native callbacks.
//!
//! The bridge is deliberately small and auditable:
//!
//! - **One realm per runtime/document.** [`crate::JsRuntime`] owns a [`dom::Document`] and a
//!   Boa `Context`; a script mutates the document through native functions.
//! - **Side table `NodeId` ↔ JS handle.** JS wrappers carry a numeric handle (index into
//!   the host side table); the actual generation-checked [`NodeId`] stays in Rust. Every access
//!   revalidates it with [`dom::Document::try_node`], so stale/removed nodes raise a typed
//!   JS error instead of panicking (handles are never dereferenced blindly).
//! - **Per-thread current host.** Boa native functions are plain `fn` pointers (no
//!   captures); the runtime publishes its host in a thread-local for the duration of an
//!   `eval`. Tests run in parallel threads, so each thread sees its own runtime.
//!
//! Limits documented for consumers: the side table keeps one entry per distinct node
//! handle ever exposed to JS (bounded by the DOM arena plus removed nodes until wrappers
//! are collected; a generational arena keeps the raw ids distinct). No network/FS APIs are
//! exposed to scripts.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use dom::{Document, NodeId};

/// One captured `console.*` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleEntry {
    /// `log`, `info`, `warn` or `error`.
    pub level: String,
    /// Space-joined stringification of the arguments.
    pub message: String,
}

/// Identity of an event target in the realm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TargetKey {
    Node(u32),
    Window,
    Document,
}

/// A registered event listener (its callback lives in the JS-side rooted table).
#[derive(Clone, Debug)]
pub(crate) struct Listener {
    pub target: TargetKey,
    pub event_type: String,
    pub capture: bool,
    pub once: bool,
    pub passive: bool,
    pub slot: u32,
}

/// An inline event handler property (`el.onclick = fn`) registered in the host.
#[derive(Clone, Debug)]
pub(crate) struct InlineHandler {
    pub target: TargetKey,
    pub event_type: String,
    pub slot: u32,
}

/// Dispatch state for the event currently being delivered.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DispatchState {
    pub prevent_default: bool,
    pub stop_propagation: bool,
    pub stop_immediate: bool,
    /// The listener currently running ignores `preventDefault` (`{passive: true}`).
    pub passive: bool,
}

/// A scheduled timer.
#[derive(Clone, Debug)]
pub(crate) struct TimerEntry {
    pub id: u64,
    pub due_ms: u64,
    pub interval_ms: Option<u64>,
    /// `requestAnimationFrame` callback: invoked with the current virtual timestamp and
    /// never rescheduled (`pending_animation_frames` counts them).
    pub animation: bool,
}

/// Viewport/scroll state exposed to scripts (`window.innerWidth`, page offsets, ...).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportInfo {
    /// Viewport width in CSS pixels (`window.innerWidth`).
    pub width: u32,
    /// Viewport height in CSS pixels (`window.innerHeight`).
    pub height: u32,
    /// Device pixel ratio (`window.devicePixelRatio`).
    pub device_pixel_ratio: f64,
    /// Horizontal scroll offset (`window.pageXOffset`).
    pub scroll_x: f64,
    /// Vertical scroll offset (`window.pageYOffset`).
    pub scroll_y: f64,
}

impl Default for ViewportInfo {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            device_pixel_ratio: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }
}

/// `matchMedia` provider injected by the host (evaluates a CSS media query string).
pub type MediaProvider = Box<dyn Fn(&str) -> bool + Send + Sync>;

/// Viewport provider injected by the host (inner dimensions / DPR / scroll base).
pub type ViewportProvider = Box<dyn Fn() -> ViewportInfo + Send + Sync>;

/// Computed-style provider injected by the host (`window.getComputedStyle`).
pub type ComputedStyleProvider =
    Box<dyn Fn(&Document, NodeId) -> Vec<(String, String)> + Send + Sync>;

/// History API action forwarded to the host provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryAction {
    /// `history.pushState(state, title, url)` (state is JSON; url already resolved).
    Push { state: String, url: Option<String> },
    /// `history.replaceState(...)`.
    Replace { state: String, url: Option<String> },
    /// `history.back()`.
    Back,
    /// `history.forward()`.
    Forward,
    /// `history.go(delta)`.
    Go(i64),
}

/// Provider invoked by the History API (no provider ⇒ documented no-op).
pub type HistoryProvider = Box<dyn Fn(HistoryAction) + Send + Sync>;

/// `location.assign/replace/reload` action forwarded to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocationAction {
    Assign(String),
    Replace(String),
    Reload,
}

/// Provider invoked by `location.*` navigation methods.
pub type LocationProvider = Box<dyn Fn(LocationAction) + Send + Sync>;

/// Which Web Storage area an operation refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageKind {
    Local,
    Session,
}

/// Viewport-space rectangle (CSS pixels) for one DOM node, as supplied by the host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRect {
    /// Border-box `(x, y, width, height)` in viewport coordinates.
    pub border_box: (f32, f32, f32, f32),
    /// Content-box `(x, y, width, height)` in viewport coordinates.
    pub content_box: (f32, f32, f32, f32),
}

/// One `fetch()` request normalized by the JS prelude (host executes the I/O).
#[derive(Clone, Debug)]
pub struct FetchRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// Host result for one `fetch()` request.
#[derive(Clone, Debug)]
pub struct FetchResult {
    pub status: u16,
    pub status_text: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Injectable `fetch()` provider; without one `fetch()` rejects with a typed error.
pub type FetchProvider =
    Box<dyn Fn(FetchRequest) -> Result<FetchResult, crate::error::JsError> + Send + Sync>;

/// Loads the source text of a worker script URL (the host owns I/O).
pub type WorkerLoader = Box<dyn Fn(&str) -> Result<String, crate::error::JsError> + Send + Sync>;

/// Injected geometry provider (`NodeId → LayoutRect`); `None` means the node has no box
/// (e.g. `display: none` or a node outside the laid-out document).
pub type LayoutProvider = Box<dyn Fn(dom::NodeId) -> Option<LayoutRect> + Send + Sync>;

/// Resolves an ES module specifier against the importing module's URL.
///
/// Returns `Ok(None)` when the specifier cannot be resolved; the runtime then reports a
/// typed `ModuleNotFound` error. The host owns all I/O (files/network); `js` never reads.
pub type ModuleResolver =
    Box<dyn Fn(&str, &str) -> Result<Option<String>, crate::error::JsError> + Send + Sync>;

/// Loads the source text of an already-resolved module URL.
pub type ModuleLoader = Box<dyn Fn(&str) -> Result<String, crate::error::JsError> + Send + Sync>;

/// Loads the persisted entries for one storage area.
pub type StorageLoadFn = Box<dyn Fn(StorageKind) -> Vec<(String, String)> + Send + Sync>;
/// Saves the current entries of one storage area (called after writes).
pub type StorageSaveFn = Box<dyn Fn(StorageKind, &[(String, String)]) + Send + Sync>;

/// Optional persistence provider: load is called once when installed, save after writes.
pub struct StorageProvider {
    pub load: StorageLoadFn,
    pub save: StorageSaveFn,
}

/// One queued MutationObserver record.
#[derive(Clone, Debug)]
pub(crate) struct MutationRecord {
    pub kind: MutationKind,
    pub target: NodeId,
    pub name: Option<String>,
    pub old_value: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MutationKind {
    ChildList,
    Attributes,
    CharacterData,
}

/// A `MutationObserver` registration (callbacks/records live in JS-side roots).
#[derive(Clone, Debug)]
pub(crate) struct ObserverRegistration {
    pub callback_slot: u32,
    pub object_slot: u32,
    pub targets: Vec<ObserverTarget>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ObserverTarget {
    pub node: NodeId,
    pub child_list: bool,
    pub attributes: bool,
    pub character_data: bool,
    pub subtree: bool,
}

/// Shared, mutable host state.
pub(crate) struct Host {
    /// The document scripts operate on.
    pub document: Document,
    /// Captured `console` calls (until the host drains them).
    pub console: Vec<ConsoleEntry>,
    /// `document.title` value.
    pub title: String,
    /// `document.URL` value.
    pub url: String,
    /// JS handle → generation-checked node handle (side table).
    pub nodes: Vec<NodeId>,
    /// Raw packed id → handle index, to avoid one entry per wrapper creation.
    node_index: HashMap<u32, usize>,
    /// Maximum number of side-table entries (anti-DoS for wrapper churn).
    pub max_handles: usize,
    /// Registered event listeners.
    pub listeners: Vec<Listener>,
    /// Scheduled timers.
    pub timers: Vec<TimerEntry>,
    /// Next timer id (insertion order for `(due, id)` ordering).
    pub next_timer_id: u64,
    /// Cumulative bytes written through `document.write` in this document (anti-DoS).
    pub document_write_bytes: usize,
    /// Maximum cumulative `document.write` payload (bytes).
    pub max_document_write_bytes: usize,
    /// Maximum node count for the document scripts write into.
    pub max_document_write_nodes: usize,
    /// Maximum live nodes a script may grow the document to through DOM mutations
    /// (`cloneNode`, parsed HTML setters, …). Distinct from the `document.write` byte budget:
    /// this one bounds *arena* growth, which a few lines of `appendChild(cloneNode(true))`
    /// can otherwise double without limit (#657).
    pub max_dom_nodes: usize,
    /// Whether the DOM node-budget rejection was already reported.
    pub dom_budget_warned: bool,
    /// Whether the `document.write` budget rejection was already reported.
    pub document_write_warned: bool,
    /// Real-clock origin.
    pub started_at: Instant,
    /// Virtual clock override (`Some(ms)` after `advance`/`run_until`).
    pub virtual_now_ms: Option<u64>,
    /// Dispatch state of the event currently being delivered.
    pub dispatch: Option<DispatchState>,
    /// Injected computed-style provider.
    pub computed_style_provider: Option<ComputedStyleProvider>,
    /// Inline `on<event>` handlers (callbacks live in the JS-side rooted table).
    pub inline_handlers: Vec<InlineHandler>,
    /// Injected `matchMedia` provider.
    pub media_provider: Option<MediaProvider>,
    /// Injected viewport provider.
    pub viewport_provider: Option<ViewportProvider>,
    /// Current `document.readyState` (`loading` until the host sets it).
    pub ready_state: String,
    /// Scroll offsets changed by `window.scrollTo`/`scrollBy`.
    pub scroll_x: f64,
    pub scroll_y: f64,
    /// Last non-fatal exception thrown by an event listener (dispatch continues).
    pub last_event_error: Option<String>,
    /// History API provider and current length.
    pub history_provider: Option<HistoryProvider>,
    pub history_length: u32,
    /// Current history state serialized as JSON (rooted JS value also kept).
    pub history_state: String,
    /// `location` navigation provider.
    pub location_provider: Option<LocationProvider>,
    /// Web Storage maps (per realm) and optional persistence provider.
    pub local_storage: HashMap<String, String>,
    pub session_storage: HashMap<String, String>,
    pub storage_provider: Option<StorageProvider>,
    /// MutationObserver registrations and pending records.
    pub observers: Vec<ObserverRegistration>,
    pub mutations: Vec<MutationRecord>,
    /// Currently focused element (best-effort `focus()`/`blur()` bookkeeping).
    pub active_element: Option<NodeId>,
    /// Injected layout geometry provider (`getBoundingClientRect`/offset/client metrics).
    pub layout_provider: Option<LayoutProvider>,
    /// Worker script loader injected by the host.
    pub worker_loader: Option<WorkerLoader>,
    /// `fetch()` provider injected by the host.
    pub fetch_provider: Option<FetchProvider>,
    /// Deterministic cooperative workers (no threads); `None` slots are terminated/dropped.
    pub workers: Vec<Option<crate::runtime::WorkerState>>,
    /// Parent host when this host belongs to a worker realm.
    pub worker_parent: Option<Rc<RefCell<Host>>>,
    /// Worker id in the parent host (worker realms only).
    pub worker_id: Option<u32>,
    /// Outbox queue of the worker realm (`postMessage` from the worker).
    pub worker_outbox: Option<Rc<RefCell<std::collections::VecDeque<String>>>>,
    /// ES module resolver/loader injected by the host (no I/O in `js`).
    pub module_resolver: Option<ModuleResolver>,
    pub module_loader: Option<ModuleLoader>,
    /// Installed import map (resolution happens before the host resolver).
    pub import_map: Option<crate::import_map::ImportMap>,
}

impl Host {
    /// Creates the host around a document.
    pub fn new(document: Document, url: String) -> Self {
        Self {
            document,
            console: Vec::new(),
            title: String::new(),
            url,
            nodes: Vec::new(),
            node_index: HashMap::new(),
            max_handles: 1_000_000,
            listeners: Vec::new(),
            timers: Vec::new(),
            next_timer_id: 1,
            document_write_bytes: 0,
            max_document_write_bytes: 8 * 1024 * 1024,
            max_document_write_nodes: 1_000_000,
            max_dom_nodes: 1_000_000,
            dom_budget_warned: false,
            document_write_warned: false,
            started_at: Instant::now(),
            virtual_now_ms: None,
            dispatch: None,
            computed_style_provider: None,
            inline_handlers: Vec::new(),
            media_provider: None,
            viewport_provider: None,
            ready_state: "loading".to_string(),
            scroll_x: 0.0,
            scroll_y: 0.0,
            last_event_error: None,
            history_provider: None,
            history_length: 1,
            history_state: "null".to_string(),
            location_provider: None,
            local_storage: HashMap::new(),
            session_storage: HashMap::new(),
            storage_provider: None,
            observers: Vec::new(),
            mutations: Vec::new(),
            active_element: None,
            layout_provider: None,
            worker_loader: None,
            fetch_provider: None,
            workers: Vec::new(),
            worker_parent: None,
            worker_id: None,
            worker_outbox: None,
            module_resolver: None,
            module_loader: None,
            import_map: None,
        }
    }

    /// Returns (and caches) the numeric JS handle for `node_id`.
    pub fn handle_for(&mut self, node_id: NodeId) -> Result<u32, String> {
        let raw = node_id.get();
        if let Some(index) = self.node_index.get(&raw) {
            return u32::try_from(*index).map_err(|_| "node handle table overflow".to_string());
        }
        if self.nodes.len() >= self.max_handles {
            return Err("too many DOM node handles exposed to JavaScript".to_string());
        }
        let index = self.nodes.len();
        self.nodes.push(node_id);
        self.node_index.insert(raw, index);
        u32::try_from(index).map_err(|_| "node handle table overflow".to_string())
    }

    /// Adds a MutationObserver record (bounded, see MAX_PENDING_MUTATIONS).
    pub fn record_mutation(&mut self, record: MutationRecord) {
        if self.mutations.len() < 4096 {
            self.mutations.push(record);
        }
    }

    /// Resolves `url` against the current document URL (absolute, root-relative or hash).
    pub fn resolve_url(&self, url: &str) -> String {
        if url.contains("://") {
            return url.to_string();
        }
        let base = self.url.clone();
        if url.starts_with('#') {
            let without_hash = base.split('#').next().unwrap_or(&base);
            return format!("{without_hash}{url}");
        }
        if url.starts_with('/') {
            if let Some(scheme) = base.find("://") {
                let rest = &base[scheme + 3..];
                let host = rest.split('/').next().unwrap_or(rest);
                return format!("{}://{}{}", &base[..scheme], host, url);
            }
            return url.to_string();
        }
        let (base_without_fragment, _) = base.split_once('#').unwrap_or((&base, ""));
        if url.starts_with('?') {
            return format!(
                "{}{}",
                base_without_fragment
                    .split('?')
                    .next()
                    .unwrap_or(base_without_fragment),
                url
            );
        }
        match base_without_fragment.rsplit_once('/') {
            Some((prefix, _)) => format!("{prefix}/{url}"),
            None => format!("{base_without_fragment}/{url}"),
        }
    }

    /// Validates a JS handle and returns the live node id.
    pub fn node_for_handle(&self, handle: u32) -> Result<NodeId, String> {
        let node_id = *self
            .nodes
            .get(handle as usize)
            .ok_or_else(|| format!("invalid DOM node handle {handle}"))?;
        if self.document.try_node(node_id).is_none() {
            return Err(format!(
                "stale DOM node handle {handle}: the node no longer exists"
            ));
        }
        Ok(node_id)
    }

    /// Current scheduler time in milliseconds (virtual clock or real elapsed).
    pub fn now_ms(&self) -> u64 {
        match self.virtual_now_ms {
            Some(now) => now,
            None => self.started_at.elapsed().as_millis() as u64,
        }
    }

    /// Advances the virtual clock by `delta`; returns the new time.
    pub fn advance_clock(&mut self, delta: Duration) -> u64 {
        let now = self.now_ms().saturating_add(delta.as_millis() as u64);
        self.virtual_now_ms = Some(now);
        now
    }

    /// Sets the virtual clock (never backwards); returns the effective time.
    pub fn set_clock(&mut self, deadline_ms: u64) -> u64 {
        let now = self.now_ms().max(deadline_ms);
        self.virtual_now_ms = Some(now);
        now
    }

    /// Drains captured console entries.
    pub fn take_console(&mut self) -> Vec<ConsoleEntry> {
        std::mem::take(&mut self.console)
    }
}

thread_local! {
    static CURRENT_HOST: RefCell<Option<Rc<RefCell<Host>>>> = const { RefCell::new(None) };
}

/// Publishes `host` as the current thread's host; restores the previous value on drop.
pub(crate) struct HostScope(Option<Rc<RefCell<Host>>>);

impl HostScope {
    pub fn enter(host: &Rc<RefCell<Host>>) -> Self {
        let previous = CURRENT_HOST.with(|cell| cell.borrow_mut().replace(Rc::clone(host)));
        Self(previous)
    }
}

impl Drop for HostScope {
    fn drop(&mut self) {
        CURRENT_HOST.with(|cell| {
            *cell.borrow_mut() = self.0.take();
        });
    }
}

/// Returns a clone of the current thread's host, if a runtime is evaluating.
pub(crate) fn current_host() -> Option<Rc<RefCell<Host>>> {
    CURRENT_HOST.with(|cell| cell.borrow().clone())
}

/// Runs `f` with exclusive access to the current host.
pub(crate) fn with_host<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
    let host = current_host()?;
    let mut borrowed = host.borrow_mut();
    Some(f(&mut borrowed))
}
