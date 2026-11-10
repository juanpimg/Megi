//! Replaced elements conformance tests (CSS 2.2 §9.2.2, §10.3.2, §10.6.2).

use std::collections::HashMap;

use css::{ComputedStyle, Display, Length};
use dom::{Attribute, Document, Namespace, NodeId};
use layout::{BoxType, LayoutBox, Rect, layout_document, layout_document_with_replaced_sizes};

fn make_block_style(width: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style
}

fn setup() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto));
    styles.insert(body, make_block_style(Length::Auto));
    styles.insert(p, make_block_style(Length::Px(400.0)));
    (doc, styles, p)
}

fn layout_tree(doc: &Document, styles: &HashMap<NodeId, ComputedStyle>) -> LayoutBox {
    layout_document(doc, styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout")
}

#[test]
fn test_img_default_intrinsic_size() {
    let (mut doc, mut styles, p) = setup();
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(p, img);
    styles.insert(img, ComputedStyle::initial());

    let tree = layout_tree(&doc, &styles);
    let line = &tree.children[0].children[0].children[0];
    let replaced = &line.children[0];
    assert!(matches!(replaced.box_type, BoxType::Replaced(_)));
    // CSS 2.2 §10.3.2: no intrinsic dimensions ⇒ 300x150.
    assert!((replaced.dimensions.content.width - 300.0).abs() < 0.001);
    assert!((replaced.dimensions.content.height - 150.0).abs() < 0.001);
}

#[test]
fn test_replaced_sizes_api_injects_intrinsic_size() {
    let (mut doc, mut styles, p) = setup();
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(p, img);
    styles.insert(img, ComputedStyle::initial());
    let mut sizes = HashMap::new();
    sizes.insert(img, (120.0, 80.0));

    let tree = layout_document_with_replaced_sizes(
        &doc,
        &styles,
        Rect::new(0.0, 0.0, 800.0, 600.0),
        &sizes,
    )
    .expect("layout");
    let replaced = &tree.children[0].children[0].children[0].children[0];
    assert!((replaced.dimensions.content.width - 120.0).abs() < 0.001);
    assert!((replaced.dimensions.content.height - 80.0).abs() < 0.001);
}

#[test]
fn test_replaced_css_width_uses_intrinsic_aspect_ratio() {
    let (mut doc, mut styles, p) = setup();
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(p, img);
    let mut img_style = ComputedStyle::initial();
    img_style.width = Length::Px(60.0);
    styles.insert(img, img_style);
    let mut sizes = HashMap::new();
    sizes.insert(img, (120.0, 80.0));

    let tree = layout_document_with_replaced_sizes(
        &doc,
        &styles,
        Rect::new(0.0, 0.0, 800.0, 600.0),
        &sizes,
    )
    .expect("layout");
    let replaced = &tree.children[0].children[0].children[0].children[0];
    assert!((replaced.dimensions.content.width - 60.0).abs() < 0.001);
    // 60 * 80 / 120 = 40 (CSS 2.2 §10.6.2 intrinsic ratio).
    assert!((replaced.dimensions.content.height - 40.0).abs() < 0.001);
}

#[test]
fn test_replaced_attribute_size_is_intrinsic_fallback() {
    let (mut doc, mut styles, p) = setup();
    let img = doc.create_element(
        Namespace::Html,
        "img",
        vec![
            Attribute::new("width", "100"),
            Attribute::new("height", "50"),
        ],
    );
    doc.append_child(p, img);
    styles.insert(img, ComputedStyle::initial());

    let tree = layout_tree(&doc, &styles);
    let replaced = &tree.children[0].children[0].children[0].children[0];
    assert!((replaced.dimensions.content.width - 100.0).abs() < 0.001);
    assert!((replaced.dimensions.content.height - 50.0).abs() < 0.001);
}

#[test]
fn test_other_replaced_defaults() {
    let (mut doc, mut styles, p) = setup();
    let input = doc.create_element(Namespace::Html, "input", vec![]);
    doc.append_child(p, input);
    styles.insert(input, ComputedStyle::initial());

    let tree = layout_tree(&doc, &styles);
    let replaced = &tree.children[0].children[0].children[0].children[0];
    assert!((replaced.dimensions.content.width - 20.0).abs() < 0.001);
    assert!((replaced.dimensions.content.height - 20.0).abs() < 0.001);
}

#[test]
fn test_block_level_replaced_box() {
    let (mut doc, mut styles, p) = setup();
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(p, img);
    let mut img_style = ComputedStyle::initial();
    img_style.display = Display::Block;
    styles.insert(img, img_style);
    let mut sizes = HashMap::new();
    sizes.insert(img, (40.0, 30.0));

    let tree = layout_document_with_replaced_sizes(
        &doc,
        &styles,
        Rect::new(0.0, 0.0, 800.0, 600.0),
        &sizes,
    )
    .expect("layout");
    let p_box = &tree.children[0].children[0];
    // Block-level replaced box participates in the normal block flow directly.
    let replaced = &p_box.children[0];
    assert!(matches!(replaced.box_type, BoxType::Replaced(_)));
    assert!((replaced.dimensions.content.width - 40.0).abs() < 0.001);
    assert!((replaced.dimensions.content.height - 30.0).abs() < 0.001);
    assert!((p_box.dimensions.content.height - 30.0).abs() < 0.001);
}
