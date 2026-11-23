use super::*;

pub(crate) fn apply_budget(context: &mut Context, budget: &Budget) {
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
pub(crate) const MAX_MODULE_DEPTH: usize = 128;
pub(crate) const MAX_MODULE_COUNT: usize = 1024;

#[allow(clippy::too_many_arguments)]
pub(crate) fn eval_module_graph(
    context: &mut Context,
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
        let dep_url = resolve_module(context, &import.specifier, url)?;
        let dep_source = load_module(context, &dep_url)?;
        let namespace = eval_module_graph(
            context,
            &dep_url,
            &dep_source,
            budget,
            visiting,
            depth + 1,
            done,
        )?;
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
        let dep_url = resolve_module(context, specifier, url)?;
        let dep_source = load_module(context, &dep_url)?;
        let namespace = eval_module_graph(
            context,
            &dep_url,
            &dep_source,
            budget,
            visiting,
            depth + 1,
            done,
        )?;
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
    let value = { context.eval(Source::from_bytes(&script)) }.map_err(module_error)?;
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
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.layout = Some(provider.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .layout_provider = Some(provider);
    }

    /// Installs the ES module resolver (specifier + importer URL → resolved URL).
    pub fn set_module_resolver(&mut self, resolver: crate::host::ModuleResolver) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.module_resolver = Some(resolver.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .module_resolver = Some(resolver);
    }

    /// Installs the ES module loader (`url → source text`; `js` performs no I/O itself).
    pub fn set_module_loader(&mut self, loader: crate::host::ModuleLoader) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.module_loader = Some(loader.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .module_loader = Some(loader);
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
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .import_map = Some(map);
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
    /// immediately; non-literal arguments are resolved at call time via the host resolver
    /// through the `__megiDynamicImport` native helper (F6). Top-level await is
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

/// Native implementation of runtime `import(specifier)` (F6-IMPORTMAP-SCOPES).
///
/// The preprocessor rewrites non-literal `import(expr)` calls here, passing the current
/// module's base URL. Resolution goes through the host resolver/import map; the module is
/// loaded and evaluated on demand and the namespace is delivered through a Promise.
pub(crate) fn dynamic_import_native(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let specifier = args
        .first()
        .map(|value| value.to_string(context))
        .transpose()?
        .map(|text| text.to_std_string_escaped())
        .unwrap_or_default();
    let base = args
        .get(1)
        .map(|value| value.to_string(context))
        .transpose()?
        .map(|text| text.to_std_string_escaped())
        .unwrap_or_default();
    // #2184: inherit the active runtime limits from the execution context rather than
    // resetting to `Budget::default()`, and propagate budget exhaustion uncatchably.
    let limits = context.runtime_limits();
    let active_budget = Budget {
        loop_iterations: limits.loop_iteration_limit(),
        recursion_limit: limits.recursion_limit(),
        stack_size_limit: limits.stack_size_limit(),
    };
    let evaluated = (|| -> Result<JsObject, JsError> {
        let url = resolve_module(context, &specifier, &base)?;
        let source = load_module(context, &url)?;
        let mut visiting = HashSet::new();
        let mut done = HashMap::new();
        eval_module_graph(
            context,
            &url,
            &source,
            &active_budget,
            &mut visiting,
            0,
            &mut done,
        )
    })();
    if let Err(JsError::Budget(ref message)) = evaluated {
        let lower = message.to_ascii_lowercase();
        let limit_err = if lower.contains("recursion") {
            boa_engine::error::RuntimeLimitError::Recursion
        } else if lower.contains("stack") {
            boa_engine::error::RuntimeLimitError::StackSize
        } else {
            boa_engine::error::RuntimeLimitError::LoopIteration
        };
        return Err(limit_err.into());
    }
    let promise = boa_engine::object::builtins::JsPromise::new(
        move |resolvers, context| {
            match evaluated {
                Ok(namespace) => resolvers.resolve.call(
                    &JsValue::undefined(),
                    &[JsValue::from(namespace)],
                    context,
                )?,
                Err(error) => resolvers.reject.call(
                    &JsValue::undefined(),
                    &[JsValue::from(js_string!(error.to_string()))],
                    context,
                )?,
            };
            Ok(JsValue::undefined())
        },
        context,
    )?;
    Ok(JsValue::from(promise))
}
