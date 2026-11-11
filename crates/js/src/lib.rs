//! # Megi JavaScript (`js`) — embedded runtime with minimal DOM bindings
//!
//! Phase 6 (WP-6.1 + base of WP-6.2): an embedded **Boa 0.21** ECMAScript runtime bound to
//! one [`dom::Document`], with a small, auditable DOM bridge and anti-DoS budgets.
//!
//! ## What works today
//!
//! - `JsRuntime::eval(script, source_name)` on a persistent realm (globals survive across
//!   calls); the completion value is stringified for diagnostics.
//! - `console.log/info/warn/error` captured into a buffer (`JsRuntime::take_console`).
//! - `window`/`document` globals; `document.write/writeln`, `getElementById`,
//!   `getElementsByTagName`/`ClassName`, `querySelector(All)`, `createElement`,
//!   `createTextNode`, `documentElement`/`body`/`head`, `title`, `URL`.
//! - DOM wrappers for nodes: `tagName`, `id`, `className`, `textContent`, `innerHTML`,
//!   `outerHTML`, `setAttribute`/`getAttribute`/`removeAttribute`/`hasAttribute`,
//!   `appendChild`/`insertBefore`/`removeChild`/`replaceChild`, `children`,
//!   `parentNode`/`firstChild`/`nextSibling`, `style` (common CSS properties) and
//!   `classList` (`add`/`remove`/`contains`/`toggle`/`length`).
//! - Budgets: Boa runtime limits (loop iterations, recursion, VM stack) applied per
//!   `eval`; violations map to [`JsError::Budget`]. No network, filesystem, timers-with-
//!   wall-clock or event APIs are exposed.
//! - **EventTarget (WP-A)**: `addEventListener`/`removeEventListener`/`dispatchEvent` on
//!   nodes, `document` and `window`; `Event`/`CustomEvent` with `preventDefault`,
//!   `stopPropagation` and `stopImmediatePropagation`; capture listeners run
//!   window→document→ancestors→target and bubble listeners target→ancestors→document→
//!   window. Listeners are stored as rooted `JsValue`s in the realm.
//! - **Timers with an injectable clock (WP-A)**: `setTimeout(fn, ms, ...args)`,
//!   `setInterval`, `clearTimeout`/`clearInterval`; the host scheduler orders callbacks by
//!   `(due, insertion id)`. [`JsRuntime::run_tasks`] executes what the real clock made due,
//!   while [`JsRuntime::advance`] / [`JsRuntime::run_until`] move the virtual clock and run
//!   what became due (deterministic tests); [`JsRuntime::pending_timers`] reports the queue.
//!   [`JsRuntime::run_microtasks`] runs Boa promise jobs.
//!
//! ## Rust-facing APIs (block 0, used by the HTML harness)
//!
//! - [`JsRuntime::eval_on_document`] evaluates a script against a **borrowed**
//!   `&mut dom::Document` (temporary realm, applies [`Budget`], writes the document back
//!   even when the script fails).
//! - [`JsRuntime::dispatch_simple_event`] dispatches a synthetic DOM event from Rust
//!   (`Some(NodeId)` targets a node and bubbles; `None` targets `window`, useful for
//!   `DOMContentLoaded`/`load`), returning whether the default was not prevented.
//! - [`JsRuntime::dispatch_event`] takes an explicit [`JsEventTarget`] (`Window`,
//!   `Document` or `Node(NodeId)`); `document` is a full EventTarget in JS too
//!   (`document.addEventListener`/`dispatchEvent`).
//! - [`JsRuntime::take_document`] extracts the runtime's document (e.g. after an
//!   instance-level [`JsRuntime::eval`]).
//!
//! ## WP-B: CSSOM and DOM utilities
//!
//! - `element.style` is a `CSSStyleDeclaration`-like object: common camelCase properties
//!   as accessors, plus `cssText` (get/set), `length`, `item(i)`,
//!   `getPropertyValue`/`setProperty`/`removeProperty` for **any** CSS property; it writes
//!   the inline `style` attribute, so it stays coherent with `getAttribute("style")`.
//!   Arbitrary `style.fooBar = ...` assignments are not intercepted (no Proxy): use
//!   `setProperty('foo-bar', value)` (documented limitation).
//! - `window.getComputedStyle(element)` returns a read-only declaration object:
//!   `getPropertyValue`, `item`, `length`, `cssText` and kebab-case data properties. The
//!   host injects the values with [`JsRuntime::set_computed_style_provider`]
//!   (`Box<dyn Fn(&Document, NodeId) -> Vec<(String, String)> + Send + Sync>`); without a
//!   provider it falls back to the element's inline declarations.
//! - `children`/`querySelectorAll`/`getElementsByTagName`/`getElementsByClassName` return
//!   **snapshot** `NodeList` objects (`length`, index access, `item`, `forEach`).
//! - `matches(selector)`, `closest(selector)`, `insertAdjacentHTML(where, html)` and
//!   `insertAdjacentText(where, text)` are available on node wrappers.
//! - **Wrapper identity cache**: one JS object per `(realm, NodeId)`, so
//!   `el === document.getElementById(id)` and `querySelector(...) === el` hold.
//!
//! Limits: NodeLists are snapshots (not live), `getComputedStyle` values depend entirely on
//! the injected provider (no style resolution inside `js`), and computed declarations are
//! read-only (`setProperty` throws a `TypeError`).
//!
//! ## Browser APIs (R95)
//!
//! - `window.matchMedia(query)` returns a `MediaQueryList` (`media`, `matches`) evaluated
//!   through [`JsRuntime::set_media_provider`] (`Box<dyn Fn(&str) -> bool + Send + Sync>`);
//!   `addListener`/`addEventListener` are accepted no-ops (no automatic `change` events).
//! - `window.innerWidth/innerHeight/devicePixelRatio` come from
//!   [`JsRuntime::set_viewport_provider`] (`Box<dyn Fn() -> ViewportInfo + Send + Sync>`,
//!   default 1280×720 @ DPR 1). `scrollTo`/`scrollBy` update host scroll offsets exposed
//!   as `pageXOffset/pageYOffset`; Rust reads them with [`JsRuntime::scroll_position`].
//! - `requestAnimationFrame(cb)`/`cancelAnimationFrame(id)` queue callbacks on the host
//!   scheduler 16 ms ahead of the **injected clock**; callbacks receive the virtual
//!   timestamp and [`JsRuntime::pending_animation_frames`] reports the queue.
//! - `document.onerror`/`window.onerror` run for uncaught exceptions in `eval`, timer
//!   callbacks and microtasks with `(message, source, lineno, colno, error)` and the error
//!   is also recorded in the console capture (`source` is `<timer>`/`<microtask>` for host
//!   callbacks). Other uncaught errors keep returning [`JsError`] from the host call.
//! - `document.readyState` starts at `loading`; the host sets it with
//!   [`JsRuntime::set_ready_state`]. `console.assert/debug/table` (and `dir`) map to the
//!   console capture levels.
//! - `window.getComputedStyle` provider format: `Vec<(String, String)>` with kebab-case CSS
//!   property names; it should cover color, background-color, font-size/family/weight/
//!   style, width/height, margin*/padding*, display, position, text-align, line-height,
//!   border-*-width/color/style and opacity (see the crate tests).
//!
//! ## Advanced events (R77)
//!
//! - `event.currentTarget` is the wrapper of the node/document/window currently visited in
//!   the capture or bubble leg (and `this` inside listeners matches it); `event.eventPhase`
//!   is 1/2/3 for capture/target/bubble and both reset to `null`/0 after dispatch.
//!   `event.target` stays the original target.
//! - `addEventListener(type, fn, { capture, once, passive })`: `once` listeners are removed
//!   after the first delivery; `passive` listeners cannot cancel the default action
//!   (`preventDefault` is ignored). `removeEventListener` now matches callback identity
//!   (and the optional capture flag). `signal`/`AbortSignal` is not supported.
//! - Inline handler properties `onclick`, `onload` and `onerror` exist on elements,
//!   `document` and `window`: assigning a function registers it (called with
//!   `this === element` and the event), assigning `null` removes it; the getter returns the
//!   current function (or `null`). Returning `false` does not cancel the default action
//!   (documented limitation; call `event.preventDefault()` instead).
//! - `document.close()` is a documented no-op (parsing is host-driven).
//! - `JsRuntime::estimated_heap_bytes()` returns a **rough** estimate of host-side memory
//!   (DOM nodes, wrapper tables, listeners, timers, console capture, strings); Boa 0.21 does
//!   not expose JS-heap statistics, which is a documented limitation.
//! - Dispatch robustness: `dispatchEvent` rejects non-`Event` arguments with a `TypeError`;
//!   ordinary exceptions thrown by listeners are reported to the console capture
//!   (`take_console`) and via `JsRuntime::last_event_error()` without interrupting the
//!   remaining listeners; budget errors (infinite loops) still abort the dispatch with
//!   [`JsError::Budget`].
//!
//! ## Feature layout
//!
//! The `js` feature is **enabled by default** and pulls `boa_engine`. With
//! `--no-default-features` the crate compiles as a stub: [`JsRuntime::new`] returns
//! [`JsError::FeatureDisabled`] and the DOM bridge is not compiled, which keeps the rest of
//! the workspace buildable without the JS engine.

#![forbid(unsafe_code)]

//! ## Round 24 browser APIs
//!
//! - `history.pushState/replaceState/back/forward/go` call an injectable
//!   `HistoryProvider`; without one they are a documented no-op. `history.length`/
//!   `state` reflect the latest push/replace.
//! - `location.href/pathname/search/hash/origin/protocol/host` derive from the
//!   document URL; `assign`/`replace`/`reload` call a `LocationProvider` and the host
//!   can dispatch `hashchange`/`popstate` via `dispatch_event`.
//! - `localStorage`/`sessionStorage`: per-realm maps, full Storage API, 1 MiB cap per
//!   realm (`QuotaExceededError`), optional `StorageProvider` persistence.
//! - `MutationObserver`: childList/attributes/characterData + subtree, disconnect,
//!   takeRecords; records are delivered when `run_microtasks` drains microtasks (in
//!   `render`, call it before the final re-render). `requestIdleCallback` is not
//!   implemented.
//!
//!
//! ## ES modules (Round 25)
//!
//! - `JsRuntime::eval_module(source, source_name, base_url, budget)` evaluates a module
//!   graph in the shared realm using an injected **resolver** (`specifier + importer URL →
//!   URL`) and **loader** (`URL → source text`); `js` never performs I/O.
//! - Subset: default/named/namespace/side-effect imports, `export
//!   const/let/var/function/class`, `export default`, `export { a as b }` and
//!   `import.meta.url`. Imports/exports must start their line; no dynamic `import()`, no
//!   top-level await, no re-exports (`export ... from`).
//! - Topological dependency order; **cycles are rejected** with
//!   `JsError::ModuleCycle`; missing modules/exports and malformed modules return typed
//!   errors (`ModuleNotFound`/`ModuleExport`/`ModuleSyntax`). `ModuleOutcome` exposes the
//!   entry URL and stringified exports. The host supplies all module text (docs/FUZZING).
pub mod error;
#[cfg(feature = "js")]
pub mod events;
#[cfg(feature = "js")]
pub mod host;
#[cfg(feature = "js")]
pub mod runtime;

pub use error::JsError;

/// Result of evaluating an ES module graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleOutcome {
    /// Resolved URL of the entry module.
    pub url: String,
    /// Exported bindings of the entry module (name → stringified value).
    pub exports: Vec<(String, String)>,
}
#[cfg(feature = "js")]
pub use events::JsEventTarget;
#[cfg(feature = "js")]
pub use host::ConsoleEntry;
#[cfg(feature = "js")]
pub use host::{MediaProvider, ViewportInfo, ViewportProvider};
#[cfg(feature = "js")]
pub use runtime::{Budget, JsRuntime};

/// Minimal stand-in types so the stub API keeps the same signatures without Boa.
#[cfg(not(feature = "js"))]
pub mod runtime {
    /// Stub budget (no engine to configure).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Budget;
}

#[cfg(not(feature = "js"))]
pub mod host {
    /// Stub module resolver type (never called in the feature-less build).
    pub type ModuleResolver =
        Box<dyn Fn(&str, &str) -> Result<Option<String>, crate::JsError> + Send + Sync>;
    /// Stub module loader type (never called in the feature-less build).
    pub type ModuleLoader = Box<dyn Fn(&str) -> Result<String, crate::JsError> + Send + Sync>;
}

/// Stub runtime used when the crate is built without the `js` feature.
#[cfg(not(feature = "js"))]
#[derive(Debug, Default)]
pub struct JsRuntime;

#[cfg(not(feature = "js"))]
impl JsRuntime {
    /// Always fails: the engine was compiled out with `--no-default-features`.
    pub fn new() -> Result<Self, JsError> {
        Err(JsError::FeatureDisabled)
    }

    /// Always fails: the engine was compiled out.
    pub fn eval(&mut self, _script: &str, _source_name: &str) -> Result<String, JsError> {
        Err(JsError::FeatureDisabled)
    }

    /// Always fails: ES modules are compiled out with `--no-default-features`.
    pub fn eval_module(
        &mut self,
        _source: &str,
        _source_name: &str,
        _base_url: &str,
        _budget: &crate::runtime::Budget,
    ) -> Result<crate::ModuleOutcome, JsError> {
        Err(JsError::FeatureDisabled)
    }

    /// Accepted as a no-op in the feature-less stub.
    pub fn set_module_resolver(&mut self, _resolver: crate::host::ModuleResolver) {}

    /// Accepted as a no-op in the feature-less stub.
    pub fn set_module_loader(&mut self, _loader: crate::host::ModuleLoader) {}
}
