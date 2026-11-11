//! `aspect-ratio` geometry (CSS Box Sizing 4 §5): block, replaced, flex and grid.
//!
//! Semantics under test:
//! * definite width + auto height ⇒ height = width / ratio;
//! * definite height + auto width ⇒ width = height * ratio (in-flow block);
//! * both auto + replaced ⇒ intrinsic geometry when `auto` and there is intrinsic
//!   content; explicit ratio uses the intrinsic width as base;
//! * both definite ⇒ the declared sizes win;
//! * `aspect-ratio: auto 1/1` on replaced content prefers the intrinsic ratio.

use std::collections::HashMap;

use css::{AspectRatio, ComputedStyle, Display, Length};
use dom::{Document, Namespace, NodeId};
use layout::{LayoutBox, Rect, layout_document, layout_document_with_replaced_sizes};

fn style(width: Length, height: Length, ratio: Option<AspectRatio>) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style.height = height;
    style.aspect_ratio = ratio;
    style
}

fn ratio(ratio: f32, auto: bool) -> Option<AspectRatio> {
    Some(AspectRatio { auto, ratio })
}

fn document_with_body() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    let mut styles = HashMap::new();
    styles.insert(html, style(Length::Auto, Length::Auto, None));
    styles.insert(body, style(Length::Auto, Length::Auto, None));
    (doc, styles, body)
}

fn layout(doc: &Document, styles: &HashMap<NodeId, ComputedStyle>) -> LayoutBox {
    layout_document(doc, styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout")
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {expected}, got {actual}"
    );
}

/// Root box -> html -> body -> first child.
fn first_child(tree: &LayoutBox) -> &LayoutBox {
    &tree.children[0].children[0]
}

#[test]
fn block_definite_width_auto_height_derives_height() {
    let (mut doc, mut styles, body) = document_with_body();
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(body, div);
    styles.insert(
        div,
        style(Length::Px(320.0), Length::Auto, ratio(16.0 / 9.0, false)),
    );

    let tree = layout(&doc, &styles);
    let box_ = first_child(&tree);
    close(box_.dimensions.content.width, 320.0);
    close(box_.dimensions.content.height, 180.0);
}

#[test]
fn block_auto_width_definite_height_derives_width() {
    let (mut doc, mut styles, body) = document_with_body();
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(body, div);
    styles.insert(
        div,
        style(Length::Auto, Length::Px(180.0), ratio(16.0 / 9.0, false)),
    );

    let tree = layout(&doc, &styles);
    let box_ = first_child(&tree);
    close(box_.dimensions.content.height, 180.0);
    close(box_.dimensions.content.width, 320.0);
}

#[test]
fn both_definite_axes_win_over_ratio() {
    let (mut doc, mut styles, body) = document_with_body();
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(body, div);
    styles.insert(
        div,
        style(
            Length::Px(200.0),
            Length::Px(50.0),
            ratio(16.0 / 9.0, false),
        ),
    );

    let tree = layout(&doc, &styles);
    let box_ = first_child(&tree);
    close(box_.dimensions.content.width, 200.0);
    close(box_.dimensions.content.height, 50.0);
}

#[test]
fn replaced_intrinsic_and_explicit_ratio() {
    let (mut doc, mut styles, body) = document_with_body();
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(body, img);
    styles.insert(
        img,
        style(Length::Px(60.0), Length::Auto, ratio(3.0 / 2.0, false)),
    );
    let mut sizes = HashMap::new();
    sizes.insert(img, (120.0, 80.0));

    let tree = layout_document_with_replaced_sizes(
        &doc,
        &styles,
        Rect::new(0.0, 0.0, 800.0, 600.0),
        &sizes,
    )
    .expect("layout");
    let replaced = first_child(&tree).clone();
    // The explicit ratio overrides the intrinsic 3:2? No: explicit 3/2 equals 120/80,
    // so use a distinguishable ratio in the next assertion set.
    drop(replaced);

    let mut styles2 = styles.clone();
    let mut img_style = style(Length::Px(60.0), Length::Auto, ratio(4.0 / 1.0, false));
    // Rebuild the document because styles are keyed by node id.
    img_style.display = Display::Block;
    styles2.insert(img, img_style);
    let tree = layout_document_with_replaced_sizes(
        &doc,
        &styles2,
        Rect::new(0.0, 0.0, 800.0, 600.0),
        &sizes,
    )
    .expect("layout");
    let replaced = first_child(&tree);
    close(replaced.dimensions.content.width, 60.0);
    close(replaced.dimensions.content.height, 15.0); // 60 / (4/1), not intrinsic 40
}

#[test]
fn replaced_auto_ratio_prefers_intrinsic_geometry() {
    let (mut doc, mut styles, body) = document_with_body();
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(body, img);
    styles.insert(img, style(Length::Auto, Length::Auto, ratio(1.0, true)));
    let mut sizes = HashMap::new();
    sizes.insert(img, (120.0, 80.0));

    let tree = layout_document_with_replaced_sizes(
        &doc,
        &styles,
        Rect::new(0.0, 0.0, 800.0, 600.0),
        &sizes,
    )
    .expect("layout");
    let replaced = first_child(&tree);
    close(replaced.dimensions.content.width, 120.0);
    close(replaced.dimensions.content.height, 80.0);
}

#[test]
fn replaced_explicit_ratio_both_auto_uses_intrinsic_width_as_base() {
    let (mut doc, mut styles, body) = document_with_body();
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(body, img);
    styles.insert(img, style(Length::Auto, Length::Auto, ratio(4.0, false)));
    let mut sizes = HashMap::new();
    sizes.insert(img, (120.0, 80.0));

    let tree = layout_document_with_replaced_sizes(
        &doc,
        &styles,
        Rect::new(0.0, 0.0, 800.0, 600.0),
        &sizes,
    )
    .expect("layout");
    let replaced = first_child(&tree);
    close(replaced.dimensions.content.width, 120.0);
    close(replaced.dimensions.content.height, 30.0); // 120 / 4
}

#[test]
fn replaced_auto_without_intrinsic_keeps_documented_fallback() {
    let (mut doc, mut styles, body) = document_with_body();
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(body, img);
    styles.insert(img, style(Length::Auto, Length::Auto, ratio(1.0, true)));

    let tree = layout(&doc, &styles);
    let replaced = first_child(&tree);
    close(replaced.dimensions.content.width, 300.0);
    close(replaced.dimensions.content.height, 150.0);
}

#[test]
fn flex_item_derives_height_from_definite_width() {
    let (mut doc, mut styles, body) = document_with_body();
    let flex = doc.create_element(Namespace::Html, "div", vec![]);
    let item = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(body, flex);
    doc.append_child(flex, item);
    let mut flex_style = style(Length::Px(400.0), Length::Auto, None);
    flex_style.display = Display::Flex;
    styles.insert(flex, flex_style);
    styles.insert(
        item,
        style(Length::Px(160.0), Length::Auto, ratio(16.0 / 9.0, false)),
    );

    let tree = layout(&doc, &styles);
    let item_box = &tree.children[0].children[0].children[0];
    close(item_box.dimensions.content.width, 160.0);
    close(item_box.dimensions.content.height, 90.0);
}

#[test]
fn grid_item_derives_height_from_definite_width() {
    let (mut doc, mut styles, body) = document_with_body();
    let grid = doc.create_element(Namespace::Html, "div", vec![]);
    let item = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(body, grid);
    doc.append_child(grid, item);
    let mut grid_style = style(Length::Px(400.0), Length::Auto, None);
    grid_style.display = Display::Grid;
    styles.insert(grid, grid_style);
    styles.insert(
        item,
        style(Length::Px(120.0), Length::Auto, ratio(2.0, false)),
    );

    let tree = layout(&doc, &styles);
    let item_box = &tree.children[0].children[0].children[0];
    close(item_box.dimensions.content.width, 120.0);
    close(item_box.dimensions.content.height, 60.0);
}
