//! Embedded JavaScript runtime (Boa) with minimal DOM bindings.
//!
//! ## Architecture
//!
//! - One **realm** per [`JsRuntime`]; the runtime owns the [`dom::Document`] that scripts
//!   mutate through native functions.
//! - The `NodeId` ↔ JS object bridge is a **side table** in [`crate::host`]: JS wrappers
//!   only carry a small numeric handle; the generation-checked `NodeId` stays in Rust and is
//!   revalidated with `Document::try_node` on every access. Stale handles produce a typed
//!   JS `TypeError`, never a panic.
//! - Native functions access the host through a thread-local installed for the duration of
//!   an `eval`; tests run in parallel threads with independent runtimes.
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
    object::builtins::JsArray,
    property::{Attribute, PropertyDescriptor},
};
use dom::{Attribute as DomAttribute, Document, Namespace, NodeId};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::rc::Rc;

use crate::error::JsError;
use crate::host::{
    ConsoleEntry, HistoryAction, Host, HostScope, LocationAction, MutationKind, MutationRecord,
    ObserverRegistration, ObserverTarget, StorageKind, StorageProvider, with_host,
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

/// Embedded JavaScript runtime bound to one DOM document.
pub struct JsRuntime {
    /// Boa context (single realm).
    context: Context,
    /// Shared host state.
    host: Rc<RefCell<Host>>,
    /// Execution budget applied before each `eval`.
    budget: Budget,
    /// Last JS exception message (diagnostics).
    last_error: Option<String>,
}

impl JsRuntime {
    /// Creates a runtime for an empty HTML document.
    pub fn new() -> Result<Self, JsError> {
        let mut document = Document::new();
        // Provide a minimal document shell so bindings always have html/head/body.
        let html = document.create_element(Namespace::Html, "html", vec![]);
        let head = document.create_element(Namespace::Html, "head", vec![]);
        let body = document.create_element(Namespace::Html, "body", vec![]);
        document.append_child(document.root(), html);
        document.append_child(html, head);
        document.append_child(html, body);
        Self::with_document(document, "about:blank".to_string())
    }

    /// Creates a runtime around an existing document (single realm).
    pub fn with_document(document: Document, url: String) -> Result<Self, JsError> {
        let host_slot: Rc<RefCell<Option<Rc<RefCell<Host>>>>> = Rc::new(RefCell::new(None));
        let module_loader = Rc::new(MegiModuleLoader {
            host: Rc::clone(&host_slot),
            base_url: url.clone(),
            cache: RefCell::new(HashMap::new()),
        });
        let mut context = Context::builder()
            .module_loader(module_loader)
            .build()
            .map_err(|error| JsError::Internal(error.to_string()))?;
        let host = Rc::new(RefCell::new(Host::new(document, url)));
        *host_slot.borrow_mut() = Some(Rc::clone(&host));
        install_globals(&mut context, &host)?;
        Ok(Self {
            context,
            host,
            budget: Budget::default(),
            last_error: None,
        })
    }

    /// Replaces the execution budget used by subsequent `eval` calls.
    pub fn set_budget(&mut self, budget: Budget) {
        self.budget = budget;
    }

    /// Returns the current budget.
    pub const fn budget(&self) -> Budget {
        self.budget
    }

    /// Read-only access to the document.
    pub fn document(&self) -> std::cell::Ref<'_, Document> {
        std::cell::Ref::map(self.host.borrow(), |host| &host.document)
    }

    /// Mutable access to the document.
    pub fn document_mut(&self) -> std::cell::RefMut<'_, Document> {
        std::cell::RefMut::map(self.host.borrow_mut(), |host| &mut host.document)
    }

    /// Drains the captured `console.*` entries.
    pub fn take_console(&mut self) -> Vec<ConsoleEntry> {
        self.host.borrow_mut().take_console()
    }

    /// Sets `document.title`.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.host.borrow_mut().title = title.into();
    }

    /// Returns the last JS exception message, if any.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Evaluates `script` and returns the completion value as a string (diagnostics).
    ///
    /// `source_name` is used for stack traces only (bounded to avoid huge source URLs).
    pub fn eval(&mut self, script: &str, source_name: &str) -> Result<String, JsError> {
        if script.len() > MAX_JS_SOURCE_BYTES {
            return Err(JsError::Budget("script source exceeds byte cap".to_owned()));
        }
        self.context
            .runtime_limits_mut()
            .set_loop_iteration_limit(self.budget.loop_iterations);
        self.context
            .runtime_limits_mut()
            .set_recursion_limit(self.budget.recursion_limit);
        self.context
            .runtime_limits_mut()
            .set_stack_size_limit(self.budget.stack_size_limit);

        let name: String = source_name.chars().take(MAX_SOURCE_NAME).collect();
        let source = Source::from_bytes(script);
        let _scope = HostScope::enter(&self.host);
        let result = self.context.eval(source);
        drop(_scope);

        match result {
            Ok(value) => {
                self.last_error = None;
                let text = value.to_string(&mut self.context).map_or_else(
                    |_| "<unprintable>".to_string(),
                    |value| value.to_std_string_escaped(),
                );
                Ok(text)
            }
            Err(error) => {
                let message = error.to_string();
                let typed = classify_error(&message);
                let _scope = HostScope::enter(&self.host);
                let _ = crate::events::report_uncaught(&mut self.context, &message, &name);
                drop(_scope);
                self.last_error = Some(message);
                Err(typed)
            }
        }
    }

    /// Executes timers whose due time has arrived (real clock by default).
    pub fn run_tasks(&mut self, max_steps: usize) -> Result<usize, JsError> {
        let _scope = HostScope::enter(&self.host);
        let deadline = crate::events::now_ms();
        let result = crate::events::run_due_timers(&mut self.context, deadline, max_steps);
        if let Err(error) = &result {
            let message = error.to_string();
            let _ = crate::events::report_uncaught(&mut self.context, &message, "<timer>");
        }
        drop(_scope);
        result
    }

    /// Installs the computed-style provider used by `window.getComputedStyle(element)`.
    ///
    /// The provider receives the live `Document` and the element `NodeId` and returns
    /// CSS property/value pairs (e.g. from `css::ComputedStyle`). Without a provider,
    /// `getComputedStyle` falls back to the element's inline declarations (read-only).
    pub fn set_computed_style_provider(&mut self, provider: crate::host::ComputedStyleProvider) {
        self.host.borrow_mut().computed_style_provider = Some(provider);
    }

    /// Installs the provider used by `window.matchMedia(query).matches`.
    ///
    /// The provider receives the raw media-query string and returns whether it currently
    /// matches the host viewport. Without a provider every query reports `matches: false`.
    /// `change` events are not fired automatically (the host re-evaluates and can re-run
    /// scripts); `addListener`/`addEventListener` are accepted as no-ops.
    pub fn set_media_provider(&mut self, provider: crate::host::MediaProvider) {
        self.host.borrow_mut().media_provider = Some(provider);
    }

    /// Installs the provider used by `window.innerWidth/innerHeight/devicePixelRatio`
    /// (and the base for `pageXOffset/pageYOffset`). Without a provider the documented
    /// default is 1280×720 at DPR 1.0.
    pub fn set_viewport_provider(&mut self, provider: crate::host::ViewportProvider) {
        // The provider's scroll offsets are the initial scroll position; `scrollTo`/
        // `scrollBy` then move the host-controlled offsets (exposed as pageX/YOffset).
        let initial = provider();
        let mut host = self.host.borrow_mut();
        host.scroll_x = initial.scroll_x;
        host.scroll_y = initial.scroll_y;
        host.viewport_provider = Some(provider);
    }

    /// Installs the History API provider (`pushState`/`replaceState`/`back`/`forward`/`go`).
    /// Without a provider those methods are a documented no-op.
    pub fn set_history_provider(&mut self, provider: crate::host::HistoryProvider) {
        self.host.borrow_mut().history_provider = Some(provider);
    }

    /// Installs the `location.assign/replace/reload` provider.
    pub fn set_location_provider(&mut self, provider: crate::host::LocationProvider) {
        self.host.borrow_mut().location_provider = Some(provider);
    }

    /// Installs optional Web Storage persistence and loads the current snapshots.
    pub fn set_storage_provider(&mut self, provider: StorageProvider) {
        let local = (provider.load)(StorageKind::Local);
        let session = (provider.load)(StorageKind::Session);
        let mut host = self.host.borrow_mut();
        host.local_storage = local.into_iter().collect();
        host.session_storage = session.into_iter().collect();
        host.storage_provider = Some(provider);
    }

    /// Current `document.readyState` (`loading` until the host changes it).
    pub fn ready_state(&self) -> String {
        self.host.borrow().ready_state.clone()
    }

    /// Sets `document.readyState` (`loading`, `interactive` or `complete`).
    pub fn set_ready_state(&mut self, state: impl Into<String>) {
        self.host.borrow_mut().ready_state = state.into();
    }

    /// Scroll offsets written by `window.scrollTo`/`scrollBy` (CSS pixels).
    pub fn scroll_position(&self) -> (f64, f64) {
        let host = self.host.borrow();
        (host.scroll_x, host.scroll_y)
    }

    /// Number of pending `requestAnimationFrame` callbacks.
    pub fn pending_animation_frames(&self) -> usize {
        self.host
            .borrow()
            .timers
            .iter()
            .filter(|timer| timer.animation)
            .count()
    }

    /// Takes the runtime's document out (the host keeps an empty one afterwards).
    pub fn take_document(&mut self) -> Document {
        let mut host = self.host.borrow_mut();
        std::mem::replace(&mut host.document, Document::new())
    }

    /// Evaluates `script` against a **borrowed** document without taking ownership.
    ///
    /// A temporary runtime/realm is created around the document, the script runs with
    /// `budget`, and the (possibly mutated) document is written back before returning.
    /// This is the entry point used by the HTML harness to execute `<script>` elements
    /// found in a parsed document.
    pub fn eval_on_document(
        document: &mut Document,
        script: &str,
        source_name: &str,
        budget: &Budget,
    ) -> Result<(), JsError> {
        let owned = std::mem::replace(document, Document::new());
        let mut runtime = Self::with_document(owned, "about:blank".to_string())?;
        runtime.set_budget(*budget);
        let result = runtime.eval(script, source_name).map(|_| ());
        *document = runtime.take_document();
        result
    }

    /// Dispatches a synthetic DOM event from Rust to the realm's registered listeners.
    ///
    /// `node = Some(id)` targets that node and bubbles through its ancestors;
    /// `node = None` targets `window` (`DOMContentLoaded`/`load` style notifications).
    /// Returns whether the event's default was **not** prevented.
    pub fn dispatch_simple_event(
        &mut self,
        node: Option<NodeId>,
        event_type: &str,
        bubbles: bool,
    ) -> Result<bool, JsError> {
        let target = match node {
            Some(node) => crate::events::JsEventTarget::Node(node),
            None => crate::events::JsEventTarget::Window,
        };
        self.dispatch_event(target, event_type, bubbles)
    }

    /// Dispatches a synthetic DOM event to an explicit [`crate::events::JsEventTarget`].
    ///
    /// The target can be `Window`, `Document` (full EventTarget, useful for
    /// `DOMContentLoaded`) or `Node(id)`. With `bubbles`, events dispatched on
    /// `document` continue to `window`, and node events bubble to document/window.
    /// Returns `Ok(true)` when the event's default was not prevented.
    pub fn dispatch_event(
        &mut self,
        target: crate::events::JsEventTarget,
        event_type: &str,
        bubbles: bool,
    ) -> Result<bool, JsError> {
        let _scope = HostScope::enter(&self.host);
        let result =
            crate::events::dispatch_rust_event(&mut self.context, target, event_type, bubbles);
        drop(_scope);
        result
    }

    /// Runs pending promise/microtask jobs (Boa's job queue).
    pub fn run_microtasks(&mut self) -> Result<(), JsError> {
        let _scope = HostScope::enter(&self.host);
        let result = match self.context.run_jobs() {
            Ok(()) => Ok(()),
            Err(error) => {
                let message = error.to_string();
                let _scope = HostScope::enter(&self.host);
                let _ = crate::events::report_uncaught(&mut self.context, &message, "<microtask>");
                drop(_scope);
                let typed = classify_error(&message);
                self.last_error = Some(message);
                Err(typed)
            }
        };
        drop(_scope);
        if result.is_ok() {
            let _scope = HostScope::enter(&self.host);
            let delivered = deliver_mutation_records(&mut self.context);
            drop(_scope);
            delivered?;
            let _ = self.run_worker_tasks(64);
        }
        result
    }

    /// Advances the injectable virtual clock by `delta` and runs the timers now due.
    pub fn advance(&mut self, delta: std::time::Duration) -> Result<u64, JsError> {
        let _scope = HostScope::enter(&self.host);
        let result = crate::events::advance_clock(&mut self.context, delta);
        if let Err(error) = &result {
            let message = error.to_string();
            let _ = crate::events::report_uncaught(&mut self.context, &message, "<timer>");
        }
        drop(_scope);
        result
    }

    /// Moves the virtual clock to `deadline_ms` and runs every timer due up to then.
    pub fn run_until(&mut self, deadline_ms: u64) -> Result<usize, JsError> {
        // Bound the unbudgeted entry point: a self-rearming zero-delay timer would never
        // let the loop return otherwise.
        self.run_until_with_budget(deadline_ms, 100_000)
    }

    /// [`Self::run_until`] that runs at most `max_timers` callbacks.
    ///
    /// The render event loop passes its remaining `max_tasks` budget so one batch cannot
    /// overshoot the documented cap.
    pub fn run_until_with_budget(
        &mut self,
        deadline_ms: u64,
        max_timers: usize,
    ) -> Result<usize, JsError> {
        let _scope = HostScope::enter(&self.host);
        let result = crate::events::run_until(&mut self.context, deadline_ms, max_timers);
        if let Err(error) = &result {
            let message = error.to_string();
            let _ = crate::events::report_uncaught(&mut self.context, &message, "<timer>");
        }
        drop(_scope);
        result
    }

    /// Number of pending timers.
    pub fn pending_timers(&self) -> usize {
        self.host.borrow().timers.len()
    }

    /// Current scheduler time in milliseconds (virtual clock after `advance`/`run_until`).
    pub fn now_ms(&self) -> u64 {
        self.host.borrow().now_ms()
    }

    /// Last non-fatal exception thrown by an event listener, if any.
    ///
    /// Listener exceptions are also pushed to the console capture buffer and do not
    /// interrupt the remaining listeners.
    pub fn last_event_error(&self) -> Option<String> {
        self.host.borrow().last_event_error.clone()
    }

    /// Rough estimate of the Rust-side (and JS host-side) memory used by the runtime.
    ///
    /// Boa 0.21 does not expose heap statistics, so this is a documented approximation:
    /// it accounts for DOM nodes, wrapper side-table entries, listeners, inline handlers,
    /// timers, console capture and the document title/URL strings. It deliberately
    /// excludes the JS heap allocated inside Boa itself (documented limitation).
    pub fn estimated_heap_bytes(&self) -> Option<usize> {
        let host = self.host.borrow();
        // Iterative: the estimate is also used for arbitrarily deep documents.
        let node_count = 1 + host
            .document
            .descendants(host.document.root())
            .filter(|node| host.document.try_node(*node).is_some())
            .count();
        let console_bytes: usize = host
            .console
            .iter()
            .map(|entry| entry.level.len() + entry.message.len())
            .sum();
        let estimate = node_count * 192
            + host.nodes.len() * 16
            + host.listeners.len() * 96
            + host.inline_handlers.len() * 64
            + host.timers.len() * 48
            + console_bytes
            + host.title.len()
            + host.url.len();
        Some(estimate)
    }
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
// Global installation
// ---------------------------------------------------------------------------

fn native(f: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>) -> NativeFunction {
    NativeFunction::from_fn_ptr(f)
}

fn accessor_fn(
    context: &mut Context,
    f: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
) -> boa_engine::object::builtins::JsFunction {
    FunctionObjectBuilder::new(context.realm(), native(f)).build()
}

/// Installs `window`, `console`, `document`, `setTimeout`/`setInterval` and helpers.
fn install_globals(context: &mut Context, host: &Rc<RefCell<Host>>) -> Result<(), JsError> {
    let global = context.global_object();
    let window_value = JsValue::from(global.clone());
    let _ = host;

    let console = build_console(context);
    let document = build_document_object(context);
    let entries: [(&str, JsValue); 3] = [
        ("window", window_value),
        ("console", JsValue::from(console)),
        ("document", JsValue::from(document)),
    ];
    for (name, value) in entries {
        global
            .set(js_string!(name), value, false, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    crate::events::install(context)?;
    install_browser_features(context)?;
    install_gap_apis(context)?;
    Ok(())
}

/// Installs History/location, Web Storage and MutationObserver (round 24).
fn install_browser_features(context: &mut Context) -> Result<(), JsError> {
    for name in [
        "__megi_history_state",
        "__megi_observer_callbacks",
        "__megi_observer_objects",
    ] {
        let array = JsArray::from_iter([], context);
        context
            .global_object()
            .set(js_string!(name), JsValue::from(array), false, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    let history = build_history(context)?;
    let location = build_location(context)?;
    let local_storage = build_storage(context, StorageKind::Local)?;
    let session_storage = build_storage(context, StorageKind::Session)?;
    for (name, value) in [
        ("history", JsValue::from(history)),
        ("location", JsValue::from(location)),
        ("localStorage", JsValue::from(local_storage)),
        ("sessionStorage", JsValue::from(session_storage)),
    ] {
        context
            .global_object()
            .set(js_string!(name), value, false, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    let constructor =
        FunctionObjectBuilder::new(context.realm(), native(mutation_observer_constructor))
            .name(js_string!("MutationObserver"))
            .length(1)
            .constructor(true)
            .build();
    let worker_constructor =
        FunctionObjectBuilder::new(context.realm(), native(worker_constructor))
            .name(js_string!("Worker"))
            .length(1)
            .constructor(true)
            .build();
    context
        .global_object()
        .set(
            js_string!("Worker"),
            JsValue::from(worker_constructor),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    context
        .global_object()
        .set(
            js_string!("MutationObserver"),
            JsValue::from(constructor),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    Ok(())
}

fn build_history(context: &mut Context) -> Result<JsObject, JsError> {
    let mut init = ObjectInitializer::new(context);
    init.function(native(history_push_state), js_string!("pushState"), 3);
    init.function(native(history_replace_state), js_string!("replaceState"), 3);
    init.function(native(history_back), js_string!("back"), 0);
    init.function(native(history_forward), js_string!("forward"), 0);
    init.function(native(history_go), js_string!("go"), 1);
    let history = init.build();
    define_accessor(context, &history, "length", history_length_get, None)?;
    define_accessor(context, &history, "state", history_state_get, None)?;
    Ok(history)
}

fn build_location(context: &mut Context) -> Result<JsObject, JsError> {
    let mut init = ObjectInitializer::new(context);
    init.function(native(location_assign), js_string!("assign"), 1);
    init.function(native(location_replace), js_string!("replace"), 1);
    init.function(native(location_reload), js_string!("reload"), 0);
    let location = init.build();
    for (name, getter, setter) in [
        (
            "href",
            location_href_get as NativeFn,
            Some(location_href_set as NativeFn),
        ),
        ("pathname", location_pathname_get as NativeFn, None),
        ("search", location_search_get as NativeFn, None),
        ("hash", location_hash_get as NativeFn, None),
        ("origin", location_origin_get as NativeFn, None),
        ("protocol", location_protocol_get as NativeFn, None),
        ("host", location_host_get as NativeFn, None),
    ] {
        let get = accessor_fn(context, getter);
        let descriptor = match setter {
            Some(setter) => {
                let set = accessor_fn(context, setter);
                PropertyDescriptor::builder()
                    .get(JsValue::from(get))
                    .set(JsValue::from(set))
                    .enumerable(true)
                    .configurable(true)
                    .build()
            }
            None => PropertyDescriptor::builder()
                .get(JsValue::from(get))
                .enumerable(true)
                .configurable(true)
                .build(),
        };
        location
            .define_property_or_throw(js_string!(name), descriptor, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    Ok(location)
}

fn define_accessor(
    context: &mut Context,
    object: &JsObject,
    name: &str,
    getter: NativeFn,
    setter: Option<NativeFn>,
) -> Result<(), JsError> {
    let get = accessor_fn(context, getter);
    let descriptor = match setter {
        Some(setter) => {
            let set = accessor_fn(context, setter);
            PropertyDescriptor::builder()
                .get(JsValue::from(get))
                .set(JsValue::from(set))
                .enumerable(true)
                .configurable(true)
                .build()
        }
        None => PropertyDescriptor::builder()
            .get(JsValue::from(get))
            .enumerable(true)
            .configurable(true)
            .build(),
    };
    object
        .define_property_or_throw(js_string!(name), descriptor, context)
        .map_err(|error| JsError::Internal(error.to_string()))?;
    Ok(())
}

fn build_storage(context: &mut Context, kind: StorageKind) -> Result<JsObject, JsError> {
    let mut init = ObjectInitializer::new(context);
    init.function(native(storage_get_item), js_string!("getItem"), 1);
    init.function(native(storage_set_item), js_string!("setItem"), 2);
    init.function(native(storage_remove_item), js_string!("removeItem"), 1);
    init.function(native(storage_clear), js_string!("clear"), 0);
    init.function(native(storage_key), js_string!("key"), 1);
    let storage = init.build();
    storage
        .set(
            js_string!("__megi_storage_kind"),
            JsValue::from(kind == StorageKind::Session),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    define_accessor(context, &storage, "length", storage_length_get, None)?;
    Ok(storage)
}

fn build_console(context: &mut Context) -> JsObject {
    let mut init = ObjectInitializer::new(context);
    let methods: [(&str, NativeFn); 8] = [
        ("log", console_log),
        ("info", console_info),
        ("warn", console_warn),
        ("error", console_error),
        ("debug", console_debug),
        ("assert", console_assert),
        ("table", console_table),
        ("dir", console_debug),
    ];
    for (name, function) in methods {
        init.function(native(function), js_string!(name), 0);
    }
    init.build()
}

fn console_log(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    record_console(this, args, context, "log")
}
fn console_info(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    record_console(this, args, context, "info")
}
fn console_warn(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    record_console(this, args, context, "warn")
}
fn console_error(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    record_console(this, args, context, "error")
}
fn console_debug(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    record_console(this, args, context, "debug")
}
fn console_table(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    record_console(this, args, context, "table")
}

/// `console.assert(condition, ...)` — logs only when the condition is falsy.
fn console_assert(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let truthy = args
        .first()
        .map(|value| value.to_boolean())
        .unwrap_or(false);
    if truthy {
        return Ok(JsValue::undefined());
    }
    let rest = args.get(1..).unwrap_or_default();
    let mut parts = vec!["Assertion failed".to_string()];
    for argument in rest {
        parts.push(match argument.to_string(context) {
            Ok(text) => text.to_std_string_escaped(),
            Err(_) => "<unprintable>".to_string(),
        });
    }
    let message = parts.join(" ");
    let _ = with_host(|host| {
        host.push_console(ConsoleEntry {
            level: "assert".to_string(),
            message,
        })
    });
    Ok(JsValue::undefined())
}

/// `console.*` — space-joined argument stringification into the capture buffer.
fn record_console(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    level: &str,
) -> JsResult<JsValue> {
    let mut parts = Vec::with_capacity(args.len());
    for argument in args {
        parts.push(match argument.to_string(context) {
            Ok(text) => text.to_std_string_escaped(),
            Err(_) => "<unprintable>".to_string(),
        });
    }
    let message = parts.join(" ");
    let level = level.to_string();
    let _ = with_host(|host| host.push_console(ConsoleEntry { level, message }));
    Ok(JsValue::undefined())
}

/// Builds the `document` host object.
fn build_document_object(context: &mut Context) -> JsObject {
    let pick = |context: &mut Context,
                get: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>| {
        accessor_fn(context, get)
    };
    let document_element = pick(context, document_element);
    let document_body = pick(context, document_body);
    let document_head = pick(context, document_head);
    let title_get = pick(context, document_title_get);
    let title_set = pick(context, document_title_set);
    let url = pick(context, document_url);
    let ready_state = pick(context, document_ready_state);
    let inline_accessors: Vec<(
        &'static str,
        boa_engine::object::builtins::JsFunction,
        boa_engine::object::builtins::JsFunction,
    )> = crate::events::inline_node_accessors()
        .into_iter()
        .map(|(name, get, set)| (name, accessor_fn(context, get), accessor_fn(context, set)))
        .collect();
    let active_element = pick(context, document_active_element);
    let mut init = ObjectInitializer::new(context);
    // Methods.
    for (name, length, function) in [
        (
            "write",
            1usize,
            document_write as fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
        ),
        ("writeln", 1, document_write),
        ("getElementById", 1, document_get_element_by_id),
        ("getElementsByTagName", 1, document_get_elements_by_tag_name),
        (
            "getElementsByClassName",
            1,
            document_get_elements_by_class_name,
        ),
        ("querySelector", 1, document_query_selector),
        ("querySelectorAll", 1, document_query_selector_all),
        ("createElement", 1, document_create_element),
        ("createTextNode", 1, document_create_text_node),
        (
            "createDocumentFragment",
            0,
            document_create_document_fragment,
        ),
        ("normalize", 0, document_normalize),
        (
            "compareDocumentPosition",
            2,
            document_compare_document_position,
        ),
    ] {
        init.function(native(function), js_string!(name), length);
    }
    // Properties.
    init.accessor(
        js_string!("documentElement"),
        Some(document_element),
        None,
        Attribute::all(),
    );
    init.accessor(
        js_string!("body"),
        Some(document_body),
        None,
        Attribute::all(),
    );
    init.accessor(
        js_string!("head"),
        Some(document_head),
        None,
        Attribute::all(),
    );
    init.accessor(
        js_string!("title"),
        Some(title_get),
        Some(title_set),
        Attribute::all(),
    );
    init.accessor(
        js_string!("activeElement"),
        Some(active_element),
        None,
        Attribute::all(),
    );
    init.accessor(js_string!("URL"), Some(url), None, Attribute::all());
    init.accessor(
        js_string!("readyState"),
        Some(ready_state),
        None,
        Attribute::all(),
    );
    init.property(js_string!("__megi_document"), true, Attribute::all());
    for (name, length, function) in crate::events::node_methods() {
        init.function(native(function), js_string!(name), length);
    }
    for (name, get, set) in inline_accessors {
        init.accessor(js_string!(name), Some(get), Some(set), Attribute::all());
    }
    init.function(native(document_close), js_string!("close"), 0);
    init.build()
}

// ---------------------------------------------------------------------------
// Node wrappers
// ---------------------------------------------------------------------------

/// Creates the JS wrapper for a side-table node handle (shared with events/utilities).
pub(crate) fn wrap_handle_public(context: &mut Context, handle: u32) -> JsResult<JsValue> {
    node_wrapper(context, handle)
}

/// Creates a JS wrapper for `node_id`, reusing the realm's cached object (identity).
pub(crate) fn node_wrapper(context: &mut Context, handle: u32) -> JsResult<JsValue> {
    let cache = context
        .global_object()
        .get(js_string!("__megi_wrappers"), context)
        .ok()
        .and_then(|value| value.as_object());
    if let Some(cache) = &cache
        && let Ok(existing) = cache.get(js_string!(handle.to_string()), context)
        && existing.is_object()
    {
        return Ok(existing);
    }
    // Accessors: tagName, id, className, textContent, innerHTML, outerHTML, children,
    // parentNode, firstChild, nextSibling, style, classList.
    let accessors: [(&str, AccessorKind); 33] = [
        ("nodeType", AccessorKind::NodeType),
        ("tagName", AccessorKind::TagName),
        ("id", AccessorKind::Id),
        ("className", AccessorKind::ClassName),
        ("textContent", AccessorKind::TextContent),
        ("innerHTML", AccessorKind::InnerHtml),
        ("outerHTML", AccessorKind::OuterHtml),
        ("children", AccessorKind::Children),
        ("parentNode", AccessorKind::ParentNode),
        ("firstChild", AccessorKind::FirstChild),
        ("lastChild", AccessorKind::LastChild),
        ("nextSibling", AccessorKind::NextSibling),
        ("previousSibling", AccessorKind::PreviousSibling),
        ("style", AccessorKind::Style),
        ("classList", AccessorKind::ClassList),
        ("childNodes", AccessorKind::ChildNodes),
        ("value", AccessorKind::Value),
        ("checked", AccessorKind::Checked),
        ("disabled", AccessorKind::Disabled),
        ("selected", AccessorKind::Selected),
        ("options", AccessorKind::Options),
        ("innerText", AccessorKind::InnerText),
        ("dataset", AccessorKind::Dataset),
        ("attributes", AccessorKind::Attributes),
        ("offsetWidth", AccessorKind::OffsetWidth),
        ("offsetHeight", AccessorKind::OffsetHeight),
        ("clientWidth", AccessorKind::ClientWidth),
        ("clientHeight", AccessorKind::ClientHeight),
        ("title", AccessorKind::Title),
        ("lang", AccessorKind::Lang),
        ("href", AccessorKind::Href),
        ("src", AccessorKind::Src),
        ("selectedIndex", AccessorKind::SelectedIndex),
    ];
    type Prepared<'a> = (
        &'a str,
        boa_engine::object::builtins::JsFunction,
        Option<boa_engine::object::builtins::JsFunction>,
    );
    let mut prepared: Vec<Prepared<'_>> = Vec::with_capacity(accessors.len());
    for (name, kind) in accessors {
        let (get, set): AccessorPair = accessor_functions(kind);
        prepared.push((
            name,
            accessor_fn(context, get),
            set.map(|set| accessor_fn(context, set)),
        ));
    }
    let inline_accessors: Vec<(
        &'static str,
        boa_engine::object::builtins::JsFunction,
        boa_engine::object::builtins::JsFunction,
    )> = crate::events::inline_node_accessors()
        .into_iter()
        .map(|(name, get, set)| (name, accessor_fn(context, get), accessor_fn(context, set)))
        .collect();
    let mut init = ObjectInitializer::new(context);
    // `__megi_node` is the Rust side-table handle; it must never be writable, enumerable or
    // configurable from script, otherwise a forged value repoints the wrapper (#644).
    init.property(
        js_string!("__megi_node"),
        f64::from(handle),
        Attribute::default(),
    );
    for (name, get, set) in prepared {
        init.accessor(js_string!(name), Some(get), set, Attribute::all());
    }
    for (name, get, set) in inline_accessors {
        init.accessor(js_string!(name), Some(get), Some(set), Attribute::all());
    }
    // Methods.
    for (name, length, function) in [
        (
            "setAttribute",
            2usize,
            node_set_attribute as fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
        ),
        ("getAttribute", 1, node_get_attribute),
        ("getAttributeNode", 1, node_get_attribute_node),
        ("removeAttributeNode", 1, node_remove_attribute_node),
        ("removeAttribute", 1, node_remove_attribute),
        ("hasAttribute", 1, node_has_attribute),
        ("appendChild", 1, node_append_child),
        ("insertBefore", 2, node_insert_before),
        ("removeChild", 1, node_remove_child),
        ("replaceChild", 2, node_replace_child),
        ("matches", 1, node_matches),
        ("closest", 1, node_closest),
        ("insertAdjacentHTML", 2, node_insert_adjacent_html),
        ("insertAdjacentText", 2, node_insert_adjacent_text),
        ("cloneNode", 1, node_clone_node),
        ("normalize", 0, node_normalize),
        ("compareDocumentPosition", 1, node_compare_document_position),
        ("focus", 0, node_focus),
        ("blur", 0, node_blur),
        ("scrollIntoView", 0, node_scroll_into_view),
        ("getBoundingClientRect", 0, node_get_bounding_client_rect),
    ] {
        init.function(native(function), js_string!(name), length);
    }
    for (name, length, function) in crate::events::node_methods() {
        init.function(native(function), js_string!(name), length);
    }
    let wrapper = init.build();
    if let Some(cache) = &cache {
        let _ = cache.set(
            js_string!(handle.to_string()),
            JsValue::from(wrapper.clone()),
            false,
            context,
        );
    }
    Ok(JsValue::from(wrapper))
}

#[derive(Clone, Copy)]
enum AccessorKind {
    NodeType,
    TagName,
    Id,
    ClassName,
    TextContent,
    InnerHtml,
    OuterHtml,
    Children,
    ParentNode,
    FirstChild,
    LastChild,
    NextSibling,
    PreviousSibling,
    Style,
    ClassList,
    ChildNodes,
    Value,
    Checked,
    Disabled,
    Selected,
    Options,
    InnerText,
    Dataset,
    Attributes,
    OffsetWidth,
    OffsetHeight,
    ClientWidth,
    ClientHeight,
    Title,
    Lang,
    Href,
    Src,
    SelectedIndex,
}

fn accessor_functions(kind: AccessorKind) -> AccessorPair {
    match kind {
        AccessorKind::NodeType => (node_node_type, None),
        AccessorKind::TagName => (node_tag_name, None),
        AccessorKind::Id => (node_id_get, Some(node_id_set)),
        AccessorKind::ClassName => (node_class_get, Some(node_class_set)),
        AccessorKind::TextContent => (node_text_content_get, Some(node_text_content_set)),
        AccessorKind::InnerHtml => (node_inner_html_get, Some(node_inner_html_set)),
        AccessorKind::OuterHtml => (node_outer_html_get, Some(node_outer_html_set)),
        AccessorKind::Children => (node_children, None),
        AccessorKind::ParentNode => (node_parent_node, None),
        AccessorKind::FirstChild => (node_first_child, None),
        AccessorKind::LastChild => (node_last_child, None),
        AccessorKind::NextSibling => (node_next_sibling, None),
        AccessorKind::PreviousSibling => (node_previous_sibling, None),
        AccessorKind::Style => (node_style_get, Some(node_style_set)),
        AccessorKind::ClassList => (node_class_list_get, None),
        AccessorKind::ChildNodes => (node_child_nodes, None),
        AccessorKind::Value => (node_value_get, Some(node_value_set)),
        AccessorKind::Checked => (node_checked_get, Some(node_checked_set)),
        AccessorKind::Disabled => (node_disabled_get, Some(node_disabled_set)),
        AccessorKind::Selected => (node_selected_get, Some(node_selected_set)),
        AccessorKind::Options => (node_options, None),
        AccessorKind::InnerText => (node_inner_text_get, Some(node_text_content_set)),
        AccessorKind::Dataset => (node_dataset_get, None),
        AccessorKind::Attributes => (node_attributes_get, None),
        AccessorKind::OffsetWidth => (node_offset_width, None),
        AccessorKind::OffsetHeight => (node_offset_height, None),
        AccessorKind::ClientWidth => (node_client_width, None),
        AccessorKind::ClientHeight => (node_client_height, None),
        AccessorKind::Title => (node_title_get, Some(node_title_set)),
        AccessorKind::Lang => (node_lang_get, Some(node_lang_set)),
        AccessorKind::Href => (node_href_get, Some(node_href_set)),
        AccessorKind::Src => (node_src_get, Some(node_src_set)),
        AccessorKind::SelectedIndex => (node_selected_index_get, Some(node_selected_index_set)),
    }
}

/// Wraps a node id from the current host, if it still resolves.
fn wrap_node(context: &mut Context, node_id: NodeId) -> JsResult<JsValue> {
    let handle = with_host(|host| host.handle_for(node_id))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;
    node_wrapper(context, handle)
}

/// Extracts the side-table handle stored on `this`.
fn this_handle(this: &JsValue, context: &mut Context) -> JsResult<u32> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("DOM method called with a non-object receiver"))?;
    let value = object
        .get(js_string!("__megi_node"), context)
        .map_err(|error| js_error(error.to_string()))?;
    value
        .as_number()
        .filter(|number| *number >= 0.0 && number.fract() == 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("invalid DOM node handle"))
}

/// Resolves the receiver to a live node id (typed error on stale handles).
fn this_node(this: &JsValue, context: &mut Context) -> JsResult<NodeId> {
    let handle = this_handle(this, context)?;
    with_host(|host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)
}

fn js_error(message: impl Into<String>) -> BoaError {
    JsNativeError::typ().with_message(message.into()).into()
}

/// `SyntaxError` for the DOM APIs that validate an enumerated string argument (#653).
fn syntax_error(message: impl Into<String>) -> BoaError {
    JsNativeError::syntax().with_message(message.into()).into()
}

/// The four `insertAdjacent*` positions, after ASCII-lowercasing (#653).
#[derive(Clone, Copy, PartialEq, Eq)]
enum AdjacentPosition {
    BeforeBegin,
    AfterBegin,
    BeforeEnd,
    AfterEnd,
}

impl AdjacentPosition {
    fn parse(position: &str) -> Option<Self> {
        match position {
            "beforebegin" => Some(Self::BeforeBegin),
            "afterbegin" => Some(Self::AfterBegin),
            "beforeend" => Some(Self::BeforeEnd),
            "afterend" => Some(Self::AfterEnd),
            _ => None,
        }
    }
}

fn arg_string(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    let value = args.get(index).cloned().unwrap_or_else(JsValue::undefined);
    value
        .to_string(context)
        .map(|text| text.to_std_string_escaped())
        .map_err(|error| js_error(error.to_string()))
}

fn wrap_optional(context: &mut Context, node: Option<NodeId>) -> JsResult<JsValue> {
    match node {
        Some(node) => wrap_node(context, node),
        None => Ok(JsValue::null()),
    }
}

fn wrap_list(context: &mut Context, nodes: Vec<NodeId>) -> JsResult<JsValue> {
    let array = boa_engine::object::builtins::JsArray::from_iter([], context);
    for node in nodes {
        array
            .push(wrap_node(context, node)?, context)
            .map_err(|error| js_error(error.to_string()))?;
    }
    Ok(JsValue::from(array))
}

// ---------------------------------------------------------------------------
// Node accessors
// ---------------------------------------------------------------------------

fn node_tag_name(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = with_host(|host| {
        host.document.try_node(node_id).and_then(|node| {
            node.as_element()
                .map(|element| element.local_name.to_ascii_uppercase())
        })
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(name)))
}

fn node_id_get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let id = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute("id"))
            })
            .map(str::to_owned)
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(id)))
}

fn node_id_set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    with_host(|host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(DomAttribute::new("id", value), true);
        }
    });
    Ok(JsValue::undefined())
}

fn node_class_get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let class = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute("class"))
            })
            .map(str::to_owned)
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(class)))
}

fn node_class_set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    with_host(|host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(DomAttribute::new("class", value), true);
        }
    });
    Ok(JsValue::undefined())
}

fn node_text_content_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let text = with_host(|host| host.document.text_content(node_id)).unwrap_or_default();
    Ok(JsValue::from(js_string!(text)))
}

fn node_text_content_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let text = arg_string(args, 0, context)?;
    let result = with_host(|host| host.document.set_text_content(node_id, &text));
    match result {
        Some(Ok(())) => {
            note_mutation(MutationKind::CharacterData, node_id, None, None);
            Ok(JsValue::undefined())
        }
        Some(Err(error)) => Err(js_error(error.to_string())),
        None => Err(js_error("no active JavaScript host")),
    }
}

fn node_inner_html_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let html =
        with_host(|host| html::serialize_fragment(&host.document, node_id)).unwrap_or_default();
    Ok(JsValue::from(js_string!(html)))
}

fn node_inner_html_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let html_text = arg_string(args, 0, context)?;
    let result = with_host(|host| {
        if !charge_document_parse(host, html_text.len(), "innerHTML assignment") {
            return Ok(());
        }
        let options = html::ParseOptions {
            max_nodes: host.max_document_write_nodes,
            ..html::ParseOptions::default()
        };
        html::set_inner_html_with_options(&mut host.document, node_id, &html_text, &options)
            .map(|_| {
                host.record_mutation(MutationRecord {
                    kind: MutationKind::ChildList,
                    target: node_id,
                    name: None,
                    old_value: None,
                });
            })
            .map_err(|error| error.to_string())
    });
    match result {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

fn node_outer_html_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let html =
        with_host(|host| html::serialize_outer_html(&host.document, node_id)).unwrap_or_default();
    Ok(JsValue::from(js_string!(html)))
}

fn node_outer_html_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let html_text = arg_string(args, 0, context)?;
    let result = with_host(|host| {
        if !charge_document_parse(host, html_text.len(), "outerHTML assignment") {
            return Ok(());
        }
        let parent = host.document.try_node(node_id).and_then(|node| node.parent);
        let options = html::ParseOptions {
            max_nodes: host.max_document_write_nodes,
            ..html::ParseOptions::default()
        };
        html::set_outer_html_with_options(&mut host.document, node_id, &html_text, &options)
            .map(|_| {
                if let Some(parent) = parent {
                    host.record_mutation(MutationRecord {
                        kind: MutationKind::ChildList,
                        target: parent,
                        name: None,
                        old_value: None,
                    });
                }
            })
            .map_err(|error| error.to_string())
    });
    match result {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

fn node_children(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let children = with_host(|host| {
        host.document
            .children(node_id)
            .filter(|child| {
                host.document
                    .try_node(*child)
                    .is_some_and(|node| node.as_element().is_some())
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    node_list(context, children)
}

fn node_parent_node(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let parent = with_host(|host| host.document.try_node(node_id).and_then(|node| node.parent));
    wrap_optional(context, parent.flatten())
}

fn node_first_child(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let child = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.first_child)
    });
    wrap_optional(context, child.flatten())
}

fn node_last_child(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let child = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.last_child)
    });
    wrap_optional(context, child.flatten())
}

fn node_previous_sibling(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let sibling = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.prev_sibling)
    });
    wrap_optional(context, sibling.flatten())
}

fn node_next_sibling(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let sibling = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.next_sibling)
    });
    wrap_optional(context, sibling.flatten())
}

fn node_style_get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let handle = this_handle(this, context)?;
    Ok(style_object(context, handle))
}

fn node_style_set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    // `element.style = "color: red"` replaces the inline style attribute.
    let node_id = this_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    with_host(|host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(DomAttribute::new("style", value), true);
        }
    });
    Ok(JsValue::undefined())
}

fn node_class_list_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let handle = this_handle(this, context)?;
    Ok(class_list_object(context, handle))
}

// ---------------------------------------------------------------------------
// Node methods
// ---------------------------------------------------------------------------

/// Returns the attribute name to use for `node_id`: HTML elements lowercase attribute names
/// (DOM `getAttribute`/`setAttribute` case-insensitivity for HTML), foreign elements keep
/// the authored case.
fn attribute_name(node_id: NodeId, name: &str) -> String {
    let is_html = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .is_some_and(|element| element.namespace == Namespace::Html)
    })
    .unwrap_or(false);
    if is_html {
        name.to_ascii_lowercase()
    } else {
        name.to_string()
    }
}

fn node_child_nodes(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let nodes: Vec<NodeId> =
        with_host(|host| host.document.children(parent).collect()).unwrap_or_default();
    node_list(context, nodes)
}

fn node_value_get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = with_host(|host| {
        let document = &host.document;
        let element = document
            .try_node(node_id)
            .and_then(|node| node.as_element())?;
        if element.local_name == "textarea" {
            return Some(document.text_content(node_id));
        }
        if element.local_name == "select" {
            let selected = document
                .descendants(node_id)
                .filter_map(|child| document.try_node(child).and_then(|node| node.as_element()))
                .find(|option| {
                    option.local_name == "option" && option.get_attribute("selected").is_some()
                });
            if let Some(option) = selected {
                if let Some(value) = option.get_attribute("value") {
                    return Some(value.to_string());
                }
                if let Some(option_id) = document.descendants(node_id).find(|child| {
                    document
                        .try_node(*child)
                        .and_then(|node| node.as_element())
                        .is_some_and(|candidate| {
                            candidate.local_name == "option"
                                && candidate.get_attribute("selected").is_some()
                        })
                }) {
                    return Some(document.text_content(option_id));
                }
            }
            return Some(String::new());
        }
        if element.local_name == "option" {
            return Some(
                element
                    .get_attribute("value")
                    .map(str::to_owned)
                    .unwrap_or_else(|| document.text_content(node_id)),
            );
        }
        Some(element.get_attribute("value").unwrap_or("").to_string())
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(value)))
}

fn node_value_set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    let result = with_host(|host| {
        let document = &mut host.document;
        let tag = document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .map(|element| element.local_name.clone())
            .ok_or_else(|| "node is not an element".to_string())?;
        if tag == "textarea" {
            document
                .set_text_content(node_id, &value)
                .map_err(|error| error.to_string())?;
            return Ok::<(), String>(());
        }
        let element = document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .ok_or_else(|| "node is not an element".to_string())?;
        element.set_attribute(DomAttribute::new("value", value.clone()), true);
        Ok(())
    });
    match result {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

fn boolean_attribute_get(this: &JsValue, name: &str, context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let present = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .is_some_and(|element| element.get_attribute(name).is_some())
    })
    .unwrap_or(false);
    Ok(JsValue::from(present))
}

fn boolean_attribute_set(
    this: &JsValue,
    name: &str,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let enabled = args.first().and_then(JsValue::as_boolean).unwrap_or(false);
    let result = with_host(|host| {
        let element = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .ok_or_else(|| "node is not an element".to_string())?;
        if enabled {
            element.set_attribute(DomAttribute::new(name, ""), true);
        } else {
            element.remove_attribute(name);
        }
        Ok::<(), String>(())
    });
    match result {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

fn node_checked_get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    boolean_attribute_get(this, "checked", context)
}

fn node_checked_set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    boolean_attribute_set(this, "checked", args, context)
}

fn node_disabled_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "disabled", context)
}

fn node_disabled_set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    boolean_attribute_set(this, "disabled", args, context)
}

fn node_selected_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "selected", context)
}

fn node_selected_set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    boolean_attribute_set(this, "selected", args, context)
}

fn node_options(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let options: Vec<NodeId> = with_host(|host| {
        host.document
            .descendants(node_id)
            .filter(|child| {
                host.document
                    .try_node(*child)
                    .and_then(|node| node.as_element())
                    .is_some_and(|element| element.local_name == "option")
            })
            .collect()
    })
    .unwrap_or_default();
    node_list(context, options)
}

fn node_set_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?);
    let value = arg_string(args, 1, context)?;
    let old_value = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(&name).map(str::to_owned))
    })
    .flatten();
    let result = with_host(|host| {
        let element = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .ok_or_else(|| "node is not an element".to_string())?;
        element.set_attribute(DomAttribute::new(name.clone(), value), true);
        Ok::<(), String>(())
    });
    match result {
        Some(Ok(())) => {
            note_mutation(MutationKind::Attributes, node_id, Some(name), old_value);
            Ok(JsValue::undefined())
        }
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

fn node_get_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?);
    let value = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute(&name))
            })
            .map(str::to_owned)
    })
    .flatten();
    match value {
        Some(value) => Ok(JsValue::from(js_string!(value))),
        None => Ok(JsValue::null()),
    }
}

fn node_remove_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?);
    let old_value = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(&name).map(str::to_owned))
    })
    .flatten();
    with_host(|host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element
                .attributes
                .retain(|attribute| attribute.local_name != name);
        }
    });
    note_mutation(MutationKind::Attributes, node_id, Some(name), old_value);
    Ok(JsValue::undefined())
}

fn node_has_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?);
    let has = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute(&name))
            })
            .is_some()
    })
    .unwrap_or(false);
    Ok(JsValue::from(has))
}

/// Extracts a child argument (wrapper → node id).
fn argument_node(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<NodeId> {
    let value = args.get(index).cloned().unwrap_or_else(JsValue::undefined);
    let object = value
        .as_object()
        .ok_or_else(|| js_error("expected a DOM node argument"))?;
    let handle = object
        .get(js_string!("__megi_node"), context)
        .map_err(|error| js_error(error.to_string()))?;
    let handle = handle
        .as_number()
        .filter(|number| *number >= 0.0 && number.fract() == 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("expected a DOM node argument"))?;
    with_host(|host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)
}

fn node_clone_node(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let deep = args.first().and_then(JsValue::as_boolean).unwrap_or(false);
    // `cloneNode(true)` doubles the arena in one call, so the subtree size is charged before
    // the clone is allocated (#657).
    let allowed = with_host(|host| {
        let extra = if deep {
            host.document.descendants(node_id).count().max(1)
        } else {
            1
        };
        let total = host.document.len().saturating_add(extra);
        if total > host.max_dom_nodes {
            if !host.dom_budget_warned {
                host.dom_budget_warned = true;
                host.push_console(ConsoleEntry {
                    level: "warn".to_string(),
                    message: format!(
                        "cloneNode rejected: the DOM node budget ({} nodes) is exhausted",
                        host.max_dom_nodes
                    ),
                });
            }
            return false;
        }
        true
    })
    .unwrap_or(false);
    if !allowed {
        return Err(js_error(format!(
            "QuotaExceededError: DOM node budget exceeded ({} nodes)",
            with_host(|host| host.max_dom_nodes).unwrap_or(0)
        )));
    }
    let clone = with_host(|host| host.document.clone_node(node_id, deep))
        .ok_or_else(|| js_error("no active JavaScript host"))?;
    wrap_node(context, clone)
}

fn node_append_child(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let child = argument_node(args, 0, context)?;
    // DOM insertion expands document fragments (their children are moved, the fragment
    // itself is never attached).
    let appended =
        with_host(|host| host.document.insert_node(parent, child, None).is_ok()).unwrap_or(false);
    if !appended {
        return Err(js_error("appendChild failed (hierarchy or unknown node)"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None);
    wrap_node(context, child)
}

fn node_insert_before(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let new_node = argument_node(args, 0, context)?;
    // `insertBefore(node, null)` (or a missing reference) appends, per DOM.
    let reference = match args.get(1) {
        Some(value) if !value.is_null_or_undefined() => Some(argument_node(args, 1, context)?),
        _ => None,
    };
    if reference == Some(new_node) {
        // DOM spec: inserting a node before itself is a no-op.
        return wrap_node(context, new_node);
    }
    let inserted = with_host(|host| {
        host.document
            .insert_node(parent, new_node, reference)
            .is_ok()
    })
    .unwrap_or(false);
    if !inserted {
        return Err(js_error("insertBefore failed (hierarchy or unknown node)"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None);
    wrap_node(context, new_node)
}

fn node_remove_child(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let child = argument_node(args, 0, context)?;
    let removed =
        with_host(|host| host.document.remove_child(parent, child).is_ok()).unwrap_or(false);
    if !removed {
        return Err(js_error("removeChild failed (node is not a child)"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None);
    wrap_node(context, child)
}

fn node_replace_child(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let new_node = argument_node(args, 0, context)?;
    let old_node = argument_node(args, 1, context)?;
    let replaced = with_host(|host| {
        host.document
            .replace_node(parent, new_node, old_node)
            .is_ok()
    })
    .unwrap_or(false);
    if !replaced {
        return Err(js_error("replaceChild failed"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None);
    wrap_node(context, old_node)
}

// ---------------------------------------------------------------------------
// document methods/accessors
// ---------------------------------------------------------------------------

/// `document.readyState` — `loading` during evaluation; the host updates it with
/// [`JsRuntime::set_ready_state`].
fn document_ready_state(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    let state = with_host(|host| host.ready_state.clone()).unwrap_or_else(|| "loading".to_string());
    Ok(JsValue::from(js_string!(state)))
}

/// `document.close()` — no-op kept for script compatibility (parsing is host-driven).
fn document_close(_this: &JsValue, _args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}

/// Charges `bytes` of markup parsing against the document's cumulative budget.
///
/// `document.write`, `innerHTML`, `outerHTML` and `insertAdjacentHTML` all parse
/// attacker-controlled markup; each parse is bounded per call, but a script loop could
/// otherwise force thousands of large parses. The byte budget is shared with the node cap
/// enforced by the parser ([`Host::max_document_write_nodes`]). Returns `false` when the
/// assignment must be ignored.
fn charge_document_parse(host: &mut Host, bytes: usize, sink: &str) -> bool {
    let total = host.document_write_bytes.saturating_add(bytes);
    if total > host.max_document_write_bytes || host.document.len() >= host.max_document_write_nodes
    {
        if !host.document_write_warned {
            host.document_write_warned = true;
            host.console.push(ConsoleEntry {
                level: "warn".to_string(),
                message: format!(
                    "{sink} ignored: the cumulative document budget ({} bytes / {} nodes) is exhausted",
                    host.max_document_write_bytes, host.max_document_write_nodes
                ),
            });
        }
        host.document_write_bytes = total;
        return false;
    }
    host.document_write_bytes = total;
    true
}

fn document_write(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let text = arg_string(args, 0, context)?;
    with_host(|host| {
        if !charge_document_parse(host, text.len(), "document.write") {
            return;
        }
        let options = html::ParseOptions {
            max_nodes: host.max_document_write_nodes,
            ..html::ParseOptions::default()
        };
        let _ = html::document_write_with_options(&mut host.document, &text, false, &options);
        let target = host
            .document
            .body()
            .or_else(|| host.document.document_element())
            .unwrap_or_else(|| host.document.root());
        host.record_mutation(MutationRecord {
            kind: MutationKind::ChildList,
            target,
            name: None,
            old_value: None,
        });
    });
    Ok(JsValue::undefined())
}

fn document_get_element_by_id(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = arg_string(args, 0, context)?;
    let node = with_host(|host| host.document.get_element_by_id(&id)).flatten();
    wrap_optional(context, node)
}

fn document_get_elements_by_tag_name(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let name = arg_string(args, 0, context)?;
    let nodes = with_host(|host| host.document.get_elements_by_tag_name(&name)).unwrap_or_default();
    node_list(context, nodes)
}

fn document_get_elements_by_class_name(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let class = arg_string(args, 0, context)?;
    let nodes =
        with_host(|host| host.document.get_elements_by_class_name(&class)).unwrap_or_default();
    wrap_list(context, nodes)
}

fn document_query_selector(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let selector = arg_string(args, 0, context)?;
    let node = with_host(|host| css::query::query_selector(&host.document, &selector)).flatten();
    wrap_optional(context, node)
}

fn document_query_selector_all(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let selector = arg_string(args, 0, context)?;
    let nodes = with_host(|host| css::query::query_selector_all(&host.document, &selector))
        .unwrap_or_default();
    wrap_list(context, nodes)
}

fn document_create_document_fragment(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let fragment = with_host(|host| host.document.create_document_fragment())
        .ok_or_else(|| js_error("no active JavaScript host"))?;
    wrap_node(context, fragment)
}

fn document_create_element(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // DOM §4.5: `createElement` lowercases the name for an HTML document, so
    // `createElement("TEMPLATE")` still creates a template with its content fragment.
    let name = arg_string(args, 0, context)?.to_ascii_lowercase();
    let node = with_host(|host| {
        host.document
            .create_element(Namespace::Html, name.clone(), Vec::new())
    });
    match node {
        Some(node_id) => wrap_node(context, node_id),
        None => Err(js_error("no active JavaScript host")),
    }
}

fn document_create_text_node(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let text = arg_string(args, 0, context)?;
    let node = with_host(|host| host.document.create_text_node(&text));
    match node {
        Some(node_id) => wrap_node(context, node_id),
        None => Err(js_error("no active JavaScript host")),
    }
}

fn document_element(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = with_host(|host| host.document.document_element()).flatten();
    wrap_optional(context, node)
}

fn document_body(_this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = with_host(|host| host.document.body()).flatten();
    wrap_optional(context, node)
}

fn document_head(_this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = with_host(|host| host.document.head()).flatten();
    wrap_optional(context, node)
}

fn document_title_get(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    let title = with_host(|host| host.title.clone()).unwrap_or_default();
    Ok(JsValue::from(js_string!(title)))
}

fn document_title_set(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let title = arg_string(args, 0, context)?;
    with_host(|host| host.title = title);
    Ok(JsValue::undefined())
}

fn document_url(_this: &JsValue, _args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    let url = with_host(|host| host.url.clone()).unwrap_or_default();
    Ok(JsValue::from(js_string!(url)))
}

// ---------------------------------------------------------------------------
// DOM utilities: NodeList, matches/closest, insertAdjacent*
// ---------------------------------------------------------------------------

/// Snapshot `NodeList` object (`length`, numeric indices, `item`, `forEach`).
fn node_list(context: &mut Context, nodes: Vec<NodeId>) -> JsResult<JsValue> {
    let mut wrappers = Vec::with_capacity(nodes.len());
    for node in nodes {
        wrappers.push(wrap_node(context, node)?);
    }
    let item = accessor_fn(context, node_list_item);
    let for_each = accessor_fn(context, node_list_for_each);
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("__megi_node_list"), true, Attribute::all());
    for (index, wrapper) in wrappers.iter().enumerate() {
        init.property(
            js_string!(index.to_string()),
            wrapper.clone(),
            Attribute::all(),
        );
    }
    init.property(
        js_string!("length"),
        f64::from(wrappers.len() as u32),
        Attribute::all(),
    )
    .function(native(node_list_item), js_string!("item"), 1)
    .function(native(node_list_for_each), js_string!("forEach"), 1);
    let _ = (item, for_each);
    Ok(JsValue::from(init.build()))
}

fn node_list_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let index = args
        .first()
        .and_then(JsValue::as_number)
        .unwrap_or(0.0)
        .max(0.0) as u64;
    let object = this
        .as_object()
        .ok_or_else(|| js_error("invalid NodeList receiver"))?;
    object
        .get(js_string!(index.to_string()), context)
        .map_err(|error| js_error(error.to_string()))
}

fn node_list_for_each(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let callback = args
        .first()
        .and_then(|value| value.as_object())
        .ok_or_else(|| js_error("forEach expects a function"))?;
    let object = this
        .as_object()
        .ok_or_else(|| js_error("invalid NodeList receiver"))?;
    let length = object
        .get(js_string!("length"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .unwrap_or(0.0) as u64;
    for index in 0..length {
        let item = object
            .get(js_string!(index.to_string()), context)
            .map_err(|error| js_error(error.to_string()))?;
        callback
            .call(
                &JsValue::undefined(),
                &[item, JsValue::from(index as f64)],
                context,
            )
            .map_err(|error| js_error(error.to_string()))?;
    }
    Ok(JsValue::undefined())
}

fn node_matches(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let selector = arg_string(args, 0, context)?;
    let matched =
        with_host(|host| css::query::matches(&host.document, node, &selector)).unwrap_or(false);
    Ok(JsValue::from(matched))
}

fn node_closest(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let selector = arg_string(args, 0, context)?;
    let mut current = Some(node);
    while let Some(candidate) = current {
        if with_host(|host| css::query::matches(&host.document, candidate, &selector))
            .unwrap_or(false)
        {
            return wrap_node(context, candidate);
        }
        current = with_host(|host| {
            host.document
                .try_node(candidate)
                .and_then(|node| node.parent)
        })
        .flatten();
    }
    Ok(JsValue::null())
}

fn insert_relative(
    host: &mut crate::host::Host,
    target: NodeId,
    node: NodeId,
    position: AdjacentPosition,
) {
    let container = match position {
        AdjacentPosition::BeforeBegin => {
            let parent = host.document.try_node(target).and_then(|node| node.parent);
            if let Some(parent) = parent {
                let _ = host
                    .document
                    .insert_before_child(parent, node, Some(target));
            }
            parent
        }
        AdjacentPosition::AfterBegin => {
            let first = host
                .document
                .try_node(target)
                .and_then(|node| node.first_child);
            let _ = host.document.insert_before_child(target, node, first);
            Some(target)
        }
        AdjacentPosition::BeforeEnd => {
            host.document.append_child(target, node);
            Some(target)
        }
        AdjacentPosition::AfterEnd => {
            let parent = host.document.try_node(target).and_then(|node| node.parent);
            if let Some(parent) = parent {
                let next = host
                    .document
                    .try_node(target)
                    .and_then(|node| node.next_sibling);
                let _ = host.document.insert_before_child(parent, node, next);
            }
            parent
        }
    };
    if let Some(container) = container {
        host.record_mutation(MutationRecord {
            kind: MutationKind::ChildList,
            target: container,
            name: None,
            old_value: None,
        });
    }
}

/// Validates the `position` argument of `insertAdjacent*`, throwing `SyntaxError` for
/// anything outside the four defined values (#653).
fn adjacent_position(args: &[JsValue], context: &mut Context) -> JsResult<AdjacentPosition> {
    let raw = arg_string(args, 0, context)?;
    let lowered = raw.to_ascii_lowercase();
    AdjacentPosition::parse(&lowered).ok_or_else(|| {
        syntax_error(format!(
            "insertAdjacent*: unknown position `{raw}` (expected beforebegin, afterbegin, beforeend or afterend)"
        ))
    })
}

fn node_insert_adjacent_html(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = this_node(this, context)?;
    let is_element = with_host(|host| {
        host.document
            .try_node(target)
            .is_some_and(|node| node.as_element().is_some())
    })
    .unwrap_or(false);
    if !is_element {
        return Err(js_error("insertAdjacentHTML is only supported on elements"));
    }
    let position = adjacent_position(args, context)?;
    let html_text = arg_string(args, 1, context)?;
    with_host(|host| {
        if !charge_document_parse(host, html_text.len(), "insertAdjacentHTML") {
            return;
        }
        let temp = host
            .document
            .create_element(Namespace::Html, "div", Vec::new());
        let options = html::ParseOptions {
            max_nodes: host.max_document_write_nodes,
            ..html::ParseOptions::default()
        };
        if html::set_inner_html_with_options(&mut host.document, temp, &html_text, &options)
            .is_err()
        {
            return;
        }
        let children: Vec<NodeId> = host.document.children(temp).collect();
        for child in children {
            insert_relative(host, target, child, position);
        }
    });
    Ok(JsValue::undefined())
}

fn node_insert_adjacent_text(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = this_node(this, context)?;
    let is_element = with_host(|host| {
        host.document
            .try_node(target)
            .is_some_and(|node| node.as_element().is_some())
    })
    .unwrap_or(false);
    if !is_element {
        return Err(js_error("insertAdjacentText is only supported on elements"));
    }
    let position = adjacent_position(args, context)?;
    let text = arg_string(args, 1, context)?;
    with_host(|host| {
        let node = host.document.create_text_node(&text);
        insert_relative(host, target, node, position);
    });
    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// CSSOM: element.style and element.classList
// ---------------------------------------------------------------------------

/// Common CSS properties exposed by the inline `style` object (camelCase → CSS name).
macro_rules! style_prop_accessors {
    ($($get:ident, $set:ident => $css:literal),+ $(,)?) => {
        $(
            fn $get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
                let node_id = this_node(this, context)?;
                let value = with_host(|host| {
                    inline_style_value(&host.document, node_id, $css)
                })
                .flatten();
                Ok(match value {
                    Some(value) => JsValue::from(js_string!(value)),
                    None => JsValue::from(js_string!("")),
                })
            }

            fn $set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
                let node_id = this_node(this, context)?;
                let value = arg_string(args, 0, context)?;
                match with_host(|host| set_inline_style(&mut host.document, node_id, $css, &value)) {
                    Some(Ok(())) => Ok(JsValue::undefined()),
                    Some(Err(message)) => Err(js_error(message)),
                    None => Err(js_error("no active JavaScript host")),
                }
            }
        )+
    };
}

style_prop_accessors!(
    style_color_get, style_color_set => "color",
    style_background_color_get, style_background_color_set => "background-color",
    style_font_size_get, style_font_size_set => "font-size",
    style_width_get, style_width_set => "width",
    style_height_get, style_height_set => "height",
    style_display_get, style_display_set => "display",
);

fn style_object(context: &mut Context, handle: u32) -> JsValue {
    let accessors: [(&str, NativeFn, NativeFn); 6] = [
        ("color", style_color_get, style_color_set),
        (
            "backgroundColor",
            style_background_color_get,
            style_background_color_set,
        ),
        ("fontSize", style_font_size_get, style_font_size_set),
        ("width", style_width_get, style_width_set),
        ("height", style_height_get, style_height_set),
        ("display", style_display_get, style_display_set),
    ];
    let mut prepared = Vec::with_capacity(accessors.len());
    for (name, get, set) in accessors {
        prepared.push((name, accessor_fn(context, get), accessor_fn(context, set)));
    }
    let css_text_get = accessor_fn(context, style_css_text_get);
    let css_text_set = accessor_fn(context, style_css_text_set);
    let length_get = accessor_fn(context, style_length_get);
    let methods: [(&str, usize, NativeFn); 4] = [
        ("getPropertyValue", 1, style_get_property_value),
        ("setProperty", 1, style_set_property),
        ("removeProperty", 1, style_remove_property),
        ("item", 1, style_item),
    ];
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_node"),
        f64::from(handle),
        Attribute::all(),
    );
    for (name, get, set) in prepared {
        init.accessor(js_string!(name), Some(get), Some(set), Attribute::all());
    }
    init.accessor(
        js_string!("cssText"),
        Some(css_text_get),
        Some(css_text_set),
        Attribute::all(),
    );
    init.accessor(
        js_string!("length"),
        Some(length_get),
        None,
        Attribute::all(),
    );
    for (name, length, function) in methods {
        init.function(native(function), js_string!(name), length);
    }
    JsValue::from(init.build())
}

/// Resolves the inline-style node from a `style` object receiver.
fn style_node(this: &JsValue, context: &mut Context) -> JsResult<NodeId> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("style receiver is not an object"))?;
    let handle = object
        .get(js_string!("__megi_node"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .filter(|number| *number >= 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("style receiver is not an element"))?;
    with_host(|host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)
}

fn inline_declarations(doc: &Document, node_id: NodeId) -> Vec<css::Declaration> {
    doc.try_node(node_id)
        .and_then(|node| node.as_element())
        .and_then(|element| element.get_attribute("style"))
        .map(css::parser::parse_list_of_declarations)
        .unwrap_or_default()
}

fn write_inline_declarations(
    doc: &mut Document,
    node_id: NodeId,
    declarations: &[css::Declaration],
) -> Result<(), String> {
    let text = declarations
        .iter()
        .map(|declaration| format!("{}: {};", declaration.name, declaration.raw_value))
        .collect::<Vec<_>>()
        .join(" ");
    let element = doc
        .try_node_mut(node_id)
        .and_then(|node| node.as_element_mut())
        .ok_or_else(|| "node is not an element".to_string())?;
    element.set_attribute(DomAttribute::new("style", text), true);
    Ok(())
}

fn style_css_text_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let text = with_host(|host| {
        inline_declarations(&host.document, node)
            .iter()
            .map(|declaration| format!("{}: {};", declaration.name, declaration.raw_value))
            .collect::<Vec<_>>()
            .join(" ")
    })
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(text)))
}

fn style_css_text_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let text = arg_string(args, 0, context)?;
    let declarations = css::parser::parse_list_of_declarations(&text);
    match with_host(|host| write_inline_declarations(&mut host.document, node, &declarations)) {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

fn style_length_get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let length = with_host(|host| inline_declarations(&host.document, node).len()).unwrap_or(0);
    Ok(JsValue::from(length as f64))
}

fn style_get_property_value(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let value = with_host(|host| inline_style_value(&host.document, node, &name)).flatten();
    Ok(JsValue::from(js_string!(value.unwrap_or_default())))
}

fn style_set_property(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let value = arg_string(args, 1, context)?;
    match with_host(|host| set_inline_style(&mut host.document, node, &name, &value)) {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

fn style_remove_property(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let previous = with_host(|host| {
        let mut declarations = inline_declarations(&host.document, node);
        let previous = declarations
            .iter()
            .find(|declaration| declaration.name.eq_ignore_ascii_case(&name))
            .map(|declaration| declaration.raw_value.clone())
            .unwrap_or_default();
        declarations.retain(|declaration| !declaration.name.eq_ignore_ascii_case(&name));
        let _ = write_inline_declarations(&mut host.document, node, &declarations);
        previous
    })
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(previous)))
}

fn style_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let index = args
        .first()
        .and_then(JsValue::as_number)
        .unwrap_or(0.0)
        .max(0.0) as usize;
    let name = with_host(|host| {
        inline_declarations(&host.document, node)
            .into_iter()
            .nth(index)
            .map(|declaration| declaration.name)
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(name)))
}

fn inline_style_value(doc: &Document, node_id: NodeId, property: &str) -> Option<String> {
    let style_text = doc.try_node(node_id).and_then(|node| {
        node.as_element()
            .and_then(|element| element.get_attribute("style"))
    })?;
    let declarations = css::parser::parse_list_of_declarations(style_text);
    declarations
        .iter()
        .find(|declaration| declaration.name.eq_ignore_ascii_case(property))
        .map(|declaration| declaration.raw_value.clone())
}

fn set_inline_style(
    doc: &mut Document,
    node_id: NodeId,
    property: &str,
    value: &str,
) -> Result<(), String> {
    let style_text = doc
        .try_node_mut(node_id)
        .and_then(|node| node.as_element_mut())
        .map(|element| element.get_attribute("style").map(str::to_owned))
        .ok_or_else(|| "node is not an element".to_string())?
        .unwrap_or_default();
    let mut declarations = css::parser::parse_list_of_declarations(&style_text);
    declarations.retain(|declaration| !declaration.name.eq_ignore_ascii_case(property));
    declarations.push(css::Declaration {
        name: property.to_ascii_lowercase(),
        value: Vec::new(),
        important: false,
        raw_value: value.to_string(),
    });
    let style = declarations
        .iter()
        .map(|declaration| format!("{}: {};", declaration.name, declaration.raw_value))
        .collect::<Vec<_>>()
        .join(" ");
    let element = doc
        .try_node_mut(node_id)
        .and_then(|node| node.as_element_mut())
        .ok_or_else(|| "node is not an element".to_string())?;
    element.set_attribute(DomAttribute::new("style", style), true);
    Ok(())
}

fn class_list_object(context: &mut Context, handle: u32) -> JsValue {
    let length = accessor_fn(context, class_length);
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_node"),
        f64::from(handle),
        Attribute::all(),
    );
    for (name, length, function) in [
        (
            "add",
            1usize,
            class_add as fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
        ),
        ("remove", 1, class_remove),
        ("contains", 1, class_contains),
        ("toggle", 1, class_toggle),
        ("toString", 0, class_to_string),
        ("item", 1, class_item),
    ] {
        init.function(native(function), js_string!(name), length);
    }
    init.accessor(js_string!("length"), Some(length), None, Attribute::all());
    JsValue::from(init.build())
}

fn classes_of(this: &JsValue, context: &mut Context) -> JsResult<(NodeId, Vec<String>)> {
    let node_id = this_node(this, context)?;
    let classes = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute("class"))
            })
            .map(|value| value.split_ascii_whitespace().map(str::to_owned).collect())
            .unwrap_or_default()
    })
    .unwrap_or_default();
    Ok((node_id, classes))
}

fn write_classes(node_id: NodeId, classes: &[String]) {
    with_host(|host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(DomAttribute::new("class", classes.join(" ")), true);
        }
    });
}

fn class_add(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (node_id, mut classes) = classes_of(this, context)?;
    for index in 0..args.len() {
        let class = arg_string(args, index, context)?;
        if !class.is_empty() && !classes.contains(&class) {
            classes.push(class);
        }
    }
    write_classes(node_id, &classes);
    Ok(JsValue::undefined())
}

fn class_remove(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (node_id, mut classes) = classes_of(this, context)?;
    for index in 0..args.len() {
        let class = arg_string(args, index, context)?;
        classes.retain(|existing| *existing != class);
    }
    write_classes(node_id, &classes);
    Ok(JsValue::undefined())
}

fn class_contains(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (_, classes) = classes_of(this, context)?;
    let class = arg_string(args, 0, context)?;
    Ok(JsValue::from(classes.contains(&class)))
}

fn class_toggle(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (node_id, mut classes) = classes_of(this, context)?;
    let class = arg_string(args, 0, context)?;
    let contains = classes.contains(&class);
    let add = match args.get(1) {
        Some(force) if force.is_boolean() => force.to_boolean(),
        _ => !contains,
    };
    if add {
        if !contains {
            classes.push(class);
        }
    } else {
        classes.retain(|existing| *existing != class);
    }
    write_classes(node_id, &classes);
    Ok(JsValue::from(add))
}

fn class_to_string(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (_, classes) = classes_of(this, context)?;
    Ok(JsValue::from(js_string!(classes.join(" "))))
}

fn class_length(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (_, classes) = classes_of(this, context)?;
    Ok(JsValue::from(classes.len() as f64))
}

// ---------------------------------------------------------------------------
// Timers

// ---------------------------------------------------------------------------
// History, location, Web Storage and MutationObserver (round 24)
// ---------------------------------------------------------------------------

fn root_array(context: &mut Context, name: &str, create: bool) -> JsResult<JsObject> {
    let global = context.global_object();
    let value = global
        .get(js_string!(name), context)
        .map_err(|error| js_error(error.to_string()))?;
    // Defensively type-check: a script may have replaced the rooted table with a
    // non-array value. Treat that as "missing" and recreate it (#644).
    if let Some(array) = value.as_object()
        && array
            .get(js_string!("length"), context)
            .ok()
            .and_then(|v| v.as_number())
            .is_some()
    {
        return Ok(array.clone());
    }
    let array = JsArray::from_iter([], context);
    global
        .set(
            js_string!(name),
            JsValue::from(array.clone()),
            false,
            context,
        )
        .map_err(|error| js_error(error.to_string()))?;
    let _ = create;
    Ok(JsObject::from(array))
}

fn root_push(context: &mut Context, name: &str, value: JsValue) -> JsResult<u32> {
    let array = root_array(context, name, true)?;
    let length = array
        .get(js_string!("length"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .unwrap_or(0.0) as u32;
    array
        .set(js_string!(length.to_string()), value, false, context)
        .map_err(|error| js_error(error.to_string()))?;
    Ok(length)
}

fn root_get(context: &mut Context, name: &str, index: u64) -> JsResult<JsValue> {
    let array = root_array(context, name, false)?;
    array
        .get(js_string!(index.to_string()), context)
        .map_err(|error| js_error(error.to_string()))
}

fn root_set(context: &mut Context, name: &str, index: u64, value: JsValue) -> JsResult<()> {
    let array = root_array(context, name, true)?;
    array
        .set(js_string!(index.to_string()), value, false, context)
        .map(|_| ())
        .map_err(|error| js_error(error.to_string()))
}

/// Serializes a JS value to JSON for the history provider (falls back to stringification).
fn json_state(value: &JsValue, context: &mut Context) -> String {
    let json = context
        .global_object()
        .get(js_string!("JSON"), context)
        .ok()
        .and_then(|value| value.as_object());
    if let Some(json) = json
        && let Ok(stringify) = json.get(js_string!("stringify"), context)
        && let Some(function) = stringify.as_object()
        && let Ok(result) =
            function.call(&JsValue::undefined(), std::slice::from_ref(value), context)
        && let Ok(text) = result.to_string(context)
    {
        return text.to_std_string_escaped();
    }
    value
        .to_string(context)
        .map(|text| text.to_std_string_escaped())
        .unwrap_or_else(|_| "null".to_string())
}

fn history_push_state(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let state = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let json = json_state(&state, context);
    if json.len() > MAX_HISTORY_STATE_BYTES {
        return Err(js_error(format!(
            "QuotaExceededError: history state limit of 1 MiB exceeded ({} bytes)",
            json.len()
        )));
    }
    let url = args.get(2).and_then(|value| {
        value
            .to_string(context)
            .ok()
            .map(|text| text.to_std_string_escaped())
            .filter(|text| !text.is_empty())
    });
    let resolved = with_host(|host| url.as_deref().map(|url| host.resolve_url(url)));
    let Some(resolved) = resolved else {
        return Err(js_error("no active JavaScript host"));
    };
    let applied = with_host(|host| {
        if let Some(provider) = &host.history_provider {
            provider(HistoryAction::Push {
                state: json.clone(),
                url: resolved.clone(),
            });
            true
        } else {
            false
        }
    })
    .unwrap_or(false);
    if applied {
        root_set(context, "__megi_history_state", 0, state)?;
        with_host(|host| {
            host.history_length = host.history_length.saturating_add(1);
            host.history_state = json.clone();
            if let Some(url) = &resolved {
                host.url = url.clone();
            }
        });
    }
    Ok(JsValue::undefined())
}

fn history_replace_state(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let state = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let json = json_state(&state, context);
    if json.len() > MAX_HISTORY_STATE_BYTES {
        return Err(js_error(format!(
            "QuotaExceededError: history state limit of 1 MiB exceeded ({} bytes)",
            json.len()
        )));
    }
    let url = args.get(2).and_then(|value| {
        value
            .to_string(context)
            .ok()
            .map(|text| text.to_std_string_escaped())
            .filter(|text| !text.is_empty())
    });
    let resolved = with_host(|host| url.as_deref().map(|url| host.resolve_url(url)));
    let Some(resolved) = resolved else {
        return Err(js_error("no active JavaScript host"));
    };
    let applied = with_host(|host| {
        if let Some(provider) = &host.history_provider {
            provider(HistoryAction::Replace {
                state: json.clone(),
                url: resolved.clone(),
            });
            true
        } else {
            false
        }
    })
    .unwrap_or(false);
    if applied {
        root_set(context, "__megi_history_state", 0, state)?;
        with_host(|host| {
            host.history_state = json.clone();
            if let Some(url) = &resolved {
                host.url = url.clone();
            }
        });
    }
    Ok(JsValue::undefined())
}

fn history_back(_this: &JsValue, _args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    with_host(|host| {
        if let Some(provider) = &host.history_provider {
            provider(HistoryAction::Back);
        }
    });
    Ok(JsValue::undefined())
}

fn history_forward(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    with_host(|host| {
        if let Some(provider) = &host.history_provider {
            provider(HistoryAction::Forward);
        }
    });
    Ok(JsValue::undefined())
}

fn history_go(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let delta = args
        .first()
        .and_then(|value| value.to_number(context).ok())
        .map(|number| number as i64)
        .unwrap_or(0);
    with_host(|host| {
        if let Some(provider) = &host.history_provider {
            provider(HistoryAction::Go(delta));
        }
    });
    Ok(JsValue::undefined())
}

fn history_length_get(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let _ = context;
    Ok(JsValue::from(
        with_host(|host| host.history_length).unwrap_or(1),
    ))
}

fn history_state_get(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let value = root_get(context, "__megi_history_state", 0)?;
    if value.is_undefined() {
        Ok(JsValue::null())
    } else {
        Ok(value)
    }
}

fn url_parts(url: &str) -> (String, String, String, String, String) {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let protocol = if scheme.is_empty() {
        String::new()
    } else {
        format!("{scheme}:")
    };
    let (authority, tail) = rest.split_once('/').unwrap_or((rest, ""));
    let origin = if scheme.is_empty() {
        String::new()
    } else {
        format!("{scheme}://{authority}")
    };
    let (path_query, hash) = tail.split_once('#').unwrap_or((tail, ""));
    let pathname = if scheme.is_empty() && rest.starts_with('/') {
        rest.split(['?', '#']).next().unwrap_or("").to_string()
    } else {
        format!("/{}", path_query.split('?').next().unwrap_or(""))
    };
    let search = path_query
        .split_once('?')
        .map(|(_, query)| format!("?{query}"))
        .unwrap_or_default();
    let hash = if hash.is_empty() {
        String::new()
    } else {
        format!("#{hash}")
    };
    (
        protocol,
        origin,
        authority.to_string(),
        pathname,
        format!("{search}\u{1f}{hash}"),
    )
}

fn current_location(
    context: &mut Context,
) -> JsResult<(String, String, String, String, String, String)> {
    let _ = context;
    let url =
        with_host(|host| host.url.clone()).ok_or_else(|| js_error("no active JavaScript host"))?;
    let (protocol, origin, host, pathname, rest) = url_parts(&url);
    let (search, hash) = rest.split_once('\u{1f}').unwrap_or(("", ""));
    Ok((
        protocol,
        origin,
        host,
        pathname,
        search.to_string(),
        hash.to_string(),
    ))
}

fn location_protocol_get(_t: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.0)))
}
fn location_origin_get(_t: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.1)))
}
fn location_host_get(_t: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.2)))
}
fn location_pathname_get(_t: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.3)))
}
fn location_search_get(_t: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.4)))
}
fn location_hash_get(_t: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.5)))
}
fn location_href_get(_t: &JsValue, _a: &[JsValue], _c: &mut Context) -> JsResult<JsValue> {
    let url =
        with_host(|host| host.url.clone()).ok_or_else(|| js_error("no active JavaScript host"))?;
    Ok(JsValue::from(js_string!(url)))
}

fn navigate_location(args: &[JsValue], context: &mut Context, replace: bool) -> JsResult<JsValue> {
    let url = arg_string(args, 0, context)?;
    let resolved = with_host(|host| host.resolve_url(&url));
    let Some(resolved) = resolved else {
        return Err(js_error("no active JavaScript host"));
    };
    with_host(|host| {
        if let Some(provider) = &host.location_provider {
            provider(if replace {
                LocationAction::Replace(resolved.clone())
            } else {
                LocationAction::Assign(resolved.clone())
            });
        }
        host.url = resolved.clone();
    });
    Ok(JsValue::undefined())
}

fn location_assign(_t: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    navigate_location(args, context, false)
}
fn location_replace(_t: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    navigate_location(args, context, true)
}
fn location_reload(_t: &JsValue, _args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    with_host(|host| {
        if let Some(provider) = &host.location_provider {
            provider(LocationAction::Reload);
        }
    });
    Ok(JsValue::undefined())
}
fn location_href_set(_t: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    navigate_location(args, context, false)
}

fn storage_kind(this: &JsValue, context: &mut Context) -> JsResult<StorageKind> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("storage receiver"))?;
    let value = object
        .get(js_string!("__megi_storage_kind"), context)
        .map_err(|error| js_error(error.to_string()))?;
    Ok(if value.to_boolean() {
        StorageKind::Session
    } else {
        StorageKind::Local
    })
}

fn storage_read<F: Fn(&Host) -> &HashMap<String, String>>(
    this: &JsValue,
    context: &mut Context,
    f: F,
) -> JsResult<HashMap<String, String>> {
    let kind = storage_kind(this, context)?;
    let map = with_host(|host| match kind {
        StorageKind::Local => host.local_storage.clone(),
        StorageKind::Session => host.session_storage.clone(),
    })
    .unwrap_or_default();
    let _ = f;
    Ok(map)
}

fn persist_storage(kind: StorageKind) {
    with_host(|host| {
        let snapshot: Vec<(String, String)> = match kind {
            StorageKind::Local => host
                .local_storage
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            StorageKind::Session => host
                .session_storage
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };
        if let Some(provider) = &host.storage_provider {
            (provider.save)(kind, &snapshot);
        }
    });
}

fn storage_get_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let key = arg_string(args, 0, context)?;
    let map = storage_read(this, context, |_| unreachable!())?;
    Ok(map.get(&key).map_or_else(JsValue::null, |value| {
        JsValue::from(js_string!(value.clone()))
    }))
}

fn storage_set_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let key = arg_string(args, 0, context)?;
    let value = arg_string(args, 1, context)?;
    let kind = storage_kind(this, context)?;
    let current = storage_read(this, context, |_| unreachable!())?;
    let mut total: usize = current.iter().map(|(k, v)| k.len() + v.len()).sum();
    if let Some(previous) = current.get(&key) {
        total = total.saturating_sub(key.len() + previous.len());
    }
    total = total.saturating_add(key.len() + value.len());
    if total > 1024 * 1024 {
        return Err(js_error(
            "QuotaExceededError: Web Storage limit of 1 MiB per realm exceeded",
        ));
    }
    with_host(|host| match kind {
        StorageKind::Local => host.local_storage.insert(key, value),
        StorageKind::Session => host.session_storage.insert(key, value),
    });
    persist_storage(kind);
    Ok(JsValue::undefined())
}

fn storage_remove_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let key = arg_string(args, 0, context)?;
    let kind = storage_kind(this, context)?;
    with_host(|host| match kind {
        StorageKind::Local => host.local_storage.remove(&key),
        StorageKind::Session => host.session_storage.remove(&key),
    });
    persist_storage(kind);
    Ok(JsValue::undefined())
}

fn storage_clear(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let kind = storage_kind(this, context)?;
    with_host(|host| match kind {
        StorageKind::Local => host.local_storage.clear(),
        StorageKind::Session => host.session_storage.clear(),
    });
    persist_storage(kind);
    Ok(JsValue::undefined())
}

fn storage_key(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let index = args
        .first()
        .and_then(|value| value.to_number(context).ok())
        .map(|number| number as usize)
        .unwrap_or(0);
    let map = storage_read(this, context, |_| unreachable!())?;
    Ok(map
        .keys()
        .nth(index)
        .map_or_else(JsValue::null, |key| JsValue::from(js_string!(key.clone()))))
}

fn storage_length_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let map = storage_read(this, context, |_| unreachable!())?;
    Ok(JsValue::from(map.len() as f64))
}

// --- MutationObserver -----------------------------------------------------

fn observer_slot(this: &JsValue, context: &mut Context) -> JsResult<u32> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("observer receiver"))?;
    object
        .get(js_string!("__megi_observer"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .filter(|number| *number >= 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("invalid MutationObserver"))
}

fn mutation_observer_constructor(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let Some(callback) = args.first().filter(|value| value.is_callable()) else {
        return Err(js_error("MutationObserver callback must be a function"));
    };
    let callback_slot = root_push(context, "__megi_observer_callbacks", callback.clone())?;
    let object_slot = {
        let array = root_array(context, "__megi_observer_objects", true)?;
        array
            .get(js_string!("length"), context)
            .map_err(|error| js_error(error.to_string()))?
            .as_number()
            .unwrap_or(0.0) as u32
    };
    let mut init = ObjectInitializer::new(context);
    init.function(native(observer_observe), js_string!("observe"), 2);
    init.function(native(observer_disconnect), js_string!("disconnect"), 0);
    init.function(native(observer_take_records), js_string!("takeRecords"), 0);
    let observer = init.build();
    observer
        .set(
            js_string!("__megi_observer"),
            JsValue::from(object_slot),
            false,
            context,
        )
        .map_err(|error| js_error(error.to_string()))?;
    root_set(
        context,
        "__megi_observer_objects",
        u64::from(object_slot),
        JsValue::from(observer.clone()),
    )?;
    with_host(|host| {
        host.observers.push(ObserverRegistration {
            callback_slot,
            object_slot,
            targets: Vec::new(),
            records: Vec::new(),
        });
    });
    Ok(JsValue::from(observer))
}

fn observer_observe(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let slot = observer_slot(this, context)?;
    let node = argument_node(args, 0, context)?;
    let options = args
        .get(1)
        .and_then(|value| value.as_object())
        .ok_or_else(|| js_error("MutationObserver.observe options are required"))?;
    let mut flag = |name: &str| -> bool {
        options
            .get(js_string!(name), context)
            .ok()
            .is_some_and(|value| value.to_boolean())
    };
    let target = ObserverTarget {
        node,
        child_list: flag("childList"),
        attributes: flag("attributes"),
        character_data: flag("characterData"),
        subtree: flag("subtree"),
    };
    with_host(|host| {
        if let Some(registration) = host.observers.iter_mut().find(|r| r.object_slot == slot) {
            registration.targets.push(target);
        }
    });
    Ok(JsValue::undefined())
}

fn observer_disconnect(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let slot = observer_slot(this, context)?;
    with_host(|host| host.observers.retain(|r| r.object_slot != slot));
    Ok(JsValue::undefined())
}

fn observer_take_records(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let slot = observer_slot(this, context)?;
    let records = collect_observer_records(context, slot, true)?;
    Ok(JsValue::from(records))
}

fn note_mutation(
    kind: MutationKind,
    target: NodeId,
    name: Option<String>,
    old_value: Option<String>,
) {
    with_host(|host| {
        if !host.observers.is_empty() {
            host.record_mutation(MutationRecord {
                kind,
                target,
                name,
                old_value,
            });
        }
    });
}

fn collect_observer_records(context: &mut Context, slot: u32, take: bool) -> JsResult<JsArray> {
    let records = with_host(|host| {
        let taken = {
            let Some(registration) = host.observers.iter_mut().find(|r| r.object_slot == slot)
            else {
                return Vec::new();
            };
            if take {
                std::mem::take(&mut registration.records)
            } else {
                registration.records.clone()
            }
        };
        taken
            .into_iter()
            .filter(|record| host.document.try_node(record.target).is_some())
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    let array = JsArray::from_iter([], context);
    for (index, record) in records.iter().enumerate() {
        let kind = match record.kind {
            MutationKind::ChildList => "childList",
            MutationKind::Attributes => "attributes",
            MutationKind::CharacterData => "characterData",
        };
        let target = wrap_node(context, record.target)?;
        let mut init = ObjectInitializer::new(context);
        init.property(
            js_string!("type"),
            JsValue::from(js_string!(kind)),
            Attribute::READONLY,
        );
        init.property(js_string!("target"), target, Attribute::READONLY);
        if let Some(name) = &record.name {
            init.property(
                js_string!("attributeName"),
                JsValue::from(js_string!(name.clone())),
                Attribute::READONLY,
            );
        }
        let record = init.build();
        array
            .set(
                js_string!(index.to_string()),
                JsValue::from(record),
                false,
                context,
            )
            .map_err(|error| js_error(error.to_string()))?;
    }
    Ok(array)
}

fn deliver_mutation_records(context: &mut Context) -> Result<(), JsError> {
    let registrations: Vec<(u32, u32)> = with_host(|host| {
        host.observers
            .iter()
            .filter(|r| !r.records.is_empty())
            .map(|r| (r.object_slot, r.callback_slot))
            .collect()
    })
    .unwrap_or_default();
    for (object_slot, callback_slot) in registrations {
        let records = collect_observer_records(context, object_slot, true)
            .map_err(|error| JsError::Runtime(error.to_string()))?;
        if records.length(context).unwrap_or(0) == 0 {
            continue;
        }
        let callback = root_get(
            context,
            "__megi_observer_callbacks",
            u64::from(callback_slot),
        )
        .map_err(|error| JsError::Runtime(error.to_string()))?;
        let object = root_get(context, "__megi_observer_objects", u64::from(object_slot))
            .map_err(|error| JsError::Runtime(error.to_string()))?;
        let Some(callback) = callback.as_object() else {
            continue;
        };
        let args = [JsValue::from(records), object];
        if let Err(error) = callback.call(&JsValue::undefined(), &args, context) {
            return Err(JsError::Runtime(error.to_string()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ES modules (round 25 subset preprocessor + host resolver/loader)
// ---------------------------------------------------------------------------

use crate::ModuleOutcome;

#[derive(Clone, Debug)]
enum ImportBinding {
    SideEffect,
    Default(String),
    Namespace(String),
    Named(Vec<(String, String)>), // local, exported
}

#[derive(Clone, Debug)]
struct ImportSpec {
    specifier: String,
    binding: ImportBinding,
}

#[derive(Clone, Debug)]
struct ParsedModule {
    imports: Vec<ImportSpec>,
    body: String,
    exports: Vec<(String, String)>, // exported name, local expression
    /// Literal specifiers of dynamic `import()` calls (evaluated with the graph; the
    /// returned Promise resolves immediately — documented subset).
    dynamic_imports: Vec<String>,
}

fn module_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn import_specifier(rest: &str) -> Option<(String, String)> {
    // Returns (binding head, specifier). Import declarations must be single-line.
    let rest = rest.trim().trim_end_matches(';').trim();
    if let Some((head, tail)) = rest.rsplit_once(" from ") {
        let specifier = tail.trim().trim_matches(['"', '\'']);
        if specifier.is_empty() {
            return None;
        }
        let head = head.trim().trim_end_matches(',').trim();
        Some((head.to_string(), specifier.to_string()))
    } else {
        let specifier = rest.trim_matches(['"', '\'']);
        (!specifier.is_empty()).then(|| (String::new(), specifier.to_string()))
    }
}

fn parse_named_list(raw: &str) -> Vec<(String, String)> {
    let raw = raw.trim().trim_start_matches('{').trim_end_matches('}');
    raw.split(',')
        .filter_map(|item| {
            let item = item.trim();
            if item.is_empty() {
                return None;
            }
            if let Some((local, exported)) = item.split_once(" as ") {
                Some((local.trim().to_string(), exported.trim().to_string()))
            } else {
                Some((item.to_string(), item.to_string()))
            }
        })
        .collect()
}

fn parse_import_named_list(raw: &str) -> Vec<(String, String)> {
    parse_named_list(raw)
        .into_iter()
        .map(|(local, exported)| (exported, local))
        .collect()
}

fn export_declared_names(decl: &str) -> Vec<String> {
    let decl = decl.trim();
    if let Some(rest) = decl
        .strip_prefix("function ")
        .or_else(|| decl.strip_prefix("class "))
    {
        return rest
            .split(|ch: char| ch == '(' || ch == '{' || ch.is_whitespace())
            .next()
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .into_iter()
            .collect();
    }
    let rest = decl
        .strip_prefix("const ")
        .or_else(|| decl.strip_prefix("let "))
        .or_else(|| decl.strip_prefix("var "))
        .unwrap_or(decl);
    // Naive top-level declarator split; enough for the documented subset.
    rest.split(',')
        .filter_map(|declarator| {
            let name = declarator
                .split('=')
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches(['{', '[', '('])
                .trim();
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

fn transform_module(source: &str, url: &str) -> Result<ParsedModule, JsError> {
    let quoted_url = module_quote(url);
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut body_lines: Vec<String> = Vec::new();
    for raw_line in source.lines() {
        let line = raw_line.replace("import.meta.url", &quoted_url);
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            let rest = trimmed
                .trim_start_matches("import ")
                .trim()
                .trim_end_matches(';')
                .trim();
            if rest.starts_with('"') || rest.starts_with('\'') {
                let specifier = rest.trim_matches(['"', '\'']).to_string();
                imports.push(ImportSpec {
                    specifier,
                    binding: ImportBinding::SideEffect,
                });
                continue;
            }
            let Some((head, specifier)) = import_specifier(rest) else {
                return Err(JsError::ModuleSyntax(format!(
                    "unsupported import statement in {url}: {trimmed}"
                )));
            };
            let binding = if head.is_empty() {
                ImportBinding::SideEffect
            } else if head.starts_with("* as ") {
                ImportBinding::Namespace(
                    head.strip_prefix("* as ")
                        .map(str::trim)
                        .unwrap_or_else(|| head.trim())
                        .to_string(),
                )
            } else if head.starts_with('{') {
                ImportBinding::Named(parse_import_named_list(&head))
            } else if let Some((default_name, named)) = head.split_once(',') {
                let mut bindings = vec![(default_name.trim().to_string(), "default".to_string())];
                bindings.extend(parse_import_named_list(named));
                ImportBinding::Named(bindings)
            } else {
                ImportBinding::Default(head.to_string())
            };
            imports.push(ImportSpec { specifier, binding });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("export ") {
            let rest = rest.trim();
            if let Some(default) = rest.strip_prefix("default ") {
                let default = default.trim().trim_end_matches(';').trim();
                body_lines.push(format!("const __megi_default = ({default});"));
                exports.push(("default".to_string(), "__megi_default".to_string()));
                continue;
            }
            if rest.starts_with('{') {
                let mapping = parse_named_list(rest);
                for (local, exported) in mapping {
                    exports.push((exported, local));
                }
                continue;
            }
            let names = export_declared_names(rest);
            if names.is_empty() {
                return Err(JsError::ModuleSyntax(format!(
                    "unsupported export statement in {url}: {trimmed}"
                )));
            }
            for name in names {
                exports.push((name.clone(), name));
            }
            body_lines.push(rest.to_string());
            continue;
        }
        body_lines.push(line);
    }
    let mut body = body_lines.join("\n");
    let dynamic_imports = rewrite_dynamic_imports(&mut body)?;
    Ok(ParsedModule {
        imports,
        body,
        exports,
        dynamic_imports,
    })
}

/// Rewrites literal `import("specifier")` calls into `__MEGI_DYNAMIC_k__` placeholders and
/// returns the specifiers. Non-literal arguments are a documented subset limitation.
fn rewrite_dynamic_imports(body: &mut String) -> Result<Vec<String>, JsError> {
    let mut specifiers: Vec<String> = Vec::new();
    let mut output = String::with_capacity(body.len());
    let mut rest = body.as_str();
    while let Some(index) = rest.find("import") {
        let after = &rest[index + "import".len()..];
        let mut spaces = after.len() - after.trim_start().len();
        let trimmed = after.trim_start();
        if !trimmed.starts_with('(') {
            output.push_str(&rest[..index + "import".len()]);
            rest = after;
            continue;
        }
        spaces += 0;
        let mut inner = &trimmed[1..];
        inner = inner.trim_start();
        let Some(quote) = inner.chars().next().filter(|ch| *ch == '\'' || *ch == '"') else {
            return Err(JsError::DynamicImport(
                "only string-literal import() specifiers are supported".to_string(),
            ));
        };
        let Some(close) = inner[1..].find(quote) else {
            return Err(JsError::DynamicImport(
                "unterminated import() specifier".to_string(),
            ));
        };
        let specifier = inner[1..1 + close].to_string();
        let after_literal = &inner[1 + close + 1..];
        let Some(paren_offset) = after_literal.find(')') else {
            return Err(JsError::DynamicImport(
                "unterminated import() call".to_string(),
            ));
        };
        if !after_literal[..paren_offset].trim().is_empty() {
            return Err(JsError::DynamicImport(
                "only one string-literal argument is supported".to_string(),
            ));
        }
        let position = specifiers
            .iter()
            .position(|existing| *existing == specifier)
            .unwrap_or_else(|| {
                specifiers.push(specifier.clone());
                specifiers.len() - 1
            });
        output.push_str(&format!("__MEGI_DYNAMIC_{position}__"));
        rest = &after_literal[paren_offset + 1..];
        let _ = spaces;
    }
    output.push_str(rest);
    *body = output;
    Ok(specifiers)
}

/// Detects top-level `await` (the synchronous module evaluator cannot suspend).
fn module_has_top_level_await(body: &str) -> bool {
    let mut depth = 0i32;
    for line in body.lines() {
        let trimmed = line.trim();
        if depth <= 0
            && (trimmed.starts_with("await ")
                || trimmed.starts_with("export default await ")
                || trimmed.contains("= await "))
        {
            return true;
        }
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
    }
    false
}

fn module_error(error: boa_engine::JsError) -> JsError {
    classify_error(&error.to_string())
}

fn resolve_module(
    host: &Rc<RefCell<Host>>,
    specifier: &str,
    base: &str,
) -> Result<String, JsError> {
    let resolved = {
        let host = host.borrow();
        if let Some(map) = host.import_map.as_ref() {
            match map.resolve(specifier) {
                Some(Some(target)) => return Ok(target),
                Some(None) => {
                    return Err(JsError::ImportMapBlocked(format!(
                        "{specifier} (base {base})"
                    )));
                }
                None => {}
            }
        }
        match host.module_resolver.as_ref() {
            Some(resolver) => resolver(specifier, base)?,
            None => None,
        }
    };
    resolved.ok_or_else(|| JsError::ModuleNotFound(format!("{specifier} (base {base})")))
}

fn load_module(host: &Rc<RefCell<Host>>, url: &str) -> Result<String, JsError> {
    let loaded = {
        let host = host.borrow();
        match host.module_loader.as_ref() {
            Some(loader) => Some(loader(url)?),
            None => None,
        }
    };
    loaded.ok_or_else(|| JsError::ModuleNotFound(url.to_string()))
}

fn apply_budget(context: &mut Context, budget: &Budget) {
    context
        .runtime_limits_mut()
        .set_loop_iteration_limit(budget.loop_iterations);
    context
        .runtime_limits_mut()
        .set_recursion_limit(budget.recursion_limit);
    context
        .runtime_limits_mut()
        .set_stack_size_limit(budget.stack_size_limit);
}

/// Maximum nesting of the static module graph (each level is one Rust stack frame).
const MAX_MODULE_DEPTH: usize = 128;
const MAX_MODULE_COUNT: usize = 1024;

#[allow(clippy::too_many_arguments)]
fn eval_module_graph(
    context: &mut Context,
    host: &Rc<RefCell<Host>>,
    url: &str,
    source: &str,
    budget: &Budget,
    visiting: &mut HashSet<String>,
    depth: usize,
    done: &mut HashMap<String, JsObject>,
) -> Result<JsObject, JsError> {
    if let Some(object) = done.get(url) {
        return Ok(object.clone());
    }
    if !visiting.insert(url.to_string()) {
        return Err(JsError::ModuleCycle(url.to_string()));
    }
    // Static imports recurse through this function: an unbounded chain (a loader can serve
    // any depth) would overflow the native stack.
    if depth >= MAX_MODULE_DEPTH {
        visiting.remove(url);
        return Err(JsError::ModuleDepth(url.to_string()));
    }
    // Cap total module count to prevent memory blowup from wide graphs.
    if done.len() + visiting.len() > MAX_MODULE_COUNT {
        visiting.remove(url);
        return Err(JsError::ModuleBudget(url.to_string()));
    }
    let parsed = transform_module(source, url)?;
    let mut namespaces = Vec::with_capacity(parsed.imports.len());
    for import in &parsed.imports {
        let dep_url = resolve_module(host, &import.specifier, url)?;
        let dep_source = load_module(host, &dep_url)?;
        let namespace =
            eval_module_graph(context, host, &dep_url, &dep_source, budget, visiting, depth + 1, done)?;
        namespaces.push(namespace);
    }
    let mut prelude = String::new();
    for (index, import) in parsed.imports.iter().enumerate() {
        let global = format!("__megi_module_{index}");
        context
            .global_object()
            .set(
                js_string!(global.clone()),
                JsValue::from(namespaces[index].clone()),
                false,
                context,
            )
            .map_err(module_error)?;
        match &import.binding {
            ImportBinding::SideEffect => {
                prelude.push_str(&format!("{global};\n"));
            }
            ImportBinding::Default(name) => {
                let value = namespaces[index]
                    .get(js_string!("default"), context)
                    .map_err(module_error)?;
                if value.is_undefined() {
                    return Err(JsError::ModuleExport(format!(
                        "{} has no default export (wanted by {url})",
                        import.specifier
                    )));
                }
                prelude.push_str(&format!("const {name} = {global}.default;\n"));
            }
            ImportBinding::Namespace(name) => {
                prelude.push_str(&format!("const {name} = {global};\n"));
            }
            ImportBinding::Named(bindings) => {
                for (local, exported) in bindings {
                    let value = namespaces[index]
                        .get(js_string!(exported.clone()), context)
                        .map_err(module_error)?;
                    if value.is_undefined() {
                        return Err(JsError::ModuleExport(format!(
                            "{} has no export named {exported} (wanted by {url})",
                            import.specifier
                        )));
                    }
                    let _ = local;
                    prelude.push_str(&format!(
                        "const {local} = {global}[{}];\n",
                        module_quote(exported)
                    ));
                }
            }
        }
    }
    // Literal dynamic imports are evaluated with the graph (documented eager subset).
    for (index, specifier) in parsed.dynamic_imports.iter().enumerate() {
        let dep_url = resolve_module(host, specifier, url)?;
        let dep_source = load_module(host, &dep_url)?;
        let namespace =
            eval_module_graph(context, host, &dep_url, &dep_source, budget, visiting, depth + 1, done)?;
        context
            .global_object()
            .set(
                js_string!(format!("__megi_dynamic_value_{index}")),
                JsValue::from(namespace),
                false,
                context,
            )
            .map_err(module_error)?;
    }
    let mut body = parsed.body;
    for index in 0..parsed.dynamic_imports.len() {
        body = body.replace(
            &format!("__MEGI_DYNAMIC_{index}__"),
            &format!("Promise.resolve(__megi_dynamic_value_{index})"),
        );
    }
    if module_has_top_level_await(&body) {
        return Err(JsError::TopLevelAwait(format!(
            "top-level await in {url} (synchronous evaluator; use eval_module_async for the typed gap)"
        )));
    }
    let return_fields: Vec<String> = parsed
        .exports
        .iter()
        .map(|(exported, local)| format!("{}: ({local})", module_quote(exported)))
        .collect();
    let script = format!(
        "'use strict';\n(function() {{\n{prelude}\n{body}\nreturn {{ {} }};\n}}).call(undefined);",
        return_fields.join(", ")
    );
    apply_budget(context, budget);
    let value = {
        let _scope = HostScope::enter(host);
        context.eval(Source::from_bytes(&script))
    }
    .map_err(module_error)?;
    for index in 0..parsed.imports.len() {
        let _ = context.global_object().set(
            js_string!(format!("__megi_module_{index}")),
            JsValue::undefined(),
            false,
            context,
        );
    }
    let object = value
        .as_object()
        .ok_or_else(|| JsError::Internal("module evaluation returned no namespace".to_string()))?
        .clone();
    visiting.remove(url);
    done.insert(url.to_string(), object.clone());
    Ok(object)
}

impl JsRuntime {
    /// Installs the layout geometry provider used by `getBoundingClientRect`,
    /// `offsetWidth/offsetHeight` and `clientWidth/clientHeight`. Without a provider those
    /// APIs return zeros (documented gap); render installs the real layout before scripts.
    pub fn set_layout_provider(&mut self, provider: crate::host::LayoutProvider) {
        self.host.borrow_mut().layout_provider = Some(provider);
    }

    /// Installs the ES module resolver (specifier + importer URL → resolved URL).
    pub fn set_module_resolver(&mut self, resolver: crate::host::ModuleResolver) {
        self.host.borrow_mut().module_resolver = Some(resolver);
    }

    /// Installs the ES module loader (`url → source text`; `js` performs no I/O itself).
    pub fn set_module_loader(&mut self, loader: crate::host::ModuleLoader) {
        self.host.borrow_mut().module_loader = Some(loader);
    }

    /// Installs an import map from JSON source (`{"imports":{...}}` subset; see
    /// [`crate::ImportMap`]). Resolution is tried before the host module resolver; a `null`
    /// mapping blocks the specifier with [`JsError::ImportMapBlocked`].
    pub fn set_import_map_json(&mut self, source: &str) -> Result<(), JsError> {
        if source.len() > MAX_JS_SOURCE_BYTES {
            return Err(JsError::Budget(
                "import map source exceeds byte cap".to_owned(),
            ));
        }
        let map = crate::ImportMap::from_json(source)?;
        self.host.borrow_mut().import_map = Some(map);
        Ok(())
    }

    /// Evaluates an ES module graph using the injected resolver/loader.
    ///
    /// Supported subset: default/named/namespace/side-effect imports, `export
    /// const/let/var/function/class`, `export default` and `export { a as b }`;
    /// `import.meta.url` is replaced with the module URL. Cycles are rejected with
    /// [`JsError::ModuleCycle`] (documented), missing modules with
    /// [`JsError::ModuleNotFound`], missing/undefined exports with
    /// [`JsError::ModuleExport`]. Import maps installed with [`Self::set_import_map_json`]
    /// are consulted first. Dynamic `import()` is supported with **string literals** and
    /// evaluates the dependency eagerly when the graph runs, returning a Promise resolved
    /// immediately; non-literal arguments are [`JsError::DynamicImport`]. Top-level await is
    /// a documented gap ([`JsError::TopLevelAwait`]). No network/filesystem access (the
    /// host supplies all module text).
    pub fn eval_module(
        &mut self,
        source: &str,
        _source_name: &str,
        base_url: &str,
        budget: &Budget,
    ) -> Result<ModuleOutcome, JsError> {
        if source.len() > MAX_JS_SOURCE_BYTES {
            return Err(JsError::Budget("module source exceeds byte cap".to_owned()));
        }
        apply_budget(&mut self.context, budget);
        let mut visiting = HashSet::new();
        let mut done = HashMap::new();
        let namespace = eval_module_graph(
            &mut self.context,
            &self.host,
            base_url,
            source,
            budget,
            &mut visiting,
            0,
            &mut done,
        )?;
        let mut values = Vec::new();
        let keys = namespace
            .own_property_keys(&mut self.context)
            .map_err(module_error)?;
        for key in keys {
            let key_text = key.to_string();
            if let Ok(value) = namespace.get(js_string!(key_text.clone()), &mut self.context) {
                let text = value
                    .to_string(&mut self.context)
                    .map(|text| text.to_std_string_escaped())
                    .unwrap_or_else(|_| "<unprintable>".to_string());
                values.push((key_text, text));
            }
        }
        values.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(ModuleOutcome {
            url: base_url.to_string(),
            exports: values,
        })
    }
}

// ---------------------------------------------------------------------------
// Round 27 DOM/JS gap APIs
// ---------------------------------------------------------------------------

fn node_inner_text_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let text = with_host(|host| host.document.text_content(node_id)).unwrap_or_default();
    Ok(JsValue::from(js_string!(text)))
}

fn camel_case_attribute(name: &str) -> Option<String> {
    let suffix = name.strip_prefix("data-")?;
    let mut out = String::new();
    let mut upper = false;
    for ch in suffix.chars() {
        if ch == '-' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

fn node_dataset_get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let entries: Vec<(String, String)> = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .map(|element| {
                element
                    .attributes
                    .iter()
                    .filter_map(|attribute| {
                        camel_case_attribute(&attribute.local_name)
                            .map(|key| (key, attribute.value.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
    .unwrap_or_default();
    let mut init = ObjectInitializer::new(context);
    for (key, value) in entries {
        init.property(
            js_string!(key),
            JsValue::from(js_string!(value)),
            Attribute::all(),
        );
    }
    Ok(JsValue::from(init.build()))
}

fn attribute_item(context: &mut Context, name: &str, value: &str) -> JsResult<JsValue> {
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("name"),
        JsValue::from(js_string!(name.to_string())),
        Attribute::all(),
    );
    init.property(
        js_string!("value"),
        JsValue::from(js_string!(value.to_string())),
        Attribute::all(),
    );
    Ok(JsValue::from(init.build()))
}

fn attributes_of(this: &JsValue, context: &mut Context) -> JsResult<(u32, Vec<(String, String)>)> {
    let node_id = this_node(this, context)?;
    let attributes = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .map(|element| {
                element
                    .attributes
                    .iter()
                    .map(|attribute| (attribute.local_name.clone(), attribute.value.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    })
    .unwrap_or_default();
    let handle = with_host(|host| host.handle_for(node_id))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;
    Ok((handle, attributes))
}

fn node_attributes_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (handle, attributes) = attributes_of(this, context)?;
    let mut items = Vec::with_capacity(attributes.len());
    for (name, value) in &attributes {
        items.push(attribute_item(context, name, value)?);
    }
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_attr_node"),
        JsValue::from(f64::from(handle)),
        Attribute::all(),
    );
    init.property(
        js_string!("length"),
        JsValue::from(attributes.len() as f64),
        Attribute::all(),
    );
    for (index, item) in items.into_iter().enumerate() {
        init.property(js_string!(index.to_string()), item, Attribute::all());
    }
    init.function(native(attributes_item), js_string!("item"), 1);
    init.function(native(attributes_get_named), js_string!("getNamedItem"), 1);
    init.function(
        native(attributes_remove_named),
        js_string!("removeNamedItem"),
        1,
    );
    Ok(JsValue::from(init.build()))
}

fn attributes_handle(this: &JsValue, context: &mut Context) -> JsResult<u32> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("attributes receiver"))?;
    object
        .get(js_string!("__megi_attr_node"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .filter(|number| *number >= 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("invalid NamedNodeMap"))
}

fn attributes_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let handle = attributes_handle(this, context)?;
    let index = args
        .first()
        .and_then(|value| value.as_number())
        .unwrap_or(0.0) as usize;
    let node_id = with_host(|host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;
    let attribute = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.attributes.get(index))
            .map(|attribute| (attribute.local_name.clone(), attribute.value.clone()))
    })
    .flatten();
    match attribute {
        Some((name, value)) => attribute_item(context, &name, &value),
        None => Ok(JsValue::null()),
    }
}

fn attributes_get_named(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let handle = attributes_handle(this, context)?;
    let name = arg_string(args, 0, context)?;
    let node_id = with_host(|host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;
    let value = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(&name).map(str::to_owned))
    })
    .flatten();
    match value {
        Some(value) => attribute_item(context, &name, &value),
        None => Ok(JsValue::null()),
    }
}

fn attributes_remove_named(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let handle = attributes_handle(this, context)?;
    let name = arg_string(args, 0, context)?;
    let node_id = with_host(|host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;
    let old = with_host(|host| {
        host.document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .and_then(|element| element.remove_attribute(&name))
    })
    .flatten();
    note_mutation(
        MutationKind::Attributes,
        node_id,
        Some(name.clone()),
        old.clone(),
    );
    match old {
        Some(value) => attribute_item(context, &name, &value),
        None => Err(js_error("NotFoundError: attribute not present")),
    }
}

fn node_normalize(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let removed = with_host(|host| {
        let removed = host.document.normalize(node_id);
        if removed > 0 {
            host.record_mutation(MutationRecord {
                kind: MutationKind::ChildList,
                target: node_id,
                name: None,
                old_value: None,
            });
        }
        removed
    })
    .unwrap_or(0);
    Ok(JsValue::from(removed as f64))
}

fn node_compare_document_position(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let other = argument_node(args, 0, context)?;
    let bits =
        with_host(|host| host.document.compare_document_position(node_id, other)).unwrap_or(0);
    Ok(JsValue::from(bits as f64))
}

fn node_focus(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    with_host(|host| host.active_element = Some(node_id));
    Ok(JsValue::undefined())
}

fn node_blur(_this: &JsValue, _args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    with_host(|host| host.active_element = None);
    Ok(JsValue::undefined())
}

fn node_scroll_into_view(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    // Documented no-op: `js` has no layout/viewport provider for scrolling.
    Ok(JsValue::undefined())
}

fn document_active_element(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    match with_host(|host| host.active_element) {
        Some(Some(node_id)) => wrap_node(context, node_id),
        _ => Ok(JsValue::null()),
    }
}

fn document_normalize(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    let removed = with_host(|host| {
        let root = host.document.document_element();
        root.map(|root| {
            let removed = host.document.normalize(root);
            if removed > 0 {
                host.record_mutation(MutationRecord {
                    kind: MutationKind::ChildList,
                    target: root,
                    name: None,
                    old_value: None,
                });
            }
            removed
        })
        .unwrap_or(0)
    })
    .unwrap_or(0);
    Ok(JsValue::from(removed as f64))
}

fn document_compare_document_position(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let first = argument_node(args, 0, context)?;
    let second = argument_node(args, 1, context)?;
    let bits =
        with_host(|host| host.document.compare_document_position(first, second)).unwrap_or(0);
    Ok(JsValue::from(bits as f64))
}

/// Installs small standard-library gap APIs (URL/SearchParams, base64, structuredClone,
/// queueMicrotask, requestIdleCallback, MouseEvent/PointerEvent and Node constants).
fn install_gap_apis(context: &mut Context) -> Result<(), JsError> {
    let global = context.global_object();
    for (name, getter) in [
        ("innerWidth", window_inner_width as NativeFn),
        ("innerHeight", window_inner_height),
        ("devicePixelRatio", window_device_pixel_ratio),
    ] {
        let get = accessor_fn(context, getter);
        let descriptor = PropertyDescriptor::builder()
            .get(JsValue::from(get))
            .enumerable(true)
            .configurable(true)
            .build();
        global
            .define_property_or_throw(js_string!(name), descriptor, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    const PRELUDE: &str = r####"
(function () {
  function define(name, value) { window[name] = value; }
  function urlNormalizePath(path) {
    var segments = String(path).split("/");
    var out = [];
    for (var i = 0; i < segments.length; i++) {
      var segment = segments[i];
      var last = i === segments.length - 1;
      if (segment === ".") { if (last) { out.push(""); } continue; }
      if (segment === "..") {
        if (out.length > 1 || (out.length === 1 && out[0] !== "")) { out.pop(); }
        if (last) { out.push(""); }
        continue;
      }
      out.push(segment);
    }
    var result = out.join("/");
    if (result === "") { return "/"; }
    if (result.charAt(0) !== "/") { result = "/" + result; }
    return result;
  }
  function urlSplitAuthority(authority) {
    var out = { userinfo: "", host: "", hostname: "", port: "" };
    if (!authority) { return out; }
    var hostport = authority;
    var at = authority.lastIndexOf("@");
    if (at >= 0) { out.userinfo = authority.substring(0, at); hostport = authority.substring(at + 1); }
    var hostname = hostport;
    var port = "";
    if (hostport.charAt(0) === "[") {
      var close = hostport.indexOf("]");
      if (close > 0) {
        hostname = hostport.substring(0, close + 1);
        var rest = hostport.substring(close + 1);
        if (rest.charAt(0) === ":") { port = rest.substring(1); }
      }
    } else {
      var colon = hostport.indexOf(":");
      if (colon >= 0) { hostname = hostport.substring(0, colon); port = hostport.substring(colon + 1); }
    }
    out.hostname = hostname;
    out.port = port;
    out.host = port ? hostname + ":" + port : hostname;
    return out;
  }
  function urlBuild(protocol, userinfo, hostname, port, path, search, hash) {
    var hasAuthority = hostname !== "";
    var pathname = path === "" ? (hasAuthority ? "/" : "") : path;
    if (hasAuthority || pathname.charAt(0) === "/") { pathname = urlNormalizePath(pathname); }
    var searchText = search || "";
    if (searchText !== "" && searchText.charAt(0) !== "?") { searchText = "?" + searchText; }
    var hashText = hash || "";
    if (hashText !== "" && hashText.charAt(0) !== "#") { hashText = "#" + hashText; }
    var authority = hasAuthority
      ? "//" + (userinfo ? userinfo + "@" : "") + hostname + (port ? ":" + port : "")
      : "";
    var host = hasAuthority ? (port ? hostname + ":" + port : hostname) : "";
    var credentials = userinfo ? userinfo.split(":") : [];
    var result = {
      href: protocol + authority + pathname + searchText + hashText,
      protocol: protocol,
      username: credentials.length > 0 ? credentials[0] : "",
      password: credentials.length > 1 ? credentials.slice(1).join(":") : "",
      host: host,
      hostname: hostname,
      port: port,
      pathname: pathname,
      search: searchText,
      hash: hashText,
      origin: hasAuthority ? protocol + "//" + host : "null"
    };
    result.searchParams = null;
    return result;
  }
  function urlParse(input, base) {
    var value = String(input);
    var baseString = base === undefined || base === null ? "" : String(base);
    var scheme = /^([A-Za-z][A-Za-z0-9+.-]*:)/.exec(value);
    var protocolRelative = false;
    if (!scheme && value.substring(0, 2) === "//") {
      // Protocol-relative: inherit the base scheme but not its authority.
      var baseScheme = /^([A-Za-z][A-Za-z0-9+.-]*:)/.exec(baseString);
      if (!baseScheme) { throw new TypeError("Invalid URL: " + value); }
      scheme = baseScheme;
      protocolRelative = true;
    }
    if (!scheme) {
      if (!baseString) { throw new TypeError("Invalid URL: " + value); }
      var resolvedBase = urlParse(baseString, null);
      var path = resolvedBase.pathname;
      var search = resolvedBase.search;
      var hash = "";
      var local = value;
      var hashIndex = local.indexOf("#");
      if (hashIndex >= 0) { hash = local.substring(hashIndex); local = local.substring(0, hashIndex); }
      var localQuery = "";
      var queryIndex = local.indexOf("?");
      if (queryIndex >= 0) { localQuery = local.substring(queryIndex); local = local.substring(0, queryIndex); }
      if (local === "") {
        if (queryIndex >= 0) { search = localQuery; } else if (hashIndex < 0) { search = resolvedBase.search; }
      } else if (local.charAt(0) === "/") {
        path = local;
        search = localQuery;
      } else {
        var slash = path.lastIndexOf("/");
        path = (slash >= 0 ? path.substring(0, slash + 1) : "/") + local;
        search = localQuery;
      }
      var baseUserinfo = resolvedBase.username
        ? (resolvedBase.password ? resolvedBase.username + ":" + resolvedBase.password : resolvedBase.username)
        : "";
      return urlBuild(
        resolvedBase.protocol, baseUserinfo, resolvedBase.hostname, resolvedBase.port, path, search, hash
      );
    }
    var protocol = scheme[1];
    var rest = protocolRelative ? value : value.substring(protocol.length);
    var hashIndex2 = rest.indexOf("#");
    var hash2 = "";
    if (hashIndex2 >= 0) { hash2 = rest.substring(hashIndex2); rest = rest.substring(0, hashIndex2); }
    var search2 = "";
    var queryIndex2 = rest.indexOf("?");
    if (queryIndex2 >= 0) { search2 = rest.substring(queryIndex2); rest = rest.substring(0, queryIndex2); }
    var authority2 = "";
    var path2 = rest;
    if (rest.substring(0, 2) === "//") {
      var afterSlashes = rest.substring(2);
      var end = afterSlashes.length;
      for (var i = 0; i < afterSlashes.length; i++) {
        if (afterSlashes.charAt(i) === "/") { end = i; break; }
      }
      authority2 = afterSlashes.substring(0, end);
      path2 = afterSlashes.substring(end);
    }
    var parts = urlSplitAuthority(authority2);
    return urlBuild(protocol, parts.userinfo, parts.hostname, parts.port, path2, search2, hash2);
  }
  function SearchParams(init) {
    this._list = [];
    if (typeof init === "string") {
      init.replace(/^\?/, "").split("&").forEach(function (pair) {
        if (!pair) { return; }
        var index = pair.indexOf("=");
        var key = index < 0 ? pair : pair.substring(0, index);
        var value = index < 0 ? "" : pair.substring(index + 1);
        this._list.push([decodeURIComponent(key.replace(/\+/g, " ")), decodeURIComponent(value.replace(/\+/g, " "))]);
      }, this);
    } else if (init && init._list) {
      this._list = init._list.map(function (pair) { return [pair[0], pair[1]]; });
    }
  }
  SearchParams.prototype.append = function (key, value) { this._list.push([String(key), String(value)]); };
  SearchParams.prototype["delete"] = function (key) { this._list = this._list.filter(function (pair) { return pair[0] !== String(key); }); };
  SearchParams.prototype.get = function (key) { for (var i = 0; i < this._list.length; i++) { if (this._list[i][0] === String(key)) { return this._list[i][1]; } } return null; };
  SearchParams.prototype.getAll = function (key) { return this._list.filter(function (pair) { return pair[0] === String(key); }).map(function (pair) { return pair[1]; }); };
  SearchParams.prototype.has = function (key) { return this.get(key) !== null; };
  SearchParams.prototype.set = function (key, value) { var found = false; var out = []; this._list.forEach(function (pair) { if (pair[0] === String(key)) { if (!found) { out.push([String(key), String(value)]); found = true; } } else { out.push(pair); } }); if (!found) { out.push([String(key), String(value)]); } this._list = out; };
  SearchParams.prototype.toString = function () { return this._list.map(function (pair) { return encodeURIComponent(pair[0]) + "=" + encodeURIComponent(pair[1]); }).join("&"); };
  SearchParams.prototype.sort = function () { this._list.sort(function (a, b) { return a[0] < b[0] ? -1 : (a[0] > b[0] ? 1 : 0); }); };
  SearchParams.prototype.forEach = function (callback, thisArg) { this._list.forEach(function (pair) { callback.call(thisArg, pair[1], pair[0], this); }, this); };
  define("URLSearchParams", SearchParams);
  function URL(input, base) {
    var parsed = urlParse(input, base);
    for (var key in parsed) { if (Object.prototype.hasOwnProperty.call(parsed, key)) { this[key] = parsed[key]; } }
    this.searchParams = new SearchParams(parsed.search);
  }
  URL.prototype.toString = function () { return this.href; };
  URL.prototype.toJSON = function () { return this.href; };
  define("URL", URL);
  var BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  define("btoa", function (input) {
    var value = String(input); var out = "";
    for (var i = 0; i < value.length; i++) {
      if (value.charCodeAt(i) > 255) { throw new DOMException("InvalidCharacterError", "InvalidCharacterError"); }
    }
    for (var i = 0; i < value.length; i += 3) {
      var a = value.charCodeAt(i), b = value.charCodeAt(i + 1), c = value.charCodeAt(i + 2);
      var triple = (a << 16) | ((isNaN(b) ? 0 : b) << 8) | (isNaN(c) ? 0 : c);
      out += BASE64.charAt((triple >> 18) & 63) + BASE64.charAt((triple >> 12) & 63) + (isNaN(b) ? "=" : BASE64.charAt((triple >> 6) & 63)) + (isNaN(c) ? "=" : BASE64.charAt(triple & 63));
    }
    return out;
  });
  define("atob", function (input) {
    var value = String(input).replace(/\s/g, ""); var out = "";
    for (var i = 0; i < value.length; i++) {
      if (BASE64.indexOf(value.charAt(i)) < 0 && value.charAt(i) !== "=") { throw new DOMException("InvalidCharacterError", "InvalidCharacterError"); }
    }
    for (var i = 0; i < value.length; i += 4) {
      var n = (BASE64.indexOf(value.charAt(i)) << 18) | (BASE64.indexOf(value.charAt(i + 1)) << 12) | ((value.charAt(i + 2) === "=" ? 0 : BASE64.indexOf(value.charAt(i + 2))) << 6) | (value.charAt(i + 3) === "=" ? 0 : BASE64.indexOf(value.charAt(i + 3)));
      out += String.fromCharCode((n >> 16) & 255);
      if (value.charAt(i + 2) !== "=") { out += String.fromCharCode((n >> 8) & 255); }
      if (value.charAt(i + 3) !== "=") { out += String.fromCharCode(n & 255); }
    }
    return out;
  });
  function clone(value, seen) {
    if (value === null || typeof value !== "object") { return value; }
    if (seen.has(value)) { throw new DOMException("DataCloneError: cyclic object value", "DataCloneError"); }
    if (value instanceof Date) { return new Date(value.getTime()); }
    if (value instanceof Map) { var map = new Map(); seen.add(value); value.forEach(function (v, k) { map.set(clone(k, seen), clone(v, seen)); }); seen.delete(value); return map; }
    if (value instanceof Set) { var set = new Set(); seen.add(value); value.forEach(function (v) { set.add(clone(v, seen)); }); seen.delete(value); return set; }
    if (Array.isArray(value)) { seen.add(value); var arr = value.map(function (item) { return clone(item, seen); }); seen.delete(value); return arr; }
    seen.add(value); var out = {}; for (var key in value) { if (Object.prototype.hasOwnProperty.call(value, key)) { out[key] = clone(value[key], seen); } } seen.delete(value); return out;
  }
  define("structuredClone", function (value) { return clone(value, new Set()); });
  if (typeof Promise !== "undefined") { define("queueMicrotask", function (callback) { Promise.resolve().then(callback); }); }
  else { define("queueMicrotask", function (callback) { setTimeout(callback, 0); }); }
  define("requestIdleCallback", function (callback) { return setTimeout(function () { callback({ didTimeout: false, timeRemaining: function () { return 50; } }); }, 0); });
  define("cancelIdleCallback", function (id) { clearTimeout(id); });
  function makeMouseEvent(type, init) {
    init = init || {};
    this.__megi_event = true;
    this.type = String(type); this.bubbles = !!init.bubbles; this.cancelable = !!init.cancelable;
    this.relatedTarget = init.relatedTarget || null;
    this.clientX = init.clientX || 0; this.clientY = init.clientY || 0;
    this.button = init.button || 0; this.defaultPrevented = false;
  }
  makeMouseEvent.prototype.preventDefault = function () { this.defaultPrevented = true; };
  makeMouseEvent.prototype.stopPropagation = function () {};
  makeMouseEvent.prototype.stopImmediatePropagation = function () {};
  define("MouseEvent", makeMouseEvent);
  define("PointerEvent", function (type, init) { makeMouseEvent.call(this, type, init); this.pointerId = (init && init.pointerId) || 0; });
  if (typeof Event === "function" && Event.prototype) { define("PointerEvent", (function (Base) { function PointerEvent(type, init) { Base.call(this, type, init || {}); this.pointerId = (init && init.pointerId) || 0; } PointerEvent.prototype = Object.create(Base.prototype); PointerEvent.prototype.constructor = PointerEvent; return PointerEvent; })(Event)); }
  define("Node", { DOCUMENT_POSITION_DISCONNECTED: 1, DOCUMENT_POSITION_PRECEDING: 2, DOCUMENT_POSITION_FOLLOWING: 4, DOCUMENT_POSITION_CONTAINS: 8, DOCUMENT_POSITION_CONTAINED_BY: 16, DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC: 32 });
  function Headers(init) {
    this._h = [];
    if (init) {
      if (init instanceof Headers) { this._h = init._h.map(function (p) { return [p[0], p[1]]; }); }
      else if (Array.isArray(init)) { for (var i = 0; i < init.length; i++) { this.append(init[i][0], init[i][1]); } }
      else { for (var key in init) { if (Object.prototype.hasOwnProperty.call(init, key)) { this.append(key, init[key]); } } }
    }
  }
  // Header names/values are serialized as `name:value\n` lines for the native side, so a CR,
  // LF or NUL inside either field would forge extra headers (or a colon inside the name would
  // re-split it). Validate at the boundary, like the fetch spec's header-name/header-value
  // grammar (#664).
  function headerName(value) {
    var name = String(value);
    if (/[\r\n\0:]/.test(name)) { throw new TypeError("invalid header name"); }
    return name;
  }
  function headerValue(value) {
    var text = String(value);
    if (/[\r\n\0]/.test(text)) { throw new TypeError("invalid header value"); }
    return text;
  }
  Headers.prototype._find = function (name) { var lower = String(name).toLowerCase(); for (var i = 0; i < this._h.length; i++) { if (this._h[i][0].toLowerCase() === lower) { return i; } } return -1; };
  Headers.prototype.get = function (name) { var lower = String(name).toLowerCase(); var values = []; for (var i = 0; i < this._h.length; i++) { if (this._h[i][0].toLowerCase() === lower) { values.push(this._h[i][1]); } } return values.length === 0 ? null : values.join(", "); };
  Headers.prototype.has = function (name) { return this._find(name) >= 0; };
  Headers.prototype.append = function (name, value) { this._h.push([headerName(name), headerValue(value)]); };
  Headers.prototype.set = function (name, value) { var lower = headerName(name).toLowerCase(); var out = []; var found = false; for (var i = 0; i < this._h.length; i++) { if (this._h[i][0].toLowerCase() === lower) { if (!found) { out.push([this._h[i][0], headerValue(value)]); found = true; } } else { out.push(this._h[i]); } } if (!found) { out.push([headerName(name), headerValue(value)]); } this._h = out; };
  Headers.prototype["delete"] = function (name) { var lower = String(name).toLowerCase(); this._h = this._h.filter(function (pair) { return pair[0].toLowerCase() !== lower; }); };
  Headers.prototype.forEach = function (callback, thisArg) { for (var i = 0; i < this._h.length; i++) { callback.call(thisArg, this._h[i][1], this._h[i][0], this); } };
  Headers.prototype.entries = function () { return this._h.slice(); };
  define("Headers", Headers);
  function AbortSignal() {
    this.aborted = false;
    this.reason = undefined;
    this._listeners = [];
    this.onabort = null;
  }
  AbortSignal.prototype.addEventListener = function (type, callback) {
    if (type === "abort" && typeof callback === "function") { this._listeners.push(callback); }
  };
  AbortSignal.prototype.removeEventListener = function (type, callback) {
    if (type === "abort") { this._listeners = this._listeners.filter(function (item) { return item !== callback; }); }
  };
  AbortSignal.prototype.throwIfAborted = function () { if (this.aborted) { throw abortError(this.reason); } };
  AbortSignal.prototype._abort = function (reason) {
    if (this.aborted) { return; }
    this.aborted = true;
    this.reason = reason;
    var event = { type: "abort", target: this };
    if (typeof this.onabort === "function") { this.onabort(event); }
    for (var i = 0; i < this._listeners.length; i++) { this._listeners[i](event); }
  };
  define("AbortSignal", AbortSignal);
  function AbortController() { this.signal = new AbortSignal(); }
  AbortController.prototype.abort = function (reason) { this.signal._abort(reason); };
  define("AbortController", AbortController);
  function abortError(reason) {
    var error = new Error("The operation was aborted");
    error.name = "AbortError";
    if (reason !== undefined) { error.reason = reason; }
    return error;
  }
  function ReadableStream(chunks) {
    this._chunks = chunks || [];
    this._index = 0;
    this._cancelled = false;
    this.locked = false;
    this._onRead = null;
  }
  ReadableStream.prototype.getReader = function () {
    var stream = this;
    this.locked = true;
    return {
      read: function () {
        if (stream._onRead) { stream._onRead(); }
        if (stream._cancelled || stream._index >= stream._chunks.length) {
          return Promise.resolve({ value: undefined, done: true });
        }
        var chunk = stream._chunks[stream._index++];
        return Promise.resolve({ value: chunk, done: false });
      },
      cancel: function (reason) { stream.cancel(reason); return Promise.resolve(); },
      releaseLock: function () { stream.locked = false; },
      closed: Promise.resolve()
    };
  };
  ReadableStream.prototype.cancel = function (reason) {
    this._cancelled = true;
    this._chunks = [];
    this._index = 0;
    return Promise.resolve(reason);
  };
  define("ReadableStream", ReadableStream);
  function byteChunks(bytes) {
    var chunks = [];
    if (!bytes || bytes.length === 0) { return chunks; }
    var parts = Math.min(3, bytes.length);
    var size = Math.ceil(bytes.length / parts);
    for (var start = 0; start < bytes.length; start += size) {
      chunks.push(new Uint8Array(bytes.slice(start, start + size)));
    }
    return chunks;
  }
  function Response(data) {
    this.status = data.status;
    this.statusText = data.statusText || "";
    this.ok = data.status >= 200 && data.status < 300;
    this.url = data.url || "";
    this.redirected = false;
    this.headers = new Headers(data.headers || []);
    this._body = data.bodyText || "";
    this._bytes = data.bodyBytes || [];
    this._bodyUsed = false;
    var self = this;
    this.body = new ReadableStream(byteChunks(this._bytes));
    this.body._onRead = function () { self._bodyUsed = true; };
  }
  Response.prototype._consume = function () { if (this._bodyUsed) { return null; } this._bodyUsed = true; return this._body; };
  Response.prototype.text = function () { var body = this._consume(); return body === null ? Promise.reject(new TypeError("body already used")) : Promise.resolve(body); };
  Response.prototype.json = function () { var body = this._consume(); if (body === null) { return Promise.reject(new TypeError("body already used")); } try { return Promise.resolve(JSON.parse(body)); } catch (e) { return Promise.reject(e); } };
  Response.prototype.arrayBuffer = function () {
    var body = this._consume();
    if (body === null) { return Promise.reject(new TypeError("body already used")); }
    return Promise.resolve(new Uint8Array(this._bytes.slice()));
  };
  Response.prototype.clone = function () {
    return new Response({ status: this.status, statusText: this.statusText, url: this.url, headers: this.headers.entries(), bodyText: this._body, bodyBytes: this._bytes.slice() });
  };
  define("Response", Response);
  function Request(input, init) {
    init = init || {};
    if (input && typeof input === "object" && typeof input.url === "string") {
      this.url = input.url;
      this.method = String(init.method || input.method || "GET").toUpperCase();
      this.headers = new Headers(init.headers || input.headers);
      this.body = init.body !== undefined ? init.body : (input.body || null);
      if (this.body !== null && (this.method === "GET" || this.method === "HEAD")) { throw new TypeError("Request with GET/HEAD method cannot have body"); }
      this.signal = init.signal || input.signal || null;
    } else {
      this.url = new URL(String(input), location.href).href;
      this.method = String(init.method || "GET").toUpperCase();
      this.headers = new Headers(init.headers);
      this.body = init.body !== undefined && init.body !== null ? init.body : null;
      if (this.body !== null && (this.method === "GET" || this.method === "HEAD")) { throw new TypeError("Request with GET/HEAD method cannot have body"); }
      this.signal = init.signal || null;
    }
  }
  define("Request", Request);
  define("fetch", function (input, init) {
    return new Promise(function (resolve, reject) {
      var request;
      try { request = new Request(input, init); } catch (error) { reject(error); return; }
      var signal = request.signal;
      if (signal && signal.aborted) { reject(abortError(signal.reason)); return; }
      var settled = false;
      var onAbort = function () { if (!settled) { settled = true; reject(abortError(signal.reason)); } };
      if (signal) { signal.addEventListener("abort", onAbort); }
      try {
        var headerText = "";
        request.headers.forEach(function (value, name) { headerText += name + ":" + value + "\n"; });
        var raw = __megi_fetch_native(request.url, request.method, headerText, request.body === null ? "" : request.body);
        Promise.resolve().then(function () {
          if (settled) { return; }
          settled = true;
          if (signal) { signal.removeEventListener("abort", onAbort); }
          var response = new Response(raw);
          if (request.method === "HEAD") { response._bytes = []; response._body = ""; response.body = new ReadableStream([]); }
          resolve(response);
        });
      } catch (error) {
        settled = true;
        if (signal) { signal.removeEventListener("abort", onAbort); }
        reject(new TypeError("fetch failed: " + String(error)));
      }
    });
  });
})();
"####;
    let fetch_fn = FunctionObjectBuilder::new(context.realm(), native(fetch_native))
        .name(js_string!("__megi_fetch_native"))
        .length(4)
        .build();
    context
        .global_object()
        .set(
            js_string!("__megi_fetch_native"),
            JsValue::from(fetch_fn),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    context
        .eval(Source::from_bytes(PRELUDE))
        .map(|_| ())
        .map_err(|error| JsError::Internal(error.to_string()))
}

// ---------------------------------------------------------------------------
// Layout geometry provider (round 28)
// ---------------------------------------------------------------------------

fn node_layout_rect(
    this: &JsValue,
    context: &mut Context,
) -> JsResult<Option<crate::host::LayoutRect>> {
    let node_id = this_node(this, context)?;
    Ok(with_host(|host| {
        host.layout_provider
            .as_ref()
            .and_then(|provider| provider(node_id))
    })
    .flatten())
}

fn rect_from_border(rect: &crate::host::LayoutRect) -> (f64, f64, f64, f64) {
    let (x, y, width, height) = rect.border_box;
    (
        f64::from(x),
        f64::from(y),
        f64::from(width),
        f64::from(height),
    )
}

fn node_get_bounding_client_rect(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let rect = node_layout_rect(this, context)?;
    let (x, y, width, height) = rect
        .as_ref()
        .map(rect_from_border)
        .unwrap_or((0.0, 0.0, 0.0, 0.0));
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("x"), JsValue::from(x), Attribute::all())
        .property(js_string!("y"), JsValue::from(y), Attribute::all())
        .property(js_string!("width"), JsValue::from(width), Attribute::all())
        .property(
            js_string!("height"),
            JsValue::from(height),
            Attribute::all(),
        )
        .property(js_string!("top"), JsValue::from(y), Attribute::all())
        .property(js_string!("left"), JsValue::from(x), Attribute::all())
        .property(
            js_string!("right"),
            JsValue::from(x + width),
            Attribute::all(),
        )
        .property(
            js_string!("bottom"),
            JsValue::from(y + height),
            Attribute::all(),
        );
    Ok(JsValue::from(init.build()))
}

fn layout_metric(
    this: &JsValue,
    context: &mut Context,
    border: bool,
    width: bool,
) -> JsResult<JsValue> {
    let rect = node_layout_rect(this, context)?;
    let Some(rect) = rect else {
        return Ok(JsValue::from(0.0));
    };
    let (x, y, w, h) = if border {
        rect.border_box
    } else {
        rect.content_box
    };
    let value = if width { w } else { h };
    let _ = (x, y);
    Ok(JsValue::from((value.round() as i64) as f64))
}

fn node_offset_width(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    layout_metric(this, c, true, true)
}
fn node_offset_height(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    layout_metric(this, c, true, false)
}
fn node_client_width(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    layout_metric(this, c, false, true)
}
fn node_client_height(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    layout_metric(this, c, false, false)
}

fn viewport_metric(context: &mut Context, width: bool) -> JsResult<JsValue> {
    let _ = context;
    let value = with_host(|host| {
        host.viewport_provider.as_ref().map(|provider| {
            let info = provider();
            if width { info.width } else { info.height }
        })
    })
    .flatten()
    .unwrap_or(if width { 1280 } else { 720 });
    Ok(JsValue::from(f64::from(value)))
}

fn window_inner_width(_t: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    viewport_metric(c, true)
}
fn window_inner_height(_t: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    viewport_metric(c, false)
}
fn window_device_pixel_ratio(_t: &JsValue, _a: &[JsValue], _c: &mut Context) -> JsResult<JsValue> {
    let ratio = with_host(|host| {
        host.viewport_provider
            .as_ref()
            .map(|provider| provider().device_pixel_ratio)
    })
    .flatten()
    .unwrap_or(1.0);
    Ok(JsValue::from(ratio))
}

// ---------------------------------------------------------------------------
// Deterministic cooperative Web Workers (round 30; no threads)
// ---------------------------------------------------------------------------

/// One live worker realm plus its FIFO queues and main-side callbacks.
pub(crate) struct WorkerState {
    pub runtime: JsRuntime,
    pub terminated: bool,
    pub to_worker: VecDeque<String>,
    pub to_main: Rc<RefCell<VecDeque<String>>>,
    pub onmessage: Vec<u32>,
    pub onerror: Option<u32>,
    pub pending_error: Option<String>,
}

const MAX_WORKERS: usize = 8;
const MAX_WORKER_MESSAGES: usize = 256;

impl JsRuntime {
    /// Sets the maximum number of live DOM nodes a script may grow the document to (#657).
    ///
    /// Script-driven arena growth (`cloneNode(true)` in a loop, parsed HTML setters) is rejected
    /// with a `QuotaExceededError` once the document reaches the budget.
    pub fn set_max_dom_nodes(&mut self, max_nodes: usize) {
        self.host.borrow_mut().max_dom_nodes = max_nodes;
    }

    /// Installs the `fetch()` provider (the host owns all network/filesystem I/O).
    /// Without a provider `fetch()` rejects with a typed error (documented).
    pub fn set_fetch_provider(&mut self, provider: crate::host::FetchProvider) {
        self.host.borrow_mut().fetch_provider = Some(provider);
    }

    /// Installs the worker script loader (`url → source text`; the host owns all I/O).
    pub fn set_worker_loader(&mut self, loader: crate::host::WorkerLoader) {
        self.host.borrow_mut().worker_loader = Some(loader);
    }

    /// Creates an isolated worker realm without DOM globals (only timers/console/
    /// `postMessage`/`onmessage`/`addEventListener('message')`).
    fn new_worker(
        parent: Rc<RefCell<Host>>,
        id: u32,
        outbox: Rc<RefCell<VecDeque<String>>>,
    ) -> Result<Self, JsError> {
        let runtime = Self::new()?;
        {
            let mut host = runtime.host.borrow_mut();
            host.worker_parent = Some(parent);
            host.worker_id = Some(id);
            host.worker_outbox = Some(outbox);
        }
        Ok(runtime)
    }

    /// Drains the deterministic worker queues: main→worker messages are delivered first,
    /// then worker→main replies/errors invoke `onmessage`/`onerror`/listeners on the main
    /// runtime. Returns the number of messages delivered.
    pub fn run_worker_tasks(&mut self, max_steps: usize) -> Result<usize, JsError> {
        let mut delivered = 0usize;
        for _ in 0..max_steps {
            let mut progressed = false;
            let ids: Vec<usize> = self
                .host
                .borrow()
                .workers
                .iter()
                .enumerate()
                .filter_map(|(id, worker)| worker.as_ref().map(|_| id))
                .collect();
            for id in &ids {
                let mut state = self.host.borrow_mut().workers[*id].take();
                let Some(mut worker) = state.take() else {
                    continue;
                };
                if worker.terminated {
                    worker.to_worker.clear();
                    worker.to_main.borrow_mut().clear();
                    self.host.borrow_mut().workers[*id] = Some(worker);
                    continue;
                }
                while let Some(json) = worker.to_worker.pop_front() {
                    progressed = true;
                    delivered += 1;
                    if let Err(error) = worker.runtime.deliver_worker_message(&json) {
                        worker.pending_error = Some(error.to_string());
                    }
                }
                if worker.pending_error.is_some() {
                    progressed = true;
                }
                self.host.borrow_mut().workers[*id] = Some(worker);
            }
            for id in &ids {
                let reply = self.host.borrow().workers[*id].as_ref().and_then(|worker| {
                    if worker.terminated {
                        None
                    } else {
                        worker.to_main.borrow_mut().pop_front()
                    }
                });
                if let Some(json) = reply {
                    progressed = true;
                    delivered += 1;
                    self.deliver_worker_reply(*id as u32, &json)?;
                }
                let error = self.host.borrow_mut().workers[*id]
                    .as_mut()
                    .and_then(|worker| worker.pending_error.take());
                if let Some(message) = error {
                    progressed = true;
                    delivered += 1;
                    self.deliver_worker_error(*id as u32, &message)?;
                }
            }
            if !progressed {
                break;
            }
        }
        Ok(delivered)
    }

    fn deliver_worker_reply(&mut self, id: u32, json: &str) -> Result<(), JsError> {
        let onmessage = {
            let host = self.host.borrow();
            match host
                .workers
                .get(id as usize)
                .and_then(|worker| worker.as_ref())
            {
                Some(worker) => worker.onmessage.clone(),
                None => return Ok(()),
            }
        };
        let event = self.parse_json_value(json)?;
        let mut init = ObjectInitializer::new(&mut self.context);
        init.property(js_string!("data"), event, Attribute::all());
        let event = JsValue::from(init.build());
        let _scope = HostScope::enter(&self.host);
        for slot in onmessage {
            if let Ok(callback) = root_get(
                &mut self.context,
                "__megi_worker_callbacks",
                u64::from(slot),
            ) && let Some(callback) = callback.as_object()
            {
                let _ = callback.call(
                    &JsValue::undefined(),
                    std::slice::from_ref(&event),
                    &mut self.context,
                );
            }
        }
        Ok(())
    }

    fn deliver_worker_error(&mut self, id: u32, message: &str) -> Result<(), JsError> {
        let slot = {
            let host = self.host.borrow();
            host.workers
                .get(id as usize)
                .and_then(|worker| worker.as_ref())
                .and_then(|worker| worker.onerror)
        };
        let Some(slot) = slot else { return Ok(()) };
        let mut init = ObjectInitializer::new(&mut self.context);
        init.property(
            js_string!("message"),
            JsValue::from(js_string!(message.to_string())),
            Attribute::all(),
        )
        .property(
            js_string!("error"),
            JsValue::from(js_string!(message.to_string())),
            Attribute::all(),
        );
        let event = JsValue::from(init.build());
        let _scope = HostScope::enter(&self.host);
        if let Ok(callback) = root_get(
            &mut self.context,
            "__megi_worker_callbacks",
            u64::from(slot),
        ) && let Some(callback) = callback.as_object()
        {
            let _ = callback.call(
                &JsValue::undefined(),
                std::slice::from_ref(&event),
                &mut self.context,
            );
        }
        Ok(())
    }

    fn parse_json_value(&mut self, json: &str) -> Result<JsValue, JsError> {
        let script = format!("JSON.parse({})", module_quote(json));
        self.context
            .eval(Source::from_bytes(&script))
            .map_err(|error| JsError::Runtime(error.to_string()))
    }
}

impl WorkerState {
    fn new(runtime: JsRuntime, outbox: Rc<RefCell<VecDeque<String>>>) -> Self {
        Self {
            runtime,
            terminated: false,
            to_worker: VecDeque::new(),
            to_main: outbox,
            onmessage: Vec::new(),
            onerror: None,
            pending_error: None,
        }
    }
}

impl JsRuntime {
    fn deliver_worker_message(&mut self, json: &str) -> Result<(), JsError> {
        let data = self.parse_json_value(json)?;
        self.context
            .global_object()
            .set(
                js_string!("__megi_worker_data"),
                data,
                false,
                &mut self.context,
            )
            .map_err(|error| JsError::Runtime(error.to_string()))?;
        let script = r#"
(function () {
  var event = { data: __megi_worker_data };
  if (typeof onmessage === "function") { onmessage(event); }
  if (typeof __megi_worker_listeners !== "undefined") {
    for (var i = 0; i < __megi_worker_listeners.length; i++) { __megi_worker_listeners[i](event); }
  }
})();
"#;
        let _scope = HostScope::enter(&self.host);
        self.context
            .eval(Source::from_bytes(script))
            .map(|_| ())
            .map_err(|error| JsError::Runtime(error.to_string()))
    }
}

fn worker_handle(this: &JsValue, context: &mut Context) -> JsResult<u32> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("worker receiver"))?;
    object
        .get(js_string!("__megi_worker"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .filter(|value| *value >= 0.0)
        .map(|value| value as u32)
        .ok_or_else(|| js_error("invalid Worker"))
}

fn worker_constructor(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let url = arg_string(args, 0, context)?;
    let parent =
        crate::host::current_host().ok_or_else(|| js_error("no active JavaScript host"))?;
    let (resolved, source) = {
        let host = parent.borrow();
        let resolved = host.resolve_url(&url);
        let loader = host
            .worker_loader
            .as_ref()
            .ok_or_else(|| js_error("Worker scripts are not available in this host"))?;
        let source = loader(&resolved).map_err(|error| js_error(error.to_string()))?;
        (resolved, source)
    };
    let id = {
        let mut host = parent.borrow_mut();
        let existing = host.workers.iter().position(|worker| worker.is_none());
        let id = match existing {
            Some(id) => id,
            None if host.workers.len() < MAX_WORKERS => {
                host.workers.push(None);
                host.workers.len() - 1
            }
            None => return Err(js_error("too many Web Workers (limit 8)")),
        };
        id as u32
    };
    let outbox = Rc::new(RefCell::new(VecDeque::new()));
    let mut runtime = JsRuntime::new_worker(parent.clone(), id, outbox.clone())
        .map_err(|error| js_error(error.to_string()))?;
    runtime
        .install_worker_globals()
        .map_err(|error| js_error(error.to_string()))?;
    let error = runtime.eval(&source, &resolved).err();
    {
        let mut host = parent.borrow_mut();
        host.workers[id as usize] = Some(WorkerState::new(runtime, outbox));
        if let Some(error) = error
            && let Some(worker) = host.workers[id as usize].as_mut()
        {
            worker.pending_error = Some(error.to_string());
        }
    }
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_worker"),
        JsValue::from(f64::from(id)),
        Attribute::all(),
    );
    init.function(native(worker_post_message), js_string!("postMessage"), 1);
    init.function(native(worker_terminate), js_string!("terminate"), 0);
    init.function(
        native(worker_add_listener),
        js_string!("addEventListener"),
        2,
    );
    let worker_object = init.build();
    define_accessor(
        context,
        &worker_object,
        "onmessage",
        worker_onmessage_get,
        Some(worker_onmessage_set),
    )
    .map_err(|error| js_error(error.to_string()))?;
    define_accessor(
        context,
        &worker_object,
        "onerror",
        worker_onerror_get,
        Some(worker_onerror_set),
    )
    .map_err(|error| js_error(error.to_string()))?;
    Ok(JsValue::from(worker_object))
}

fn worker_post_message(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    let value = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let json = json_state(&value, context);
    let queued = with_host(|host| {
        let Some(Some(worker)) = host.workers.get_mut(id as usize) else {
            return false;
        };
        if worker.terminated || worker.to_worker.len() >= MAX_WORKER_MESSAGES {
            return false;
        }
        worker.to_worker.push_back(json);
        true
    })
    .unwrap_or(false);
    if !queued {
        return Err(js_error("worker is terminated or its queue is full"));
    }
    Ok(JsValue::undefined())
}

fn worker_terminate(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    with_host(|host| {
        if let Some(Some(worker)) = host.workers.get_mut(id as usize) {
            worker.terminated = true;
            worker.to_worker.clear();
            worker.to_main.borrow_mut().clear();
        }
    });
    Ok(JsValue::undefined())
}

fn worker_add_listener(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    let event_type = arg_string(args, 0, context)?;
    if event_type != "message" {
        return Ok(JsValue::undefined());
    }
    let callback = args.get(1).cloned().unwrap_or_else(JsValue::undefined);
    let slot = root_push(context, "__megi_worker_callbacks", callback)?;
    with_host(|host| {
        if let Some(Some(worker)) = host.workers.get_mut(id as usize) {
            worker.onmessage.push(slot);
        }
    });
    Ok(JsValue::undefined())
}

fn worker_onmessage_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    let slot = with_host(|host| {
        host.workers
            .get(id as usize)
            .and_then(|worker| worker.as_ref())
            .and_then(|worker| worker.onmessage.first().copied())
    })
    .flatten();
    match slot {
        Some(slot) => root_get(context, "__megi_worker_callbacks", u64::from(slot)),
        None => Ok(JsValue::null()),
    }
}

fn worker_onmessage_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    let callback = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let slot = root_push(context, "__megi_worker_callbacks", callback)?;
    with_host(|host| {
        if let Some(Some(worker)) = host.workers.get_mut(id as usize) {
            worker.onmessage.clear();
            worker.onmessage.push(slot);
        }
    });
    Ok(JsValue::undefined())
}

fn worker_onerror_get(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::null())
}

fn worker_onerror_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    let callback = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let slot = root_push(context, "__megi_worker_callbacks", callback)?;
    with_host(|host| {
        if let Some(Some(worker)) = host.workers.get_mut(id as usize) {
            worker.onerror = Some(slot);
        }
    });
    Ok(JsValue::undefined())
}

impl JsRuntime {
    fn install_worker_globals(&mut self) -> Result<(), JsError> {
        let global = self.context.global_object();
        let worker_post =
            FunctionObjectBuilder::new(self.context.realm(), native(worker_self_post_message))
                .name(js_string!("postMessage"))
                .length(1)
                .build();
        global
            .set(
                js_string!("postMessage"),
                JsValue::from(worker_post),
                false,
                &mut self.context,
            )
            .map_err(|error| JsError::Internal(error.to_string()))?;
        global
            .set(
                js_string!("self"),
                JsValue::from(global.clone()),
                false,
                &mut self.context,
            )
            .map_err(|error| JsError::Internal(error.to_string()))?;
        global
            .set(
                js_string!("document"),
                JsValue::undefined(),
                false,
                &mut self.context,
            )
            .map_err(|error| JsError::Internal(error.to_string()))?;
        global
            .set(
                js_string!("window"),
                JsValue::undefined(),
                false,
                &mut self.context,
            )
            .map_err(|error| JsError::Internal(error.to_string()))?;
        let prelude = r#"
var __megi_worker_listeners = [];
function addEventListener(type, callback) { if (type === "message" && typeof callback === "function") { __megi_worker_listeners.push(callback); } }
function removeEventListener() {}
"#;
        self.context
            .eval(Source::from_bytes(prelude))
            .map(|_| ())
            .map_err(|error| JsError::Internal(error.to_string()))
    }
}

fn worker_self_post_message(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let value = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let json = json_state(&value, context);
    let sent = with_host(|host| {
        let Some(outbox) = &host.worker_outbox else {
            return false;
        };
        let mut outbox = outbox.borrow_mut();
        if outbox.len() >= MAX_WORKER_MESSAGES {
            return false;
        }
        outbox.push_back(json);
        true
    })
    .unwrap_or(false);
    if !sent {
        return Err(js_error("worker message queue is full"));
    }
    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// fetch() subset (round 31): host provider, Headers/Request/Response prelude
// ---------------------------------------------------------------------------

const MAX_FETCH_HEADERS: usize = 64;

/// Per-entry cap for `history.pushState`/`replaceState` state, matching the Web Storage quota.
/// The JSON snapshot is kept in Rust (`host.history_state`), rooted as a JS value and copied to
/// the host provider, so an unbounded state is unbounded retention (#663).
const MAX_HISTORY_STATE_BYTES: usize = 1024 * 1024;
const MAX_FETCH_BODY: usize = 8 * 1024 * 1024;

/// Request headers the browser owns; page scripts must not set them through `fetch()`.
const FORBIDDEN_FETCH_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "cookie",
    "origin",
    "referer",
    "user-agent",
    "accept-encoding",
    "expect",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "upgrade",
];

/// RFC 9110 §5.1 header-name token grammar.
fn is_valid_fetch_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

/// RFC 9110 §5.5 field-value: printable bytes and HTAB only; no CR/LF/NUL/CTL/DEL.
fn is_valid_fetch_header_value(value: &str) -> bool {
    !value
        .bytes()
        .any(|byte| byte == 0x7f || (byte < 0x20 && byte != b'\t'))
}

fn fetch_native(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let url = arg_string(args, 0, context)?;
    let method = arg_string(args, 1, context)?.trim().to_ascii_uppercase();
    let headers_raw = arg_string(args, 2, context)?;
    let body_text = arg_string(args, 3, context)?;
    let mut headers = Vec::new();
    for line in headers_raw.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if !is_valid_fetch_header_name(name) {
            return Err(js_error("fetch: invalid request header name"));
        }
        if FORBIDDEN_FETCH_HEADERS
            .iter()
            .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
        {
            return Err(js_error(format!(
                "fetch: forbidden request header `{name}`"
            )));
        }
        if !is_valid_fetch_header_value(value) {
            return Err(js_error(format!(
                "fetch: invalid value for request header `{name}`"
            )));
        }
        if headers.len() >= MAX_FETCH_HEADERS {
            return Err(js_error("fetch headers limit exceeded"));
        }
        headers.push((name.to_string(), value.to_string()));
    }
    if body_text.len() > MAX_FETCH_BODY {
        return Err(js_error("fetch body limit exceeded"));
    }
    if !matches!(
        method.as_str(),
        "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS"
    ) {
        return Err(js_error(
            "fetch supports GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS",
        ));
    }
    let request = crate::host::FetchRequest {
        url,
        method: method.clone(),
        headers,
        body: (!body_text.is_empty() && method != "GET" && method != "HEAD")
            .then(|| body_text.into_bytes()),
    };
    let result = with_host(|host| {
        host.fetch_provider
            .as_ref()
            .ok_or_else(|| js_error("fetch is not available in this host"))
            .and_then(|provider| provider(request).map_err(|error| js_error(error.to_string())))
    })
    .ok_or_else(|| js_error("no active JavaScript host"))??;
    if result.body.len() > MAX_FETCH_BODY {
        return Err(js_error("fetch response body limit exceeded"));
    }
    let header_array = JsArray::from_iter([], context);
    for (index, (name, value)) in result.headers.iter().enumerate() {
        let pair = JsArray::from_iter([], context);
        pair.set(
            js_string!("0"),
            JsValue::from(js_string!(name.clone())),
            false,
            context,
        )
        .map_err(|error| js_error(error.to_string()))?;
        pair.set(
            js_string!("1"),
            JsValue::from(js_string!(value.clone())),
            false,
            context,
        )
        .map_err(|error| js_error(error.to_string()))?;
        header_array
            .set(
                js_string!(index.to_string()),
                JsValue::from(pair),
                false,
                context,
            )
            .map_err(|error| js_error(error.to_string()))?;
    }
    let body_text = String::from_utf8_lossy(&result.body).to_string();
    let body_bytes = JsArray::from_iter([], context);
    for (index, byte) in result.body.iter().enumerate() {
        body_bytes
            .set(
                js_string!(index.to_string()),
                JsValue::from(f64::from(*byte)),
                false,
                context,
            )
            .map_err(|error| js_error(error.to_string()))?;
    }
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("status"),
        JsValue::from(f64::from(result.status)),
        Attribute::all(),
    )
    .property(
        js_string!("statusText"),
        JsValue::from(js_string!(result.status_text)),
        Attribute::all(),
    )
    .property(
        js_string!("url"),
        JsValue::from(js_string!(result.url)),
        Attribute::all(),
    )
    .property(
        js_string!("headers"),
        JsValue::from(header_array),
        Attribute::all(),
    )
    .property(
        js_string!("bodyText"),
        JsValue::from(js_string!(body_text)),
        Attribute::all(),
    )
    .property(
        js_string!("bodyLength"),
        JsValue::from(result.body.len() as f64),
        Attribute::all(),
    )
    .property(
        js_string!("bodyBytes"),
        JsValue::from(body_bytes),
        Attribute::all(),
    );

    Ok(JsValue::from(init.build()))
}

// ---------------------------------------------------------------------------
// Native Boa modules for real top-level await + live bindings (round 32)
// ---------------------------------------------------------------------------

struct MegiModuleLoader {
    host: Rc<RefCell<Option<Rc<RefCell<Host>>>>>,
    base_url: String,
    cache: RefCell<HashMap<(String, String), Module>>,
}

impl ModuleLoader for MegiModuleLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        request: boa_engine::module::ModuleRequest,
        context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let specifier = request.specifier().to_std_string_escaped();
        let base = referrer
            .path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| self.base_url.clone());
        if let Some(module) = self
            .cache
            .borrow()
            .get(&(base.clone(), specifier.clone()))
            .cloned()
        {
            return Ok(module);
        }
        let host = self
            .host
            .borrow()
            .clone()
            .ok_or_else(|| js_error("no active JavaScript host"))?;
        // Consult the import map first (like resolve_module does).
        let import_map_result = {
            let host = host.borrow();
            host.import_map
                .as_ref()
                .and_then(|map| map.resolve(&specifier))
        };
        if let Some(result) = import_map_result {
            match result {
                Some(target) => {
                    // Load from the import-map target.
                    let source = {
                        let host = host.borrow();
                        match host.module_loader.as_ref() {
                            Some(loader) => {
                                loader(&target).map_err(|error| js_error(error.to_string()))?
                            }
                            None => return Err(js_error(format!("module not found: {target}"))),
                        }
                    };
                    let mut context = context.borrow_mut();
                    let module = Module::parse(
                        Source::from_bytes(&source).with_path(Path::new(&target)),
                        None,
                        &mut context,
                    )?;
                    self.cache
                        .borrow_mut()
                        .insert((base, specifier), module.clone());
                    return Ok(module);
                }
                None => {
                    return Err(js_error(format!(
                        "import map blocked: {specifier} (base {base})"
                    )));
                }
            }
        }
        let resolved = {
            let host = host.borrow();
            match host.module_resolver.as_ref() {
                Some(resolver) => {
                    resolver(&specifier, &base).map_err(|error| js_error(error.to_string()))?
                }
                None => None,
            }
        };
        let url = resolved
            .ok_or_else(|| js_error(format!("module not found: {specifier} (base {base})")))?;
        let source = {
            let host = host.borrow();
            match host.module_loader.as_ref() {
                Some(loader) => loader(&url).map_err(|error| js_error(error.to_string()))?,
                None => return Err(js_error(format!("module not found: {url}"))),
            }
        };
        let mut context = context.borrow_mut();
        let module = Module::parse(
            Source::from_bytes(&source).with_path(Path::new(&url)),
            None,
            &mut context,
        )?;
        self.cache
            .borrow_mut()
            .insert((base, specifier), module.clone());
        Ok(module)
    }

    fn init_import_meta(
        self: Rc<Self>,
        import_meta: &JsObject,
        module: &Module,
        context: &mut Context,
    ) {
        if let Some(path) = module.path() {
            let _ = import_meta.set(
                js_string!("url"),
                JsValue::from(js_string!(path.to_string_lossy().to_string())),
                false,
                context,
            );
        }
    }
}

impl JsRuntime {
    /// Evaluates an ES module with the **native Boa module system**, giving real
    /// top-level `await` (continuations driven by host microtasks/timers) and real
    /// **live bindings** (imports observe later `let`/`var` mutations; `const` stays
    /// immutable). Dependency order follows the module graph; errors reject with typed
    /// errors. Import maps are not consulted here (use [`Self::eval_module`] for the
    /// preprocessed subset).
    pub fn eval_module_async(
        &mut self,
        source: &str,
        source_name: &str,
        base_url: &str,
        budget: &Budget,
    ) -> Result<ModuleOutcome, JsError> {
        self.eval_module_async_with_budget(
            source,
            source_name,
            base_url,
            budget,
            ModuleAsyncBudget::default(),
        )
        .map(|evaluated| evaluated.outcome)
    }

    /// [`Self::eval_module_async`] with an explicit timer/time budget.
    ///
    /// The deterministic loop that drives top-level `await` pumps timers; the render event
    /// loop passes its remaining `max_tasks`/virtual-time budget here so module evaluation
    /// cannot overshoot the page budget.
    pub fn eval_module_async_with_budget(
        &mut self,
        source: &str,
        _source_name: &str,
        base_url: &str,
        budget: &Budget,
        timer_budget: ModuleAsyncBudget,
    ) -> Result<ModuleAsyncOutcome, JsError> {
        apply_budget(&mut self.context, budget);
        let module = Module::parse(
            Source::from_bytes(source).with_path(Path::new(base_url)),
            None,
            &mut self.context,
        )
        .map_err(module_error)?;
        let promise = module.load_link_evaluate(&mut self.context);
        let _scope = HostScope::enter(&self.host);
        let mut timer_tasks = 0usize;
        let mut virtual_time_ms = 0u64;
        let mut limited = false;
        for _ in 0..10_000 {
            if self.context.run_jobs().is_err() {
                break;
            }
            if !matches!(promise.state(), PromiseState::Pending) {
                break;
            }
            if timer_tasks >= timer_budget.timer_tasks
                || virtual_time_ms >= timer_budget.virtual_time_ms
            {
                limited = true;
                break;
            }
            let pump = (timer_budget.timer_tasks - timer_tasks).min(64);
            if let Ok((_, pumped)) = crate::events::advance_clock_pumping(
                &mut self.context,
                std::time::Duration::from_millis(1),
                pump,
            ) {
                timer_tasks += pumped;
            }
            virtual_time_ms += 1;
            let now = crate::events::now_ms();
            let remaining = timer_budget.timer_tasks - timer_tasks;
            if let Ok(pumped) =
                crate::events::run_due_timers(&mut self.context, now, remaining.min(64))
            {
                timer_tasks += pumped;
            }
        }
        drop(_scope);
        match promise.state() {
            PromiseState::Fulfilled(_) => {}
            PromiseState::Rejected(error) => {
                return Err(JsError::Runtime(
                    error.to_string(&mut self.context).map_or_else(
                        |_| "module evaluation rejected".to_string(),
                        |value| value.to_std_string_escaped(),
                    ),
                ));
            }
            PromiseState::Pending => {
                return Err(JsError::Runtime(
                    "top-level await did not settle within the deterministic budget".to_string(),
                ));
            }
        }
        let namespace = module.namespace(&mut self.context);
        let mut values = Vec::new();
        for key in namespace
            .own_property_keys(&mut self.context)
            .map_err(module_error)?
        {
            let key_text = key.to_string();
            if let Ok(value) = namespace.get(js_string!(key_text.clone()), &mut self.context) {
                let text = value
                    .to_string(&mut self.context)
                    .map(|text| text.to_std_string_escaped())
                    .unwrap_or_else(|_| "<unprintable>".to_string());
                values.push((key_text, text));
            }
        }
        values.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(ModuleAsyncOutcome {
            outcome: ModuleOutcome {
                url: base_url.to_string(),
                exports: values,
            },
            timer_tasks,
            virtual_time_ms,
            limited,
        })
    }
}

fn node_node_type(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let kind = with_host(|host| {
        host.document
            .try_node(node_id)
            .map(|node| match &node.kind {
                dom::NodeKind::Element(_) => 1.0,
                dom::NodeKind::Text(_) => 3.0,
                dom::NodeKind::Comment(_) => 8.0,
                dom::NodeKind::Document => 9.0,
                dom::NodeKind::DocumentFragment => 11.0,
                dom::NodeKind::Doctype { .. } => 10.0,
                dom::NodeKind::ProcessingInstruction { .. } => 7.0,
            })
    })
    .flatten()
    .unwrap_or(0.0);
    Ok(JsValue::from(kind))
}

fn string_attribute_get(this: &JsValue, name: &str, context: &mut Context) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(name).map(str::to_owned))
    })
    .flatten();
    Ok(value.map_or_else(JsValue::null, |value| JsValue::from(js_string!(value))))
}

fn string_attribute_set(
    this: &JsValue,
    name: &str,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    with_host(|host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(dom::Attribute::new(name.to_string(), value), true);
        }
    });
    Ok(JsValue::undefined())
}

fn node_title_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "title", c)
}
fn node_title_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "title", a, c)
}
fn node_lang_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "lang", c)
}
fn node_lang_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "lang", a, c)
}
fn node_href_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "href", c)
}
fn node_href_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "href", a, c)
}
fn node_src_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "src", c)
}
fn node_src_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "src", a, c)
}

fn option_ids(document: &dom::Document, select: NodeId) -> Vec<NodeId> {
    document
        .descendants(select)
        .filter(|id| {
            document
                .try_node(*id)
                .and_then(|node| node.as_element())
                .is_some_and(|element| element.local_name == "option")
        })
        .collect()
}

fn node_selected_index_get(
    this: &JsValue,
    _a: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let index = with_host(|host| {
        option_ids(&host.document, node_id)
            .iter()
            .position(|id| {
                host.document
                    .try_node(*id)
                    .and_then(|node| node.as_element())
                    .is_some_and(|element| element.get_attribute("selected").is_some())
            })
            .map(|value| value as f64)
            .unwrap_or(-1.0)
    })
    .unwrap_or(-1.0);
    Ok(JsValue::from(index))
}

fn node_selected_index_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let index = args
        .first()
        .and_then(|value| value.to_number(context).ok())
        .unwrap_or(-1.0) as i64;
    with_host(|host| {
        let options = option_ids(&host.document, node_id);
        for (position, option) in options.iter().enumerate() {
            if let Some(element) = host
                .document
                .try_node_mut(*option)
                .and_then(|node| node.as_element_mut())
            {
                if position as i64 == index {
                    element.set_attribute(dom::Attribute::new("selected", String::new()), true);
                } else {
                    element
                        .attributes
                        .retain(|attribute| attribute.local_name != "selected");
                }
            }
        }
    });
    Ok(JsValue::undefined())
}

fn class_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (_, classes) = classes_of(this, context)?;
    let index = args
        .first()
        .and_then(|value| value.to_number(context).ok())
        .unwrap_or(0.0) as usize;
    Ok(classes.get(index).map_or_else(JsValue::null, |value| {
        JsValue::from(js_string!(value.clone()))
    }))
}

fn node_get_attribute_node(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let value = with_host(|host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(&name).map(str::to_owned))
    })
    .flatten();
    match value {
        Some(value) => attribute_item(context, &name, &value),
        None => Ok(JsValue::null()),
    }
}

fn node_remove_attribute_node(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let item = args
        .first()
        .and_then(|value| value.as_object())
        .ok_or_else(|| js_error("removeAttributeNode expects an Attr"))?;
    let name = item
        .get(js_string!("name"), context)
        .map_err(|error| js_error(error.to_string()))?
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
        .unwrap_or_default();
    let old = with_host(|host| {
        host.document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .and_then(|element| element.remove_attribute(&name))
    })
    .flatten();
    match old {
        Some(value) => attribute_item(context, &name, &value),
        None => Ok(JsValue::null()),
    }
}

#[cfg(test)]
mod document_write_budget_tests {
    use super::*;

    #[test]
    fn document_write_stops_at_the_cumulative_budget() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime.host.borrow_mut().max_document_write_bytes = 4 * 1024;
        runtime
            .eval(
                "var s = 'A'.repeat(1024);
                 for (var i = 0; i < 6; i++) { document.write(s); }",
                "write_budget.js",
            )
            .expect("document.write loop");
        let console = runtime.take_console();
        let document = runtime.take_document();
        let body = document.body().expect("body");
        assert_eq!(
            document.text_content(body).len(),
            4 * 1024,
            "only the writes that fit the budget are parsed into the document"
        );
        assert!(
            console.iter().any(|entry| {
                entry.level == "warn" && entry.message.contains("document.write ignored")
            }),
            "the exceeded budget must be reported: {console:?}"
        );
    }
}

#[cfg(test)]
mod document_write_node_budget_tests {
    use super::*;

    #[test]
    fn document_write_node_budget_is_enforced_and_warned_once() {
        let mut runtime = JsRuntime::new().expect("runtime");
        // The parser must use the host's node cap, not the 1M default: 100 elements with a
        // 64-node budget leave ~62 nodes.
        runtime.host.borrow_mut().max_document_write_nodes = 64;
        runtime
            .eval("document.write('<i></i>'.repeat(100));", "nodes.js")
            .expect("first write");
        let after_first = runtime.document().len();
        assert!(
            after_first <= 64,
            "the write parser stops at the host node cap: {after_first} nodes"
        );
        // Host-side rejection: a cap below the current size rejects every write; the
        // warning must appear exactly once.
        runtime.host.borrow_mut().max_document_write_nodes = 1;
        runtime
            .eval(
                "for (var j = 0; j < 5; j++) { document.write('<b></b>'); }",
                "nodes.js",
            )
            .expect("rejected writes");
        let console = runtime.take_console();
        let warnings = console
            .iter()
            .filter(|entry| entry.message.contains("document.write ignored"))
            .count();
        assert_eq!(warnings, 1, "the rejection is reported once: {console:?}");
        let document = runtime.take_document();
        assert_eq!(
            document.len(),
            after_first,
            "rejected writes add nothing to the document"
        );
    }
}

#[cfg(test)]
mod markup_parse_budget_tests {
    use super::*;

    #[test]
    fn innerhtml_is_charged_against_the_cumulative_budget() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime.host.borrow_mut().max_document_write_bytes = 64;
        runtime
            .eval("document.body.innerHTML = '<b>first</b>';", "inner.js")
            .expect("first assignment");
        runtime
            .eval(
                "document.body.innerHTML = '<b>' + 'x'.repeat(200) + '</b>';",
                "inner.js",
            )
            .expect("over-budget assignment");
        let text = {
            let document = runtime.document();
            let body = document.body().expect("body");
            document.text_content(body)
        };
        assert_eq!(text, "first", "the over-budget assignment is ignored");
        let console = runtime.take_console();
        let warnings = console
            .iter()
            .filter(|entry| entry.message.contains("innerHTML assignment ignored"))
            .count();
        assert_eq!(warnings, 1, "reported once: {console:?}");
    }
}

#[cfg(test)]
mod mutation_observer_delivery_tests {
    use super::*;

    #[test]
    fn mutation_observer_records_markup_insert_normalize_and_does_not_starve_siblings() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime
            .eval(
                r#"
                var box = document.createElement('div');
                document.body.appendChild(box);
                var seenA = 0;
                var seenB = 0;
                var obsA = new MutationObserver(function(records) { seenA += records.length; });
                var obsB = new MutationObserver(function(records) { seenB += records.length; });
                obsA.observe(document.body, { childList: true, subtree: true });
                obsB.observe(document.body, { childList: true, subtree: true });

                box.innerHTML = '<span>hi</span>';
                box.firstChild.outerHTML = '<b>there</b>';
                box.insertAdjacentHTML('beforeend', '<i>!</i>');
                box.insertAdjacentText('afterbegin', 'pre');
                box.appendChild(document.createTextNode('a'));
                box.appendChild(document.createTextNode('b'));
                box.normalize();
                document.write('<p>written</p>');

                var takenA = obsA.takeRecords();
                var takenB = obsB.takeRecords();
                console.log('counts:' + takenA.length + ':' + takenB.length + ':' + seenA + ':' + seenB);
                "#,
                "observer.js",
            )
            .expect("eval observer script");
        let console = runtime.take_console();
        let line = console
            .iter()
            .find(|entry| entry.message.starts_with("counts:"))
            .expect("counts log");
        // All 7 mutations (innerHTML, outerHTML, insertAdjacentHTML, insertAdjacentText,
        // 2x appendChild, normalize, document.write = 8 mutations) must be seen by BOTH observers.
        assert_eq!(
            line.message, "counts:8:8:0:0",
            "both sibling observers must receive all 8 records without starvation: {console:?}"
        );
    }

    #[test]
    fn document_normalize_emits_child_list_record_when_nodes_are_merged() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime
            .eval(
                r#"
                var count = 0;
                var obs = new MutationObserver(function(records) { count += records.length; });
                obs.observe(document.documentElement, { childList: true, subtree: true });
                document.body.appendChild(document.createTextNode('x'));
                document.body.appendChild(document.createTextNode('y'));
                obs.takeRecords();
                document.normalize();
                var after = obs.takeRecords().length;
                console.log('norm:' + after + ':' + count);
                "#,
                "norm.js",
            )
            .expect("eval normalize script");
        let console = runtime.take_console();
        let line = console
            .iter()
            .find(|entry| entry.message.starts_with("norm:"))
            .expect("norm log");
        assert_eq!(line.message, "norm:1:0");
    }
}
