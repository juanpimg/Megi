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
//! ## Pending (WP-B)
//!
//! Still minimal: `element.style` exposes the common CSS properties only (no full
//! `CSSStyleDeclaration`/`cssText`), there is no `getComputedStyle` provider, `NodeList`
//! live objects, `matches`/`closest`, `insertAdjacentHTML/Text` or wrapper identity cache,
//! and `removeEventListener` may remove more than one matching listener (documented
//! simplification). Those land in WP-B.
//!
//! ## Safety model
//!
//! The bridge lives in [`host`]: JS objects only carry a numeric side-table handle; the
//! real `NodeId` (generation-checked arena slot) stays in Rust and is revalidated with
//! `Document::try_node` on every access. A stale or foreign node therefore raises a typed
//! JS `TypeError` ([`JsError::Dom`] when surfaced from Rust) instead of panicking or
//! touching freed memory. Native callbacks reach the host through a per-thread scope
//! installed by `eval`, so parallel test threads never share a realm.
//!
//! ## Not included yet
//!
//! `<script>` execution during parsing / external scripts / `defer`/`async`, event
//! listeners and wall-clock timers, `getComputedStyle`, live `NodeList`s,
//! `MutationObserver`, modules, workers, fetch/XHR, and DOM↔JS wrapper identity caching
//! (each access returns a fresh wrapper). `render` integration is the next round.
//!
//! ## Feature layout
//!
//! The `js` feature is **enabled by default** and pulls `boa_engine`. With
//! `--no-default-features` the crate compiles as a stub: [`JsRuntime::new`] returns
//! [`JsError::FeatureDisabled`] and the DOM bridge is not compiled, which keeps the rest of
//! the workspace buildable without the JS engine.

#![forbid(unsafe_code)]

pub mod error;
#[cfg(feature = "js")]
pub mod events;
#[cfg(feature = "js")]
pub mod host;
#[cfg(feature = "js")]
pub mod runtime;

pub use error::JsError;
#[cfg(feature = "js")]
pub use host::ConsoleEntry;
#[cfg(feature = "js")]
pub use runtime::{Budget, JsRuntime};

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
}
