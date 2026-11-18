//! Embedded JavaScript runtime (Boa) with minimal DOM bindings.
//!
//! ## Architecture
//!
//! - One **realm** per [`JsRuntime`]; the runtime owns the [`megi_core::Document`] that scripts
//!   mutate through native functions.
//! - The `NodeId` ↔ JS object bridge is a **side table** in [`crate::host`]: JS wrappers
//!   only carry a small numeric handle; the generation-checked `NodeId` stays in Rust and is
//!   revalidated with `Document::try_node` on every access. Stale handles produce a typed
//!   JS `TypeError`, never a panic.
//! - Native functions borrow the host from the `Context` they receive (stored in Boa's
//!   `HostDefined` data); nested iframe/worker realms therefore always reach their own
//!   host, and tests run in parallel threads with independent runtimes.
//!
//! ## Anti-DoS budgets
//!
//! Boa 0.21 no longer exposes an interrupt handler; the equivalent limits are its
//! [`boa_engine::vm::RuntimeLimits`]: loop-iteration limit (fresh budget per `eval`),
//! recursion limit and VM stack-size limit. Exhausting them maps to
//! [`JsError::Budget`]. There is no general memory accounting: the DOM parser and the
//! handle side-table are capped separately ([`crate::host`]), `document.write` has a
//! cumulative per-document budget (`Host::max_document_write_bytes`, 8 MiB, and
//! `Host::max_document_write_nodes`, 1M; past it writes are ignored with a single console
//! warning), and scripts cannot allocate network or filesystem resources (no such APIs are
//! exposed). The `innerHTML`/`outerHTML`/`insertAdjacentHTML` setters share the same
//! cumulative parse budget and node cap as `document.write`.
//!
//! ## Deliberate limits (this WP)
//!
//! - `innerHTML`/`outerHTML`, `textContent`, attributes, `appendChild`/`insertBefore`/
//!   `removeChild`/`replaceChild`, `children`/`parentNode`/`firstChild`/`nextSibling`,
//!   `style` (common properties) and `classList` are provided; there are no event
//!   listeners, no `MutationObserver`, no NodeList live objects (arrays are returned) and
//!   no `getComputedStyle`.
//! - `setTimeout`/`setInterval` enqueue timers and `run_tasks` executes due callbacks; full
//!   event-loop/timer integration with the browser is the next WP.
//! - Scripts execute synchronously from [`JsRuntime::eval`]; parser-inserted `<script>` and
//!   external scripts are not integrated into `render` yet.

use boa_engine::{
    Context, JsError as BoaError, JsNativeError, JsObject, JsResult, JsValue, NativeFunction,
    Source,
    builtins::promise::PromiseState,
    js_string,
    module::{Module, ModuleLoader, Referrer},
    object::FunctionObjectBuilder,
    object::ObjectInitializer,
    object::builtins::{AlignedVec, JsArray, JsArrayBuffer, JsUint8Array},
    property::{Attribute, PropertyDescriptor},
};
use megi_core::{Attribute as DomAttribute, Document, Namespace, NodeId};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::rc::Rc;

use crate::ModuleOutcome;
use crate::error::JsError;
use crate::host::{
    ConsoleEntry, HistoryAction, Host, LocationAction, MAX_OBSERVERS, MutationKind, MutationRecord,
    ObserverRegistration, ObserverTarget, SandboxFlags, StorageKind, StorageProvider, host_cell,
    host_ref,
};

impl From<BoaError> for JsError {
    fn from(error: BoaError) -> Self {
        classify_error(&error.to_string())
    }
}

/// Native Boa function signature used by the bindings.
type NativeFn = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;
/// Getter/setter pair for a JS accessor property.
type AccessorPair = (NativeFn, Option<NativeFn>);

/// Maximum source name length accepted by [`JsRuntime::eval`].
const MAX_SOURCE_NAME: usize = 128;
/// Maximum source bytes accepted by `eval`/module/import-map entry points (issue #569).
const MAX_JS_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// Anti-DoS budget for script execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Maximum loop iterations per `eval` (Boa loop-iteration limit).
    pub loop_iterations: u64,
    /// Maximum JS call recursion depth.
    pub recursion_limit: usize,
    /// Boa VM stack-size limit (abstract units, Boa default is 10_240).
    pub stack_size_limit: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            loop_iterations: 5_000_000,
            recursion_limit: 512,
            stack_size_limit: 20_480,
        }
    }
}

impl Budget {
    /// A conservative budget for small pages and tests.
    pub const SMALL: Self = Self {
        loop_iterations: 100_000,
        recursion_limit: 256,
        stack_size_limit: 10_240,
    };
}

/// Timer/time budget for the deterministic loop that drives top-level `await`.
#[derive(Clone, Copy, Debug)]
pub struct ModuleAsyncBudget {
    /// Maximum timer callbacks pumped while the module promise is pending.
    pub timer_tasks: usize,
    /// Maximum virtual milliseconds the waiting loop may advance.
    pub virtual_time_ms: u64,
}

impl Default for ModuleAsyncBudget {
    fn default() -> Self {
        Self {
            timer_tasks: 10_000,
            virtual_time_ms: 1_000,
        }
    }
}

/// Result of [`JsRuntime::eval_module_async_with_budget`].
#[derive(Debug)]
pub struct ModuleAsyncOutcome {
    /// The evaluated module.
    pub outcome: ModuleOutcome,
    /// Timer callbacks pumped while waiting for top-level `await`.
    pub timer_tasks: usize,
    /// Virtual milliseconds advanced while waiting.
    pub virtual_time_ms: u64,
    /// Whether the waiting loop stopped because its timer/time budget ran out.
    pub limited: bool,
}

struct CspEvalHooks {
    allow_eval: Rc<std::cell::Cell<bool>>,
}

impl boa_engine::context::HostHooks for CspEvalHooks {
    fn ensure_can_compile_strings(
        &self,
        _realm: boa_engine::realm::Realm,
        _parameters: &[boa_engine::JsString],
        _body: &boa_engine::JsString,
        _direct: bool,
        _context: &mut Context,
    ) -> JsResult<()> {
        if self.allow_eval.get() {
            Ok(())
        } else {
            Err(JsNativeError::eval()
                .with_message(
                    "Content-Security-Policy: 'unsafe-eval' is required for eval/Function",
                )
                .into())
        }
    }
}

/// Embedded JavaScript runtime bound to one DOM document.
pub struct JsRuntime {
    /// Boa context (single realm). It owns the shared host cell in `HostDefined`.
    context: Context,
    /// Execution budget applied before each `eval`.
    budget: Budget,
    /// Whether `eval` / `new Function` are allowed by CSP (`'unsafe-eval'`).
    allow_eval: Rc<std::cell::Cell<bool>>,
    /// Last JS exception message (diagnostics).
    last_error: Option<String>,
}

/// Maps a Boa error message to a typed [`JsError`].
fn classify_error(message: &str) -> JsError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("loop iteration limit")
        || lower.contains("runtime limit")
        || lower.contains("runtimelimiterror")
        || lower.contains("iteration loops")
        || lower.contains("stack size")
        || lower.contains("maximum call stack")
        || lower.contains("recursion")
    {
        return JsError::Budget(message.to_string());
    }
    if lower.contains("syntaxerror") || lower.contains("syntax error") {
        return JsError::Syntax(message.to_string());
    }
    JsError::Runtime(message.to_string())
}

// ---------------------------------------------------------------------------
// Binding modules (split verbatim from the former monolithic runtime.rs)
// ---------------------------------------------------------------------------
mod api;
mod cssom;
mod document;
mod dom_utils;
mod frames;
mod gap_apis;
mod geometry;
mod globals;
mod history;
mod location_storage;
mod module_eval;
mod module_parse;
mod modules;
mod network;
mod node_accessors;
mod node_methods;
mod node_wrappers;
mod observers;
mod tla;
mod workers;

// Re-export every binding item so all intra-runtime paths and the public
// `runtime::{JsRuntime, Budget, ...}` API keep resolving unchanged.
pub(crate) use cssom::*;
pub(crate) use document::*;
pub(crate) use dom_utils::*;
pub(crate) use frames::*;
pub(crate) use gap_apis::*;
pub(crate) use geometry::*;
pub(crate) use globals::*;
pub(crate) use history::*;
pub(crate) use location_storage::*;
pub(crate) use module_eval::*;
pub(crate) use module_parse::*;
pub(crate) use modules::*;
pub(crate) use network::*;
pub(crate) use node_accessors::*;
pub(crate) use node_methods::*;
pub(crate) use node_wrappers::*;
pub(crate) use observers::*;
pub(crate) use tla::*;
pub(crate) use workers::*;
