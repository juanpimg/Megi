use super::*;

impl JsRuntime {
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

    /// Returns `true` when at least one listener or inline handler is registered for any event in
    /// `event_types`. Used by the renderer to skip building CSS event objects nobody observes.
    pub fn has_any_event_listener(&self, event_types: &[&str]) -> bool {
        host_ref(&self.context)
            .map(|host| {
                host.listeners
                    .iter()
                    .any(|listener| event_types.contains(&listener.event_type.as_str()))
                    || host
                        .inline_handlers
                        .iter()
                        .any(|handler| event_types.contains(&handler.event_type.as_str()))
            })
            .unwrap_or(false)
    }

    /// Sets the `<script>` element being evaluated, exposed as `document.currentScript` (#1887).
    pub fn set_current_script(&mut self, node: Option<NodeId>) {
        let _ = with_context_host(&mut self.context, |host| host.current_script = node);
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
