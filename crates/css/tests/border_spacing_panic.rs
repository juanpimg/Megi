//! w3-css-001: `border-spacing` with more than two lengths must not panic.
//! `parse_border_spacing` wrote `lengths[count]` before checking the bound,
//! so `border-spacing: 1px 2px 3px` panicked from an untrusted stylesheet.

use megi_core::{Attribute, Document, Namespace};
use megi_css::resolve_document_styles;

fn spacing(css: &str) -> (f32, f32) {
    let mut doc = Document::new();
    let root = doc.root();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(root, html);
    let table = doc.create_element(Namespace::Html, "table", vec![Attribute::new("id", "t")]);
    doc.append_child(html, table);
    let styles = resolve_document_styles(&doc, Some(css));
    styles.get(&table).expect("computed style").border_spacing
}

#[test]
fn three_border_spacing_lengths_are_rejected_without_panic() {
    assert_eq!(spacing("#t { border-spacing: 1px 2px 3px; }"), (0.0, 0.0));
}

#[test]
fn valid_border_spacing_still_parses() {
    assert_eq!(spacing("#t { border-spacing: 0; }"), (0.0, 0.0));
    assert_eq!(spacing("#t { border-spacing: 1px; }"), (1.0, 1.0));
    assert_eq!(spacing("#t { border-spacing: 1px 2px; }"), (1.0, 2.0));
}
