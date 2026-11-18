//! # Float budget regression tests (issue #1417)
//!
//! Placing a float scans every float already placed in the formatting context to find the
//! lowest position where it fits (CSS 2.2 §9.5.1), so a formatting context with F floats
//! costs O(F²) comparisons. `megi_layout::block::MAX_FLOATS_PER_CONTEXT` bounds that: past the
//! cap, new floats are laid out in flow instead of out of flow, which is observable
//! geometry (they advance the block flow) and documented on the constant itself.

use std::collections::HashMap;

use megi_css::{ComputedStyle, Display, Float, Length};
use megi_core::{Document, Namespace, NodeId};
use megi_layout::block::MAX_FLOATS_PER_CONTEXT;
use megi_layout::{LayoutBox, Rect, layout_document};

fn make_block_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style.height = height;
    style
}

fn make_float_style(side: Float, width: f32, height: f32) -> ComputedStyle {
    let mut style = make_block_style(Length::Px(width), Length::Px(height));
    style.float = side;
    style
}

fn make_div(doc: &mut Document, parent: NodeId) -> NodeId {
    let id = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(parent, id);
    id
}

/// Builds `<html><body><div id=container>` with a 200px wide container.
fn setup_container() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let container = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, container);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(container, make_block_style(Length::Px(200.0), Length::Auto));
    (doc, styles, container)
}

fn layout_doc(doc: &Document, styles: &HashMap<NodeId, ComputedStyle>) -> LayoutBox {
    layout_document(doc, styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout must succeed")
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 0.001,
        "{what}: expected {expected}, got {actual}"
    );
}

/// The container box lives at `html > body > div`.
fn container_box(tree: &LayoutBox) -> &LayoutBox {
    &tree.children[0].children[0]
}

/// A formatting context with more floats than the cap still lays out, and the excess is
/// in flow instead of out of flow: it advances the block flow by its own height.
#[test]
fn floats_past_the_cap_are_laid_out_in_flow() {
    let (mut doc, mut styles, container) = setup_container();
    for _ in 0..MAX_FLOATS_PER_CONTEXT + 5 {
        let id = make_div(&mut doc, container);
        styles.insert(id, make_float_style(Float::Left, 10.0, 10.0));
    }
    let probe = make_div(&mut doc, container);
    styles.insert(probe, make_block_style(Length::Auto, Length::Px(10.0)));

    let tree = layout_doc(&doc, &styles);
    let container_box = container_box(&tree);
    let probe_layout = container_box.children.last().expect("probe box");

    // The first MAX_FLOATS_PER_CONTEXT floats are out of flow and do not move the probe;
    // the five excess floats are in flow (10px tall each), so the probe starts at 50px.
    assert_close(probe_layout.dimensions.content.y, 50.0, "probe y");
    // The excess floats are in flow: they advance the block flow (10px each), so the
    // probe starts at 50px and the container's automatic height reaches the probe bottom.
    assert_close(probe_layout.dimensions.content.y, 50.0, "probe y");
    assert_close(
        container_box.dimensions.content.height,
        60.0,
        "container height",
    );
}

/// Exactly at the cap nothing changes: every float is still placed out of flow.
#[test]
fn floats_at_the_cap_are_still_placed_out_of_flow() {
    let (mut doc, mut styles, container) = setup_container();
    for _ in 0..MAX_FLOATS_PER_CONTEXT {
        let id = make_div(&mut doc, container);
        styles.insert(id, make_float_style(Float::Left, 10.0, 10.0));
    }
    let probe = make_div(&mut doc, container);
    styles.insert(probe, make_block_style(Length::Auto, Length::Px(10.0)));

    let tree = layout_doc(&doc, &styles);
    let container_box = container_box(&tree);
    let probe_layout = container_box.children.last().expect("probe box");
    assert_close(probe_layout.dimensions.content.y, 0.0, "probe y");
    // Floats do not advance the block flow, so the only in-flow child is the probe.
    assert_close(
        container_box.dimensions.content.height,
        10.0,
        "container height",
    );
}

/// The single-pass placement keeps the geometry of small float sets unchanged.
#[test]
fn small_float_sets_keep_their_geometry() {
    let (mut doc, mut styles, container) = setup_container();
    for _ in 0..3 {
        let id = make_div(&mut doc, container);
        styles.insert(id, make_float_style(Float::Left, 50.0, 30.0));
    }
    let following = make_div(&mut doc, container);
    styles.insert(following, make_block_style(Length::Auto, Length::Px(10.0)));

    let tree = layout_doc(&doc, &styles);
    let container_box = container_box(&tree);
    for (index, expected_x) in [0.0_f32, 50.0, 100.0].into_iter().enumerate() {
        let float_layout = &container_box.children[index];
        assert_close(float_layout.dimensions.content.x, expected_x, "float x");
        assert_close(float_layout.dimensions.content.y, 0.0, "float y");
    }
    let following_layout = &container_box.children[3];
    assert_close(following_layout.dimensions.content.y, 0.0, "following y");
    assert_close(following_layout.dimensions.content.x, 150.0, "following x");
    assert_close(
        following_layout.dimensions.content.width,
        50.0,
        "following width",
    );
}

/// A hostile document with 5,000 floats completes with bounded work: the cap keeps only
/// `MAX_FLOATS_PER_CONTEXT` of them tracked.
#[test]
fn five_thousand_floats_lay_out() {
    let (mut doc, mut styles, container) = setup_container();
    for _ in 0..5_000 {
        let id = make_div(&mut doc, container);
        styles.insert(id, make_float_style(Float::Left, 10.0, 10.0));
    }

    let tree = layout_doc(&doc, &styles);
    let container_box = container_box(&tree);
    // 5,000 - MAX_FLOATS_PER_CONTEXT floats are in flow, 10px tall each.
    let in_flow = 5_000 - MAX_FLOATS_PER_CONTEXT;
    assert_close(
        container_box.dimensions.content.height,
        in_flow as f32 * 10.0,
        "container height",
    );
}
