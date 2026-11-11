//! Round 25: ES module subset (import/export, resolver/loader, graph order, errors).

#![cfg(feature = "js")]

use std::collections::HashMap;

use js::JsError;
use js::runtime::{Budget, JsRuntime};

struct Modules {
    resolver: HashMap<String, String>,
    sources: HashMap<String, String>,
}

fn runtime_with(modules: &Modules) -> JsRuntime {
    let resolver = modules.resolver.clone();
    let sources = modules.sources.clone();
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_module_resolver(Box::new(move |specifier, _base| {
        Ok(resolver.get(specifier).cloned())
    }));
    runtime.set_module_loader(Box::new(move |url| {
        sources
            .get(url)
            .cloned()
            .ok_or_else(|| JsError::ModuleNotFound(url.to_string()))
    }));
    runtime
}

fn fixture_modules() -> Modules {
    Modules {
        resolver: HashMap::from([
            ("./a.js".to_string(), "mem://a".to_string()),
            ("./b.js".to_string(), "mem://b".to_string()),
            ("./c.js".to_string(), "mem://c".to_string()),
            ("./missing.js".to_string(), "mem://missing".to_string()),
        ]),
        sources: HashMap::from([
            ("mem://a".to_string(), "export default 42;\nexport const named = 7;\nexport function fn() { return 'fn'; }\nexport class Klass {}\n".to_string()),
            ("mem://b".to_string(), "import def, { named as alias } from './a.js'; window.__b = def + alias;\n".to_string()),
            ("mem://c".to_string(), "import * as ns from './a.js'; window.__c = ns.named;\n".to_string()),
        ]),
    }
}

#[test]
fn default_and_named_imports_work() {
    let modules = fixture_modules();
    let mut runtime = runtime_with(&modules);
    let outcome = runtime
        .eval_module(
            "import def, { named } from './a.js';\nexport const sum = def + named;\n",
            "main",
            "mem://main",
            &Budget::default(),
        )
        .expect("module");
    assert!(
        outcome
            .exports
            .iter()
            .any(|(name, value)| name == "sum" && value == "49"),
        "{:?}",
        outcome.exports
    );
}

#[test]
fn named_alias_and_namespace_imports() {
    let modules = fixture_modules();
    let mut runtime = runtime_with(&modules);
    runtime
        .eval_module(
            "import { named as alias } from './a.js';\nexport const v = alias;\n",
            "m",
            "mem://m",
            &Budget::default(),
        )
        .expect("named");
    assert_eq!(runtime.eval("window.__unused", "t").unwrap(), "undefined");
    let modules2 = fixture_modules();
    let mut runtime2 = runtime_with(&modules2);
    runtime2
        .eval_module(
            "import * as ns from './a.js';\nexport const v = ns.named;\n",
            "m",
            "mem://m",
            &Budget::default(),
        )
        .expect("namespace");
    let outcome = runtime2
        .eval_module(
            "import * as ns from './a.js';\nexport const v = ns.named;\n",
            "m",
            "mem://m",
            &Budget::default(),
        )
        .expect("namespace exports");
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
fn three_module_graph_runs_in_dependency_order() {
    let modules = Modules {
        resolver: HashMap::from([
            ("./a.js".to_string(), "mem://a".to_string()),
            ("./b.js".to_string(), "mem://b".to_string()),
            ("./c.js".to_string(), "mem://c".to_string()),
        ]),
        sources: HashMap::from([
            (
                "mem://a".to_string(),
                "window.__order.push('a');\nexport const a = 1;\n".to_string(),
            ),
            (
                "mem://b".to_string(),
                "import { a } from './a.js';\nwindow.__order.push('b');\nexport const b = a + 1;\n"
                    .to_string(),
            ),
            (
                "mem://c".to_string(),
                "import { b } from './b.js';\nwindow.__order.push('c');\nexport const c = b + 1;\n"
                    .to_string(),
            ),
        ]),
    };
    let mut runtime2 = runtime_with(&modules);
    runtime2.eval("window.__order = []", "t").unwrap();
    let outcome = runtime2
        .eval_module(
            "import { c } from './c.js';\nwindow.__order.push('main');\nexport const total = c;\n",
            "main",
            "mem://main",
            &Budget::default(),
        )
        .expect("graph");
    assert_eq!(
        outcome
            .exports
            .iter()
            .find(|(n, _)| n == "total")
            .map(|(_, v)| v.as_str()),
        Some("3")
    );
    assert_eq!(
        runtime2.eval("window.__order.join(',')", "t").unwrap(),
        "a,b,c,main"
    );
}

#[test]
fn module_mutates_the_dom() {
    let modules = Modules {
        resolver: HashMap::new(),
        sources: HashMap::new(),
    };
    let mut runtime = runtime_with(&modules);
    runtime
        .eval_module(
            "const p = document.createElement('p');\np.textContent = 'from module';\ndocument.body.appendChild(p);\nexport const ok = document.body.children.length;\n",
            "main",
            "mem://main",
            &Budget::default(),
        )
        .expect("dom module");
    assert_eq!(
        runtime.eval("document.body.children.length", "t").unwrap(),
        "1"
    );
}

#[test]
fn import_meta_url_is_available() {
    let modules = Modules {
        resolver: HashMap::new(),
        sources: HashMap::new(),
    };
    let mut runtime = runtime_with(&modules);
    runtime
        .eval_module(
            "export const url = import.meta.url;\n",
            "main",
            "mem://dir/main.js",
            &Budget::default(),
        )
        .expect("meta");
    let outcome = runtime
        .eval_module(
            "export const marker = 1;\n",
            "main",
            "mem://dir/other.js",
            &Budget::default(),
        )
        .expect("meta2");
    assert_eq!(outcome.url, "mem://dir/other.js");
}

#[test]
fn missing_module_is_typed_error() {
    let modules = fixture_modules();
    let mut runtime = runtime_with(&modules);
    let error = runtime
        .eval_module(
            "import './nope.js';\n",
            "main",
            "mem://main",
            &Budget::default(),
        )
        .expect_err("missing");
    assert!(matches!(error, JsError::ModuleNotFound(_)), "{error:?}");
}

#[test]
fn missing_export_is_typed_error() {
    let modules = fixture_modules();
    let mut runtime = runtime_with(&modules);
    let error = runtime
        .eval_module(
            "import { doesNotExist } from './a.js';\n",
            "main",
            "mem://main",
            &Budget::default(),
        )
        .expect_err("missing export");
    assert!(matches!(error, JsError::ModuleExport(_)), "{error:?}");
}

#[test]
fn circular_dependency_is_typed_error() {
    let modules = Modules {
        resolver: HashMap::from([
            ("./a.js".to_string(), "mem://a".to_string()),
            ("./b.js".to_string(), "mem://b".to_string()),
        ]),
        sources: HashMap::from([
            (
                "mem://a".to_string(),
                "import { b } from './b.js';\nexport const a = 1;\n".to_string(),
            ),
            (
                "mem://b".to_string(),
                "import { a } from './a.js';\nexport const b = 2;\n".to_string(),
            ),
        ]),
    };
    let mut runtime = runtime_with(&modules);
    let error = runtime
        .eval_module(
            "import { a } from './a.js';\nexport const x = a;\n",
            "main",
            "mem://main",
            &Budget::default(),
        )
        .expect_err("cycle");
    assert!(matches!(error, JsError::ModuleCycle(_)), "{error:?}");
}

#[test]
fn malformed_import_is_typed_syntax_error() {
    let modules = Modules {
        resolver: HashMap::from([("./a.js".to_string(), "mem://a".to_string())]),
        sources: HashMap::from([("mem://a".to_string(), "export const ok = 1;\n".to_string())]),
    };
    let mut runtime = runtime_with(&modules);
    let error = runtime
        .eval_module(
            "import './a.js';\nconst = ;\n",
            "main",
            "mem://main",
            &Budget::default(),
        )
        .expect_err("syntax");
    assert!(
        matches!(error, JsError::ModuleSyntax(_) | JsError::Syntax(_)),
        "{error:?}"
    );
}

#[test]
fn evaluation_is_deterministic() {
    let modules = fixture_modules();
    let source = "import def from './a.js';\nexport const value = def;\n";
    let first = {
        let mut runtime = runtime_with(&modules);
        runtime
            .eval_module(source, "main", "mem://main", &Budget::default())
            .expect("first")
    };
    let second = {
        let mut runtime = runtime_with(&modules);
        runtime
            .eval_module(source, "main", "mem://main", &Budget::default())
            .expect("second")
    };
    assert_eq!(first, second);
}

#[test]
fn budget_is_applied() {
    let modules = Modules {
        resolver: HashMap::new(),
        sources: HashMap::new(),
    };
    let mut runtime = runtime_with(&modules);
    let strict = Budget {
        loop_iterations: 10,
        recursion_limit: 8,
        stack_size_limit: 1024,
    };
    let error = runtime
        .eval_module(
            "let i = 0; while (i < 1000) { i += 1; }\nexport const v = i;\n",
            "main",
            "mem://main",
            &strict,
        )
        .expect_err("budget");
    assert!(matches!(error, JsError::Budget(_)), "{error:?}");
}

#[test]
fn side_effect_import_executes_dependency() {
    let modules = Modules {
        resolver: HashMap::from([("./side.js".to_string(), "mem://side".to_string())]),
        sources: HashMap::from([(
            "mem://side".to_string(),
            "window.__side = (window.__side || 0) + 1;\n".to_string(),
        )]),
    };
    let mut runtime = runtime_with(&modules);
    runtime.eval("window.__side = 0", "t").unwrap();
    runtime
        .eval_module(
            "import './side.js';\nexport const done = true;\n",
            "main",
            "mem://main",
            &Budget::default(),
        )
        .expect("side effect");
    assert_eq!(runtime.eval("window.__side", "t").unwrap(), "1");
}
