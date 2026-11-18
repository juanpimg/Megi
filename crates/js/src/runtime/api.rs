use super::*;

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
        Self::with_document_realm(document, url, false)
    }

    pub(crate) fn with_document_realm(
        document: Document,
        url: String,
        is_worker: bool,
    ) -> Result<Self, JsError> {
        let module_loader = Rc::new(MegiModuleLoader {
            base_url: url.clone(),
            cache: RefCell::new(HashMap::new()),
            max_cache_entries: 256,
        });
        let allow_eval = Rc::new(std::cell::Cell::new(true));
        let mut context = Context::builder()
            .module_loader(module_loader)
            .host_hooks(Rc::new(CspEvalHooks {
                allow_eval: Rc::clone(&allow_eval),
            }))
            .build()
            .map_err(|error| JsError::Internal(error.to_string()))?;
        // The Context is the single owner of the host cell; no thread-local mirror.
        crate::host::install_host(
            &mut context,
            Rc::new(RefCell::new(Host::new(document, url))),
        );
        // Make the immutable provider slot reachable from every binding.
        crate::providers::install_providers(
            &mut context,
            crate::providers::HostProviders::default(),
        );
        if is_worker {
            install_bare_worker_globals(&mut context)?;
        } else {
            install_globals(&mut context)?;
        }
        Ok(Self {
            context,
            budget: Budget::default(),
            allow_eval,
            last_error: None,
        })
    }

    /// Controls whether dynamic code evaluation (`eval`, `new Function`) is permitted (#1526).
    pub fn set_allow_eval(&self, allow: bool) {
        self.allow_eval.set(allow);
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
        // The host is installed in the Context at construction, so the outer
        // borrow cannot fail; `borrow()` on the old cell only panicked on
        // re-entry, which this path never does.
        let host = host_ref(&self.context).expect("JavaScript host installed in context");
        std::cell::Ref::map(host, |host| &host.document)
    }

    /// Mutable access to the document, borrowed from the context's host cell.
    pub fn document_mut(&mut self) -> std::cell::RefMut<'_, Document> {
        let host = host_cell(&mut self.context).expect("JavaScript host installed in context");
        std::cell::RefMut::map(host, |host| &mut host.document)
    }

    /// Snapshot of the canvas 2D surfaces created by scripts (#1545), keyed by `<canvas>` node.
    pub fn canvas_surfaces(
        &self,
    ) -> std::collections::HashMap<megi_core::NodeId, crate::canvas::CanvasSurface> {
        host_ref(&self.context)
            .map(|host| host.canvas_store.clone_surfaces())
            .unwrap_or_default()
    }

    /// Drains the captured `console.*` entries.
    pub fn take_console(&mut self) -> Vec<ConsoleEntry> {
        with_context_host(&mut self.context, |host| host.take_console()).unwrap_or_default()
    }

    /// Sets `document.title`.
    pub fn set_title(&mut self, title: impl Into<String>) {
        let _ = with_context_host(&mut self.context, |host| host.title = title.into());
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
        let result = self.context.eval(source);

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
                let _ = crate::events::report_uncaught(&mut self.context, &message, &name);
                self.last_error = Some(message);
                Err(typed)
            }
        }
    }

    /// Executes timers whose due time has arrived (real clock by default).
    pub fn run_tasks(&mut self, max_steps: usize) -> Result<usize, JsError> {
        let deadline = crate::events::now_ms();
        let result = crate::events::run_due_timers(&mut self.context, deadline, max_steps);
        if let Err(error) = &result {
            let message = error.to_string();
            let _ = crate::events::report_uncaught(&mut self.context, &message, "<timer>");
        }
        result
    }

    /// Installs the computed-style provider used by `window.getComputedStyle(element)`.
    ///
    /// The provider receives the live `Document` and the element `NodeId` and returns
    /// CSS property/value pairs (e.g. from `megi_css::ComputedStyle`). Without a provider,
    /// `getComputedStyle` falls back to the element's inline declarations (read-only).
    pub fn set_computed_style_provider(&mut self, provider: crate::host::ComputedStyleProvider) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.computed_style = Some(provider.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .computed_style_provider = Some(provider);
    }

    /// Installs the provider used by `window.matchMedia(query).matches`.
    ///
    /// The provider receives the raw media-query string and returns whether it currently
    /// matches the host viewport. Without a provider every query reports `matches: false`.
    /// `change` events are not fired automatically (the host re-evaluates and can re-run
    /// scripts); `addListener`/`addEventListener` are accepted as no-ops.
    pub fn set_media_provider(&mut self, provider: crate::host::MediaProvider) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.media = Some(provider.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .media_provider = Some(provider);
    }

    /// Installs the provider used by `window.innerWidth/innerHeight/devicePixelRatio`
    /// (and the base for `pageXOffset/pageYOffset`). Without a provider the documented
    /// default is 1280×720 at DPR 1.0.
    pub fn set_viewport_provider(&mut self, provider: crate::host::ViewportProvider) {
        // The provider's scroll offsets are the initial scroll position; `scrollTo`/
        // `scrollBy` then move the host-controlled offsets (exposed as pageX/YOffset).
        let initial = provider();
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.viewport = Some(provider.clone());
        }
        let mut host = host_cell(&mut self.context).expect("JavaScript host installed in context");
        host.scroll_x = initial.scroll_x;
        host.scroll_y = initial.scroll_y;
        host.viewport_provider = Some(provider);
    }

    /// Installs the History API provider (`pushState`/`replaceState`/`back`/`forward`/`go`).
    /// Without a provider those methods are a documented no-op.
    pub fn set_history_provider(&mut self, provider: crate::host::HistoryProvider) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.history = Some(provider.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .history_provider = Some(provider);
    }

    /// Installs the `location.assign/replace/reload` provider.
    pub fn set_location_provider(&mut self, provider: crate::host::LocationProvider) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.location = Some(provider.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .location_provider = Some(provider);
    }

    /// Installs optional Web Storage persistence and loads the current snapshots.
    pub fn set_storage_provider(&mut self, provider: StorageProvider) {
        let local = (provider.load)(StorageKind::Local);
        let session = (provider.load)(StorageKind::Session);
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.storage_load = Some(provider.load.clone());
            slot.storage_save = Some(provider.save.clone());
        }
        let mut host = host_cell(&mut self.context).expect("JavaScript host installed in context");
        host.local_storage = local.into_iter().collect();
        host.session_storage = session.into_iter().collect();
        host.storage_provider = Some(provider);
    }

    /// Installs the host cookie jar provider used by `document.cookie`.
    pub fn set_cookie_provider(&mut self, provider: crate::host::CookieProvider) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.cookie_get = Some(provider.get.clone());
            slot.cookie_set = Some(provider.set.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .cookie_provider = Some(provider);
    }

    /// Current `document.readyState` (`loading` until the host changes it).
    pub fn ready_state(&self) -> String {
        host_ref(&self.context)
            .map(|host| host.ready_state.clone())
            .unwrap_or_default()
    }

    /// Sets `document.readyState` (`loading`, `interactive` or `complete`).
    pub fn set_ready_state(&mut self, state: impl Into<String>) {
        let _ = with_context_host(&mut self.context, |host| host.ready_state = state.into());
    }

    /// Scroll offsets written by `window.scrollTo`/`scrollBy` (CSS pixels).
    pub fn scroll_position(&self) -> (f64, f64) {
        host_ref(&self.context)
            .map(|host| (host.scroll_x, host.scroll_y))
            .unwrap_or_default()
    }

    /// Number of pending `requestAnimationFrame` callbacks.
    pub fn pending_animation_frames(&self) -> usize {
        host_ref(&self.context)
            .map(|host| host.timers.iter().filter(|timer| timer.animation).count())
            .unwrap_or(0)
    }

    /// Takes the runtime's document out (the host keeps an empty one afterwards).
    pub fn take_document(&mut self) -> Document {
        with_context_host(&mut self.context, |host| {
            std::mem::replace(&mut host.document, Document::new())
        })
        .expect("JavaScript host installed in context")
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
        // Listeners must use the currently configured budget, not the one left over
        // from the last script evaluation (#1215).
        self.apply_current_budget();
        crate::events::dispatch_rust_event(&mut self.context, target, event_type, bubbles)
    }

    /// Dispatches a structured UI event (`KeyboardEvent`, `InputEvent`, `FocusEvent`, or
    /// `Event`) to `target` with the given bubbling/cancelable flags (#1504).
    pub fn dispatch_ui_event(
        &mut self,
        target: crate::events::JsEventTarget,
        event_type: &str,
        bubbles: bool,
        cancelable: bool,
        kind: &crate::events::JsUiEventKind,
    ) -> Result<bool, JsError> {
        self.apply_current_budget();
        crate::events::dispatch_rust_ui_event(
            &mut self.context,
            target,
            event_type,
            bubbles,
            cancelable,
            kind,
        )
    }

    /// Updates `document.activeElement` from the host UI (e.g. shell focus changes, #1504).
    pub fn set_active_element(&mut self, node: Option<NodeId>) {
        let _ = with_context_host(&mut self.context, |host| host.active_element = node);
    }

    /// Pushes the runtime's configured budget into the Boa VM.
    pub(crate) fn apply_current_budget(&mut self) {
        let budget = self.budget;
        apply_budget(&mut self.context, &budget);
    }

    /// Runs pending promise/microtask jobs (Boa's job queue).
    pub fn run_microtasks(&mut self) -> Result<(), JsError> {
        let result = match self.context.run_jobs() {
            Ok(()) => Ok(()),
            Err(error) => {
                let message = error.to_string();
                let _ = crate::events::report_uncaught(&mut self.context, &message, "<microtask>");
                let typed = classify_error(&message);
                self.last_error = Some(message);
                Err(typed)
            }
        };
        if result.is_ok() {
            let delivered = deliver_mutation_records(&mut self.context);
            delivered?;
            if let Err(error) = self.run_worker_tasks(64) {
                self.report_worker_delivery_error(error, "<worker tasks>");
            }
        }
        result
    }

    /// Advances the injectable virtual clock by `delta` and runs the timers now due.
    pub fn advance(&mut self, delta: std::time::Duration) -> Result<u64, JsError> {
        let result = crate::events::advance_clock(&mut self.context, delta);
        if let Err(error) = &result {
            let message = error.to_string();
            let _ = crate::events::report_uncaught(&mut self.context, &message, "<timer>");
        }
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
        // Timer callbacks must not inherit the loop limit of the last script eval (#1215).
        self.apply_current_budget();
        let result = crate::events::run_until(&mut self.context, deadline_ms, max_timers);
        if let Err(error) = &result {
            let message = error.to_string();
            let _ = crate::events::report_uncaught(&mut self.context, &message, "<timer>");
        }
        result
    }

    /// Number of pending timers.
    pub fn pending_timers(&self) -> usize {
        host_ref(&self.context)
            .map(|host| host.timers.len())
            .unwrap_or(0)
    }

    /// Current scheduler time in milliseconds (virtual clock after `advance`/`run_until`).
    pub fn now_ms(&self) -> u64 {
        host_ref(&self.context)
            .map(|host| host.now_ms())
            .unwrap_or(0)
    }

    /// Last non-fatal exception thrown by an event listener, if any.
    ///
    /// Listener exceptions are also pushed to the console capture buffer and do not
    /// interrupt the remaining listeners.
    pub fn last_event_error(&self) -> Option<String> {
        host_ref(&self.context)
            .ok()
            .and_then(|host| host.last_event_error.clone())
    }

    /// Rough estimate of the Rust-side (and JS host-side) memory used by the runtime.
    ///
    /// Boa 0.21 does not expose heap statistics, so this is a documented approximation:
    /// it accounts for DOM nodes, wrapper side-table entries, listeners, inline handlers,
    /// timers, console capture and the document title/URL strings. It deliberately
    /// excludes the JS heap allocated inside Boa itself (documented limitation).
    pub fn estimated_heap_bytes(&self) -> Option<usize> {
        let host = host_ref(&self.context).ok()?;
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
            + host.shadow_roots.len() * 32
            + host.timers.len() * 48
            + console_bytes
            + host.title.len()
            + host.url.len();
        Some(estimate)
    }
}

#[cfg(test)]
mod context_bridge_migration_tests {
    use super::*;

    /// A native binding must resolve its host solely from the Boa `Context`.
    /// There is no thread-local fallback any more, so this test guards the
    /// context-installed bridge used by every realm (including nested ones).
    #[test]
    pub(crate) fn binding_reads_host_from_context() {
        let mut runtime = JsRuntime::new().unwrap();
        runtime.set_title("from-context");
        let value = document_title_get(&JsValue::undefined(), &[], &mut runtime.context).unwrap();
        let text = value
            .to_string(&mut runtime.context)
            .unwrap()
            .to_std_string_escaped();
        assert_eq!(text, "from-context");
    }

    /// The adapter must drop its `RefMut` as soon as the closure returns:
    /// two sequential borrows have to succeed without a typed re-entry error.
    #[test]
    pub(crate) fn context_host_borrow_is_released_when_closure_returns() {
        let mut runtime = JsRuntime::new().unwrap();
        let before = with_context_host(&mut runtime.context, |host| host.title.clone()).unwrap();
        let after = with_context_host(&mut runtime.context, |host| {
            host.title = "second-borrow".into();
            host.title.clone()
        })
        .unwrap();
        assert_eq!(before, "");
        assert_eq!(after, "second-borrow");
    }
}
