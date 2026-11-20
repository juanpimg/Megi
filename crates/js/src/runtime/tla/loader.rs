use super::*;
/// Boa 0.22's module parser reports `abrupt end` when a module ends with an
/// `export{...}` / `import{...}` list without a trailing semicolon — a valid ASI
/// form used by minified bundles (e.g. Stencil on european-union.europa.eu).
/// Appending an empty statement `;` is harmless for every other module (#1919).
pub(crate) fn ensure_trailing_semicolon(source: &str) -> String {
    if source.trim_end().ends_with(';') {
        return source.to_owned();
    }
    let mut owned = String::with_capacity(source.len() + 2);
    owned.push_str(source);
    if !source.ends_with(['\n', '\r']) {
        owned.push('\n');
    }
    owned.push(';');
    owned
}

// ---------------------------------------------------------------------------
// Native Boa modules for real top-level await + live bindings (round 32)
// ---------------------------------------------------------------------------

pub(crate) struct MegiModuleLoader {
    pub(crate) base_url: String,
    pub(crate) cache: RefCell<HashMap<(String, String), Module>>,
    pub(crate) max_cache_entries: usize,
}

impl ModuleLoader for MegiModuleLoader {
    async fn load_imported_module(
        // ARCH-EXCEPTION(#1853): Boa 0.22 ModuleLoader requires Rc<Self> for the loader.
        // Plan: #1853 — replace with an arena handle once Boa accepts Arc/owned loaders.
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
        // Resolve the host from the context Boa hands us. There is no separate
        // loader-side host slot (and no thread-local): every realm resolves its
        // own provider/import-map state from the Context it is loading into.
        // Clone the providers out before invoking them: a resolver or loader may
        // re-enter JavaScript and must not observe a live host borrow.
        let (resolver, loader, import_map_result) = {
            let ctx_ref = context.borrow();
            let host =
                crate::host::host_ref(&ctx_ref).map_err(|error| js_error(error.to_string()))?;
            (
                host.module_resolver.clone(),
                host.module_loader.clone(),
                // Consult the import map first (like resolve_module does).
                host.import_map
                    .as_ref()
                    .and_then(|map| map.resolve_with_base(&specifier, Some(&base))),
            )
        };
        if let Some(result) = import_map_result {
            match result {
                Some(mut target) => {
                    if !target.contains("://") && !target.starts_with("data:") {
                        let resolved_target = if let Some(resolver) = &resolver {
                            resolver(&target, &base).map_err(|error| js_error(error.to_string()))?
                        } else {
                            None
                        };
                        if let Some(abs) = resolved_target {
                            target = abs;
                        }
                    }
                    // Load from the import-map target.
                    let source = if let Some(loader) = &loader {
                        loader(&target).map_err(|error| js_error(error.to_string()))?
                    } else {
                        return Err(js_error(format!("module not found: {target}")));
                    };
                    let mut context = context.borrow_mut();
                    let module = Module::parse(
                        Source::from_bytes(&ensure_trailing_semicolon(&source))
                            .with_path(Path::new(&target)),
                        None,
                        &mut context,
                    )?;
                    {
                        let mut cache = self.cache.borrow_mut();
                        if cache.len() >= self.max_cache_entries {
                            cache.clear();
                        }
                        cache.insert((base, specifier), module.clone());
                    }
                    return Ok(module);
                }
                None => {
                    return Err(js_error(format!(
                        "import map blocked: {specifier} (base {base})"
                    )));
                }
            }
        }
        let resolved = if let Some(resolver) = &resolver {
            resolver(&specifier, &base).map_err(|error| js_error(error.to_string()))?
        } else {
            None
        };
        let url = resolved
            .ok_or_else(|| js_error(format!("module not found: {specifier} (base {base})")))?;
        let source = if let Some(loader) = &loader {
            loader(&url).map_err(|error| js_error(error.to_string()))?
        } else {
            return Err(js_error(format!("module not found: {url}")));
        };
        let mut context = context.borrow_mut();
        let module = Module::parse(
            Source::from_bytes(&ensure_trailing_semicolon(&source)).with_path(Path::new(&url)),
            None,
            &mut context,
        )?;
        {
            let mut cache = self.cache.borrow_mut();
            if cache.len() >= self.max_cache_entries {
                cache.clear();
            }
            cache.insert((base, specifier), module.clone());
        }
        Ok(module)
    }

    fn init_import_meta(
        // ARCH-EXCEPTION(#1853): Boa 0.22 ModuleLoader requires Rc<Self> for the loader.
        // Plan: #1853 — replace with an arena handle once Boa accepts Arc/owned loaders.
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
            Source::from_bytes(&ensure_trailing_semicolon(source)).with_path(Path::new(base_url)),
            None,
            &mut self.context,
        )
        .map_err(module_error)?;
        let promise = module.load_link_evaluate(&mut self.context);
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
