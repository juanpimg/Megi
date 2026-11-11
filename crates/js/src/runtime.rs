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
//! [`JsError::Budget`]. There is no memory accounting: the DOM parser and the handle
//! side-table are capped separately ([`crate::host`]) and scripts cannot allocate network
//! or filesystem resources (no such APIs are exposed).
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
    Source, js_string,
    object::FunctionObjectBuilder,
    object::ObjectInitializer,
    object::builtins::JsArray,
    property::{Attribute, PropertyDescriptor},
};
use dom::{Attribute as DomAttribute, Document, Namespace, NodeId};
use std::cell::RefCell;
use std::collections::HashMap;
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
        let mut context = Context::default();
        let host = Rc::new(RefCell::new(Host::new(document, url)));
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
        if result.is_ok() {
            let _scope = HostScope::enter(&self.host);
            let delivered = deliver_mutation_records(&mut self.context);
            drop(_scope);
            delivered?;
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
        let _scope = HostScope::enter(&self.host);
        let result = crate::events::run_until(&mut self.context, deadline_ms);
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
        fn count_nodes(document: &Document, node: NodeId) -> usize {
            1 + document
                .children(node)
                .map(|child| count_nodes(document, child))
                .sum::<usize>()
        }
        let node_count = count_nodes(&host.document, host.document.root());
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
    Ok(())
}

/// Installs History/location, Web Storage and MutationObserver (round 24).
fn install_browser_features(context: &mut Context) -> Result<(), JsError> {
    for name in [
        "__megi_history_state",
        "__megi_observer_callbacks",
        "__megi_observer_objects",
    ] {
        let array = JsArray::new(context);
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
        host.console.push(ConsoleEntry {
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
    let _ = with_host(|host| host.console.push(ConsoleEntry { level, message }));
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
    let accessors: [(&str, AccessorKind); 12] = [
        ("tagName", AccessorKind::TagName),
        ("id", AccessorKind::Id),
        ("className", AccessorKind::ClassName),
        ("textContent", AccessorKind::TextContent),
        ("innerHTML", AccessorKind::InnerHtml),
        ("outerHTML", AccessorKind::OuterHtml),
        ("children", AccessorKind::Children),
        ("parentNode", AccessorKind::ParentNode),
        ("firstChild", AccessorKind::FirstChild),
        ("nextSibling", AccessorKind::NextSibling),
        ("style", AccessorKind::Style),
        ("classList", AccessorKind::ClassList),
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
    init.property(
        js_string!("__megi_node"),
        f64::from(handle),
        Attribute::all(),
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
    TagName,
    Id,
    ClassName,
    TextContent,
    InnerHtml,
    OuterHtml,
    Children,
    ParentNode,
    FirstChild,
    NextSibling,
    Style,
    ClassList,
}

fn accessor_functions(kind: AccessorKind) -> AccessorPair {
    match kind {
        AccessorKind::TagName => (node_tag_name, None),
        AccessorKind::Id => (node_id_get, Some(node_id_set)),
        AccessorKind::ClassName => (node_class_get, Some(node_class_set)),
        AccessorKind::TextContent => (node_text_content_get, Some(node_text_content_set)),
        AccessorKind::InnerHtml => (node_inner_html_get, Some(node_inner_html_set)),
        AccessorKind::OuterHtml => (node_outer_html_get, Some(node_outer_html_set)),
        AccessorKind::Children => (node_children, None),
        AccessorKind::ParentNode => (node_parent_node, None),
        AccessorKind::FirstChild => (node_first_child, None),
        AccessorKind::NextSibling => (node_next_sibling, None),
        AccessorKind::Style => (node_style_get, Some(node_style_set)),
        AccessorKind::ClassList => (node_class_list_get, None),
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
    let array = boa_engine::object::builtins::JsArray::new(context);
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
    let result = with_host(|host| html::set_inner_html(&mut host.document, node_id, &html_text));
    match result {
        Some(Ok(_)) => Ok(JsValue::undefined()),
        Some(Err(error)) => Err(js_error(error.to_string())),
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
    let result = with_host(|host| html::set_outer_html(&mut host.document, node_id, &html_text));
    match result {
        Some(Ok(_)) => Ok(JsValue::undefined()),
        Some(Err(error)) => Err(js_error(error.to_string())),
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

fn node_set_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = arg_string(args, 0, context)?;
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
    let name = arg_string(args, 0, context)?;
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
    let name = arg_string(args, 0, context)?;
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
    let name = arg_string(args, 0, context)?;
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

fn node_append_child(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let child = argument_node(args, 0, context)?;
    let appended = with_host(|host| host.document.append_child(parent, child)).unwrap_or(false);
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
    let reference = argument_node(args, 1, context)?;
    let inserted = with_host(|host| {
        host.document
            .insert_before_child(parent, new_node, Some(reference))
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
        if host.document.remove_child(parent, old_node).is_err() {
            return false;
        }
        if host
            .document
            .insert_before_child(parent, new_node, None)
            .is_err()
        {
            host.document.append_child(parent, new_node);
        }
        true
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

fn document_write(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let text = arg_string(args, 0, context)?;
    with_host(|host| {
        let _ = html::document_write(&mut host.document, &text);
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

fn document_create_element(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let name = arg_string(args, 0, context)?;
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

fn insert_relative(host: &mut crate::host::Host, target: NodeId, node: NodeId, position: &str) {
    match position {
        "beforebegin" => {
            let parent = host.document.try_node(target).and_then(|node| node.parent);
            if let Some(parent) = parent {
                let _ = host
                    .document
                    .insert_before_child(parent, node, Some(target));
            }
        }
        "afterbegin" => {
            let first = host
                .document
                .try_node(target)
                .and_then(|node| node.first_child);
            let _ = host.document.insert_before_child(target, node, first);
        }
        "beforeend" => {
            host.document.append_child(target, node);
        }
        "afterend" => {
            let parent = host.document.try_node(target).and_then(|node| node.parent);
            if let Some(parent) = parent {
                let next = host
                    .document
                    .try_node(target)
                    .and_then(|node| node.next_sibling);
                let _ = host.document.insert_before_child(parent, node, next);
            }
        }
        _ => {}
    }
}

fn node_insert_adjacent_html(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = this_node(this, context)?;
    let position = arg_string(args, 0, context)?.to_ascii_lowercase();
    let html_text = arg_string(args, 1, context)?;
    with_host(|host| {
        let temp = host
            .document
            .create_element(Namespace::Html, "div", Vec::new());
        if html::set_inner_html(&mut host.document, temp, &html_text).is_err() {
            return;
        }
        let children: Vec<NodeId> = host.document.children(temp).collect();
        for child in children {
            insert_relative(host, target, child, &position);
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
    let position = arg_string(args, 0, context)?.to_ascii_lowercase();
    let text = arg_string(args, 1, context)?;
    with_host(|host| {
        let node = host.document.create_text_node(&text);
        insert_relative(host, target, node, &position);
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
    let class = arg_string(args, 0, context)?;
    if !classes.contains(&class) {
        classes.push(class);
        write_classes(node_id, &classes);
    }
    Ok(JsValue::undefined())
}

fn class_remove(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (node_id, mut classes) = classes_of(this, context)?;
    let class = arg_string(args, 0, context)?;
    classes.retain(|existing| *existing != class);
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
    if contains {
        classes.retain(|existing| *existing != class);
    } else {
        classes.push(class);
    }
    write_classes(node_id, &classes);
    Ok(JsValue::from(!contains))
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
    if let Some(array) = value.as_object() {
        return Ok(array.clone());
    }
    let array = JsArray::new(context);
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

fn record_matches(host: &Host, observer: &ObserverRegistration, record: &MutationRecord) -> bool {
    observer.targets.iter().any(|target| {
        let kind_enabled = match record.kind {
            MutationKind::ChildList => target.child_list,
            MutationKind::Attributes => target.attributes,
            MutationKind::CharacterData => target.character_data,
        };
        if !kind_enabled {
            return false;
        }
        if target.node == record.target {
            return true;
        }
        if !target.subtree {
            return false;
        }
        let mut current = host.document.node(record.target).parent;
        while let Some(node) = current {
            if node == target.node {
                return true;
            }
            current = host.document.node(node).parent;
        }
        false
    })
}

fn collect_observer_records(context: &mut Context, slot: u32, take: bool) -> JsResult<JsArray> {
    let (registration, records) = with_host(|host| {
        let registration = host
            .observers
            .iter()
            .find(|r| r.object_slot == slot)
            .cloned();
        let records = match &registration {
            Some(registration) => host
                .mutations
                .iter()
                .filter(|record| record_matches(host, registration, record))
                .cloned()
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };
        if take {
            host.mutations.retain(|record| {
                !records.iter().any(|taken| {
                    taken.kind == record.kind
                        && taken.target == record.target
                        && taken.name == record.name
                        && taken.old_value == record.old_value
                })
            });
        }
        (registration, records)
    })
    .unwrap_or((None, Vec::new()));
    let _ = registration;
    let array = JsArray::new(context);
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
    let registrations = with_host(|host| host.observers.clone()).unwrap_or_default();
    for registration in registrations {
        let records = collect_observer_records(context, registration.object_slot, true)
            .map_err(|error| JsError::Runtime(error.to_string()))?;
        if records.length(context).unwrap_or(0) == 0 {
            continue;
        }
        let callback = root_get(
            context,
            "__megi_observer_callbacks",
            u64::from(registration.callback_slot),
        )
        .map_err(|error| JsError::Runtime(error.to_string()))?;
        let object = root_get(
            context,
            "__megi_observer_objects",
            u64::from(registration.object_slot),
        )
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
