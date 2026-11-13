//! # `<script>` discovery, loading and execution (Phase 6, host side of WP-6.2/6.3)
//!
//! The render pipeline executes the page's classic scripts **after parsing** and then
//! recomputes styles, layout and paint on the mutated DOM. This module owns the host side:
//!
//! 1. [`discover_scripts`] walks the parsed document in tree order and collects classic
//!    and module `<script>` elements (inline or `src`):
//!    - missing/empty `type`, `text/javascript`, `application/javascript`,
//!      `application/ecmascript`, `text/ecmascript` ⇒ classic script;
//!    - `type="module"` ⇒ ES module evaluated through the js crate's module subset
//!      (`eval_module` + host resolver/loader, R162);
//!    - any other `type` (JSON, templates, ...) ⇒ ignored, as browsers do;
//!    - `nomodule` only affects classic scripts (module scripts ignore it).
//! 2. [`load_scripts`] applies the loading policy: external sources are resolved against
//!    the **document base URL** and fetched with the shared [`net::ResourceLoader`]
//!    (`--no-network` is honoured, HTTP cache/redirects/TLS included), deduplicated by
//!    URL, and limited by [`ScriptingLimits`] caps (8 MiB per script / 32 MiB total /
//!    64 scripts by default). Failures are non-fatal warnings.
//! 3. [`execute_scripts`] runs the scripts in document order through
//!    [`js::JsRuntime::with_document`] with a per-script [`ScriptBudget`] *and* a total
//!    loop-iteration budget, then runs a **minimal event loop** on a deterministic virtual
//!    clock: promise microtasks → `DOMContentLoaded` on `document` → timers in `(due, id)`
//!    order (delays > 0 included, clock advanced in 1 ms virtual steps up to
//!    [`ScriptingLimits::max_virtual_time_ms`]) → `load` on `window` → remaining timers
//!    scheduled by `load` listeners → optional host events (e.g. `hashchange`/`popstate`)
//!    and their timers. Timer callbacks are bounded by [`ScriptingLimits::max_tasks`].
//!    **Microtask checkpoints** run after every script, before/inside every timer batch and
//!    after each host event (plus a final checkpoint before the document is returned), so
//!    `MutationObserver` records queued by those callbacks are delivered and their DOM
//!    mutations reach the re-render. The mutated [`Document`] is returned so the pipeline
//!    re-runs styles → layout → paint **once** at the end.
//!
//! Module scripts use a host `set_module_resolver` (relative/absolute specifiers resolved
//! with the net `Url` against the importing module URL; bare specifiers fail as
//! `ModuleNotFound`) and a host `set_module_loader` (`file://` from the filesystem,
//! `http(s)` through a `ResourceLoader` with the render's FetchOptions; `--no-network`,
//! per-module/total byte caps enforced). A failing module is a non-fatal warning; the
//! remaining scripts and the re-render continue.
//!
//! Host providers wired in this stage (R155): `History` (`history.pushState/replaceState/
//! back/forward/go`), `location.assign/replace/reload` and Web Storage. History/location
//! actions are queued and exposed through [`ScriptExecution::history_actions`] /
//! [`ScriptExecution::location_actions`] (the renderer never navigates); `localStorage` is
//! persisted only when the caller passes a shared [`RenderStorage`], while `sessionStorage`
//! is always a fresh per-render area.
//!
//! ## Deliberate limitations (WP-6.4 and later)
//!
//! - Scripts execute **after** the document has been parsed, not inline during parsing:
//!   therefore `document.write` only appends to the current tree (no re-tokenization) and
//!   the html5lib `scripted_*` cases that depend on parse-time execution are still out of
//!   scope.
//! - `async`/`defer` are recorded but every script runs in document order after parsing
//!   (there is no parser blocking yet); `async` ordering across external downloads is not
//!   emulated.
//! - The event loop is **virtual-time only** (no wall-clock waiting): the clock starts at
//!   0 ms when scripting begins, timers run in `(due, id)` order, and the loop stops when no
//!   tasks remain or the `max_tasks`/`max_virtual_time_ms` budgets are exhausted
//!   (`ScriptExecution::limited` plus a warning). Timer resolution is 1 ms. Promise jobs run
//!   after each script, after each dispatch and after every timer batch.
//! - Lifecycle events are dispatched from Rust through `js::JsRuntime::dispatch_event` with
//!   explicit targets: `DOMContentLoaded` at `document` (bubbling to `window`) and `load` at
//!   `window`, so both `document.addEventListener('DOMContentLoaded', …)` and
//!   `window.addEventListener('load'/'DOMContentLoaded', …)` work with browser-like capture/
//!   target/bubble phases. `event.currentTarget` is `null` inside listeners (use
//!   `event.target`), and inline `onclick`-style attributes are not executed.
//! - `<script type="module">` is supported for the js crate's ESM subset (one realm, each
//!   script is an independent module graph root; see `execute_scripts`). `<script
//!   type="importmap">` is parsed before modules (first map only; duplicates/invalid JSON are
//!   non-fatal warnings), dynamic `import()` accepts **string literals** (evaluated eagerly
//!   with the graph, Promise resolved immediately) and top-level await is a documented typed
//!   gap. Workers are not supported.
//! - Without the `js` crate feature this module compiles to a no-op: scripts are counted
//!   as skipped and the DOM is left untouched (documented in [`crate::ScriptingMode`]).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::{HistoryAction, LocationAction};

use css::ComputedStyle;
use dom::{Document, NodeId};
use net::{ResourceLoader, Scheme, Url};

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
const VIRTUAL_EPOCH_MS: u64 = 1_000_000;

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
    /// Conservative budget for small pages and tests (same values as `js::Budget::SMALL`).
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
    /// `localStorage` entries (`(key, value)` pairs, insertion order).
    pub local: Vec<(String, String)>,
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
    pub media: css::MediaContext,
    /// Viewport size in CSS pixels (`window.innerWidth`/`innerHeight`).
    pub viewport: (u32, u32),
    /// Shared `localStorage` (see [`RenderStorage`]); `None` = per-render memory only.
    pub storage: Option<Arc<Mutex<RenderStorage>>>,
    /// Fetch configuration used by the ES module loader (`http(s)` imports share the same
    /// timeouts/caps/cookie jar as the rest of the render).
    pub fetch_options: net::FetchOptions,
    /// `--no-network`: `http(s)` module imports fail with a non-fatal warning.
    pub no_network: bool,
    /// Enable deterministic cooperative Web Workers (file:// scripts).
    pub enable_workers: bool,
    /// Events dispatched on `window` by the host after `load` (e.g. `hashchange`,
    /// `popstate`) so their listeners can mutate the DOM before the final re-render.
    pub host_events: Vec<String>,
    /// Pre-script layout geometry (`NodeId → viewport rects`) installed into the JS
    /// runtime so scripts can measure the initial layout (viewport coords, scroll = 0).
    #[cfg(feature = "js")]
    pub layout:
        Option<std::sync::Arc<std::collections::HashMap<dom::NodeId, js::host::LayoutRect>>>,
}

impl ScriptingHostContext {
    /// Creates the host context for a render.
    pub fn new(
        styles: Option<Arc<std::collections::HashMap<NodeId, ComputedStyle>>>,
        media: css::MediaContext,
        viewport: (u32, u32),
    ) -> Self {
        Self {
            styles,
            media,
            viewport,
            storage: None,
            fetch_options: net::FetchOptions::default(),
            no_network: false,
            host_events: Vec::new(),
            enable_workers: true,
            #[cfg(feature = "js")]
            layout: None,
        }
    }

    /// Empty context for direct `execute_scripts` callers: no style snapshot, default media
    /// and a 800x600 viewport (matches the pipeline default).
    pub fn empty() -> Self {
        Self {
            styles: None,
            media: css::MediaContext::with_viewport(800.0, 600.0),
            viewport: (800, 600),
            storage: None,
            fetch_options: net::FetchOptions::default(),
            no_network: false,
            host_events: Vec::new(),
            enable_workers: true,
            #[cfg(feature = "js")]
            layout: None,
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

/// Returns `true` when `type_attr` denotes a classic JavaScript script.
fn is_classic_script_type(type_attr: Option<&str>) -> bool {
    let Some(raw) = type_attr else {
        return true;
    };
    // MIME types may carry parameters (`text/javascript; charset=utf-8`).
    let essence = raw
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    essence.is_empty()
        || matches!(
            essence.as_str(),
            "text/javascript"
                | "application/javascript"
                | "application/ecmascript"
                | "text/ecmascript"
                | "application/x-javascript"
        )
}

/// Returns `true` when `type_attr` explicitly requests an ES module.
fn is_module_script(type_attr: Option<&str>) -> bool {
    type_attr.is_some_and(|raw| {
        raw.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("module")
    })
}

/// Discovers classic and module `<script>` elements in document order.
///
/// Unsupported/ignored script types (JSON data blocks, templates, ...) are skipped
/// silently; `<script type="module">` elements are included and executed later through the
/// ES module subset. `warnings` is currently unused by this stage (kept for API stability).
pub fn discover_scripts(document: &Document, _warnings: &mut Vec<String>) -> Vec<ScriptCandidate> {
    let mut candidates = Vec::new();
    for element in document.get_elements_by_tag_name("script") {
        if candidates.len() >= 1024 {
            break;
        }
        let Some(data) = document.node(element).as_element() else {
            continue;
        };
        let type_attr = data.get_attribute("type");
        let is_module = is_module_script(type_attr);
        if !is_module && !is_classic_script_type(type_attr) {
            // JSON data blocks, templates and other non-JS types are ignored silently.
            continue;
        }
        if !is_module && data.has_attribute("nomodule") {
            // `nomodule` only affects classic scripts; module scripts ignore it.
            continue;
        }
        let src = data
            .get_attribute("src")
            .map(str::trim)
            .filter(|src| !src.is_empty())
            .map(str::to_owned);
        let inline_text = if src.is_none() {
            Some(document.text_content(element))
        } else {
            None
        };
        candidates.push(ScriptCandidate {
            element,
            inline_text,
            src,
            deferred: data.has_attribute("defer"),
            is_async: data.has_attribute("async"),
            is_module,
        });
    }
    candidates
}

/// Loads the external scripts of `candidates` and applies the byte/count/duplication caps.
///
/// `loader` must be the shared [`net::ResourceLoader`] so script fetches share the HTTP
/// cache with stylesheets/images/backgrounds. `no_network` blocks `http(s)` script sources
/// with a warning (offline rendering). All failures are non-fatal.
pub fn load_scripts(
    candidates: &[ScriptCandidate],
    base_url: &Url,
    loader: &mut ResourceLoader,
    limits: ScriptingLimits,
    no_network: bool,
    warnings: &mut Vec<String>,
) -> ScriptLoadReport {
    let mut report = ScriptLoadReport::default();
    let mut seen_urls: HashSet<String> = HashSet::new();
    // `max_scripts` bounds the scripts that execute; `max_script_attempts` bounds the
    // fetches independently so failing URLs cannot starve later valid scripts.
    let mut attempts = 0usize;
    for (index, candidate) in candidates.iter().enumerate() {
        if report.scripts.len() >= limits.max_scripts {
            report.skipped += 1;
            warnings.push(format!(
                "page references more than {} scripts; only the first {} execute (cap)",
                limits.max_scripts, limits.max_scripts
            ));
            break;
        }
        let source_name;
        let text;
        let mut module_url = None;
        if let Some(src) = &candidate.src {
            let url = match base_url.join(src) {
                Ok(url) => url,
                Err(error) => {
                    report.skipped += 1;
                    warnings.push(format!("skipping script {src:?}: invalid URL ({error})"));
                    continue;
                }
            };
            if no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
                report.skipped += 1;
                warnings.push(format!("skipping script {url} (--no-network is set)"));
                continue;
            }
            let cache_key = url.cache_key();
            if !seen_urls.insert(cache_key) {
                report.skipped += 1;
                warnings.push(format!("skipping duplicate script {url} (already loaded)"));
                continue;
            }
            if attempts >= limits.max_script_attempts {
                report.skipped += 1;
                warnings.push(format!(
                    "page attempted more than {} script loads; the remaining scripts are not fetched (cap)",
                    limits.max_script_attempts
                ));
                break;
            }
            attempts += 1;
            let resource = match loader.load(&url) {
                Ok(resource) => resource,
                Err(error) => {
                    report.skipped += 1;
                    warnings.push(format!("skipping script {url}: {error}"));
                    continue;
                }
            };
            if resource.bytes.len() > limits.max_script_bytes {
                report.skipped += 1;
                warnings.push(format!(
                    "skipping script {url}: {} bytes exceeds max_script_bytes={}",
                    resource.bytes.len(),
                    limits.max_script_bytes
                ));
                continue;
            }
            source_name = url.as_str();
            if candidate.is_module {
                module_url = Some(url.as_str().to_owned());
            }
            text = match String::from_utf8(resource.bytes.clone()) {
                Ok(text) => text,
                Err(error) => {
                    warnings.push(format!(
                        "script {url} is not valid UTF-8; decoding lossily ({error})"
                    ));
                    String::from_utf8_lossy(&resource.bytes).into_owned()
                }
            };
        } else {
            let Some(inline) = &candidate.inline_text else {
                continue;
            };
            source_name = format!("inline-script-{index}");
            text = inline.clone();
            if candidate.is_module {
                module_url = Some(base_url.as_str().to_owned());
            }
        }

        if text.trim().is_empty() {
            continue;
        }
        if text.len() > limits.max_script_bytes {
            report.skipped += 1;
            warnings.push(format!(
                "skipping script {source_name}: {} bytes exceeds max_script_bytes={}",
                text.len(),
                limits.max_script_bytes
            ));
            continue;
        }
        if report.script_bytes + text.len() > limits.max_total_bytes {
            report.skipped += 1;
            warnings.push(format!(
                "skipping script {source_name}: total script bytes would exceed max_total_bytes={}",
                limits.max_total_bytes
            ));
            continue;
        }
        report.script_bytes += text.len();
        report.scripts.push(LoadedScript {
            bytes: text.len(),
            text,
            source_name,
            is_module: candidate.is_module,
            module_url,
        });
    }
    report
}

/// Executes `scripts` against `document` and returns the mutated document.
///
/// When the `js` feature is enabled this creates a [`js::JsRuntime`] bound to the document,
/// evaluates every script with the configured budgets, drains promise microtasks and a
/// bounded number of timer callbacks, captures `console` output, and hands the document
/// back. Without the feature it is a no-op that reports the scripts as skipped.
/// Returns the JSON source of the first `<script type="importmap">`, recording a warning
/// for duplicates or unparsable maps. Import maps must appear before module scripts; this
/// helper scans the document once up front (the executor runs scripts after parsing).
#[cfg(feature = "js")]
fn import_map_source(
    document: &dom::Document,
    max_bytes: usize,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let mut maps = Vec::new();
    for node in document.descendants(document.root()) {
        let Some(element) = document.try_node(node).and_then(|node| node.as_element()) else {
            continue;
        };
        if element.local_name != "script"
            || element
                .get_attribute("type")
                .is_none_or(|kind| !kind.eq_ignore_ascii_case("importmap"))
        {
            continue;
        }
        maps.push(document.text_content(node));
    }
    if maps.len() > 1 {
        warnings.push(format!(
            "multiple import maps ({}); only the first one is used",
            maps.len()
        ));
    }
    let source = maps.into_iter().next()?;
    if source.len() > max_bytes {
        // Import maps bypassed the classic/inline script caps: reject an oversized map
        // before parsing it (issue #465).
        warnings.push(format!(
            "import map is {} bytes, larger than the {max_bytes}-byte script cap; ignored",
            source.len()
        ));
        return None;
    }
    match js::ImportMap::from_json(&source) {
        Ok(_) => Some(source),
        Err(error) => {
            warnings.push(format!("invalid import map: {error}"));
            None
        }
    }
}

/// Whether a `file://` fetch stays inside the document's directory.
///
/// Canonicalising both paths also rejects symlinks that escape the directory; a missing
/// document or target path denies the request (the loader then surfaces its usual 404).
#[cfg(feature = "js")]
fn file_fetch_allowed(base: &Url, requested: &Url) -> bool {
    let Ok(base_path) = net::file::path_from_file_url(base) else {
        return false;
    };
    let Ok(requested_path) = net::file::path_from_file_url(requested) else {
        return false;
    };
    let Some(directory) = base_path.parent() else {
        return false;
    };
    let (Ok(directory), Ok(requested_path)) = (
        std::fs::canonicalize(directory),
        std::fs::canonicalize(&requested_path),
    ) else {
        return false;
    };
    requested_path.starts_with(&directory)
}

/// Resolves a Web Worker script for the host under the same local-file policy as every
/// other subresource, bounded by the per-script byte cap.
#[cfg(feature = "js")]
fn load_worker_source(
    url: &str,
    allow_file: bool,
    max_bytes: usize,
) -> Result<String, js::JsError> {
    let parsed = Url::parse(url)
        .map_err(|error| js::JsError::Runtime(format!("worker script URL {url}: {error}")))?;
    if parsed.scheme() != Scheme::File {
        return Err(js::JsError::Runtime(format!(
            "worker scripts only support file:// in render: {url}"
        )));
    }
    if !allow_file {
        return Err(js::JsError::Runtime(format!(
            "worker script {url}: local file access is blocked for this document"
        )));
    }
    let bytes = net::load_file_url_with_limit(&parsed, max_bytes)
        .map_err(|error| js::JsError::Runtime(format!("worker script {url}: {error}")))?;
    String::from_utf8(bytes)
        .map_err(|error| js::JsError::Runtime(format!("worker script {url} is not UTF-8: {error}")))
}

#[cfg(feature = "js")]
pub fn execute_scripts(
    document: Document,
    document_url: &Url,
    scripts: Vec<LoadedScript>,
    limits: ScriptingLimits,
    host: &ScriptingHostContext,
    warnings: &mut Vec<String>,
) -> ScriptExecution {
    let mut execution = ScriptExecution {
        document,
        executed: 0,
        failed: 0,
        console: Vec::new(),
        tasks_ran: 0,
        limited: false,
        loop_iterations: 0,
        virtual_time_ms: 0,
        timers_pending: 0,
        events_dispatched: 0,
        event_loop: std::time::Duration::ZERO,
        location_actions: Vec::new(),
        history_actions: Vec::new(),
    };
    if scripts.is_empty() {
        return execution;
    }

    // Import maps are installed before any module is evaluated (R187). Only the first
    // `<script type="importmap">` is used; duplicates/invalid JSON are non-fatal warnings.
    let import_map_json = import_map_source(&execution.document, limits.max_script_bytes, warnings);

    // Keep the original document so a runtime-construction failure is not fatal.
    let mut runtime = match js::JsRuntime::with_document(
        execution.document.clone(),
        document_url.as_str().to_owned(),
    ) {
        Ok(runtime) => runtime,
        Err(error) => {
            warnings.push(format!("cannot start the JavaScript runtime: {error}"));
            return execution;
        }
    };

    if let Some(json) = &import_map_json
        && let Err(error) = runtime.set_import_map_json(json)
    {
        warnings.push(format!("invalid import map: {error}"));
    }

    // Browser-facing host providers (R96): getComputedStyle reads the pre-script snapshot,
    // matchMedia evaluates with the render's MediaContext (same viewport as the cascade),
    // and innerWidth/innerHeight/devicePixelRatio come from the real viewport (DPR 1.0).
    if let Some(styles) = host.styles.clone() {
        runtime.set_computed_style_provider(Box::new(move |document, node_id| {
            crate::browser_host::computed_style_for(&styles, document, node_id)
        }));
    }
    let media = host.media;
    runtime.set_media_provider(Box::new(move |query: &str| {
        crate::browser_host::media_matches(&media, query)
    }));
    let viewport_info = js::host::ViewportInfo {
        width: host.viewport.0,
        height: host.viewport.1,
        device_pixel_ratio: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };
    runtime.set_viewport_provider(Box::new(move || viewport_info));
    if host.enable_workers {
        // R203: fetch() provider backed by the render's effective FetchOptions. Scripts run
        // synchronously; the Promise settles when the host drains microtasks. CORS policy is
        // same-origin only (cross-origin http(s) fetch rejects, documented); file:// misses
        // surface as a synthetic 404 so scripts can inspect `response.ok`.
        {
            let base = document_url.clone();
            let options = host.fetch_options.clone();
            let no_network = host.no_network;
            // `fetch` volume is budgeted per page: the per-call loader caps bound one response,
            // but a script could still issue thousands of calls (#660).
            let fetch_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let fetch_bytes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let max_fetch_calls = limits.max_fetch_calls;
            let max_fetch_bytes = limits.max_fetch_bytes;
            runtime.set_fetch_provider(Box::new(move |request| {
                let calls = fetch_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if calls > max_fetch_calls {
                    return Err(js::JsError::Runtime(format!(
                        "fetch budget exhausted ({max_fetch_calls} calls per page)"
                    )));
                }
                let charge = |body: &[u8]| -> Result<(), js::JsError> {
                    let total = fetch_bytes
                        .fetch_add(body.len(), std::sync::atomic::Ordering::Relaxed)
                        + body.len();
                    if total > max_fetch_bytes {
                        return Err(js::JsError::Runtime(format!(
                            "fetch budget exhausted ({max_fetch_bytes} bytes per page)"
                        )));
                    }
                    Ok(())
                };
                let url = Url::parse(&request.url)
                    .map_err(|error| js::JsError::Runtime(format!("fetch URL: {error}")))?;
                if matches!(url.scheme(), Scheme::Http | Scheme::Https) && no_network {
                    return Err(js::JsError::Runtime(
                        "network disabled (--no-network)".to_string(),
                    ));
                }
                let same_origin = match (base.scheme(), url.scheme()) {
                    (Scheme::Http | Scheme::Https, Scheme::Http | Scheme::Https) => {
                        base.scheme() == url.scheme()
                            && base.host() == url.host()
                            && base.port_or_default() == url.port_or_default()
                    }
                    // Same scheme; the directory confinement is enforced below with a
                    // uniform 404 so out-of-directory and missing files are indistinguishable.
                    (Scheme::File, Scheme::File) => true,
                    _ => false,
                };
                if !same_origin {
                    return Err(js::JsError::Runtime(
                        "CORS: cross-origin fetch denied (same-origin only)".to_string(),
                    ));
                }
                // `file://` reads are confined to the document's directory. Answer any
                // out-of-directory request with the same synthetic 404 as a missing file,
                // so scripts cannot read (or probe the existence of) arbitrary local files.
                if matches!(url.scheme(), Scheme::File) && !file_fetch_allowed(&base, &url) {
                    return Ok(js::host::FetchResult {
                        status: 404,
                        status_text: "Not Found".to_string(),
                        url: request.url,
                        headers: Vec::new(),
                        body: Vec::new(),
                    });
                }
                if matches!(url.scheme(), Scheme::Http | Scheme::Https) {
                    let header_refs: Vec<(&str, &str)> = request
                        .headers
                        .iter()
                        .map(|(name, value)| (name.as_str(), value.as_str()))
                        .collect();
                    let response = net::http::fetch_with_method(
                        &url,
                        &options,
                        &request.method,
                        &header_refs,
                        request.body.as_deref(),
                    )
                    .map_err(|error| js::JsError::Runtime(format!("network: {error}")))?;
                    charge(&response.body)?;
                    return Ok(js::host::FetchResult {
                        status: response.status,
                        status_text: response.reason,
                        url: response.final_url.to_string(),
                        headers: response.headers,
                        body: response.body,
                    });
                }
                let mut loader =
                    ResourceLoader::with_options(options.clone()).with_allow_file(base.is_file());
                match loader.load_str(&request.url) {
                    Ok(resource) => {
                        charge(&resource.bytes)?;
                        let mut headers =
                            vec![("content-type".to_string(), resource.content_type.clone())];
                        headers.truncate(64);
                        Ok(js::host::FetchResult {
                            status: 200,
                            status_text: "OK".to_string(),
                            url: resource.url.to_string(),
                            headers,
                            body: resource.bytes.clone(),
                        })
                    }
                    Err(_) => Ok(js::host::FetchResult {
                        status: 404,
                        status_text: "Not Found".to_string(),
                        url: request.url,
                        headers: Vec::new(),
                        body: Vec::new(),
                    }),
                }
            }));
        }
        let worker_allow_file = document_url.is_file();
        let max_worker_bytes = limits.max_script_bytes;
        runtime.set_worker_loader(Box::new(move |url| {
            load_worker_source(url, worker_allow_file, max_worker_bytes)
        }));
    }
    #[cfg(feature = "js")]
    if let Some(layout) = host.layout.clone() {
        runtime.set_layout_provider(Box::new(move |node_id| layout.get(&node_id).copied()));
    }

    // Browser-facing host providers (R155): History/location actions are queued for the
    // host (the shell decides whether to navigate/reload after the render) and Web Storage
    // is persisted for `localStorage` when the caller shares `RenderOptions::storage`;
    // `sessionStorage` is always a fresh per-render in-memory area.
    let location_actions: Arc<Mutex<Vec<LocationAction>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let queue = Arc::clone(&location_actions);
        runtime.set_location_provider(Box::new(move |action| {
            queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(match action {
                    js::host::LocationAction::Assign(url) => LocationAction::Assign(url),
                    js::host::LocationAction::Replace(url) => LocationAction::Replace(url),
                    js::host::LocationAction::Reload => LocationAction::Reload,
                });
        }));
    }
    let history_actions: Arc<Mutex<Vec<HistoryAction>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let queue = Arc::clone(&history_actions);
        runtime.set_history_provider(Box::new(move |action| {
            queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(match action {
                    js::host::HistoryAction::Push { state, url } => {
                        HistoryAction::Push { state, url }
                    }
                    js::host::HistoryAction::Replace { state, url } => {
                        HistoryAction::Replace { state, url }
                    }
                    js::host::HistoryAction::Back => HistoryAction::Back,
                    js::host::HistoryAction::Forward => HistoryAction::Forward,
                    js::host::HistoryAction::Go(delta) => HistoryAction::Go(delta),
                });
        }));
    }
    let per_render_local: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let per_render_session: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let shared_for_load = host.storage.clone();
        let shared_for_save = host.storage.clone();
        let local_load = Arc::clone(&per_render_local);
        let session_load = Arc::clone(&per_render_session);
        let local_save = Arc::clone(&per_render_local);
        let session_save = Arc::clone(&per_render_session);
        runtime.set_storage_provider(js::host::StorageProvider {
            load: Box::new(move |kind| match kind {
                js::host::StorageKind::Local => match &shared_for_load {
                    Some(storage) => storage
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .local
                        .clone(),
                    None => local_load
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .clone(),
                },
                js::host::StorageKind::Session => session_load
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone(),
            }),
            save: Box::new(move |kind, entries| match kind {
                js::host::StorageKind::Local => match &shared_for_save {
                    Some(storage) => {
                        storage
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .local = entries.to_vec();
                    }
                    None => {
                        *local_save
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = entries.to_vec();
                    }
                },
                js::host::StorageKind::Session => {
                    *session_save
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = entries.to_vec();
                }
            }),
        });
    }
    // ES module support (R162): relative/absolute specifiers resolve against the importing
    // module URL (or the document URL for inline modules) with the net `Url`; the loader
    // reads `file://` from disk or `http(s)` through a dedicated `ResourceLoader` configured
    // with the render's FetchOptions (`--no-network` and size caps are enforced). Each
    // `<script type="module">` is an independent root (no shared graph between scripts).
    if scripts.iter().any(|script| script.is_module) {
        runtime.set_module_resolver(Box::new(
            |specifier: &str, base: &str| -> Result<Option<String>, js::JsError> {
                let resolved = match Url::parse(specifier) {
                    Ok(url) => Some(url),
                    Err(_) => Url::parse(base)
                        .ok()
                        .and_then(|base_url| base_url.join(specifier).ok()),
                };
                Ok(resolved
                    .filter(|url| {
                        matches!(
                            url.scheme(),
                            Scheme::File | Scheme::Http | Scheme::Https | Scheme::Data
                        )
                    })
                    .map(|url| url.as_str().to_owned()))
            },
        ));
        let module_loader = Arc::new(Mutex::new(
            ResourceLoader::with_options(host.fetch_options.clone())
                .with_allow_file(document_url.is_file()),
        ));
        let no_network = host.no_network;
        let max_module_bytes = limits.max_script_bytes;
        let total_module_bytes = Arc::new(Mutex::new(0usize));
        let max_total_module_bytes = limits.max_total_bytes;
        // The module graph is fetched by the JS module loader, outside `load_scripts`: an
        // unbounded graph of tiny/empty modules would otherwise bypass the attempt cap.
        let module_fetches = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_module_fetches = limits.max_module_fetches;
        runtime.set_module_loader(Box::new(
            move |url_str: &str| -> Result<String, js::JsError> {
                if module_fetches.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    >= max_module_fetches
                {
                    return Err(js::JsError::ModuleNotFound(format!(
                        "module graph exceeded max_module_fetches={max_module_fetches}"
                    )));
                }
                let url = Url::parse(url_str).map_err(|error| {
                    js::JsError::ModuleNotFound(format!("invalid module URL {url_str}: {error}"))
                })?;
                if no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
                    return Err(js::JsError::ModuleNotFound(format!(
                        "cannot load module {url}: --no-network is set"
                    )));
                }
                let resource = module_loader
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .load(&url)
                    .map_err(|error| {
                        js::JsError::ModuleNotFound(format!("cannot load module {url}: {error}"))
                    })?;
                if resource.bytes.len() > max_module_bytes {
                    return Err(js::JsError::ModuleNotFound(format!(
                        "module {url}: {} bytes exceeds max_script_bytes={max_module_bytes}",
                        resource.bytes.len()
                    )));
                }
                let text = String::from_utf8(resource.bytes.clone()).map_err(|error| {
                    js::JsError::ModuleNotFound(format!("module {url} is not valid UTF-8: {error}"))
                })?;
                let mut total = total_module_bytes
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *total = total.saturating_add(text.len());
                if *total > max_total_module_bytes {
                    return Err(js::JsError::ModuleNotFound(format!(
                        "module graph exceeds max_total_bytes={max_total_module_bytes}"
                    )));
                }
                Ok(text)
            },
        ));
    }
    runtime.set_ready_state("loading");

    // Deterministic virtual clock: pin `now` to a fixed epoch before any script runs (the
    // host clock never goes backwards, so `run_until(0)` would keep the real startup
    // elapsed time and make timer/rAF due times machine-speed dependent).
    let _ = runtime.run_until(VIRTUAL_EPOCH_MS);

    let mut remaining_loops = limits.total_loop_iterations;
    // Timers pumped while driving top-level `await` count against the page timer budget.
    let mut module_tasks = 0usize;
    let mut module_virtual_ms = 0u64;
    for script in scripts {
        let grant = limits.budget.loop_iterations.min(remaining_loops);
        if grant == 0 {
            execution.limited = true;
            warnings.push(
                "total JavaScript loop-iteration budget exhausted; skipping remaining scripts"
                    .to_owned(),
            );
            break;
        }
        remaining_loops -= grant;
        let budget = js::Budget {
            loop_iterations: grant,
            recursion_limit: limits.budget.recursion_limit,
            stack_size_limit: limits.budget.stack_size_limit,
        };
        runtime.set_budget(budget);
        let result = if script.is_module {
            let base = script
                .module_url
                .clone()
                .unwrap_or_else(|| document_url.as_str());
            if script.text.contains("await") {
                // Real top-level await: native Boa modules (live bindings included). The
                // deterministic host loop drives its microtasks/timers to completion, with
                // the page's remaining timer budget so it cannot overshoot `max_tasks`.
                let module_budget = js::ModuleAsyncBudget {
                    timer_tasks: limits.max_tasks.saturating_sub(module_tasks),
                    virtual_time_ms: limits.max_virtual_time_ms.saturating_sub(module_virtual_ms),
                };
                runtime
                    .eval_module_async_with_budget(
                        &script.text,
                        &script.source_name,
                        &base,
                        &budget,
                        module_budget,
                    )
                    .map(Some)
            } else {
                runtime
                    .eval_module(&script.text, &script.source_name, &base, &budget)
                    .map(|_| None)
            }
        } else {
            runtime
                .eval(&script.text, &script.source_name)
                .map(|_| None)
        };
        match result {
            Ok(Some(evaluated)) => {
                execution.executed += 1;
                module_tasks = module_tasks.saturating_add(evaluated.timer_tasks);
                module_virtual_ms = module_virtual_ms.saturating_add(evaluated.virtual_time_ms);
                if evaluated.limited {
                    execution.limited = true;
                    warnings.push(
                        "top-level await timer budget exhausted; module may be incomplete"
                            .to_owned(),
                    );
                }
            }
            Ok(None) => execution.executed += 1,
            Err(error) => {
                execution.failed += 1;
                if matches!(error, js::JsError::Budget(_)) {
                    execution.limited = true;
                }
                let kind = if script.is_module { "module" } else { "script" };
                warnings.push(format!("{kind} {} failed: {error}", script.source_name));
            }
        }
        // Promise jobs scheduled by the script (microtask checkpoint).
        drain_microtasks(&mut runtime, warnings);
    }

    let event_loop_start = std::time::Instant::now();
    // Lifecycle events and timers run with a fresh budget: the per-script loop budget must
    // not be exhausted by the last script when its `DOMContentLoaded` listener fires.
    runtime.set_budget(js::Budget {
        loop_iterations: limits.budget.loop_iterations,
        recursion_limit: limits.budget.recursion_limit,
        stack_size_limit: limits.budget.stack_size_limit,
    });

    let mut loop_state = EventLoopState {
        // Timer callbacks already spent driving top-level `await` count against the budget.
        tasks: module_tasks,
        virtual_time_ms: VIRTUAL_EPOCH_MS.saturating_add(module_virtual_ms),
        ..EventLoopState::default()
    };
    drain_microtasks(&mut runtime, warnings);

    runtime.set_ready_state("interactive");
    // 1. `DOMContentLoaded` (after scripts, before timers), dispatched at the real
    // `document` target with bubbling: `document.addEventListener('DOMContentLoaded', ...)`
    // listeners fire, then capture/bubble visitors and (with `bubbles: true`) the single
    // bubble leg to `window`, exactly like a browser.
    match runtime.dispatch_event(js::JsEventTarget::Document, "DOMContentLoaded", true) {
        Ok(_) => execution.events_dispatched += 1,
        Err(error) => warnings.push(format!("dispatching DOMContentLoaded failed: {error}")),
    }
    drain_microtasks(&mut runtime, warnings);

    // 2. Timers (delays included) on the virtual clock, bounded by max_tasks/max_virtual_time.
    run_event_loop(&mut runtime, &limits, &mut loop_state, warnings);

    runtime.set_ready_state("complete");
    // 3. `load` on `window`, then timers scheduled by `load` listeners (same budget).
    match runtime.dispatch_event(js::JsEventTarget::Window, "load", true) {
        Ok(_) => execution.events_dispatched += 1,
        Err(error) => warnings.push(format!("dispatching load failed: {error}")),
    }
    drain_microtasks(&mut runtime, warnings);
    run_event_loop(&mut runtime, &limits, &mut loop_state, warnings);

    // 4. Host-dispatched events (e.g. `hashchange`/`popstate` after a shell navigation).
    // Listeners run on `window`, may mutate the DOM and schedule timers; microtasks and the
    // timer queue are pumped again so the final re-render sees their effects.
    for event in &host.host_events {
        match runtime.dispatch_event(js::JsEventTarget::Window, event, false) {
            Ok(_) => execution.events_dispatched += 1,
            Err(error) => warnings.push(format!("dispatching host event {event} failed: {error}")),
        }
        drain_microtasks(&mut runtime, warnings);
        run_event_loop(&mut runtime, &limits, &mut loop_state, warnings);
    }
    // 5. Deterministic cooperative workers: drain main→worker and worker→main queues,
    // then pump timers/microtasks scheduled by worker callbacks before the re-render.
    if host.enable_workers {
        match runtime.run_worker_tasks(limits.max_tasks) {
            Ok(delivered) => {
                loop_state.tasks = loop_state.tasks.saturating_add(delivered);
            }
            Err(error) => warnings.push(format!("worker tasks failed: {error}")),
        }
        drain_microtasks(&mut runtime, warnings);
        run_event_loop(&mut runtime, &limits, &mut loop_state, warnings);
    }
    // Final microtask checkpoint: MutationObserver records queued by the last timer/host
    // callback callbacks are delivered **before** the document is handed back, so their
    // own DOM mutations are included in the re-render.
    drain_microtasks(&mut runtime, warnings);

    if let Some(error) = runtime.last_event_error() {
        warnings.push(format!("uncaught script error: {error}"));
    }
    execution.event_loop = event_loop_start.elapsed();
    execution.tasks_ran = loop_state.tasks;
    execution.loop_iterations = loop_state.iterations;
    execution.virtual_time_ms = loop_state.virtual_time_ms.saturating_sub(VIRTUAL_EPOCH_MS);
    execution.limited |= loop_state.limited;
    execution.timers_pending = runtime.pending_timers();

    for entry in runtime.take_console() {
        let message = ConsoleMessage {
            level: entry.level.clone(),
            message: entry.message,
        };
        if matches!(message.level.as_str(), "warn" | "error") {
            warnings.push(format!("console.{}: {}", message.level, message.message));
        }
        execution.console.push(message);
    }
    execution.document = std::mem::take(&mut *runtime.document_mut());
    execution.location_actions = std::mem::take(
        &mut *location_actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    );
    execution.history_actions = std::mem::take(
        &mut *history_actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    );
    execution
}

/// Progress of one event-loop drain (shared by the initial timers and the `load` pass).
#[cfg(feature = "js")]
#[derive(Default)]
struct EventLoopState {
    tasks: usize,
    iterations: usize,
    virtual_time_ms: u64,
    limited: bool,
}

#[cfg(feature = "js")]
fn drain_microtasks(runtime: &mut js::JsRuntime, warnings: &mut Vec<String>) {
    if let Err(error) = runtime.run_microtasks() {
        warnings.push(format!("microtasks failed: {error}"));
    }
}

/// Runs due timers until the queue is empty or the task/virtual-time budgets are exhausted.
///
/// Order: pending microtasks are drained before each batch, then every timer due at the
/// current virtual time runs in `(due, id)` order; when nothing is due the clock advances
/// [`VIRTUAL_TIME_STEP_MS`] and the timers that just became due run. The clock never
/// exceeds `max_virtual_time_ms` and the callbacks never exceed `max_tasks`; hitting either
/// cap sets [`EventLoopState::limited`] and leaves the remaining timers pending.
#[cfg(feature = "js")]
fn run_event_loop(
    runtime: &mut js::JsRuntime,
    limits: &ScriptingLimits,
    state: &mut EventLoopState,
    warnings: &mut Vec<String>,
) {
    loop {
        drain_microtasks(runtime, warnings);
        let remaining = limits.max_tasks.saturating_sub(state.tasks);
        if remaining == 0 {
            if runtime.pending_timers() > 0 {
                state.limited = true;
                warnings.push(format!(
                    "timer task budget exhausted ({} tasks); dropping remaining callbacks",
                    limits.max_tasks
                ));
            }
            return;
        }
        // Only run timers that are due on the *virtual* clock. `run_tasks`
        // follows the wall clock, so on a slow host it can fire a timer before
        // its delay has elapsed on the virtual timeline, making the number of
        // executed callbacks depend on machine speed (this was caught by CI:
        // the same render produced different task counts on different runs).
        match runtime.run_until_with_budget(state.virtual_time_ms, remaining) {
            Ok(0) => {}
            Ok(ran) => {
                state.tasks += ran;
                state.iterations += 1;
                continue;
            }
            Err(error) => {
                warnings.push(format!("timer callback failed: {error}"));
                state.limited = true;
                return;
            }
        }
        if runtime.pending_timers() == 0 {
            return;
        }
        if state.virtual_time_ms.saturating_sub(VIRTUAL_EPOCH_MS) >= limits.max_virtual_time_ms {
            state.limited = true;
            warnings.push(format!(
                "virtual-time budget exhausted ({} ms); dropping {} pending timer(s)",
                limits.max_virtual_time_ms,
                runtime.pending_timers()
            ));
            return;
        }
        let next = state.virtual_time_ms.saturating_add(VIRTUAL_TIME_STEP_MS);
        let remaining = limits.max_tasks.saturating_sub(state.tasks);
        match runtime.run_until_with_budget(next, remaining) {
            Ok(ran) => {
                state.tasks += ran;
                state.iterations += 1;
                // The loop advances the virtual clock by exactly one step, so
                // the timeline does not depend on how long the host took.
                state.virtual_time_ms = next;
            }
            Err(error) => {
                warnings.push(format!("timer callback failed: {error}"));
                state.limited = true;
                return;
            }
        }
    }
}

/// No-op execution used when the crate is built without the `js` feature.
#[cfg(not(feature = "js"))]
pub fn execute_scripts(
    document: Document,
    _document_url: &Url,
    scripts: Vec<LoadedScript>,
    _limits: ScriptingLimits,
    _host: &ScriptingHostContext,
    warnings: &mut Vec<String>,
) -> ScriptExecution {
    if !scripts.is_empty() {
        warnings.push(format!(
            "skipping {} script(s): the engine was built without the `js` feature",
            scripts.len()
        ));
    }
    ScriptExecution {
        document,
        executed: 0,
        failed: 0,
        console: Vec::new(),
        tasks_ran: 0,
        limited: false,
        loop_iterations: 0,
        virtual_time_ms: 0,
        timers_pending: 0,
        location_actions: Vec::new(),
        history_actions: Vec::new(),
        events_dispatched: 0,
        event_loop: std::time::Duration::ZERO,
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn document_with(html: &str) -> Document {
        html::parse_document(html, false)
    }

    #[cfg(not(feature = "js"))]
    #[test]
    fn scripting_defaults_to_disabled_without_the_feature() {
        assert_eq!(
            crate::ScriptingMode::default(),
            crate::ScriptingMode::Disabled
        );
    }

    #[cfg(feature = "js")]
    #[test]
    fn oversized_import_maps_are_ignored() {
        let document = document_with(
            "<body><script type='importmap'>{\"imports\":{\"a\":\"b\"}}</script></body>",
        );
        let mut warnings = Vec::new();
        assert!(import_map_source(&document, 8, &mut warnings).is_none());
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("import map is")),
            "{warnings:?}"
        );
        let mut warnings = Vec::new();
        assert!(import_map_source(&document, 1024, &mut warnings).is_some());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn discovers_classic_inline_and_external_scripts_in_order() {
        let doc = document_with(
            "<html><head><script src='a.js'></script><script type='text/javascript'>x()</script></head>\
             <body><script type='application/json'>{}</script><script type='module'>m()</script>\
             <script nomodule>n()</script><script>y()</script></body></html>",
        );
        let mut warnings = Vec::new();
        let candidates = discover_scripts(&doc, &mut warnings);
        // Classic scripts + the inline module are candidates; `application/json`,
        // `nomodule` and unsupported types are skipped.
        assert_eq!(candidates.len(), 4);
        assert_eq!(candidates[0].src.as_deref(), Some("a.js"));
        assert_eq!(candidates[1].inline_text.as_deref(), Some("x()"));
        assert!(candidates[2].is_module);
        assert_eq!(candidates[2].inline_text.as_deref(), Some("m()"));
        assert_eq!(candidates[3].inline_text.as_deref(), Some("y()"));
        assert!(
            warnings.is_empty(),
            "module discovery is not a warning: {warnings:?}"
        );
    }

    #[cfg(feature = "js")]
    #[test]
    fn worker_loader_enforces_the_file_policy() {
        let path = std::env::temp_dir().join(format!("megi-worker-{}.js", std::process::id()));
        std::fs::write(&path, "self.onmessage = function () {};").expect("write worker");
        let url = format!("file://{}", path.display());

        let blocked = load_worker_source(&url, false, 4096).expect_err("blocked");
        assert!(blocked.to_string().contains("blocked"), "{blocked}");
        let source = load_worker_source(&url, true, 4096).expect("allowed");
        assert!(source.contains("onmessage"), "{source}");
        assert!(load_worker_source(&url, true, 4).is_err(), "cap");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_scripts_applies_network_and_duplicate_policy() {
        let doc = document_with(
            "<html><body><script src='/a.js'></script><script src='/a.js'></script>\
             <script src='https://example.com/b.js'></script><script>x()</script></body></html>",
        );
        let base = Url::parse("file:///tmp/index.html").expect("file URL");
        let mut loader = ResourceLoader::with_options(net::FetchOptions::default());
        let mut warnings = Vec::new();
        let candidates = discover_scripts(&doc, &mut warnings);
        let report = load_scripts(
            &candidates,
            &base,
            &mut loader,
            ScriptingLimits::default(),
            true,
            &mut warnings,
        );
        // /a.js is skipped (missing file) and the duplicate/newtork script too; only inline runs.
        assert_eq!(report.scripts.len(), 1);
        assert_eq!(report.scripts[0].text, "x()");
        assert_eq!(report.scripts[0].source_name, "inline-script-3");
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("--no-network"))
        );
    }
    #[cfg(feature = "js")]
    #[test]
    fn file_fetch_is_confined_to_the_document_directory() {
        let root = std::env::temp_dir().join(format!("megi-file-fetch-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).expect("mkdir");
        std::fs::write(site.join("index.html"), "x").expect("doc");
        std::fs::write(site.join("data.txt"), "x").expect("inside");
        std::fs::write(root.join("secret.txt"), "x").expect("outside");
        let base = Url::parse(&format!("file://{}/index.html", site.display())).expect("base");
        let inside = Url::parse(&format!("file://{}/data.txt", site.display())).expect("inside");
        let outside =
            Url::parse(&format!("file://{}/secret.txt", root.display())).expect("outside");
        let system = Url::parse("file:///etc/passwd").expect("system");
        assert!(file_fetch_allowed(&base, &inside));
        assert!(!file_fetch_allowed(&base, &outside));
        assert!(!file_fetch_allowed(&base, &system));
        // A `..` traversal that resolves outside the directory is denied too.
        let traversal = Url::parse(&format!("file://{}/site/../secret.txt", root.display()))
            .expect("traversal");
        assert!(!file_fetch_allowed(&base, &traversal));
        let _ = std::fs::remove_dir_all(&root);
    }
}
