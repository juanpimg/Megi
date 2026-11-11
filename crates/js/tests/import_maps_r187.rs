//! R187: import maps, literal dynamic import and the documented top-level-await gap.
#![cfg(feature = "js")]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use js::{Budget, ImportMap, JsError, JsRuntime};

fn runtime_with_modules() -> (JsRuntime, Arc<Mutex<HashMap<String, String>>>) {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    let modules: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let modules_for_resolver = Arc::clone(&modules);
    runtime.set_module_resolver(Box::new(move |specifier, base| {
        let modules = modules_for_resolver.lock().expect("modules");
        let url = if specifier.starts_with("file:") {
            Some(specifier.to_string())
        } else if specifier.starts_with('.') {
            let directory = base
                .rsplit_once('/')
                .map_or(base, |(directory, _)| directory);
            let relative = specifier.trim_start_matches("./");
            Some(format!("{directory}/{relative}"))
        } else {
            None
        };
        if url.as_ref().is_some_and(|url| modules.contains_key(url)) {
            Ok(url)
        } else {
            Ok(None)
        }
    }));
    let modules_for_loader = Arc::clone(&modules);
    runtime.set_module_loader(Box::new(move |url| {
        modules_for_loader
            .lock()
            .expect("modules")
            .get(url)
            .cloned()
            .ok_or_else(|| JsError::ModuleNotFound(url.to_string()))
    }));
    (runtime, modules)
}

fn add_module(modules: &Arc<Mutex<HashMap<String, String>>>, url: &str, source: &str) {
    modules
        .lock()
        .expect("modules")
        .insert(url.to_string(), source.to_string());
}

#[test]
fn import_map_parses_exact_prefix_and_null_fallbacks() {
    let map = ImportMap::from_json(
        r#"{"imports":{"exact":"file:///exact.js","pkg/":"file:///pkg/","blocked":null}}"#,
    )
    .expect("parse");
    assert_eq!(map.exact_len(), 2);
    assert_eq!(map.prefix_len(), 1);
    assert_eq!(
        map.resolve("exact"),
        Some(Some("file:///exact.js".to_string()))
    );
    assert_eq!(
        map.resolve("pkg/mod.js"),
        Some(Some("file:///pkg/mod.js".to_string()))
    );
    assert_eq!(map.resolve("blocked"), Some(None));
    assert_eq!(map.resolve("unmapped"), None);
    assert!(ImportMap::from_json("{bad").is_err());
    assert!(ImportMap::from_json("[]").is_err());
}

#[test]
fn import_map_resolves_bare_specifiers_before_the_host_resolver() {
    let (mut runtime, modules) = runtime_with_modules();
    add_module(
        &modules,
        "file:///deps/dep.js",
        "export const value = 'mapped';",
    );
    runtime
        .set_import_map_json(r#"{"imports":{"dep":"file:///deps/dep.js"}}"#)
        .expect("map");
    let outcome = runtime
        .eval_module(
            "import { value } from 'dep';\nexport const marker = value;",
            "app.js",
            "file:///app/app.js",
            &Budget::SMALL,
        )
        .expect("module");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "marker" && value.contains("mapped"))
    );
}

#[test]
fn import_map_null_mapping_blocks_the_specifier() {
    let (mut runtime, _modules) = runtime_with_modules();
    runtime
        .set_import_map_json(r#"{"imports":{"blocked":null}}"#)
        .expect("map");
    let error = runtime
        .eval_module(
            "import x from 'blocked';",
            "app.js",
            "file:///app.js",
            &Budget::SMALL,
        )
        .expect_err("blocked");
    assert!(matches!(error, JsError::ImportMapBlocked(_)), "{error:?}");
}

#[test]
fn dynamic_import_literal_returns_a_promise_and_mutates_the_dom() {
    let (mut runtime, modules) = runtime_with_modules();
    add_module(
        &modules,
        "file:///app/dyn.js",
        "document.body.setAttribute('data-dyn', 'dynamic');\nexport const value = 'dynamic';",
    );
    runtime
        .eval_module(
            "globalThis.__dynType = typeof import('./dyn.js');",
            "app.js",
            "file:///app/app.js",
            &Budget::SMALL,
        )
        .expect("module");
    let _ = runtime.run_microtasks();
    let attribute = runtime.document().body().and_then(|body| {
        runtime
            .document()
            .try_node(body)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute("data-dyn").map(str::to_owned))
    });
    assert_eq!(attribute.as_deref(), Some("dynamic"));
}

#[test]
fn dynamic_import_non_literal_is_a_typed_error() {
    let (mut runtime, _modules) = runtime_with_modules();
    let error = runtime
        .eval_module(
            "const name = './x.js'; import(name);",
            "app.js",
            "file:///app.js",
            &Budget::SMALL,
        )
        .expect_err("non-literal");
    assert!(matches!(error, JsError::DynamicImport(_)), "{error:?}");
}

#[test]
fn top_level_await_is_a_documented_typed_gap() {
    let (mut runtime, _modules) = runtime_with_modules();
    let error = runtime
        .eval_module(
            "await Promise.resolve(1);",
            "app.js",
            "file:///app.js",
            &Budget::SMALL,
        )
        .expect_err("TLA");
    assert!(matches!(error, JsError::TopLevelAwait(_)), "{error:?}");
    let async_error = runtime
        .eval_module_async(
            "await Promise.resolve(1);",
            "app.js",
            "file:///app.js",
            &Budget::SMALL,
        )
        .expect_err("TLA async");
    assert!(
        matches!(async_error, JsError::TopLevelAwait(_)),
        "{async_error:?}"
    );
}

#[test]
fn import_map_json_errors_are_typed() {
    let (mut runtime, _modules) = runtime_with_modules();
    assert!(matches!(
        runtime.set_import_map_json("not json"),
        Err(JsError::ImportMap(_))
    ));
    assert!(matches!(
        runtime.set_import_map_json(r#"{"imports":{"x":1}}"#),
        Err(JsError::ImportMap(_))
    ));
}
