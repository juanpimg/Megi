//! Minified ES namespace imports (`import*as ns from"spec"`), as emitted by
//! GitHub's rspack bundles (#2242).

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;
use std::sync::{Arc, Mutex};

fn runtime_with_modules() -> (
    JsRuntime,
    Arc<Mutex<std::collections::HashMap<String, String>>>,
) {
    let mut rt = JsRuntime::new().expect("runtime");
    let modules: Arc<Mutex<std::collections::HashMap<String, String>>> = Arc::new(Mutex::new(
        [(
            "file:///react.js".to_owned(),
            "export const version='19';".to_owned(),
        )]
        .into_iter()
        .collect(),
    ));
    let _m_unused = Arc::clone(&modules);
    rt.set_module_resolver(Arc::new(move |spec, _base| {
        Ok(if spec == "react" {
            Some("file:///react.js".to_owned())
        } else {
            None
        })
    }));
    let m = Arc::clone(&modules);
    rt.set_module_loader(Arc::new(move |url| {
        m.lock()
            .unwrap()
            .get(url)
            .cloned()
            .ok_or_else(|| megi_js::JsError::ModuleNotFound(url.to_owned()))
    }));
    (rt, modules)
}

#[test]
fn minified_namespace_import_parses_specifier() {
    let (mut rt, _m) = runtime_with_modules();
    // No space after `import`, no space before `from`, no space before the quote.
    let source = "import*as React from\"react\"; globalThis.__v = React.version;";
    let result = rt.eval_module(source, "app.js", "file:///app.js", &Default::default());
    if let Err(ref e) = result {
        eprintln!("ERR: {}", e);
    }
    assert!(
        result.is_ok(),
        "minified namespace import failed: {:?}",
        result.err()
    );
    let v = rt.eval("globalThis.__v", "check.js").expect("read __v");
    assert_eq!(v, "19");
}
