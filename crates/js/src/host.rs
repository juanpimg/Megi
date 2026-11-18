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
use std::collections::{HashMap, VecDeque};
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
    pub redirect: Option<String>,
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

/// Maximum simultaneous `MutationObserver` registrations per realm (#1134).
pub(crate) const MAX_OBSERVERS: usize = 1024;
/// Distinct nodes a single `MutationObserver` may track (#437).
pub(crate) const MAX_OBSERVER_TARGETS: usize = 1024;
/// Maximum queued `MutationRecord`s per observer between deliveries.
pub(crate) const MAX_OBSERVER_RECORDS: usize = 4096;

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
    pub records: Vec<MutationRecord>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ObserverTarget {
    pub node: NodeId,
    pub child_list: bool,
    pub attributes: bool,
    pub attribute_old_value: bool,
    pub character_data: bool,
    pub subtree: bool,
}

impl ObserverTarget {
    fn matches_kind(self, kind: MutationKind) -> bool {
        match kind {
            MutationKind::ChildList => self.child_list,
            MutationKind::Attributes => self.attributes,
            MutationKind::CharacterData => self.character_data,
        }
    }
}

/// Insertion-order-preserving key-value map for Web Storage (`localStorage`/`sessionStorage`)
/// so `storage.key(index)` is deterministic (#571).
#[derive(Clone, Debug, Default)]
pub struct StorageMap {
    /// O(1) lookups; the 1 MiB storage cap bounds the number of entries.
    values: HashMap<String, String>,
    /// Insertion order for deterministic `Storage.key(index)` (#571).
    order: Vec<String>,
}

impl StorageMap {
    /// Returns the value for `key`, if present.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }

    /// Inserts or updates `key` with `value`, preserving insertion order for existing keys.
    pub fn insert(&mut self, key: String, value: String) -> Option<String> {
        if let Some(existing) = self.values.get_mut(&key) {
            return Some(std::mem::replace(existing, value));
        }
        self.order.push(key.clone());
        self.values.insert(key, value)
    }

    /// Removes `key`, returning its value if present.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let value = self.values.remove(key)?;
        if let Some(pos) = self.order.iter().position(|existing| existing == key) {
            self.order.remove(pos);
        }
        Some(value)
    }

    /// Clears all entries.
    pub fn clear(&mut self) {
        self.values.clear();
        self.order.clear();
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` when there are no entries.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates keys in insertion order.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.order.iter()
    }

    /// Iterates `(key, value)` pairs in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.order.iter().map(|key| (key, &self.values[key]))
    }
}

impl FromIterator<(String, String)> for StorageMap {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        let mut map = Self::default();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}

/// Shared, mutable host state.
pub const MAX_CONSOLE_ENTRIES: usize = 4096;

pub(crate) struct Host {
    /// The document scripts operate on.
    pub document: Document,
    /// Captured `console` calls (until the host drains them). A deque keeps the
    /// 4096-entry cap O(1) per push instead of shifting the whole Vec (#1144).
    pub console: VecDeque<ConsoleEntry>,
    /// `document.title` value.
    pub title: String,
    /// `document.URL` value.
    pub url: String,
    /// Live `NodeId` table referenced by JS handles (`__megi_node`).
    pub nodes: Vec<NodeId>,
    /// Raw packed id → handle index, to avoid one entry per wrapper creation.
    node_index: HashMap<u32, usize>,
    /// Maximum number of side-table entries (anti-DoS for wrapper churn).
    pub max_handles: usize,
    /// Registered event listeners.
    pub listeners: Vec<Listener>,
    /// Reusable slots in the JS `__megi_listeners` array freed by `removeEventListener` / `once` (#567).
    pub free_listener_slots: Vec<u32>,
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
    pub local_storage: StorageMap,
    pub session_storage: StorageMap,
    pub storage_provider: Option<StorageProvider>,
    /// MutationObserver registrations and per-observer pending records.
    pub observers: Vec<ObserverRegistration>,
    /// Observed target `NodeId` -> `(observer_index_in_observers, target_options)` index (#1134).
    pub observer_index: HashMap<NodeId, Vec<(usize, ObserverTarget)>>,
    /// Currently focused element (best-effort `focus()`/`blur()` bookkeeping).
    pub active_element: Option<NodeId>,
    /// Injected layout geometry provider (`getBoundingClientRect`/offset/client metrics).
    pub layout_provider: Option<LayoutProvider>,
    /// Worker script loader injected by the host.
    pub worker_loader: Option<WorkerLoader>,
    /// `fetch()` provider injected by the host.
    pub fetch_provider: Option<FetchProvider>,
    /// Shadow DOM (#1541): host element `NodeId` -> shadow-root `NodeId`.
    pub shadow_roots: std::collections::HashMap<NodeId, NodeId>,
    /// Reverse map: shadow-root `NodeId` -> host element `NodeId`.
    pub shadow_hosts: std::collections::HashMap<NodeId, NodeId>,
    /// Canvas 2D surfaces keyed by `<canvas>` node id (#1545).
    pub canvas_store: crate::canvas::CanvasStore,
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
            console: VecDeque::new(),
            title: String::new(),
            url,
            nodes: Vec::new(),
            node_index: HashMap::new(),
            max_handles: 1_000_000,
            listeners: Vec::new(),
            free_listener_slots: Vec::new(),
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
            local_storage: StorageMap::default(),
            session_storage: StorageMap::default(),
            storage_provider: None,
            observers: Vec::new(),
            observer_index: HashMap::new(),
            active_element: None,
            layout_provider: None,
            worker_loader: None,
            fetch_provider: None,
            shadow_roots: std::collections::HashMap::new(),
            shadow_hosts: std::collections::HashMap::new(),
            canvas_store: crate::canvas::CanvasStore::new(),
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

    /// Registers or updates an observed target for `slot`, keeping `observer_index` in sync.
    pub fn observe_target(&mut self, slot: u32, target: ObserverTarget) -> Result<(), String> {
        let Some(obs_idx) = self.observers.iter().position(|r| r.object_slot == slot) else {
            return Ok(());
        };
        let registration = &mut self.observers[obs_idx];
        // Re-observing a node only updates its options: the DOM spec keeps one registered
        // observer per (observer, target) pair (#437).
        if let Some(existing) = registration
            .targets
            .iter_mut()
            .find(|existing| existing.node == target.node)
        {
            *existing = target;
            if let Some(entries) = self.observer_index.get_mut(&target.node) {
                for (indexed_obs, indexed_target) in entries.iter_mut() {
                    if *indexed_obs == obs_idx {
                        *indexed_target = target;
                    }
                }
            }
            return Ok(());
        }
        if registration.targets.len() >= MAX_OBSERVER_TARGETS {
            return Err(format!(
                "MutationObserver target limit of {MAX_OBSERVER_TARGETS} exceeded"
            ));
        }
        registration.targets.push(target);
        self.observer_index
            .entry(target.node)
            .or_default()
            .push((obs_idx, target));
        Ok(())
    }

    /// Clears targets and queued records for `slot`, keeping the observer registration re-armable (#1135).
    pub fn disconnect_observer(&mut self, slot: u32) {
        let Some(obs_idx) = self.observers.iter().position(|r| r.object_slot == slot) else {
            return;
        };
        let drained = std::mem::take(&mut self.observers[obs_idx].targets);
        self.observers[obs_idx].records.clear();
        for target in drained {
            if let Some(entries) = self.observer_index.get_mut(&target.node) {
                entries.retain(|(idx, _)| *idx != obs_idx);
                if entries.is_empty() {
                    self.observer_index.remove(&target.node);
                }
            }
        }
    }

    /// Adds a MutationObserver record to each matching observer's queue (bounded per observer).
    ///
    /// Walks `record.target`'s ancestor chain once against `observer_index` instead of scanning
    /// every observer and walking ancestors per observer (`O(depth + matches)` vs `O(observers * depth)`).
    pub fn record_mutation(&mut self, record: MutationRecord) {
        if self.observer_index.is_empty() || self.observers.is_empty() {
            return;
        }
        let mut matched_bits = [0u64; MAX_OBSERVERS.div_ceil(64)];
        let mut want_old_value = [0u64; MAX_OBSERVERS.div_ceil(64)];
        let mut matched_indices: Vec<usize> = Vec::new();
        let max_steps = self.document.len().saturating_add(1);
        let mut current = Some(record.target);
        let mut steps = 0usize;
        while let Some(node_id) = current {
            let is_direct = steps == 0;
            if let Some(entries) = self.observer_index.get(&node_id) {
                for &(obs_idx, target) in entries {
                    let word = obs_idx / 64;
                    let Some(slot_word) = matched_bits.get_mut(word) else {
                        continue;
                    };
                    if obs_idx >= self.observers.len()
                        || !target.matches_kind(record.kind)
                        || (!is_direct && !target.subtree)
                    {
                        continue;
                    }
                    let bit = 1u64 << (obs_idx % 64);
                    if target.attribute_old_value {
                        want_old_value[word] |= bit;
                    }
                    if *slot_word & bit == 0 {
                        *slot_word |= bit;
                        matched_indices.push(obs_idx);
                    }
                }
            }
            if steps >= max_steps {
                break;
            }
            steps += 1;
            current = self.document.try_node(node_id).and_then(|node| node.parent);
        }
        if matched_indices.is_empty() {
            return;
        }
        if matched_indices.len() > 1 {
            matched_indices.sort_unstable();
        }
        for obs_idx in matched_indices {
            let observer = &mut self.observers[obs_idx];
            if observer.records.len() >= MAX_OBSERVER_RECORDS {
                continue;
            }
            let keep_old = record.kind == MutationKind::Attributes
                && (want_old_value[obs_idx / 64] & (1u64 << (obs_idx % 64))) != 0;
            let mut pushed = record.clone();
            if !keep_old {
                pushed.old_value = None;
            }
            observer.records.push(pushed);
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

    /// Pushes a console entry, dropping oldest when the cap is exceeded (issue #566).
    pub fn push_console(&mut self, entry: ConsoleEntry) {
        if self.console.len() >= MAX_CONSOLE_ENTRIES {
            self.console.pop_front();
        }
        self.console.push_back(entry);
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
        std::mem::take(&mut self.console).into()
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

#[cfg(test)]
mod storage_and_console_tests {
    use super::*;

    #[test]
    fn storage_map_keeps_insertion_order_with_hash_lookup() {
        let mut map = StorageMap::default();
        assert_eq!(map.insert("b".into(), "1".into()), None);
        assert_eq!(map.insert("a".into(), "2".into()), None);
        assert_eq!(map.insert("b".into(), "3".into()), Some("1".into()));
        assert_eq!(
            map.keys().map(String::as_str).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(map.get("a").map(String::as_str), Some("2"));
        assert_eq!(map.remove("b").as_deref(), Some("3"));
        assert_eq!(map.keys().map(String::as_str).collect::<Vec<_>>(), ["a"]);
        assert_eq!(map.iter().count(), 1);
    }

    #[test]
    fn console_drops_oldest_at_cap_in_constant_time() {
        let mut host = Host::new(Document::new(), "about:blank".into());
        for i in 0..MAX_CONSOLE_ENTRIES + 3 {
            host.push_console(ConsoleEntry {
                level: "log".into(),
                message: i.to_string(),
            });
        }
        assert_eq!(host.console.len(), MAX_CONSOLE_ENTRIES);
        assert_eq!(host.console.front().expect("oldest").message, "3");
        assert_eq!(host.console.back().expect("newest").message, "4098");
    }
}

#[cfg(test)]
mod mutation_observer_index_tests {
    use super::*;
    use dom::Namespace;

    #[test]
    fn observer_index_updates_cleans_up_and_preserves_order() {
        let mut doc = Document::new();
        let parent = doc.create_element(Namespace::Html, "div", vec![]);
        let child = doc.create_element(Namespace::Html, "span", vec![]);
        let other = doc.create_element(Namespace::Html, "p", vec![]);
        assert!(doc.append_child(doc.root(), parent));
        assert!(doc.append_child(parent, child));
        assert!(doc.append_child(doc.root(), other));

        let mut host = Host::new(doc, "https://example.com".to_string());
        for slot in 0..3u32 {
            host.observers.push(ObserverRegistration {
                callback_slot: slot,
                object_slot: slot,
                targets: Vec::new(),
                records: Vec::new(),
            });
        }
        host.observe_target(
            0,
            ObserverTarget {
                node: parent,
                child_list: true,
                attributes: true,
                attribute_old_value: false,
                character_data: false,
                subtree: true,
            },
        )
        .expect("observe 0");
        host.observe_target(
            1,
            ObserverTarget {
                node: child,
                child_list: false,
                attributes: true,
                attribute_old_value: true,
                character_data: false,
                subtree: false,
            },
        )
        .expect("observe 1");
        host.observe_target(
            2,
            ObserverTarget {
                node: other,
                child_list: true,
                attributes: true,
                attribute_old_value: true,
                character_data: false,
                subtree: true,
            },
        )
        .expect("observe 2");
        assert_eq!(host.observer_index.len(), 3);

        host.observe_target(
            0,
            ObserverTarget {
                node: child,
                child_list: false,
                attributes: true,
                attribute_old_value: true,
                character_data: false,
                subtree: false,
            },
        )
        .expect("observe 0 child");

        host.record_mutation(MutationRecord {
            kind: MutationKind::Attributes,
            target: child,
            name: Some("data-x".to_string()),
            old_value: Some("before".to_string()),
        });
        assert_eq!(host.observers[0].records.len(), 1);
        assert_eq!(
            host.observers[0].records[0].old_value.as_deref(),
            Some("before")
        );
        assert_eq!(host.observers[1].records.len(), 1);
        assert_eq!(
            host.observers[1].records[0].old_value.as_deref(),
            Some("before")
        );
        assert!(host.observers[2].records.is_empty());

        host.observe_target(
            0,
            ObserverTarget {
                node: child,
                child_list: false,
                attributes: true,
                attribute_old_value: false,
                character_data: false,
                subtree: false,
            },
        )
        .expect("re-observe 0 child");
        host.record_mutation(MutationRecord {
            kind: MutationKind::Attributes,
            target: child,
            name: Some("data-x".to_string()),
            old_value: Some("after".to_string()),
        });
        assert_eq!(host.observers[0].records.len(), 2);
        assert_eq!(host.observers[0].records[1].old_value, None);
        assert_eq!(
            host.observers[1].records[1].old_value.as_deref(),
            Some("after")
        );

        host.disconnect_observer(0);
        assert!(!host.observer_index.contains_key(&parent));
        assert_eq!(host.observer_index.get(&child).map(Vec::len), Some(1));
        host.disconnect_observer(1);
        assert!(!host.observer_index.contains_key(&child));
        host.disconnect_observer(2);
        assert!(host.observer_index.is_empty());
    }

    #[test]
    fn record_mutation_terminates_on_cyclic_parent_chain() {
        let mut doc = Document::new();
        let a = doc.create_element(Namespace::Html, "div", vec![]);
        let b = doc.create_element(Namespace::Html, "span", vec![]);
        let unrelated = doc.create_element(Namespace::Html, "p", vec![]);
        doc.try_node_mut(a).expect("a").parent = Some(b);
        doc.try_node_mut(b).expect("b").parent = Some(a);

        let mut host = Host::new(doc, "https://example.com".to_string());
        host.observers.push(ObserverRegistration {
            callback_slot: 0,
            object_slot: 0,
            targets: Vec::new(),
            records: Vec::new(),
        });
        host.observe_target(
            0,
            ObserverTarget {
                node: unrelated,
                child_list: true,
                attributes: true,
                attribute_old_value: false,
                character_data: false,
                subtree: true,
            },
        )
        .expect("observe");

        host.record_mutation(MutationRecord {
            kind: MutationKind::ChildList,
            target: b,
            name: None,
            old_value: None,
        });
        assert!(host.observers[0].records.is_empty());
    }
}
