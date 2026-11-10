//! # Positioning layout conformance tests (CSS 2.2 §9.3, §9.6, §9.7, §10.3.7, App. E)
//!
//! Covers WP-4.6: `position: relative|absolute|fixed|static`, containing blocks,
//! static positions, shrink-to-fit widths, and the stacking/clip data contract consumed
//! by paint (`position`, `z_index`, `opacity`, `establishing_stacking_context`, `clip`).

use std::collections::HashMap;

use css::{ComputedStyle, Display, Length, Position};
use dom::{Document, Namespace, NodeId};
use layout::{LayoutBox, Rect, layout_document};

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

fn layout_doc(doc: &Document, styles: &HashMap<NodeId, ComputedStyle>) -> LayoutBox {
    layout_document(doc, styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout must succeed")
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 0.001,
        "{what}: expected {expected}, got {actual}"
    );
}

// ---------------------------------------------------------------------------
// position: relative (CSS 2.2 §9.7)
// ---------------------------------------------------------------------------

#[test]
fn test_relative_offset_does_not_affect_siblings() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let div1 = make_div(&mut doc, body);
    let div2 = make_div(&mut doc, body);
    let div3 = make_div(&mut doc, body);

    styles.insert(div1, make_block_style(Length::Auto, Length::Px(10.0)));
    let mut relative = make_block_style(Length::Auto, Length::Px(10.0));
    relative.position = Position::Relative;
    relative.left = Length::Px(20.0);
    relative.top = Length::Px(5.0);
    styles.insert(div2, relative);
    styles.insert(div3, make_block_style(Length::Auto, Length::Px(10.0)));

    let tree = layout_doc(&doc, &styles);
    let body_box = &tree.children[0];
    let box1 = &body_box.children[0];
    let box2 = &body_box.children[1];
    let box3 = &body_box.children[2];

    // Normal flow positions are untouched: the relative box keeps its flow slot.
    assert_close(box1.dimensions.content.y, 0.0, "div1 y");
    assert_close(
        box2.dimensions.content.y,
        15.0,
        "div2 y (10 flow + 5 offset)",
    );
    assert_close(
        box3.dimensions.content.y,
        20.0,
        "div3 y (sibling not displaced)",
    );
    assert_close(box2.dimensions.content.x, 20.0, "div2 x offset");
}

#[test]
fn test_relative_offset_moves_whole_subtree() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let wrapper = make_div(&mut doc, body);
    let child = make_div(&mut doc, wrapper);

    let mut wrapper_style = make_block_style(Length::Auto, Length::Px(50.0));
    wrapper_style.position = Position::Relative;
    wrapper_style.left = Length::Px(7.0);
    wrapper_style.top = Length::Px(3.0);
    styles.insert(wrapper, wrapper_style);
    styles.insert(child, make_block_style(Length::Px(20.0), Length::Px(10.0)));

    let tree = layout_doc(&doc, &styles);
    let wrapper_box = &tree.children[0].children[0];
    let child_box = &wrapper_box.children[0];
    assert_close(wrapper_box.dimensions.content.x, 7.0, "wrapper x");
    assert_close(wrapper_box.dimensions.content.y, 3.0, "wrapper y");
    assert_close(child_box.dimensions.content.x, 7.0, "child x translated");
    assert_close(child_box.dimensions.content.y, 3.0, "child y translated");
}

// ---------------------------------------------------------------------------
// position: absolute / fixed (CSS 2.2 §9.6, §10.1)
// ---------------------------------------------------------------------------

#[test]
fn test_absolute_uses_positioned_ancestor_containing_block() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let wrapper = make_div(&mut doc, body);
    let absolute = make_div(&mut doc, wrapper);

    let mut wrapper_style = make_block_style(Length::Px(300.0), Length::Px(200.0));
    wrapper_style.position = Position::Relative;
    wrapper_style.padding = css::RectEdges::all(Length::Px(10.0));
    styles.insert(wrapper, wrapper_style);

    let mut abs_style = make_block_style(Length::Px(50.0), Length::Px(25.0));
    abs_style.position = Position::Absolute;
    abs_style.left = Length::Px(30.0);
    abs_style.top = Length::Px(40.0);
    styles.insert(absolute, abs_style);

    let tree = layout_doc(&doc, &styles);
    let wrapper_box = &tree.children[0].children[0];
    // The absolute containing block is the wrapper's *padding box* (origin 0,0 here).
    let abs_box = &wrapper_box.children[0];
    assert_close(abs_box.dimensions.content.x, 30.0, "absolute x");
    assert_close(abs_box.dimensions.content.y, 40.0, "absolute y");
    assert_close(abs_box.dimensions.content.width, 50.0, "absolute width");
    assert_close(abs_box.dimensions.content.height, 25.0, "absolute height");
    // Out of flow: it does not change the wrapper's height.
    assert_close(
        wrapper_box.dimensions.content.height,
        200.0,
        "wrapper height",
    );
}

#[test]
fn test_absolute_without_positioned_ancestor_uses_icb() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let absolute = make_div(&mut doc, body);
    let mut abs_style = make_block_style(Length::Px(10.0), Length::Px(10.0));
    abs_style.position = Position::Absolute;
    abs_style.left = Length::Px(10.0);
    abs_style.top = Length::Px(20.0);
    styles.insert(absolute, abs_style);

    let tree = layout_doc(&doc, &styles);
    // `body` is static, so the containing block is the initial containing block.
    let abs_box = &tree.children[0].children[0];
    assert_close(abs_box.dimensions.content.x, 10.0, "absolute x against ICB");
    assert_close(abs_box.dimensions.content.y, 20.0, "absolute y against ICB");
}

#[test]
fn test_fixed_uses_viewport_even_inside_positioned_ancestor() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let wrapper = make_div(&mut doc, body);
    let fixed = make_div(&mut doc, wrapper);

    let mut wrapper_style = make_block_style(Length::Px(300.0), Length::Px(200.0));
    wrapper_style.position = Position::Relative;
    wrapper_style.margin.left = Length::Px(100.0);
    wrapper_style.margin.top = Length::Px(50.0);
    styles.insert(wrapper, wrapper_style);

    let mut fixed_style = make_block_style(Length::Px(10.0), Length::Px(10.0));
    fixed_style.position = Position::Fixed;
    fixed_style.left = Length::Px(5.0);
    fixed_style.top = Length::Px(6.0);
    styles.insert(fixed, fixed_style);

    let tree = layout_doc(&doc, &styles);
    let wrapper_box = &tree.children[0].children[0];
    let fixed_box = &wrapper_box.children[0];
    assert_close(
        fixed_box.dimensions.content.x,
        5.0,
        "fixed x against viewport",
    );
    assert_close(
        fixed_box.dimensions.content.y,
        6.0,
        "fixed y against viewport",
    );
}

#[test]
fn test_absolute_auto_offsets_use_static_position() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let div1 = make_div(&mut doc, body);
    let absolute = make_div(&mut doc, body);
    styles.insert(div1, make_block_style(Length::Auto, Length::Px(10.0)));

    let mut abs_style = make_block_style(Length::Px(20.0), Length::Px(5.0));
    abs_style.position = Position::Absolute;
    // left/top stay auto: the static position is where the box would have been.
    styles.insert(absolute, abs_style);

    let tree = layout_doc(&doc, &styles);
    let abs_box = &tree.children[0].children[1];
    assert_close(abs_box.dimensions.content.x, 0.0, "static x");
    assert_close(abs_box.dimensions.content.y, 10.0, "static y (after div1)");
}

#[test]
fn test_absolute_shrink_to_fit_width() {
    // Absolutely positioned box with `width: auto` shrinks to its text content.
    // DefaultTextMeasurer: proportional font advance = 0.5em = 8px at 16px font-size.
    let (mut doc, mut styles, _html, body) = setup_document();
    let absolute = make_div(&mut doc, body);
    doc.append_text(absolute, "Hello");

    let mut abs_style = make_block_style(Length::Auto, Length::Auto);
    abs_style.position = Position::Absolute;
    abs_style.left = Length::Px(0.0);
    abs_style.top = Length::Px(0.0);
    abs_style.font_size = 16.0;
    styles.insert(absolute, abs_style);

    let tree = layout_doc(&doc, &styles);
    let abs_box = &tree.children[0].children[0];
    assert_close(
        abs_box.dimensions.content.width,
        40.0,
        "shrink-to-fit width",
    );
}

#[test]
fn test_right_and_bottom_offsets_position_absolute_box() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let absolute = make_div(&mut doc, body);
    let mut abs_style = make_block_style(Length::Px(100.0), Length::Px(50.0));
    abs_style.position = Position::Absolute;
    abs_style.right = Length::Px(20.0);
    abs_style.bottom = Length::Px(30.0);
    styles.insert(absolute, abs_style);

    let tree = layout_doc(&doc, &styles);
    let abs_box = &tree.children[0].children[0];
    // ICB is 800x600: right 20 => x = 800 - 20 - 100 = 680; bottom 30 => y = 600 - 30 - 50 = 520.
    assert_close(abs_box.dimensions.content.x, 680.0, "right-offset x");
    assert_close(abs_box.dimensions.content.y, 520.0, "bottom-offset y");
}

// ---------------------------------------------------------------------------
// Stacking context and clip data contract (CSS 2.2 Appendix E, §11)
// ---------------------------------------------------------------------------

#[test]
fn test_stacking_context_flags_and_z_index() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let static_box = make_div(&mut doc, body);
    let relative_z = make_div(&mut doc, body);
    let relative_auto = make_div(&mut doc, body);
    let transparent = make_div(&mut doc, body);

    styles.insert(static_box, make_block_style(Length::Auto, Length::Px(5.0)));
    let mut relative_z_style = make_block_style(Length::Auto, Length::Px(5.0));
    relative_z_style.position = Position::Relative;
    relative_z_style.z_index = Some(3);
    styles.insert(relative_z, relative_z_style);
    let mut relative_auto_style = make_block_style(Length::Auto, Length::Px(5.0));
    relative_auto_style.position = Position::Relative;
    styles.insert(relative_auto, relative_auto_style);
    let mut transparent_style = make_block_style(Length::Auto, Length::Px(5.0));
    transparent_style.opacity = 0.5;
    styles.insert(transparent, transparent_style);

    let tree = layout_doc(&doc, &styles);
    // The root element always establishes the root stacking context.
    assert!(tree.establishes_stacking_context, "root stacking context");
    let body_box = &tree.children[0];
    assert!(
        !body_box.children[0].establishes_stacking_context,
        "static box"
    );
    assert_eq!(
        body_box.children[0].z_index, 0,
        "z-index ignored on static boxes"
    );
    assert!(
        body_box.children[1].establishes_stacking_context,
        "positioned with z-index establishes a stacking context"
    );
    assert_eq!(body_box.children[1].z_index, 3);
    assert!(
        !body_box.children[2].establishes_stacking_context,
        "positioned with z-index:auto does not establish one"
    );
    assert!(
        body_box.children[3].establishes_stacking_context,
        "opacity < 1 establishes a stacking context"
    );
    assert_close(body_box.children[3].opacity, 0.5, "opacity propagated");
}

#[test]
fn test_opacity_and_fixed_produce_stacking_context() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let fixed = make_div(&mut doc, body);
    let mut fixed_style = make_block_style(Length::Px(10.0), Length::Px(10.0));
    fixed_style.position = Position::Fixed;
    styles.insert(fixed, fixed_style);

    let tree = layout_doc(&doc, &styles);
    let fixed_box = &tree.children[0].children[0];
    assert!(
        fixed_box.establishes_stacking_context,
        "fixed box stacking context"
    );
    assert_eq!(fixed_box.position, Position::Fixed);
}
