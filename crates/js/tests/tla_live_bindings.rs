//! Round 32: real top-level await and live module bindings (native Boa modules).

#![cfg(feature = "js")]

use std::collections::HashMap;

use js::host::FetchResult;
use js::runtime::{Budget, JsRuntime};

fn runtime_with(modules: &[(&str, &str)]) -> JsRuntime {
    let resolver: HashMap<String, String> = modules
        .iter()
        .map(|(spec, _)| {
            (
                spec.to_string(),
                format!(
                    "mem://{}",
                    spec.trim_start_matches("./").trim_end_matches(".js")
                ),
            )
        })
        .collect();
    let sources: HashMap<String, String> = modules
        .iter()
        .map(|(spec, source)| {
            (
                format!(
                    "mem://{}",
                    spec.trim_start_matches("./").trim_end_matches(".js")
                ),
                source.to_string(),
            )
        })
        .collect();
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_module_resolver(Box::new(move |specifier, _base| {
        Ok(resolver.get(specifier).cloned())
    }));
    runtime.set_module_loader(Box::new(move |url| {
        sources
            .get(url)
            .cloned()
            .ok_or_else(|| js::JsError::ModuleNotFound(url.to_string()))
    }));
    runtime
}

fn eval(runtime: &mut JsRuntime, source: &str) -> Result<js::ModuleOutcome, js::JsError> {
    runtime.eval_module_async(source, "main", "mem://main", &Budget::default())
}

#[test]
fn top_level_await_happy_path() {
    let mut runtime = runtime_with(&[]);
    let outcome = eval(
        &mut runtime,
        "const v = await Promise.resolve(5); export const x = v;",
    )
    .expect("tla");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "x" && value == "5"),
        "{:?}",
        outcome.exports
    );
}

#[test]
fn top_level_await_dependency_order() {
    let mut runtime = runtime_with(&[("./a.js", "await Promise.resolve(); export const a = 1;")]);
    let outcome = eval(
        &mut runtime,
        "import { a } from './a.js'; await Promise.resolve(); export const total = a + 1;",
    )
    .expect("order");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "total" && value == "2"),
        "{:?}",
        outcome.exports
    );
}

#[test]
fn top_level_await_error_is_typed() {
    let mut runtime = runtime_with(&[]);
    let error = eval(
        &mut runtime,
        "await Promise.reject(new Error('tla boom')); export const x = 1;",
    )
    .expect_err("reject");
    assert!(error.to_string().contains("tla boom"), "{error}");
}

#[test]
fn top_level_await_with_timer() {
    let mut runtime = runtime_with(&[]);
    let outcome = eval(
        &mut runtime,
        "await new Promise(function (resolve) { setTimeout(resolve, 10); }); export const v = 7;",
    )
    .expect("timer");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "v" && value == "7"),
        "{:?}",
        outcome.exports
    );
}

#[test]
fn top_level_await_with_fetch_provider() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_fetch_provider(Box::new(|_request| {
        Ok(FetchResult {
            status: 200,
            status_text: "OK".into(),
            url: "mem://data".into(),
            headers: vec![],
            body: br#"{"answer":42}"#.to_vec(),
        })
    }));
    let outcome = eval(&mut runtime, "const r = await fetch('/data'); const data = await r.json(); export const answer = data.answer;").expect("fetch tla");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "answer" && value == "42"),
        "{:?}",
        outcome.exports
    );
}

#[test]
fn live_let_binding_sees_mutation() {
    let mut runtime = runtime_with(&[(
        "./a.js",
        "export let count = 0; export function bump() { count += 1; }",
    )]);
    let outcome = eval(
        &mut runtime,
        "import { count, bump } from './a.js'; bump(); bump(); export const read = count;",
    )
    .expect("live");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "read" && value == "2"),
        "{:?}",
        outcome.exports
    );
}

#[test]
fn live_var_binding_sees_mutation() {
    let mut runtime = runtime_with(&[(
        "./a.js",
        "export var value = 1; export function set(v) { value = v; }",
    )]);
    let outcome = eval(
        &mut runtime,
        "import { value, set } from './a.js'; set(9); export const read = value;",
    )
    .expect("var");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "read" && value == "9"),
        "{:?}",
        outcome.exports
    );
}

#[test]
fn const_binding_is_immutable() {
    let mut runtime = runtime_with(&[("./a.js", "export const fixed = 1;")]);
    let outcome = eval(&mut runtime, "import { fixed } from './a.js'; var error = ''; try { fixed = 2; } catch (e) { error = String(e); } export const result = error;").expect("const");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "result" && value.contains("TypeError")),
        "{:?}",
        outcome.exports
    );
}

#[test]
fn module_cycle_is_supported_by_native_modules() {
    let mut runtime = runtime_with(&[
        (
            "./a.js",
            "import { b } from './b.js'; export const a = 1; export function sum() { return a + b; }",
        ),
        (
            "./b.js",
            "import { a } from './a.js'; export const b = 2; export function check() { return a + b; }",
        ),
    ]);
    let outcome = eval(
        &mut runtime,
        "import { sum } from './a.js'; export const total = sum();",
    )
    .expect("cycle");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "total" && value == "3"),
        "{:?}",
        outcome.exports
    );
}

#[test]
fn import_meta_url_available_in_native_modules() {
    let mut runtime = runtime_with(&[("./a.js", "export const url = import.meta.url;")]);
    let outcome = eval(
        &mut runtime,
        "import { url } from './a.js'; export const seen = url;",
    )
    .expect("meta");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "seen" && value.contains("a")),
        "{:?}",
        outcome.exports
    );
}
