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
}
