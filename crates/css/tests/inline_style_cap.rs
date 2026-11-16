//! The inline `style="…"` attribute is capped before parsing to bound the work a
//! single element can trigger. The cap must be applied on a UTF-8 character
//! boundary: slicing at a fixed byte offset panics when that offset lands inside
//! a multi-byte code point, which any page could trigger.

use std::collections::HashMap;

use css::{ComputedStyle, Length};
use dom::{Attribute, Document, Namespace, NodeId};

/// Builds `html > body > div[style]` and resolves the document styles.
fn styled_document(style_attribute: &str) -> (HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let target = doc.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new("style", style_attribute)],
    );
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, target);
    let styles = css::resolve_document_styles(&doc, None);
    (styles, target)
}

#[test]
fn inline_style_truncation_lands_on_a_char_boundary() {
    const CAP: usize = 64 * 1024;
    // A two-byte code point that starts one byte before the cap, so the cap
    // falls in the middle of it.
    let declaration = "width: 10px;";
    let padding = " ".repeat(CAP - declaration.len() - 1);
    let style = format!("{declaration}{padding}\u{e9} height: 20px;");

    let (styles, target) = styled_document(&style);
    let computed = styles.get(&target).expect("style");

    // The declaration before the cut still applies…
    assert_eq!(computed.width, Length::Px(10.0));
    // …and everything after the cut is dropped, not mis-parsed.
    assert_ne!(computed.height, Length::Px(20.0));
}

#[test]
fn inline_style_exactly_at_the_cap_is_kept() {
    const CAP: usize = 64 * 1024;
    let declaration = "width: 10px;";
    let style = format!("{declaration}{}", " ".repeat(CAP - declaration.len()));

    let (styles, target) = styled_document(&style);
    assert_eq!(styles.get(&target).expect("style").width, Length::Px(10.0));
}
