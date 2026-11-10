//! # `<script>` discovery, loading and execution (Phase 6, host side of WP-6.2/6.3)
//!
//! The render pipeline executes the page's classic scripts **after parsing** and then
//! recomputes styles, layout and paint on the mutated DOM. This module owns the host side:
//!
//! 1. [`discover_scripts`] walks the parsed document in tree order and collects classic
//!    `<script>` elements (inline or `src`):
//!    - missing/empty `type`, `text/javascript`, `application/javascript`,
//!      `application/ecmascript`, `text/ecmascript` ⇒ classic script;
//!    - `type="module"` ⇒ skipped with a warning (modules are not implemented);
//!    - any other `type` (JSON, templates, ...) ⇒ ignored, as browsers do;
//!    - `nomodule` scripts are ignored (we only support classic scripts).
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
//!    scheduled by `load` listeners. Timer callbacks are bounded by
//!    [`ScriptingLimits::max_tasks`]. The mutated [`Document`] is returned so the pipeline
//!    re-runs styles → layout → paint **once** at the end.
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
//! - Script types/modules/workers/imports/network APIs are not supported.
//! - Without the `js` crate feature this module compiles to a no-op: scripts are counted
//!   as skipped and the DOM is left untouched (documented in [`crate::ScriptingMode`]).

use std::collections::HashSet;
use std::sync::Arc;

use css::ComputedStyle;
use dom::{Document, NodeId};
use net::{ResourceLoader, Scheme, Url};

/// Default maximum size of one script source (8 MiB).
pub const DEFAULT_MAX_SCRIPT_BYTES: usize = 8 * 1024 * 1024;
/// Default total script byte budget per page (32 MiB).
pub const DEFAULT_MAX_TOTAL_SCRIPT_BYTES: usize = 32 * 1024 * 1024;
/// Default maximum number of scripts executed per page.
pub const DEFAULT_MAX_SCRIPTS: usize = 64;
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

/// Host environment exposed to page scripts (WP-6.5): the **pre-script computed style
/// snapshot**, the render's media context and the real viewport.
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
        }
    }

    /// Empty context for direct `execute_scripts` callers: no style snapshot, default media
    /// and a 800x600 viewport (matches the pipeline default).
    pub fn empty() -> Self {
        Self {
            styles: None,
            media: css::MediaContext::with_viewport(800.0, 600.0),
            viewport: (800, 600),
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

/// Discovers classic `<script>` elements in document order.
///
/// Unsupported/ignored scripts are skipped (module scripts also emit a warning) and
/// appended to `warnings`.
pub fn discover_scripts(document: &Document, warnings: &mut Vec<String>) -> Vec<ScriptCandidate> {
    let mut candidates = Vec::new();
    for element in document.get_elements_by_tag_name("script") {
        let Some(data) = document.node(element).as_element() else {
            continue;
        };
        let type_attr = data.get_attribute("type");
        if is_module_script(type_attr) {
            warnings
                .push("skipping <script type=\"module\">: modules are not supported".to_owned());
            continue;
        }
        if !is_classic_script_type(type_attr) {
            // JSON data blocks, templates and other non-JS types are ignored silently.
            continue;
        }
        if data.has_attribute("nomodule") {
            // Classic-only engine: `nomodule` fallbacks are intentionally ignored.
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
    };
    if scripts.is_empty() {
        return execution;
    }

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
    runtime.set_ready_state("loading");

    // Deterministic virtual clock: pin `now` to a fixed epoch before any script runs (the
    // host clock never goes backwards, so `run_until(0)` would keep the real startup
    // elapsed time and make timer/rAF due times machine-speed dependent).
    let _ = runtime.run_until(VIRTUAL_EPOCH_MS);

    let mut remaining_loops = limits.total_loop_iterations;
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
        runtime.set_budget(js::Budget {
            loop_iterations: grant,
            recursion_limit: limits.budget.recursion_limit,
            stack_size_limit: limits.budget.stack_size_limit,
        });
        match runtime.eval(&script.text, &script.source_name) {
            Ok(_) => execution.executed += 1,
            Err(error) => {
                execution.failed += 1;
                if matches!(error, js::JsError::Budget(_)) {
                    execution.limited = true;
                }
                warnings.push(format!("script {} failed: {error}", script.source_name));
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
        virtual_time_ms: VIRTUAL_EPOCH_MS,
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
        match runtime.run_until(state.virtual_time_ms) {
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
        match runtime.run_until(next) {
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

    #[test]
    fn discovers_classic_inline_and_external_scripts_in_order() {
        let doc = document_with(
            "<html><head><script src='a.js'></script><script type='text/javascript'>x()</script></head>\
             <body><script type='application/json'>{}</script><script type='module'>m()</script>\
             <script nomodule>n()</script><script>y()</script></body></html>",
        );
        let mut warnings = Vec::new();
        let candidates = discover_scripts(&doc, &mut warnings);
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].src.as_deref(), Some("a.js"));
        assert_eq!(candidates[1].inline_text.as_deref(), Some("x()"));
        assert_eq!(candidates[2].inline_text.as_deref(), Some("y()"));
        assert!(warnings.iter().any(|warning| warning.contains("module")));
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
}
