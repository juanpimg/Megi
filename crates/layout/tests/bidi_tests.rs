//! Integration tests for CSS `direction` / `unicode-bidi` and UAX #9 inline reordering (#986, #944).

use css::{Direction, TextAlign, UnicodeBidi, resolve_document_styles_with_viewport};
use dom::{Attribute, Document, Namespace};
use layout::{BoxType, LayoutBox, Rect, layout_document};

fn collect_line_text(b: &LayoutBox) -> String {
    let mut out = String::new();
    let mut stack = vec![b];
    let mut parts: Vec<(f32, String)> = Vec::new();
    while let Some(node) = stack.pop() {
        if let BoxType::TextNode(ref text) = node.box_type {
            parts.push((node.dimensions.content.x, text.clone()));
        }
        for child in node.children.iter().rev() {
            stack.push(child);
        }
    }
    parts.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, part) in parts {
        out.push_str(&part);
    }
    out
}

fn doc_with_p(style_attr: &str, text: &str) -> (Document, dom::NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(
        Namespace::Html,
        "p",
        vec![
            Attribute::new("id", "target"),
            Attribute::new("style", style_attr),
        ],
    );
    let txt = doc.create_text_node(text);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, txt);
    (doc, p)
}

#[test]
fn css_direction_rtl_inherits_and_resolves_start_end_text_align() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let parent = doc.create_element(Namespace::Html, "div", vec![Attribute::new("id", "parent")]);
    let child = doc.create_element(Namespace::Html, "span", vec![Attribute::new("id", "child")]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, parent);
    doc.append_child(parent, child);

    let css = "#parent { direction: rtl; unicode-bidi: bidi-override; }";
    let styles = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);

    assert_eq!(styles[&parent].direction, Direction::Rtl);
    assert_eq!(styles[&parent].unicode_bidi, UnicodeBidi::BidiOverride);
    assert_eq!(styles[&parent].text_align, TextAlign::Right);

    // `direction` is inherited; `unicode-bidi` is NOT inherited.
    assert_eq!(styles[&child].direction, Direction::Rtl);
    assert_eq!(styles[&child].unicode_bidi, UnicodeBidi::Normal);
    assert_eq!(styles[&child].text_align, TextAlign::Right);

    // `text-align: end` in RTL resolves to Left.
    let css2 = "#parent { direction: rtl; text-align: end; }";
    let styles2 = resolve_document_styles_with_viewport(&doc, Some(css2), 800.0, 600.0);
    assert_eq!(styles2[&parent].text_align, TextAlign::Left);
}

#[test]
fn rtl_paragraph_right_aligns_and_reorders_words_visually() {
    let (doc, _) = doc_with_p("", "שלום עולם");
    let css = "#target { width: 400px; direction: rtl; }";
    let styles = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).unwrap();
    let visual = collect_line_text(&root);
    // Logical "שלום עולם" -> visual left-to-right "םלוע םולש".
    assert_eq!(visual, "םלוע םולש");
}

#[test]
fn mixed_ltr_and_rtl_keeps_ltr_outer_order_and_reverses_rtl_word() {
    let (doc, _) = doc_with_p("", "Hello שלום world");
    let css = "#target { width: 400px; direction: ltr; }";
    let styles = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).unwrap();
    let visual = collect_line_text(&root);
    assert_eq!(visual, "Hello םולש world");
}

#[test]
fn bidi_override_rtl_reverses_latin_text() {
    let (doc, _) = doc_with_p("", "abc def");
    let css = "#target { width: 400px; direction: rtl; unicode-bidi: bidi-override; }";
    let styles = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).unwrap();
    let visual = collect_line_text(&root);
    assert_eq!(visual, "fed cba");
}
