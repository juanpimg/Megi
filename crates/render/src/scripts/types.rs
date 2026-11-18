//! Public data types, defensive budgets and host context for the scripting stage.

use std::sync::{Arc, Mutex};

use crate::{HistoryAction, LocationAction};

use megi_core::{Document, NodeId};
use megi_css::ComputedStyle;

/// Default maximum size of one script source (8 MiB).
pub const DEFAULT_MAX_SCRIPT_BYTES: usize = 8 * 1024 * 1024;
/// Default total script byte budget per page (32 MiB).
pub const DEFAULT_MAX_TOTAL_SCRIPT_BYTES: usize = 32 * 1024 * 1024;
/// Default maximum number of scripts executed per page.
pub const DEFAULT_MAX_SCRIPTS: usize = 64;
/// Maximum script fetch attempts (successful or not) per page.
const DEFAULT_MAX_SCRIPT_ATTEMPTS: usize = 256;
/// Maximum module-graph fetches (including nested imports) per page.
const DEFAULT_MAX_MODULE_FETCHES: usize = 256;
/// Maximum number of `fetch()` calls a page's scripts may make (successful or not).
const DEFAULT_MAX_FETCH_CALLS: usize = 64;
/// Maximum total bytes a page's scripts may pull through `fetch()` (#660).
const DEFAULT_MAX_FETCH_BYTES: usize = 32 * 1024 * 1024;
/// Default total loop-iteration budget across all scripts of the page.
pub const DEFAULT_MAX_TOTAL_LOOP_ITERATIONS: u64 = 1_000_000;
/// Default maximum number of timer callbacks pumped after the scripts.
pub const DEFAULT_MAX_TASKS: usize = 10_000;
/// Default virtual-time budget of the event loop (milliseconds).
pub const DEFAULT_MAX_VIRTUAL_TIME_MS: u64 = 1_000;
/// Virtual clock step used between timer batches (milliseconds).
pub const VIRTUAL_TIME_STEP_MS: u64 = 1;
/// Absolute virtual-time origin used for script execution.
///
/// The js host clock is monotonic and its `run_until` setter uses `max(real elapsed, deadline)`,
/// so starting at 0 would leak a few milliseconds of real startup time into timer/rAF due
/// times and make `script_virtual_time_ms` machine-speed dependent (CI flake). Pinning a
/// large fixed origin (16 min) before any script runs gives every callback a stable absolute
/// timestamp; the reported virtual time is relative to this origin.
#[cfg(feature = "js")]
pub(crate) const VIRTUAL_EPOCH_MS: u64 = 1_000_000;

/// Per-script execution budget (mirrors the `js` crate's `Budget` without requiring the
/// feature to be compiled in, so [`ScriptingLimits`] is part of the stable render API).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptBudget {
    /// Maximum loop iterations for one script.
    pub loop_iterations: u64,
    /// Maximum JS call recursion depth.
    pub recursion_limit: usize,
    /// Boa VM stack-size limit (abstract units).
    pub stack_size_limit: usize,
}

impl ScriptBudget {
    /// Conservative budget for small pages and tests (same values as `megi_js::Budget::SMALL`).
    pub const SMALL: Self = Self {
        loop_iterations: 100_000,
        recursion_limit: 256,
        stack_size_limit: 10_240,
    };
}

impl Default for ScriptBudget {
    fn default() -> Self {
        Self::SMALL
    }
}

/// Defensive caps and budgets applied by the scripting stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptingLimits {
    /// Maximum number of scripts executed.
    pub max_scripts: usize,
    /// Maximum number of script fetch attempts (successful or not).
    pub max_script_attempts: usize,
    /// Maximum number of module-graph fetches (including nested imports).
    pub max_module_fetches: usize,
    /// Maximum number of `fetch()` calls a page's scripts may make (#660). The per-call loader
    /// caps bound one response, but nothing bounded the number of calls.
    pub max_fetch_calls: usize,
    /// Maximum total bytes fetched through `fetch()` per page (#660).
    pub max_fetch_bytes: usize,
    /// Maximum size of one script source (bytes).
    pub max_script_bytes: usize,
    /// Maximum total script bytes per page (inline + external, bytes).
    pub max_total_bytes: usize,
    /// Per-script execution budget.
    pub budget: ScriptBudget,
    /// Total loop-iteration budget shared by all scripts of the page.
    pub total_loop_iterations: u64,
    /// Maximum number of timer callbacks pumped after execution.
    pub max_tasks: usize,
    /// Maximum virtual time the event loop may advance (milliseconds). Timers whose delay
    /// would need more virtual time are left pending and the run is marked limited.
    pub max_virtual_time_ms: u64,
}

impl Default for ScriptingLimits {
    fn default() -> Self {
        Self {
            max_scripts: DEFAULT_MAX_SCRIPTS,
            max_script_attempts: DEFAULT_MAX_SCRIPT_ATTEMPTS,
            max_module_fetches: DEFAULT_MAX_MODULE_FETCHES,
            max_fetch_calls: DEFAULT_MAX_FETCH_CALLS,
            max_fetch_bytes: DEFAULT_MAX_FETCH_BYTES,
            max_script_bytes: DEFAULT_MAX_SCRIPT_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_SCRIPT_BYTES,
            budget: ScriptBudget::default(),
            total_loop_iterations: DEFAULT_MAX_TOTAL_LOOP_ITERATIONS,
            max_tasks: DEFAULT_MAX_TASKS,
            max_virtual_time_ms: DEFAULT_MAX_VIRTUAL_TIME_MS,
        }
    }
}

/// A `<script>` element found in the document (before loading).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptCandidate {
    /// `<script>` element handle.
    pub element: NodeId,
    /// Inline source text, when the script has no `src`.
    pub inline_text: Option<String>,
    /// Raw `src` attribute, when present.
    pub src: Option<String>,
    /// `defer` attribute (recorded; execution is always after parsing).
    pub deferred: bool,
    /// `async` attribute (recorded; ordering is document order, see module docs).
    pub is_async: bool,
    /// `<script type="module">` (ES module evaluated with the js crate's module subset).
    pub is_module: bool,
    /// `integrity` attribute (Subresource Integrity); verified against the fetched bytes before
    /// execution (#1527). Inline scripts are not subject to SRI.
    pub integrity: Option<String>,
    /// `nonce` attribute, used by CSP `script-src 'nonce-…'` (#1526).
    pub nonce: Option<String>,
    /// `crossorigin` attribute; classic scripts without it fetch in `no-cors` mode (#1537).
    pub crossorigin: bool,
}

/// A script ready to be evaluated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedScript {
    /// JavaScript source text.
    pub text: String,
    /// Source name used in diagnostics (`inline-script-N` or the resolved URL).
    pub source_name: String,
    /// Source size in bytes (for the byte budget).
    pub bytes: usize,
    /// `true` for `<script type="module">`.
    pub is_module: bool,
    /// Module base URL used for relative imports (external `src` URL or the document URL for
    /// inline modules); `None` for classic scripts.
    pub module_url: Option<String>,
}

/// Result of the discovery + loading stage.
#[derive(Clone, Debug, Default)]
pub struct ScriptLoadReport {
    /// Scripts ready to execute, in document order.
    pub scripts: Vec<LoadedScript>,
    /// Bytes accepted by the caps (inline + external).
    pub script_bytes: usize,
    /// Candidates dropped before execution (unsupported type, cap, duplicate, fetch error).
    pub skipped: usize,
}

/// Persistent Web Storage shared across renders (R155).
///
/// Only `localStorage` is persisted: `sessionStorage` is a per-render in-memory area (a
/// new render is a new browsing context). Pass it through
/// [`crate::RenderOptions::storage`] as `Arc<Mutex<RenderStorage>>` to emulate a browser
/// profile: writes made by scripts in one render are visible to later renders sharing the
/// same value. Without a shared value every render starts with empty storage.
#[derive(Clone, Debug, Default)]
pub struct RenderStorage {
    /// `localStorage` entries for non-HTTP(S) documents (single-origin compatibility).
    pub local: Vec<(String, String)>,
    /// Per-origin `localStorage` entries; the key is the serialized tuple origin.
    /// Prevents a page on origin A from reading/writing origin B's storage (#1476).
    pub by_origin: std::collections::HashMap<String, Vec<(String, String)>>,
}

/// Host environment exposed to page scripts (WP-6.5): the **pre-script computed style
/// snapshot**, the render's media context, the real viewport, shared Web Storage and any
/// host-dispatched events (e.g. `hashchange`/`popstate`).
///
/// The style snapshot is taken right after parsing (author + UA cascade) and before scripts
/// run, so `getComputedStyle` reads what the page looked like when scripting started;
/// layout/paint are recomputed afterwards from the mutated DOM. `matchMedia` and
/// `innerWidth`/`innerHeight` use the same viewport as the stylesheet cascade and layout.
pub struct ScriptingHostContext {
    /// Snapshot of the computed styles (pre-script) used by `getComputedStyle`.
    pub styles: Option<Arc<std::collections::HashMap<NodeId, ComputedStyle>>>,
    /// Media context (viewport + screen medium) used to evaluate `matchMedia` queries.
    pub media: megi_css::MediaContext,
    /// Viewport size in CSS pixels (`window.innerWidth`/`innerHeight`).
    pub viewport: (u32, u32),
    /// Shared `localStorage` (see [`RenderStorage`]); `None` = per-render memory only.
    pub storage: Option<Arc<Mutex<RenderStorage>>>,
    /// Shared cookie jar backing `document.cookie` (and HTTP requests); `None` = per-render jar.
    pub cookie_jar: Option<Arc<Mutex<megi_net::CookieJar>>>,
    /// Fetch configuration used by the ES module loader (`http(s)` imports share the same
    /// timeouts/caps/cookie jar as the rest of the render).
    pub fetch_options: megi_net::FetchOptions,
    /// `--no-network`: `http(s)` module imports fail with a non-fatal warning.
    pub no_network: bool,
    /// Enable deterministic cooperative Web Workers (file:// scripts).
    pub enable_workers: bool,
    /// Events dispatched on `window` by the host after `load` (e.g. `hashchange`,
    /// `popstate`) so their listeners can mutate the DOM before the final re-render.
    pub host_events: Vec<String>,
    #[cfg(feature = "js")]
    pub frame_documents: Vec<(megi_core::NodeId, megi_core::Document)>,
    /// Optional Content Security Policy enforced across `fetch`/`XHR`, modules, and `eval` (#1526).
    pub csp: Option<megi_net::csp::ContentSecurityPolicy>,
    /// Structured DOM/UI events forwarded from the host UI into the live JS realm (#1504).
    pub host_dom_events: Vec<crate::HostDomEvent>,
    /// Pre-script layout geometry (`NodeId → viewport rects`) installed into the JS
    /// runtime so scripts can measure the initial layout (viewport coords, scroll = 0).
    #[cfg(feature = "js")]
    pub layout: Option<
        std::sync::Arc<std::collections::HashMap<megi_core::NodeId, megi_js::host::LayoutRect>>,
    >,
    /// Lazy pre-script layout resolver invoked only if a script queries geometry (#534).
    #[cfg(feature = "js")]
    pub layout_provider: Option<
        Arc<
            dyn Fn() -> std::collections::HashMap<megi_core::NodeId, megi_js::host::LayoutRect>
                + Send
                + Sync,
        >,
    >,
    /// Optional `<iframe sandbox>` flags applied when this document is a framed subdocument (#1544).
    #[cfg(feature = "js")]
    pub sandbox_flags: Option<megi_js::SandboxFlags>,
    /// Current nesting depth in the frame tree (`0` for top-level document).
    pub frame_depth: usize,
    /// Deterministic animation/transition timeline timestamp (`RenderOptions::time_ms`).
    #[cfg(feature = "js")]
    pub animation_time_ms: f64,
    /// Lazy post-script animation/transition event resolver invoked only when listeners exist (#941).
    #[cfg(feature = "js")]
    pub timeline_events_provider: Option<TimelineEventsProvider>,
}

/// Callback that computes deterministic CSS animation and transition events for a document.
#[cfg(feature = "js")]
pub type TimelineEventsProvider = Arc<dyn Fn(&Document, f32) -> Vec<TimelineEvent> + Send + Sync>;

/// Deterministic CSS animation/transition DOM event computed from style diffs (#941).
#[cfg(feature = "js")]
#[derive(Clone, Debug)]
pub struct TimelineEvent {
    /// Element receiving the event.
    pub target: NodeId,
    /// DOM event type (`"animationstart"`, `"animationend"`, `"transitionend"`, ...).
    pub event_type: &'static str,
    /// Typed event payload.
    pub kind: TimelineEventKind,
}

/// Typed payload for [`TimelineEvent`].
#[cfg(feature = "js")]
#[derive(Clone, Debug)]
pub enum TimelineEventKind {
    /// `AnimationEvent` payload.
    Animation {
        animation_name: String,
        elapsed_time_secs: f64,
    },
    /// `TransitionEvent` payload.
    Transition {
        property_name: String,
        elapsed_time_secs: f64,
    },
}

impl ScriptingHostContext {
    /// Creates the host context for a render.
    pub fn new(
        styles: Option<Arc<std::collections::HashMap<NodeId, ComputedStyle>>>,
        media: megi_css::MediaContext,
        viewport: (u32, u32),
    ) -> Self {
        Self {
            styles,
            media,
            viewport,
            storage: None,
            cookie_jar: None,
            fetch_options: megi_net::FetchOptions::default(),
            no_network: false,
            host_events: Vec::new(),
            #[cfg(feature = "js")]
            frame_documents: Vec::new(),
            host_dom_events: Vec::new(),
            enable_workers: true,
            csp: None,
            #[cfg(feature = "js")]
            layout: None,
            #[cfg(feature = "js")]
            layout_provider: None,
            #[cfg(feature = "js")]
            sandbox_flags: None,
            frame_depth: 0,
            #[cfg(feature = "js")]
            animation_time_ms: 0.0,
            #[cfg(feature = "js")]
            timeline_events_provider: None,
        }
    }

    /// Empty context for direct `execute_scripts` callers: no style snapshot, default media
    /// and a 800x600 viewport (matches the pipeline default).
    pub fn empty() -> Self {
        Self {
            styles: None,
            media: megi_css::MediaContext::with_viewport(800.0, 600.0),
            viewport: (800, 600),
            storage: None,
            cookie_jar: None,
            fetch_options: megi_net::FetchOptions::default(),
            no_network: false,
            host_events: Vec::new(),
            #[cfg(feature = "js")]
            frame_documents: Vec::new(),
            host_dom_events: Vec::new(),
            enable_workers: true,
            csp: None,
            #[cfg(feature = "js")]
            layout: None,
            #[cfg(feature = "js")]
            layout_provider: None,
            #[cfg(feature = "js")]
            sandbox_flags: None,
            frame_depth: 0,
            #[cfg(feature = "js")]
            animation_time_ms: 0.0,
            #[cfg(feature = "js")]
            timeline_events_provider: None,
        }
    }
}

/// A captured `console.*` call (level + already-stringified message).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleMessage {
    /// `log`, `info`, `warn` or `error`.
    pub level: String,
    /// Space-joined stringification of the arguments.
    pub message: String,
}

/// Result of the execution stage: the mutated document plus observability data.
#[derive(Clone, Debug)]
pub struct ScriptExecution {
    /// Document after all scripts, microtasks and timer callbacks have run.
    pub document: Document,
    /// Canvas 2D surfaces created/filled by scripts (#1545), keyed by `<canvas>` node.
    /// Only available when the `js` feature is enabled (no canvas without scripts).
    #[cfg(feature = "js")]
    pub canvas_surfaces:
        std::collections::HashMap<megi_core::NodeId, megi_js::canvas::CanvasSurface>,
    /// Mutated/initialized child `<iframe>` documents captured during scripting (#1544).
    #[cfg(feature = "js")]
    pub frame_documents: Vec<(NodeId, Document, megi_js::SandboxFlags, String)>,
    /// Scripts evaluated without an error.
    pub executed: usize,
    /// Scripts that raised an exception or exhausted a budget.
    pub failed: usize,
    /// Captured `console.*` entries in execution order.
    pub console: Vec<ConsoleMessage>,
    /// Number of timer callbacks pumped.
    pub tasks_ran: usize,
    /// `true` when the total loop-iteration budget or the timer budget ran out.
    pub limited: bool,
    /// Event-loop iterations (timer batches + virtual-time steps).
    pub loop_iterations: usize,
    /// Virtual time advanced by the event loop (milliseconds).
    pub virtual_time_ms: u64,
    /// Timers still pending when the event loop stopped.
    pub timers_pending: usize,
    /// `location.assign/replace/reload` actions requested by scripts, in order.
    pub location_actions: Vec<LocationAction>,
    /// `history.pushState/replaceState/back/forward/go` actions, in order.
    pub history_actions: Vec<HistoryAction>,
    /// Lifecycle events successfully dispatched (`DOMContentLoaded`, `load`).
    pub events_dispatched: usize,
    /// Wall-clock duration of the event-loop stage (microtasks + events + timers).
    pub event_loop: std::time::Duration,
}
