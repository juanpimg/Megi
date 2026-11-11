#![no_main]
//! Fuzz target: ES modules + import maps preprocessing, without page DOM.
//!
//! Round 27 added the module subset (`JsRuntime::eval_module`: static imports,
//! resolver/loader callbacks) and import-map parsing
//! (`js::ImportMap::from_json` / `set_import_map_json`). This target splits the
//! input on NUL bytes into: (0) import-map JSON, (1) entry module source,
//! (2..) dependency module sources. It installs a bounded resolver/loader
//! (max module count and call budget), applies a tiny [`js::Budget`], and
//! evaluates the module graph the same way a `<script type="module">` does.
//!
//! Cycles, missing exports, unsupported dynamic imports and budget exhaustion
//! are expected to return typed `JsError`s; no network, filesystem or wall-clock
//! access is provided. The target looks for panics/aborts/UB/timeouts.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer input.
const MAX_FUZZ_INPUT: usize = 64 * 1024;
/// Bounded module graph sizes.
const MAX_MODULES: usize = 8;
const MAX_MODULE_BYTES: usize = 8 * 1024;
const MAX_LOADER_CALLS: usize = 24;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }

    let mut parts = data.split(|byte| *byte == 0);
    let import_map_source = parts.next().unwrap_or_default();
    let entry_source = parts.next().unwrap_or_default();
    let mut modules: Vec<String> = Vec::new();
    for part in parts.take(MAX_MODULES) {
        let text = String::from_utf8_lossy(part);
        modules.push(text[..text.len().min(MAX_MODULE_BYTES)].to_string());
    }
    let entry_source = entry_source[..entry_source.len().min(MAX_MODULE_BYTES)].to_vec();
    let import_map_source =
        import_map_source[..import_map_source.len().min(MAX_MODULE_BYTES)].to_vec();

    let Ok(mut runtime) = js::JsRuntime::new() else {
        return;
    };
    let budget = js::Budget {
        loop_iterations: 2_000,
        recursion_limit: 64,
        stack_size_limit: 2_048,
    };
    runtime.set_budget(budget);

    // Import-map parsing (arbitrary bytes; valid map seeds live in the corpus).
    let map_text = String::from_utf8_lossy(&import_map_source);
    let _ = runtime.set_import_map_json(&map_text);
    let _ = js::ImportMap::from_json(&map_text);

    // Resolver: every specifier maps into the synthetic "fuzz://module/…" origin.
    runtime.set_module_resolver(Box::new(|specifier: &str, _importer: &str| {
        Ok(Some(format!(
            "fuzz://module/{}",
            &specifier[..specifier.len().min(256)]
        )))
    }));

    // Loader: bounded number of calls; module texts cycle through the input.
    let modules = Arc::new(modules);
    let calls = Arc::new(AtomicUsize::new(0));
    let loads = Arc::clone(&calls);
    runtime.set_module_loader(Box::new(move |url: &str| {
        let call = loads.fetch_add(1, Ordering::Relaxed);
        if call >= MAX_LOADER_CALLS {
            return Err(js::JsError::Budget("module loader call cap".to_string()));
        }
        if modules.is_empty() {
            return Ok(String::new());
        }
        let index = call.wrapping_add(url.len()) % modules.len();
        Ok(modules[index].clone())
    }));

    let entry_text = String::from_utf8_lossy(&entry_source).to_string();
    let mut cache: HashMap<String, String> = HashMap::new();
    let entry_name = format!("fuzz://module/entry-{}", data.len());
    cache.insert(entry_name.clone(), entry_text.clone());
    let _ = runtime.eval_module(&entry_text, "fuzz.js", &entry_name, &budget);

    // A second graph root (fresh module cache per eval_module) with the same
    // bounded host: exercises cache reset/recursion across roots.
    let _ = runtime.eval_module(
        "import './dep.js';",
        "fuzz2.js",
        "fuzz://module/second",
        &budget,
    );
    let _ = cache.len();
});
