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

pub mod error;
#[cfg(feature = "js")]
pub mod events;
#[cfg(feature = "js")]
pub mod host;
#[cfg(feature = "js")]
pub mod runtime;

pub use error::JsError;
#[cfg(feature = "js")]
pub use events::JsEventTarget;
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
