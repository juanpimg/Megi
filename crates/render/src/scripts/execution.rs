//! Orchestrates script evaluation in the JS realm and returns the mutated document.

use megi_core::Document;
use megi_net::Url;

use super::types::{LoadedScript, ScriptExecution, ScriptingHostContext, ScriptingLimits};
#[cfg(feature = "js")]
use super::{VIRTUAL_EPOCH_MS, event_loop, lifecycle, modules, network, providers};

/// Executes `scripts` against `document` and returns the mutated document.
///
/// When the `js` feature is enabled this creates a [`megi_js::JsRuntime`] bound to the document,
/// evaluates every script with the configured budgets, drains promise microtasks and a
/// bounded number of timer callbacks, captures `console` output, and hands the document
/// back. Without the feature it is a no-op that reports the scripts as skipped.
#[cfg(feature = "js")]
pub fn execute_scripts(
    document: Document,
    document_url: &Url,
    scripts: Vec<LoadedScript>,
    limits: ScriptingLimits,
    host: &ScriptingHostContext,
    warnings: &mut Vec<String>,
) -> ScriptExecution {
    let mut execution = ScriptExecution {
        document,
        canvas_surfaces: std::collections::HashMap::new(),
        frame_documents: Vec::new(),
        executed: 0,
        failed: 0,
        console: Vec::new(),
        tasks_ran: 0,
        limited: false,
        loop_iterations: 0,
        virtual_time_ms: 0,
        timers_pending: 0,
        events_dispatched: 0,
        event_loop: std::time::Duration::ZERO,
        location_actions: Vec::new(),
        history_actions: Vec::new(),
    };
    if scripts.is_empty() {
        return execution;
    }

    // Import maps are installed before any module is evaluated (R187). Only the first
    // `<script type="importmap">` is used; duplicates/invalid JSON are non-fatal warnings.
    let import_map_json =
        modules::import_map_source(&execution.document, limits.max_script_bytes, warnings);

    // Keep the original document so a runtime-construction failure is not fatal.
    let mut runtime = match megi_js::JsRuntime::with_document(
        execution.document.clone(),
        document_url.as_str().to_owned(),
    ) {
        Ok(runtime) => runtime,
        Err(error) => {
            warnings.push(format!("cannot start the JavaScript runtime: {error}"));
            return execution;
        }
    };

    providers::install_frame_providers(&mut runtime, document_url, host);
    let import_map_integrity = providers::install_import_map(
        &mut runtime,
        import_map_json.as_deref(),
        document_url,
        host.csp.as_ref(),
        warnings,
    );
    providers::install_style_providers(&mut runtime, host);
    network::install_network_providers(&mut runtime, document_url, host, limits);
    providers::install_layout_provider(&mut runtime, host);
    let (location_actions, history_actions) = providers::install_navigation_providers(&mut runtime);
    providers::install_storage_providers(&mut runtime, document_url, host);
    modules::install_module_providers(
        &mut runtime,
        document_url,
        host,
        limits,
        import_map_integrity,
        &scripts,
    );

    runtime.set_ready_state("loading");

    // Deterministic virtual clock: pin `now` to a fixed epoch before any script runs (the
    // host clock never goes backwards, so `run_until(0)` would keep the real startup
    // elapsed time and make timer/rAF due times machine-speed dependent).
    let _ = runtime.run_until(VIRTUAL_EPOCH_MS);

    let mut remaining_loops = limits.total_loop_iterations;
    // Timers pumped while driving top-level `await` count against the page timer budget.
    let mut module_tasks = 0usize;
    let mut module_virtual_ms = 0u64;
    for script in scripts {
        let grant = limits.budget.loop_iterations.min(remaining_loops);
        if grant == 0 {
            execution.limited = true;
            warnings.push(
                "total JavaScript loop-iteration budget exhausted; skipping remaining scripts"
                    .to_owned(),
            );
            break;
        }
        remaining_loops -= grant;
        let budget = megi_js::Budget {
            loop_iterations: grant,
            recursion_limit: limits.budget.recursion_limit,
            stack_size_limit: limits.budget.stack_size_limit,
        };
        runtime.set_budget(budget);
        // `document.currentScript` is only non-null while this script is evaluating (#1887).
        runtime.set_current_script(Some(script.element));
        let result = if script.is_module {
            let base = script
                .module_url
                .clone()
                .unwrap_or_else(|| document_url.as_str());
            if script.text.contains("await") || script.text.contains("import(") {
                // Real top-level await and native dynamic `import(expr)`: native Boa modules
                // (live bindings included). The deterministic host loop drives microtasks/timers
                // to completion within the page budget.
                let module_budget = megi_js::ModuleAsyncBudget {
                    timer_tasks: limits.max_tasks.saturating_sub(module_tasks),
                    virtual_time_ms: limits.max_virtual_time_ms.saturating_sub(module_virtual_ms),
                };
                runtime
                    .eval_module_async_with_budget(
                        &script.text,
                        &script.source_name,
                        &base,
                        &budget,
                        module_budget,
                    )
                    .map(Some)
            } else {
                match runtime.eval_module(&script.text, &script.source_name, &base, &budget) {
                    Ok(_) => Ok(None),
                    Err(
                        megi_js::JsError::DynamicImport(_) | megi_js::JsError::TopLevelAwait(_),
                    ) => {
                        let module_budget = megi_js::ModuleAsyncBudget {
                            timer_tasks: limits.max_tasks.saturating_sub(module_tasks),
                            virtual_time_ms: limits
                                .max_virtual_time_ms
                                .saturating_sub(module_virtual_ms),
                        };
                        runtime
                            .eval_module_async_with_budget(
                                &script.text,
                                &script.source_name,
                                &base,
                                &budget,
                                module_budget,
                            )
                            .map(Some)
                    }
                    Err(error) => Err(error),
                }
            }
        } else {
            runtime
                .eval(&script.text, &script.source_name)
                .map(|_| None)
        };
        runtime.set_current_script(None);
        match result {
            Ok(Some(evaluated)) => {
                execution.executed += 1;
                module_tasks = module_tasks.saturating_add(evaluated.timer_tasks);
                module_virtual_ms = module_virtual_ms.saturating_add(evaluated.virtual_time_ms);
                if evaluated.limited {
                    execution.limited = true;
                    warnings.push(
                        "top-level await timer budget exhausted; module may be incomplete"
                            .to_owned(),
                    );
                }
            }
            Ok(None) => execution.executed += 1,
            Err(error) => {
                execution.failed += 1;
                if matches!(error, megi_js::JsError::Budget(_)) {
                    execution.limited = true;
                }
                let kind = if script.is_module { "module" } else { "script" };
                warnings.push(format!("{kind} {} failed: {error}", script.source_name));
            }
        }
        // Promise jobs scheduled by the script (microtask checkpoint).
        event_loop::drain_microtasks(&mut runtime, warnings);
    }

    lifecycle::run_lifecycle_events(
        &mut runtime,
        &limits,
        host,
        module_tasks,
        module_virtual_ms,
        &mut execution,
        &location_actions,
        &history_actions,
        warnings,
    );
    execution
}

/// No-op execution used when the crate is built without the `js` feature.
#[cfg(not(feature = "js"))]
pub fn execute_scripts(
    document: Document,
    _document_url: &Url,
    scripts: Vec<LoadedScript>,
    _limits: ScriptingLimits,
    _host: &ScriptingHostContext,
    warnings: &mut Vec<String>,
) -> ScriptExecution {
    if !scripts.is_empty() {
        warnings.push(format!(
            "skipping {} script(s): the engine was built without the `js` feature",
            scripts.len()
        ));
    }
    ScriptExecution {
        document,
        executed: 0,
        failed: 0,
        console: Vec::new(),
        tasks_ran: 0,
        limited: false,
        loop_iterations: 0,
        virtual_time_ms: 0,
        timers_pending: 0,
        location_actions: Vec::new(),
        history_actions: Vec::new(),
        events_dispatched: 0,
        event_loop: std::time::Duration::ZERO,
    }
}
