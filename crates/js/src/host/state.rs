use super::*;

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
    pub(crate) node_index: HashMap<u32, usize>,
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
    /// Optional host cookie jar provider and per-realm fallback store for `document.cookie`.
    pub cookie_provider: Option<CookieProvider>,
    pub cookies_fallback: Vec<(String, String)>,
    /// MutationObserver registrations and per-observer pending records.
    pub observers: Vec<ObserverRegistration>,
    /// Observed target `NodeId` -> `(observer_index_in_observers, target_options)` index (#1134).
    pub observer_index: HashMap<NodeId, Vec<(usize, ObserverTarget)>>,
    /// Currently focused element (best-effort `focus()`/`blur()` bookkeeping).
    pub active_element: Option<NodeId>,
    /// `<script>` element currently being evaluated (`document.currentScript`, #1887).
    pub current_script: Option<NodeId>,
    /// Injected layout geometry provider (`getBoundingClientRect`/offset/client metrics).
    pub layout_provider: Option<LayoutProvider>,
    /// Worker script loader injected by the host.
    pub worker_loader: Option<WorkerLoader>,
    /// `fetch()` provider injected by the host.
    pub fetch_provider: Option<FetchProvider>,
    /// Injected WebSocket transport provider (#1534).
    pub websocket_provider: Option<WebSocketProvider>,
    /// Live WebSocket transports keyed by an opaque handle.
    pub websockets: std::collections::HashMap<u32, Box<dyn WebSocketTransport>>,
    /// Monotonic handle counter for WebSocket transports.
    pub websocket_next_handle: u32,
    /// Shadow DOM (#1541): host element `NodeId` -> shadow-root `NodeId`.
    pub shadow_roots: std::collections::HashMap<NodeId, NodeId>,
    /// Reverse map: shadow-root `NodeId` -> host element `NodeId`.
    pub shadow_hosts: std::collections::HashMap<NodeId, NodeId>,
    /// Canvas 2D surfaces keyed by `<canvas>` node id (#1545).
    pub canvas_store: crate::canvas::CanvasStore,
    /// Custom element registry (#1541): name -> integer handle; the JS constructor is stored on
    /// `globalThis.__megi_ce_<handle>` so it stays reachable without a cross-crate handle type.
    pub custom_elements: std::collections::HashMap<String, u32>,
    /// Monotonic handle counter for custom element constructors.
    pub custom_element_next_handle: u32,
    /// Same-origin framed documents for `iframe.contentDocument` (#1544).
    pub frame_documents: std::collections::HashMap<u32, megi_core::Document>,
    pub iframe_frames: std::collections::HashMap<megi_core::NodeId, u32>,
    pub frame_next_handle: u32,
    pub frame_nodes: std::collections::HashMap<u32, megi_core::NodeId>,
    pub frame_node_next_handle: u32,
    /// Deterministic cooperative workers (no threads); `None` slots are terminated/dropped.
    pub workers: Vec<Option<crate::runtime::WorkerState>>,
    /// Worker id in the parent host (worker realms only).
    pub worker_id: Option<u32>,
    /// Outbox queue of the worker realm (`postMessage` from the worker), owned by this host.
    pub worker_outbox: Option<VecDeque<String>>,
    /// ES module resolver/loader injected by the host (no I/O in `js`).
    pub module_resolver: Option<ModuleResolver>,
    pub module_loader: Option<ModuleLoader>,
    /// Installed import map (resolution happens before the host resolver).
    pub import_map: Option<crate::import_map::ImportMap>,
    /// Pre-mutation `value` attribute (or textarea text) for form controls, used by `form.reset()`.
    pub initial_control_values: HashMap<NodeId, Option<String>>,
    /// Pre-mutation `checked` / `selected` state for form controls/options, used by `form.reset()`.
    pub initial_control_checked: HashMap<NodeId, bool>,
    /// True when sandboxed without `allow-same-origin` (`origin === "null"`).
    pub opaque_origin: bool,
    /// Sandbox `allow-forms` capability flag (true when unsandboxed or explicitly allowed).
    pub allow_forms: bool,
    /// Sandbox `allow-popups` capability flag (true when unsandboxed or explicitly allowed).
    pub allow_popups: bool,
    /// Current frame nesting depth (`0` for top-level document).
    pub frame_depth: usize,
    /// Optional host loader for `<iframe src="...">` documents.
    pub frame_loader: Option<FrameLoader>,
    /// True when this host represents a child `<iframe>` browsing context.
    pub is_child_frame: bool,
    /// Parent host's serialized origin when `is_child_frame` is true.
    pub parent_origin: Option<String>,
    /// Whether this child frame is same-origin with its parent (and not opaque).
    pub parent_same_origin: bool,
    /// Outbox for `window.parent.postMessage(...)` (`(json_payload, child_origin, target_origin)`),
    /// owned by this (child) host; the parent drains it through the child's context.
    pub parent_outbox: Option<VecDeque<(String, String, String)>>,
    /// Pending self-targeted `window.postMessage(...)` messages (`(json_payload, origin)`).
    pub self_messages: VecDeque<(String, String)>,
    /// Child `<iframe>` browsing contexts owned by this document.
    pub frames: Vec<Option<Rc<RefCell<crate::runtime::IframeState>>>>,
    /// Map from `<iframe>` element `NodeId` to index in `frames`.
    pub frame_by_node: HashMap<NodeId, u32>,
    /// IDL (dirty) `value` of an input, distinct from its `value` content attribute (#1953).
    pub idl_values: HashMap<NodeId, String>,
    /// IDL (dirty) `checked` state of a control, distinct from its attribute (#1953).
    pub idl_checked: HashMap<NodeId, bool>,
}
