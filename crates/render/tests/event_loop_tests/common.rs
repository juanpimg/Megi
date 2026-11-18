//! Shared helpers for the JavaScript event-loop integration tests.

pub(crate) use std::process::Command;

pub(crate) use megi_core::{Document, NodeKind};
pub(crate) use megi_render::{
    RenderOptions, RenderStats, ScriptBudget, ScriptingLimits, ScriptingMode, Viewport, render_url,
};

use std::path::{Path, PathBuf};

pub(crate) fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/render")
        .join(name)
}

pub(crate) fn fixture_url(name: &str) -> String {
    format!("file://{}", fixture_path(name).display())
}

pub(crate) fn base_options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(400, 240).expect("viewport"),
        ..RenderOptions::default()
    }
}

pub(crate) fn disabled_options() -> RenderOptions {
    RenderOptions {
        scripting: ScriptingMode::Disabled,
        ..base_options()
    }
}

pub(crate) fn render_fixture(name: &str, options: &RenderOptions) -> megi_render::RenderOutput {
    render_url(&fixture_url(name), options).expect("fixture renders")
}

/// Collects the concatenated text descendants of `node_id`.
pub(crate) fn collect_text(doc: &Document, node_id: megi_core::NodeId) -> String {
    let mut out = String::new();
    let mut stack = vec![node_id];
    while let Some(id) = stack.pop() {
        let node = doc.node(id);
        if let NodeKind::Text(text) = &node.kind {
            out.push_str(text);
        }
        let mut child = node.first_child;
        while let Some(child_id) = child {
            stack.push(child_id);
            child = doc.node(child_id).next_sibling;
        }
    }
    out
}

/// Returns the trimmed text content of the element with `id`.
pub(crate) fn element_text(doc: &Document, wanted_id: &str) -> String {
    let mut stack = vec![doc.root()];
    while let Some(node_id) = stack.pop() {
        let node = doc.node(node_id);
        if let Some(element) = node.as_element()
            && element.get_attribute("id") == Some(wanted_id)
        {
            return collect_text(doc, node_id).trim().to_string();
        }
        let mut child = node.first_child;
        while let Some(child_id) = child {
            stack.push(child_id);
            child = doc.node(child_id).next_sibling;
        }
    }
    panic!("element with id not found");
}

pub(crate) fn json_number(document: &str, key: &str) -> u64 {
    let marker = format!("\"{key}\":");
    let start = document.find(&marker).unwrap_or_else(|| {
        panic!("missing JSON key {key:?} in {document}");
    }) + marker.len();
    let digits: String = document[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("JSON key {key:?} is not a number"))
}
