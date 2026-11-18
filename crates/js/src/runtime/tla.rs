use super::*;

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
                        Source::from_bytes(&source).with_path(Path::new(&target)),
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
            Source::from_bytes(&source).with_path(Path::new(&url)),
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

pub(crate) fn node_node_type(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let kind = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .map(|node| match &node.kind {
                megi_core::NodeKind::Element(_) => 1.0,
                megi_core::NodeKind::Text(_) => 3.0,
                megi_core::NodeKind::Comment(_) => 8.0,
                megi_core::NodeKind::Document => 9.0,
                megi_core::NodeKind::DocumentFragment => 11.0,
                megi_core::NodeKind::Doctype { .. } => 10.0,
                megi_core::NodeKind::ProcessingInstruction { .. } => 7.0,
            })
    })
    .flatten()
    .unwrap_or(0.0);
    Ok(JsValue::from(kind))
}

pub(crate) fn string_attribute_get(
    this: &JsValue,
    name: &str,
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(name).map(str::to_owned))
    })
    .flatten();
    Ok(value.map_or_else(JsValue::null, |value| JsValue::from(js_string!(value))))
}

pub(crate) fn string_attribute_set(
    this: &JsValue,
    name: &str,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    with_context_host(context, |host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(megi_core::Attribute::new(name.to_string(), value), true);
        }
        if matches!(name, "src" | "srcdoc" | "sandbox") {
            invalidate_iframe_state(host, node_id);
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn node_title_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "title", c)
}
pub(crate) fn node_title_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "title", a, c)
}
pub(crate) fn node_lang_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "lang", c)
}
pub(crate) fn node_lang_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "lang", a, c)
}
pub(crate) fn node_href_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "href", c)
}
pub(crate) fn node_href_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "href", a, c)
}
pub(crate) fn node_src_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "src", c)
}
pub(crate) fn node_src_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "src", a, c)
}

pub(crate) fn node_action_get(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, c)?;
    let value = with_context_host(c, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute("action").map(str::to_owned))
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(value)))
}
pub(crate) fn node_action_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "action", a, c)
}

pub(crate) fn node_method_get(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, c)?;
    let method = with_context_host(c, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute("method"))
            .map(|value| {
                if value.eq_ignore_ascii_case("post") {
                    "post"
                } else if value.eq_ignore_ascii_case("dialog") {
                    "dialog"
                } else {
                    "get"
                }
            })
            .unwrap_or("get")
    })
    .unwrap_or("get");
    Ok(JsValue::from(js_string!(method)))
}
pub(crate) fn node_method_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "method", a, c)
}

pub(crate) fn node_enctype_get(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, c)?;
    let enctype = with_context_host(c, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute("enctype"))
            .map(|value| {
                if value.eq_ignore_ascii_case("multipart/form-data") {
                    "multipart/form-data"
                } else if value.eq_ignore_ascii_case("text/plain") {
                    "text/plain"
                } else {
                    "application/x-www-form-urlencoded"
                }
            })
            .unwrap_or("application/x-www-form-urlencoded")
    })
    .unwrap_or("application/x-www-form-urlencoded");
    Ok(JsValue::from(js_string!(enctype)))
}
pub(crate) fn node_enctype_set(
    this: &JsValue,
    a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    string_attribute_set(this, "enctype", a, c)
}

pub(crate) fn node_no_validate_get(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "novalidate", c)
}
pub(crate) fn node_no_validate_set(
    this: &JsValue,
    a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_set(this, "novalidate", a, c)
}

pub(crate) fn node_elements_get(
    this: &JsValue,
    _a: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let controls: Vec<NodeId> = with_context_host(context, |host| {
        let document = &host.document;
        document
            .descendants(node_id)
            .filter(|child| {
                document
                    .try_node(*child)
                    .and_then(|node| node.as_element())
                    .is_some_and(|el| match el.local_name.as_str() {
                        "button" | "fieldset" | "select" | "textarea" => true,
                        "input" => !el
                            .get_attribute("type")
                            .is_some_and(|t| t.eq_ignore_ascii_case("image")),
                        _ => false,
                    })
            })
            .collect()
    })
    .unwrap_or_default();
    node_list(context, controls)
}

pub(crate) fn option_ids(document: &megi_core::Document, select: NodeId) -> Vec<NodeId> {
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

pub(crate) fn node_selected_index_get(
    this: &JsValue,
    _a: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let index = with_context_host(context, |host| {
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

pub(crate) fn node_selected_index_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let index = args
        .first()
        .and_then(|value| value.to_number(context).ok())
        .unwrap_or(-1.0) as i64;
    with_context_host(context, |host| {
        let options = option_ids(&host.document, node_id);
        for (position, option) in options.iter().enumerate() {
            if !host.initial_control_checked.contains_key(option) {
                let was_selected = host
                    .document
                    .try_node(*option)
                    .and_then(|n| n.as_element())
                    .is_some_and(|el| el.get_attribute("selected").is_some());
                host.initial_control_checked.insert(*option, was_selected);
            }
            if let Some(element) = host
                .document
                .try_node_mut(*option)
                .and_then(|node| node.as_element_mut())
            {
                if position as i64 == index {
                    element
                        .set_attribute(megi_core::Attribute::new("selected", String::new()), true);
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

pub(crate) fn class_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (_, classes) = classes_of(this, context)?;
    let index = crate::events::webidl_uint32_index(args, context);
    Ok(classes.get(index).map_or_else(JsValue::null, |value| {
        JsValue::from(js_string!(value.clone()))
    }))
}

pub(crate) fn node_get_attribute_node(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let value = with_context_host(context, |host| {
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

pub(crate) fn node_remove_attribute_node(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let item = args
        .first()
        .and_then(|value| value.as_object())
        .ok_or_else(|| js_error("removeAttributeNode expects an Attr"))?;
    let raw_name = item
        .get(js_string!("name"), context)
        .map_err(|error| js_error(error.to_string()))?
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
        .unwrap_or_default();
    let name = attribute_name(node_id, &raw_name, context);
    let old = with_context_host(context, |host| {
        host.document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .and_then(|element| element.remove_attribute(&name))
    })
    .flatten();
    match old {
        Some(value) => {
            // The other attribute-removal paths record a mutation; removeAttributeNode
            // must too, otherwise observers watching `attributes` miss the removal (#1137).
            note_mutation(
                MutationKind::Attributes,
                node_id,
                Some(name.clone()),
                Some(value.clone()),
                context,
            );
            attribute_item(context, &name, &value)
        }
        None => Ok(JsValue::null()),
    }
}
