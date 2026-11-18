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
//!    the **document base URL** and fetched with the shared [`megi_net::ResourceLoader`]
//!    (`--no-network` is honoured, HTTP cache/redirects/TLS included), deduplicated by
//!    URL, and limited by [`ScriptingLimits`] caps (8 MiB per script / 32 MiB total /
//!    64 scripts by default). Failures are non-fatal warnings.
//! 3. [`execute_scripts`] runs the scripts in document order through
//!    [`megi_js::JsRuntime::with_document`] with a per-script [`ScriptBudget`] *and* a total
//!    loop-iteration budget, then runs a **minimal event loop** on a deterministic virtual
//!    clock: promise microtasks → `DOMContentLoaded` on `document` → timers in `(due, id)`
//!    order (delays > 0 included, clock advanced in 1 ms virtual steps up to
//!    [`ScriptingLimits::max_virtual_time_ms`]) → `load` on `window` → remaining timers
//!    scheduled by `load` listeners → optional host events (e.g. `hashchange`/`popstate`)
//!    and their timers. Timer callbacks are bounded by [`ScriptingLimits::max_tasks`].
//!    **Microtask checkpoints** run after every script, before/inside every timer batch and
//!    after each host event (plus a final checkpoint before the document is returned), so
//!    `MutationObserver` records queued by those callbacks are delivered and their DOM
//!    mutations reach the re-render. The mutated [`megi_core::Document`] is returned so the pipeline
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
//! - Lifecycle events are dispatched from Rust through `megi_js::JsRuntime::dispatch_event` with
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

mod classic;
#[cfg(feature = "js")]
mod event_loop;
mod execution;
#[cfg(feature = "js")]
mod lifecycle;
#[cfg(feature = "js")]
mod modules;
#[cfg(feature = "js")]
mod network;
#[cfg(feature = "js")]
mod providers;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use classic::MAX_DISCOVERED_SCRIPTS;
pub use classic::{discover_scripts, load_scripts, load_scripts_with_csp};
pub use execution::execute_scripts;
#[cfg(all(test, feature = "js"))]
pub(crate) use modules::{file_fetch_allowed, import_map_source, load_worker_source};
#[cfg(all(test, feature = "js"))]
pub(crate) use providers::storage_origin_key;
#[cfg(feature = "js")]
pub(crate) use types::VIRTUAL_EPOCH_MS;
pub use types::{
    ConsoleMessage, DEFAULT_MAX_SCRIPT_BYTES, DEFAULT_MAX_SCRIPTS, DEFAULT_MAX_TASKS,
    DEFAULT_MAX_TOTAL_LOOP_ITERATIONS, DEFAULT_MAX_TOTAL_SCRIPT_BYTES, DEFAULT_MAX_VIRTUAL_TIME_MS,
    LoadedScript, RenderStorage, ScriptBudget, ScriptCandidate, ScriptExecution, ScriptLoadReport,
    ScriptingHostContext, ScriptingLimits, VIRTUAL_TIME_STEP_MS,
};
#[cfg(feature = "js")]
pub use types::{TimelineEvent, TimelineEventKind, TimelineEventsProvider};
