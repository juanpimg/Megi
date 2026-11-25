//! DevTools Protocol `Runtime.evaluate` bridges (M7).
//!
//! Both helpers return completion values as text so the protocol layer can map primitive
//! results to JSON. `evaluate_expression` is an isolated realm for headless/no-document
//! targets; `evaluate_on_document` binds a fresh realm to the real page arena and its
//! computed-style snapshot when a document has been loaded.

use std::collections::HashMap;
use std::sync::Arc;

use megi_core::{Document, NodeId};
use megi_css::ComputedStyle;

/// Evaluates a console-style expression in a fresh, budgeted, isolated JS realm.
///
/// No page document, host providers or navigation state are attached: this is the
/// fallback for headless/no-document DevTools targets.
pub fn evaluate_expression(source: &str) -> Result<String, String> {
    let mut runtime = megi_js::JsRuntime::new().map_err(|error| error.to_string())?;
    runtime
        .eval(source, "devtools://evaluate")
        .map_err(|error| error.to_string())
}

/// Evaluates `source` in a realm bound to the **real** page document.
///
/// Node handles are the document arena's own and `getComputedStyle` reads the live cascade
/// snapshot through the same provider page scripts use (CSSOM §6). The realm is rebuilt
/// per call: Boa's `ContextHost` is `Rc<RefCell<Host>>` (the ADR-002/#1853 exception) and
/// cannot cross the DevTools accept threads. DOM mutations are written back into
/// `document`; realm-local JS variables are not retained between calls (limitation).
pub fn evaluate_on_document(
    document: &mut Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    source: &str,
    url: &str,
) -> Result<String, String> {
    let mut runtime = megi_js::JsRuntime::with_document(document.clone(), url.to_owned())
        .map_err(|error| error.to_string())?;
    let snapshot = Arc::new(styles.clone());
    runtime.set_computed_style_provider(Arc::new(move |_document, node_id| {
        snapshot
            .get(&node_id)
            .map(crate::browser_host::computed_style_pairs)
            .unwrap_or_default()
    }));
    let result = runtime
        .eval(source, "devtools://evaluate")
        .map_err(|error| error.to_string());
    *document = runtime.take_document();
    result
}
