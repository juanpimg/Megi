#![no_main]
//! Fuzz target: ES modules + import maps preprocessing, without page DOM.
//!
//! `JsRuntime::eval_module` (static imports, resolver/loader callbacks), import-map parsing
//! (`js::ImportMap::from_json` / `set_import_map_json`) and the module graph walker.
//!
//! Input framing: a **length-prefixed** split (`[u16 LE len][bytes]…`, see
//! [`megi_fuzz::length_prefixed_parts`]) — part 0 is the import-map JSON, part 1 the entry
//! module, parts 2.. its dependencies. The previous NUL-split framing plus a fixed 256-char
//! truncation hid most of the interesting byte space (a mutated `"` or `\` inside a JSON
//! string was clipped away, and NULs made the split degenerate), so both were replaced
//! (#701). Truncation is char-boundary safe ([`megi_fuzz::lossy_prefix`]).
//!
//! The host is bounded: ≤ 8 modules, ≤ 8 KiB per module, ≤ 24 loader calls, tiny
//! [`js::Budget`]. Cycles, missing exports, unsupported dynamic imports and budget
//! exhaustion are expected to return typed `JsError`s; no network, filesystem or wall-clock
//! access is provided. The target looks for panics/aborts/UB/Sanitizer findings/timeouts.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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

    // Length-prefixed framing: keeps NULs and arbitrary bytes inside each part, so JSON
    // strings and module sources are fuzzed instead of being clipped at the first NUL.
    let parts = megi_fuzz::length_prefixed_parts(data, MAX_MODULES + 2);
    let import_map_source = parts.first().copied().unwrap_or_default();
    let entry_source = parts.get(1).copied().unwrap_or_default();
    let modules: Vec<String> = parts
        .iter()
        .skip(2)
        .map(|part| megi_fuzz::lossy_prefix(part, MAX_MODULE_BYTES))
        .collect();

    // Import-map parsing (arbitrary bytes; valid map seeds live in the corpus).
    let map_text = megi_fuzz::lossy_prefix(import_map_source, MAX_MODULE_BYTES);
    let _ = js::ImportMap::from_json(&map_text);

    let Ok(mut runtime) = js::JsRuntime::new() else {
        return;
    };
    let budget = js::Budget {
        loop_iterations: 2_000,
        recursion_limit: 64,
        stack_size_limit: 2_048,
    };
    runtime.set_budget(budget);
    let _ = runtime.set_import_map_json(&map_text);

    // Resolver: every specifier maps into the synthetic "fuzz://module/…" origin. Specifiers
    // are truncated on a char boundary (the old `&specifier[..256]` panicked on a
    // multi-byte code point straddling the cut).
    runtime.set_module_resolver(Box::new(|specifier: &str, _importer: &str| {
        Ok(Some(format!(
            "fuzz://module/{}",
            megi_fuzz::lossy_prefix(specifier.as_bytes(), 256)
        )))
    }));

    // Loader: bounded number of calls; module texts cycle through the input parts.
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

    let entry_text = megi_fuzz::lossy_prefix(entry_source, MAX_MODULE_BYTES);
    let entry_name = format!("fuzz://module/entry-{}", data.len());
    let _ = runtime.eval_module(&entry_text, "fuzz.js", &entry_name, &budget);

    // A second graph root (fresh module cache per eval_module) with the same bounded host:
    // exercises cache reset/recursion across roots.
    let _ = runtime.eval_module(
        "import './dep.js';",
        "fuzz2.js",
        "fuzz://module/second",
        &budget,
    );
});
