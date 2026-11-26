//! Replaced elements conformance tests (CSS 2.2 §9.2.2, §10.3.2, §10.6.2).

use std::collections::HashMap;

use megi_core::{Attribute, Document, Namespace, NodeId};
use megi_css::{ComputedStyle, Display, Length};
use megi_layout::{BoxType, LayoutBox, Rect, layout_document, layout_document_with_replaced_sizes};

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

fn find_replaced_box(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
    if matches!(root.box_type, BoxType::Replaced(node) if node == id) {
        return Some(root);
    }
    root.children
        .iter()
        .find_map(|child| find_replaced_box(child, id))
}

#[test]
fn wiki_header_floated_img_keeps_replaced_box() {
    // CSS 2.2 §9.5 / §9.7: a float blockifies *non-replaced* inline boxes; a
    // replaced element keeps its intrinsic dimensions. Wikipedia's logo is a
    // floated `<img width=50 height=50>` inside a flex item.
    let (mut doc, mut styles, p) = setup();
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(p, img);
    let mut img_style = ComputedStyle::initial();
    img_style.float = megi_css::Float::Left;
    img_style.width = megi_css::Length::Px(50.0);
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
    let replaced = find_replaced_box(&tree, img).expect("floated img is a replaced box");
    assert!(replaced.is_float, "the box still floats");
    assert_eq!(replaced.intrinsic_size, Some((120.0, 80.0)));
    assert!((replaced.dimensions.content.width - 50.0).abs() < 0.001);
    assert!((replaced.dimensions.content.height - 33.333).abs() < 0.01);
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

#[test]
fn test_inline_block_replaced_keeps_intrinsic_size() {
    let (mut doc, mut styles, p) = setup();
    let img = doc.create_element(
        Namespace::Html,
        "img",
        vec![
            Attribute::new("width", "90"),
            Attribute::new("height", "45"),
        ],
    );
    doc.append_child(p, img);
    let mut img_style = ComputedStyle::initial();
    img_style.display = Display::InlineBlock;
    styles.insert(img, img_style);

    let tree = layout_tree(&doc, &styles);
    let line = &tree.children[0].children[0].children[0];
    let replaced = &line.children[0];
    assert!(matches!(replaced.box_type, BoxType::Replaced(_)));
    assert!((replaced.dimensions.content.width - 90.0).abs() < 0.001);
    assert!((replaced.dimensions.content.height - 45.0).abs() < 0.001);
}

#[test]
fn test_replaced_sizes_api_rejects_invalid_numbers() {
    for invalid in [
        (f32::NAN, 50.0),
        (50.0, f32::INFINITY),
        (-10.0, 50.0),
        (10_001.0, 50.0),
    ] {
        let (mut doc, mut styles, p) = setup();
        let img = doc.create_element(
            Namespace::Html,
            "img",
            vec![
                Attribute::new("width", "70"),
                Attribute::new("height", "35"),
            ],
        );
        doc.append_child(p, img);
        styles.insert(img, ComputedStyle::initial());
        let mut sizes = HashMap::new();
        sizes.insert(img, invalid);

        let tree = layout_document_with_replaced_sizes(
            &doc,
            &styles,
            Rect::new(0.0, 0.0, 800.0, 600.0),
            &sizes,
        )
        .expect("layout");
        let replaced = &tree.children[0].children[0].children[0].children[0];
        assert!((replaced.dimensions.content.width - 70.0).abs() < 0.001);
        assert!((replaced.dimensions.content.height - 35.0).abs() < 0.001);
    }
}

/// HTML `<input size="N">` gives a text field an intrinsic width of roughly
/// N average character advances (#2247, google.com's search box uses size=57).
#[test]
fn test_input_size_attribute_sets_intrinsic_width() {
    let (mut doc, mut styles, p) = setup();
    let input = doc.create_element(
        Namespace::Html,
        "input",
        vec![Attribute::new("size".to_owned(), "57".to_owned())],
    );
    doc.append_child(p, input);
    styles.insert(input, ComputedStyle::initial());

    let tree = layout_tree(&doc, &styles);
    let line = &tree.children[0].children[0].children[0];
    let replaced = &line.children[0];
    assert!(matches!(replaced.box_type, BoxType::Replaced(_)));
    assert!(
        replaced.dimensions.content.width > 300.0,
        "input size=57 should be much wider than the 20px default, got {}",
        replaced.dimensions.content.width
    );
}

fn find_replaced_width(box_node: &LayoutBox) -> Option<f32> {
    if matches!(box_node.box_type, BoxType::Replaced(_)) {
        return Some(box_node.dimensions.content.width);
    }
    box_node.children.iter().find_map(find_replaced_width)
}

/// An `<input style="width:496px">` inside a shrink-to-fit inline-block gets an
/// intrinsic width of ~20px, so its IFC is ~20px wide. The UA `max-width:100%`
/// (#2247, WHATWG HTML §15.3.2) must not clip it there.
#[test]
fn ua_max_width_cap_is_ignored_for_atomic_inline_with_author_width() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let wrapper = doc.create_element(Namespace::Html, "div", vec![]);
    let input = doc.create_element(Namespace::Html, "input", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, wrapper);
    doc.append_child(wrapper, input);
    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto));
    styles.insert(body, make_block_style(Length::Auto));
    styles.insert(wrapper, {
        let mut style = ComputedStyle::initial();
        style.display = Display::InlineBlock;
        style
    });
    styles.insert(input, {
        let mut style = ComputedStyle::initial();
        style.width = Length::Px(496.0);
        style.max_width = Length::Percent(100.0);
        style.max_width_from_ua = true;
        style
    });

    let tree = layout_tree(&doc, &styles);
    let width = find_replaced_width(&tree).expect("input replaced box");
    assert!(
        (width - 496.0).abs() < 1.0,
        "UA max-width:100% clipped the author width to {width}"
    );
}

/// The same `max-width:100%` authored by the page is a different declaration
/// origin and must keep capping the atomic inline.
#[test]
fn author_max_width_100_percent_still_caps_atomic_inline() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let wrapper = doc.create_element(Namespace::Html, "div", vec![]);
    let input = doc.create_element(Namespace::Html, "input", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, wrapper);
    doc.append_child(wrapper, input);
    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto));
    styles.insert(body, make_block_style(Length::Auto));
    styles.insert(wrapper, {
        let mut style = ComputedStyle::initial();
        style.display = Display::InlineBlock;
        style
    });
    styles.insert(input, {
        let mut style = ComputedStyle::initial();
        style.width = Length::Px(496.0);
        style.max_width = Length::Percent(100.0);
        // max_width_from_ua stays false: this is the author's declaration.
        style
    });

    let tree = layout_tree(&doc, &styles);
    let width = find_replaced_width(&tree).expect("input replaced box");
    assert!(
        width < 100.0,
        "author max-width:100% must still cap the input, got {width}"
    );
}

/// The UA cap is legitimate for block-level form controls: percentages resolve
/// against the real containing block there, so it must not be dropped.
#[test]
fn ua_max_width_cap_still_caps_block_level_replaced() {
    let (mut doc, mut styles, p) = setup();
    let input = doc.create_element(Namespace::Html, "input", vec![]);
    doc.append_child(p, input);
    styles.insert(input, {
        let mut style = ComputedStyle::initial();
        style.display = Display::Block;
        style.width = Length::Px(496.0);
        style.max_width = Length::Percent(100.0);
        style.max_width_from_ua = true;
        style
    });

    let tree = layout_tree(&doc, &styles);
    let width = find_replaced_width(&tree).expect("input replaced box");
    assert!(
        (width - 400.0).abs() < 1.0,
        "UA max-width:100% must cap a block input to the 400px containing block, got {width}"
    );
}
