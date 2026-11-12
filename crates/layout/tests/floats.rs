//! # Float / clear / overflow layout conformance tests
//! (CSS 2.2 §9.4.1, §9.5, §9.5.1, §9.5.2, §10.6.7, §11)
//!
//! Covers WP-4.7/4.10: `float: left|right`, `clear`, BFC float containment and the
//! `overflow` clip data contract consumed by paint.

use std::collections::HashMap;

use css::{ComputedStyle, Display, Float, Length, Overflow};
use dom::{Document, Namespace, NodeId};
use layout::{BoxType, LayoutBox, Rect, layout_document};

fn make_block_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style.height = height;
    style
}

fn setup_document() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    (doc, styles, html, body)
}

fn make_div(doc: &mut Document, parent: NodeId) -> NodeId {
    let id = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(parent, id);
    id
}

fn make_float_style(side: Float, width: f32, height: f32) -> ComputedStyle {
    let mut style = make_block_style(Length::Px(width), Length::Px(height));
    style.float = side;
    style
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

/// Builds `<html><body><div id=container>` with a 200px wide container.
fn setup_container() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let (mut doc, mut styles, _html, body) = setup_document();
    let container = make_div(&mut doc, body);
    styles.insert(container, make_block_style(Length::Px(200.0), Length::Auto));
    (doc, styles, container)
}

// ---------------------------------------------------------------------------
// Floats (CSS 2.2 §9.5)
// ---------------------------------------------------------------------------

#[test]
fn test_float_left_shortens_following_block() {
    let (mut doc, mut styles, container) = setup_container();
    let float_box = make_div(&mut doc, container);
    let following = make_div(&mut doc, container);
    styles.insert(float_box, make_float_style(Float::Left, 50.0, 30.0));
    styles.insert(following, make_block_style(Length::Auto, Length::Px(10.0)));

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    let float_layout = &container_box.children[0];
    let following_layout = &container_box.children[1];

    // The float is out of flow: it does not advance the block flow.
    assert_close(float_layout.dimensions.content.x, 0.0, "float x");
    assert_close(float_layout.dimensions.content.y, 0.0, "float y");
    assert_close(
        following_layout.dimensions.content.y,
        0.0,
        "following block y",
    );
    // The following block is narrowed to the float edge (line boxes are shortened).
    assert_close(
        following_layout.dimensions.content.x,
        50.0,
        "following block x",
    );
    assert_close(
        following_layout.dimensions.content.width,
        150.0,
        "following block width",
    );
    // A non-BFC container does not enclose its floats: height comes from in-flow content.
    assert_close(
        container_box.dimensions.content.height,
        10.0,
        "container height",
    );
}

#[test]
fn test_float_right_shortens_following_block_from_the_right() {
    let (mut doc, mut styles, container) = setup_container();
    let float_box = make_div(&mut doc, container);
    let following = make_div(&mut doc, container);
    styles.insert(float_box, make_float_style(Float::Right, 50.0, 30.0));
    styles.insert(following, make_block_style(Length::Auto, Length::Px(10.0)));

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    let float_layout = &container_box.children[0];
    let following_layout = &container_box.children[1];

    assert_close(
        float_layout.dimensions.content.x,
        150.0,
        "right float x (200 - 50)",
    );
    assert_close(
        following_layout.dimensions.content.x,
        0.0,
        "following block x",
    );
    assert_close(
        following_layout.dimensions.content.width,
        150.0,
        "following block width",
    );
}

#[test]
fn test_two_left_floats_stack_horizontally() {
    let (mut doc, mut styles, container) = setup_container();
    let float1 = make_div(&mut doc, container);
    let float2 = make_div(&mut doc, container);
    styles.insert(float1, make_float_style(Float::Left, 50.0, 20.0));
    styles.insert(float2, make_float_style(Float::Left, 60.0, 20.0));

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    assert_close(
        container_box.children[0].dimensions.content.x,
        0.0,
        "float 1 x",
    );
    assert_close(
        container_box.children[1].dimensions.content.x,
        50.0,
        "float 2 x",
    );
}

#[test]
fn test_float_shrink_to_fit_from_text() {
    // DefaultTextMeasurer: 8px per character at font-size 16px => "Hi" = 16px.
    let (mut doc, mut styles, container) = setup_container();
    let float_box = make_div(&mut doc, container);
    doc.append_text(float_box, "Hi");
    let following = make_div(&mut doc, container);
    let mut float_style = make_block_style(Length::Auto, Length::Auto);
    float_style.float = Float::Left;
    float_style.font_size = 16.0;
    styles.insert(float_box, float_style);
    styles.insert(following, make_block_style(Length::Auto, Length::Px(10.0)));

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    let float_layout = &container_box.children[0];
    assert_close(
        float_layout.dimensions.content.width,
        16.0,
        "float shrink-to-fit width",
    );
    assert_close(
        container_box.children[1].dimensions.content.x,
        16.0,
        "following content shifted past the float",
    );
}

#[test]
fn test_float_blockifies_inline_display() {
    let (mut doc, mut styles, container) = setup_container();
    let span = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(container, span);
    let mut span_style = ComputedStyle::initial();
    span_style.display = Display::Inline;
    span_style.float = Float::Left;
    span_style.width = Length::Px(40.0);
    span_style.height = Length::Px(20.0);
    styles.insert(span, span_style);

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    // A floated inline box is blockified (CSS 2.2 §9.7).
    assert_eq!(container_box.children[0].box_type, BoxType::BlockNode(span));
    assert!(container_box.children[0].is_float);
}

// ---------------------------------------------------------------------------
// clear (CSS 2.2 §9.5.2)
// ---------------------------------------------------------------------------

#[test]
fn test_clear_both_moves_box_below_float() {
    let (mut doc, mut styles, container) = setup_container();
    let float_box = make_div(&mut doc, container);
    let cleared = make_div(&mut doc, container);
    styles.insert(float_box, make_float_style(Float::Left, 50.0, 30.0));
    let mut cleared_style = make_block_style(Length::Auto, Length::Px(10.0));
    cleared_style.clear = css::Clear::Both;
    styles.insert(cleared, cleared_style);

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    let cleared_layout = &container_box.children[1];
    assert_close(cleared_layout.dimensions.content.y, 30.0, "cleared box y");
    assert_close(cleared_layout.dimensions.content.x, 0.0, "cleared box x");
    // Below the float, the box is no longer narrowed.
    assert_close(
        cleared_layout.dimensions.content.width,
        200.0,
        "cleared box width",
    );
    assert_close(
        container_box.dimensions.content.height,
        40.0,
        "container height",
    );
}

#[test]
fn test_clear_left_ignores_right_float() {
    let (mut doc, mut styles, container) = setup_container();
    let right_float = make_div(&mut doc, container);
    let cleared = make_div(&mut doc, container);
    styles.insert(right_float, make_float_style(Float::Right, 50.0, 30.0));
    let mut cleared_style = make_block_style(Length::Auto, Length::Px(10.0));
    cleared_style.clear = css::Clear::Left;
    styles.insert(cleared, cleared_style);

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    let cleared_layout = &container_box.children[1];
    // `clear: left` must not move below a right float.
    assert_close(cleared_layout.dimensions.content.y, 0.0, "cleared box y");
}

// ---------------------------------------------------------------------------
// BFC float containment + overflow clip rect (CSS 2.2 §10.6.7, §11)
// ---------------------------------------------------------------------------

#[test]
fn test_overflow_hidden_bfc_height_includes_floats_and_clip() {
    let (mut doc, mut styles, container) = setup_container();
    let float_box = make_div(&mut doc, container);
    let following = make_div(&mut doc, container);
    styles.insert(float_box, make_float_style(Float::Left, 50.0, 30.0));
    styles.insert(following, make_block_style(Length::Auto, Length::Px(10.0)));
    let mut container_style = make_block_style(Length::Px(200.0), Length::Auto);
    container_style.overflow = Overflow::Hidden;
    // Recompute so the container's own box keeps its width but gains overflow.
    styles.insert(container, container_style);

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    // CSS 2.2 §10.6.7: a BFC root encloses its floats in auto height.
    assert_close(
        container_box.dimensions.content.height,
        30.0,
        "BFC height with float",
    );
    // Clip rectangle: the container's own padding box in absolute coordinates.
    let clip = container_box
        .clip
        .expect("overflow hidden must set a clip rect");
    assert_close(clip.x, 0.0, "clip x");
    assert_close(clip.y, 0.0, "clip y");
    assert_close(clip.width, 200.0, "clip width");
    assert_close(clip.height, 30.0, "clip height");
    // Descendants inherit the (intersected) clip.
    let following_clip = container_box.children[1].clip.expect("descendant clip");
    assert_close(following_clip.width, 200.0, "descendant clip width");
}

#[test]
fn test_overflow_visible_sets_no_clip() {
    let (mut doc, mut styles, container) = setup_container();
    let child = make_div(&mut doc, container);
    styles.insert(child, make_block_style(Length::Auto, Length::Px(10.0)));

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    assert!(
        container_box.clip.is_none(),
        "visible overflow must not clip"
    );
    assert!(
        container_box.children[0].clip.is_none(),
        "descendant not clipped"
    );
}

#[test]
fn test_nested_overflow_clips_intersect() {
    let (mut doc, mut styles, container) = setup_container();
    let inner = make_div(&mut doc, container);
    let mut inner_style = make_block_style(Length::Px(100.0), Length::Px(20.0));
    inner_style.overflow = Overflow::Clip;
    styles.insert(inner, inner_style);
    let mut outer_style = make_block_style(Length::Px(200.0), Length::Auto);
    outer_style.overflow = Overflow::Hidden;
    styles.insert(container, outer_style);

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    let inner_box = &container_box.children[0];
    let outer_clip = container_box.clip.expect("outer clip");
    let inner_clip = inner_box.clip.expect("inner clip");
    // The inner clip is the intersection of the outer clip and the inner padding box.
    assert_close(
        inner_clip.x,
        outer_clip.x.max(inner_box.dimensions.padding_box().x),
        "intersection x",
    );
    assert_close(inner_clip.width, 100.0, "intersection width");
    assert_close(inner_clip.height, 20.0, "intersection height");
}

#[test]
fn test_overflow_hidden_prevents_margin_collapse_with_children() {
    let (mut doc, mut styles, container) = setup_container();
    let child = make_div(&mut doc, container);
    let mut child_style = make_block_style(Length::Auto, Length::Px(10.0));
    child_style.margin.top = Length::Px(15.0);
    styles.insert(child, child_style);
    let mut container_style = make_block_style(Length::Px(200.0), Length::Auto);
    container_style.overflow = Overflow::Hidden;
    styles.insert(container, container_style);

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    let child_box = &container_box.children[0];
    // A BFC root does not collapse its top margin with its first child's: the 15px
    // margin applies inside the container.
    assert_close(child_box.dimensions.content.y, 15.0, "child y inside BFC");
    assert_close(
        container_box.dimensions.content.height,
        25.0,
        "BFC height (15 + 10)",
    );
}

#[test]
fn test_float_context_isolated_per_formatting_context() {
    // A float inside an inner BFC must not affect blocks outside it.
    let (mut doc, mut styles, container) = setup_container();
    let inner = make_div(&mut doc, container);
    let float_box = make_div(&mut doc, inner);
    styles.insert(float_box, make_float_style(Float::Left, 50.0, 30.0));
    let mut inner_style = make_block_style(Length::Px(100.0), Length::Auto);
    inner_style.overflow = Overflow::Hidden;
    styles.insert(inner, inner_style);
    let sibling = make_div(&mut doc, container);
    styles.insert(sibling, make_block_style(Length::Auto, Length::Px(10.0)));

    let tree = layout_doc(&doc, &styles);
    let container_box = &tree.children[0].children[0];
    let inner_box = &container_box.children[0];
    let sibling_box = &container_box.children[1];
    // The inner BFC (overflow: hidden) encloses its float: 30px tall.
    assert_close(
        inner_box.dimensions.content.height,
        30.0,
        "inner BFC height",
    );
    // The next sibling starts below the inner BFC and is not narrowed by its float.
    assert_close(sibling_box.dimensions.content.y, 30.0, "sibling y");
    assert_close(
        sibling_box.dimensions.content.x,
        0.0,
        "float isolated to inner BFC",
    );
    assert_close(
        sibling_box.dimensions.content.width,
        200.0,
        "sibling not shortened",
    );
}

#[test]
fn test_float_with_a_nan_margin_terminates() {
    // A NaN margin made the fit test false forever while `next_top` stayed NaN: the
    // placement loop never made progress (true hang).
    let (mut doc, mut styles, container) = setup_container();
    let float_box = make_div(&mut doc, container);
    let mut float_style = make_float_style(Float::Left, 50.0, 30.0);
    float_style.margin.top = Length::Px(f32::NAN);
    styles.insert(float_box, float_style);

    let tree = layout_doc(&doc, &styles);
    let float_layout = &tree.children[0].children[0].children[0];
    assert!(
        float_layout.dimensions.content.y.is_finite(),
        "a NaN-sized float must still be placed"
    );
}

#[test]
fn test_over_wide_float_is_placed_at_the_flow_position() {
    // 300px float in a 200px container: it can never fit next to anything, so it must be
    // placed at the flow position and overflow instead of moving down for ~2^24 steps.
    let (mut doc, mut styles, container) = setup_container();
    let float_box = make_div(&mut doc, container);
    let inner = make_div(&mut doc, float_box);
    styles.insert(float_box, make_float_style(Float::Left, 300.0, 30.0));
    styles.insert(inner, make_block_style(Length::Px(120.0), Length::Px(20.0)));

    let tree = layout_doc(&doc, &styles);
    let float_layout = &tree.children[0].children[0].children[0];
    assert_close(float_layout.dimensions.content.y, 0.0, "flow position");
    assert_eq!(
        float_layout.children.len(),
        1,
        "the over-wide float's contents are laid out"
    );
}
