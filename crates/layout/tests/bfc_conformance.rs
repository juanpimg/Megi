//! # BFC & Box Model Conformance Tests (CSS 2.2 §8.3.1, §9.2.1.1, §10.1–§10.7)
//!
//! Regression and conformance coverage for audit finding F-02 (anonymous boxes must not
//! inherit non-inherited properties), `<br>` forced breaks, collapsing margins, the
//! initial containing block height, `box-sizing: border-box`, min/max sizes and
//! percentage heights.
//!
//! The DOM is built by hand (as in the rest of the layout test suites) so the tests are
//! independent from the HTML parser and the CSS cascade.

use std::collections::HashMap;

use css::{BoxSizing, Color, ComputedStyle, Display, Length, RectEdges};
use dom::{Document, Namespace, NodeId};
use layout::{BoxType, Rect, layout_document};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_block_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style.height = height;
    style
}

fn make_inline_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Inline;
    style
}

fn margin_all(px: f32) -> Length {
    Length::Px(px)
}

/// Creates a `Document` with `<html><body>` and zeroed UA styles, returning the ids.
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

/// Creates a `Document` with a single `<html>` root (no `<body>`), so tests can put
/// their own block structure directly under the root element.
fn setup_html_only() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    (doc, styles, html)
}

fn make_div(doc: &mut Document, parent: NodeId) -> NodeId {
    let id = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(parent, id);
    id
}

fn make_br(doc: &mut Document, parent: NodeId) -> NodeId {
    let id = doc.create_element(Namespace::Html, "br", vec![]);
    doc.append_child(parent, id);
    id
}

fn make_p(doc: &mut Document, parent: NodeId) -> NodeId {
    let id = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(parent, id);
    id
}

fn layout_doc(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    viewport: Rect,
) -> layout::LayoutBox {
    layout_document(doc, styles, viewport).expect("layout must succeed")
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 0.001,
        "{what}: expected {expected}, got {actual}"
    );
}

// ---------------------------------------------------------------------------
// F-02: anonymous boxes / text nodes must only carry inherited properties
// ---------------------------------------------------------------------------

#[test]
fn test_anonymous_blocks_do_not_inherit_non_inherited_properties() {
    // body has a UA-like 8px margin, a red background and a green 20px font.
    // Its mixed content (text + div) forces an AnonymousBlock wrapper around the text.
    let (mut doc, mut styles, _html, body) = setup_document();
    doc.append_text(body, "Hello");
    let div = make_div(&mut doc, body);

    {
        let body_style = styles.get_mut(&body).expect("body style");
        body_style.margin = RectEdges::all(Length::Px(8.0));
        body_style.background_color = Color::RED;
        body_style.color = Color::GREEN;
        body_style.font_size = 20.0;
    }
    styles.insert(div, make_block_style(Length::Auto, Length::Auto));

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let body_box = &tree.children[0];
    let anon = &body_box.children[0];

    // Mixed inline/block content is wrapped in an anonymous block...
    assert_eq!(anon.box_type, BoxType::AnonymousBlock);
    // ...which must have initial (zero) margins/paddings/borders and no background.
    assert_eq!(anon.dimensions.margin, layout::EdgeSizes::ZERO);
    assert_eq!(anon.dimensions.padding, layout::EdgeSizes::ZERO);
    assert_eq!(anon.dimensions.border, layout::EdgeSizes::ZERO);
    let anon_style = anon.style.as_ref().expect("anonymous style");
    assert_eq!(anon_style.background_color, Color::TRANSPARENT);
    // Inherited properties are still inherited from the parent.
    assert_eq!(anon_style.color, Color::GREEN);
    assert_close(anon_style.font_size, 20.0, "anonymous inherited font-size");

    // The anonymous block spans exactly the body content box: no double indentation.
    assert_close(anon.dimensions.content.x, 8.0, "anonymous block x");
    assert_close(
        anon.dimensions.content.width,
        784.0,
        "anonymous block width",
    );

    // The text line starts at the body content edge, not indented twice.
    let line_box = &anon.children[0];
    assert_eq!(line_box.box_type, BoxType::LineBox);
    assert_close(line_box.dimensions.content.x, 8.0, "line box x");
    assert_close(line_box.dimensions.content.width, 784.0, "line box width");

    // Text boxes are formatting artifacts: inherited font/colour only, no decoration.
    let text_box = &line_box.children[0];
    assert!(matches!(text_box.box_type, BoxType::TextNode(_)));
    let text_style = text_box.style.as_ref().expect("text style");
    assert_eq!(text_style.background_color, Color::TRANSPARENT);
    assert_eq!(text_style.color, Color::GREEN);
    assert_close(text_style.font_size, 20.0, "text font-size");
    assert_eq!(text_box.dimensions.margin, layout::EdgeSizes::ZERO);
}

#[test]
fn test_pretty_printed_blocks_have_no_phantom_vertical_gap() {
    // Pretty-printed HTML: whitespace-only text nodes between block-level siblings.
    let (mut doc, mut styles, _html, body) = setup_document();
    let div1 = make_div(&mut doc, body);
    doc.append_text(body, "\n  ");
    let div2 = make_div(&mut doc, body);
    doc.append_text(body, "\n\t");

    styles.get_mut(&body).expect("body style").margin = RectEdges::all(margin_all(8.0));
    for div in [div1, div2] {
        styles.insert(div, make_block_style(Length::Auto, Length::Px(10.0)));
    }

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let body_box = &tree.children[0];

    // The whitespace-only inline runs are insignificant between blocks.
    assert_eq!(
        body_box.children.len(),
        2,
        "whitespace runs must not create boxes"
    );
    let box1 = &body_box.children[0];
    let box2 = &body_box.children[1];

    // body margin-top 8: content starts at y = 8, the first div is flush with it.
    assert_close(box1.dimensions.content.y, 8.0, "first block y");
    // Second div stacks immediately below the first: 8 + 10 = 18 (no phantom gap).
    assert_close(box2.dimensions.content.y, 18.0, "second block y");
    assert_close(body_box.dimensions.content.height, 20.0, "body height");
    assert_close(
        tree.dimensions.content.height,
        36.0,
        "html height (body margin box)",
    );

    // Both blocks span the body content box: 800 - 2*8 = 784.
    assert_close(box1.dimensions.content.x, 8.0, "first block x");
    assert_close(box1.dimensions.content.width, 784.0, "first block width");
}

// ---------------------------------------------------------------------------
// CSS 2.2 §8.3.1: collapsing margins
// ---------------------------------------------------------------------------

#[test]
fn test_sibling_margins_collapse_to_max() {
    let (mut doc, mut styles, html) = setup_html_only();
    // html > wrapper > div1, div2 (wrapper isolates the test from the root element).
    let wrapper = make_div(&mut doc, html);
    let div1 = make_div(&mut doc, wrapper);
    let div2 = make_div(&mut doc, wrapper);

    let mut s1 = make_block_style(Length::Auto, Length::Px(10.0));
    s1.margin.bottom = Length::Px(20.0);
    let mut s2 = make_block_style(Length::Auto, Length::Px(10.0));
    s2.margin.top = Length::Px(20.0);
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Auto));
    styles.insert(div1, s1);
    styles.insert(div2, s2);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    let box1 = &wrapper_box.children[0];
    let box2 = &wrapper_box.children[1];

    assert_close(box1.dimensions.content.y, 0.0, "first sibling y");
    // max(20, 20) == 20, not 40.
    assert_close(box2.dimensions.content.y, 30.0, "second sibling y");
    let separation =
        box2.dimensions.content.y - (box1.dimensions.content.y + box1.dimensions.content.height);
    assert_close(separation, 20.0, "collapsed sibling separation");
    // Auto height of the wrapper: last child's border box (no bottom margin escape).
    assert_close(
        wrapper_box.dimensions.content.height,
        40.0,
        "wrapper height",
    );
}

#[test]
fn test_parent_child_top_margins_collapse() {
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);

    let mut wrapper_style = make_block_style(Length::Auto, Length::Auto);
    wrapper_style.margin.top = Length::Px(10.0);
    let mut child_style = make_block_style(Length::Auto, Length::Px(10.0));
    child_style.margin.top = Length::Px(20.0);
    styles.insert(wrapper, wrapper_style);
    styles.insert(child, child_style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    let child_box = &wrapper_box.children[0];

    // collapsed margin = max(10, 20) = 20 applied above the wrapper; the child starts at
    // the wrapper's content top (its own 20px margin does not apply twice).
    assert_close(wrapper_box.dimensions.content.y, 20.0, "wrapper y");
    assert_close(child_box.dimensions.content.y, 20.0, "child y");
    assert_close(
        wrapper_box.dimensions.content.height,
        10.0,
        "wrapper height",
    );
}

#[test]
fn test_parent_padding_prevents_top_margin_collapse() {
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);

    let mut wrapper_style = make_block_style(Length::Auto, Length::Auto);
    wrapper_style.margin.top = Length::Px(10.0);
    wrapper_style.padding.top = Length::Px(5.0);
    let mut child_style = make_block_style(Length::Auto, Length::Px(10.0));
    child_style.margin.top = Length::Px(20.0);
    styles.insert(wrapper, wrapper_style);
    styles.insert(child, child_style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    let child_box = &wrapper_box.children[0];

    // The wrapper's padding blocks collapsing: 10px above, then 5px padding, then the
    // child's own 20px margin inside.
    assert_close(
        wrapper_box.dimensions.content.y,
        15.0,
        "wrapper content y (10 margin + 5 padding)",
    );
    assert_close(child_box.dimensions.content.y, 15.0 + 20.0, "child y");
}

#[test]
fn test_negative_margins_collapse_max_positive_plus_min_negative() {
    // Pair 1: +20 and -10 => 10 (max positive + min negative).
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let div1 = make_div(&mut doc, wrapper);
    let div2 = make_div(&mut doc, wrapper);

    let mut s1 = make_block_style(Length::Auto, Length::Px(10.0));
    s1.margin.bottom = Length::Px(-10.0);
    let mut s2 = make_block_style(Length::Auto, Length::Px(10.0));
    s2.margin.top = Length::Px(20.0);
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Auto));
    styles.insert(div1, s1);
    styles.insert(div2, s2);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    assert_close(
        wrapper_box.children[1].dimensions.content.y,
        20.0,
        "mixed-sign collapsed margin (+20, -10)",
    );

    // Pair 2: -5 and -10 => -10 (most negative when all margins are negative).
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let div1 = make_div(&mut doc, wrapper);
    let div2 = make_div(&mut doc, wrapper);

    let mut s1 = make_block_style(Length::Auto, Length::Px(10.0));
    s1.margin.bottom = Length::Px(-5.0);
    let mut s2 = make_block_style(Length::Auto, Length::Px(10.0));
    s2.margin.top = Length::Px(-10.0);
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Auto));
    styles.insert(div1, s1);
    styles.insert(div2, s2);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    assert_close(
        wrapper_box.children[1].dimensions.content.y,
        0.0,
        "negative collapsed margin (-5, -10)",
    );
}

#[test]
fn test_bottom_margin_does_not_collapse_through_padding_or_border() {
    // Parent without bottom padding: the last child's bottom margin escapes and the
    // parent's auto height excludes it.
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);
    let sibling = make_div(&mut doc, html);

    let mut child_style = make_block_style(Length::Auto, Length::Px(10.0));
    child_style.margin.bottom = Length::Px(30.0);
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Auto));
    styles.insert(child, child_style);
    styles.insert(sibling, make_block_style(Length::Auto, Length::Px(5.0)));

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    let sibling_box = &tree.children[1];
    assert_close(
        wrapper_box.dimensions.content.height,
        10.0,
        "wrapper height",
    );
    // The escaped 30px margin separates the wrapper from the next sibling.
    assert_close(sibling_box.dimensions.content.y, 40.0, "sibling y");

    // Parent with bottom padding: no collapsing, the margin is part of the parent height.
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);
    let sibling = make_div(&mut doc, html);

    let mut wrapper_style = make_block_style(Length::Auto, Length::Auto);
    wrapper_style.padding.bottom = Length::Px(5.0);
    let mut child_style = make_block_style(Length::Auto, Length::Px(10.0));
    child_style.margin.bottom = Length::Px(30.0);
    styles.insert(wrapper, wrapper_style);
    styles.insert(child, child_style);
    styles.insert(sibling, make_block_style(Length::Auto, Length::Px(5.0)));

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    let sibling_box = &tree.children[1];
    assert_close(
        wrapper_box.dimensions.content.height,
        40.0,
        "wrapper height includes child margin (10 + 30)",
    );
    // 10 content + 30 margin + 5 padding = 45.
    assert_close(sibling_box.dimensions.content.y, 45.0, "sibling y");
}

// ---------------------------------------------------------------------------
// CSS 2.2 §10.1 / §10.5: initial containing block and percentage heights
// ---------------------------------------------------------------------------

#[test]
fn test_initial_containing_block_uses_viewport_height() {
    let (doc, mut styles, html, body) = setup_document();
    styles.get_mut(&html).expect("html style").height = Length::Percent(100.0);
    styles.get_mut(&body).expect("body style").height = Length::Percent(100.0);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    // html: 100% of the ICB (viewport) height.
    assert_close(
        tree.dimensions.content.height,
        600.0,
        "root percentage height",
    );
    // body: 100% of html's definite content height.
    assert_close(
        tree.children[0].dimensions.content.height,
        600.0,
        "body percentage height",
    );
}

#[test]
fn test_percentage_height_resolves_against_fixed_parent() {
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);

    let mut child_style = make_block_style(Length::Auto, Length::Percent(50.0));
    child_style.display = Display::Block;
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Px(300.0)));
    styles.insert(child, child_style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    assert_close(
        wrapper_box.dimensions.content.height,
        300.0,
        "wrapper height",
    );
    assert_close(
        wrapper_box.children[0].dimensions.content.height,
        150.0,
        "50% height resolves against 300px parent",
    );
}

#[test]
fn test_percentage_height_against_auto_parent_is_auto() {
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Auto));
    styles.insert(child, make_block_style(Length::Auto, Length::Percent(50.0)));

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    // The parent height is auto, so the percentage behaves as auto; the empty child has
    // an intrinsic height of 0.
    assert_close(
        wrapper_box.children[0].dimensions.content.height,
        0.0,
        "percentage height against auto parent",
    );
    assert_close(
        wrapper_box.dimensions.content.height,
        0.0,
        "auto wrapper height",
    );
}

// ---------------------------------------------------------------------------
// CSS Sizing 3: box-sizing, min/max sizes and over-constrained equations
// ---------------------------------------------------------------------------

#[test]
fn test_box_sizing_border_box_reduces_content_box() {
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Px(200.0), Length::Px(100.0));
    style.box_sizing = BoxSizing::BorderBox;
    style.padding = RectEdges::all(Length::Px(10.0));
    style.border_width = RectEdges::all(5.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let box_ = &tree.children[0];
    // width: 200px includes padding (2*10) and border (2*5).
    assert_close(
        box_.dimensions.content.width,
        170.0,
        "border-box content width",
    );
    assert_close(
        box_.dimensions.border_box().width,
        200.0,
        "border-box width",
    );
    assert_close(
        box_.dimensions.content.height,
        70.0,
        "border-box content height",
    );
    assert_close(
        box_.dimensions.border_box().height,
        100.0,
        "border-box height",
    );

    // content-box keeps the width as the content width (CSS Box Model default).
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Px(200.0), Length::Px(100.0));
    style.padding = RectEdges::all(Length::Px(10.0));
    style.border_width = RectEdges::all(5.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let box_ = &tree.children[0];
    assert_close(
        box_.dimensions.content.width,
        200.0,
        "content-box content width",
    );
    assert_close(
        box_.dimensions.border_box().width,
        230.0,
        "content-box border-box width",
    );
}

#[test]
fn test_min_max_width_clamp_used_width() {
    // CSS 2.2 §10.4: min-width wins over max-width when max < min.
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Auto, Length::Auto);
    style.min_width = Length::Px(500.0);
    style.max_width = Length::Px(100.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_close(
        tree.children[0].dimensions.content.width,
        500.0,
        "min-width wins",
    );

    // width: 300px clamped by max-width: 100px.
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Px(300.0), Length::Auto);
    style.max_width = Length::Px(100.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_close(
        tree.children[0].dimensions.content.width,
        100.0,
        "max-width clamps",
    );

    // width: 100px clamped by min-width: 500px.
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Px(100.0), Length::Auto);
    style.min_width = Length::Px(500.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_close(
        tree.children[0].dimensions.content.width,
        500.0,
        "min-width clamps",
    );
}

#[test]
fn test_max_width_with_auto_margins_centers() {
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Auto, Length::Auto);
    style.max_width = Length::Px(200.0);
    style.margin.left = Length::Auto;
    style.margin.right = Length::Auto;
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let box_ = &tree.children[0];
    assert_close(box_.dimensions.content.width, 200.0, "clamped width");
    assert_close(box_.dimensions.margin.left, 300.0, "auto left margin");
    assert_close(box_.dimensions.margin.right, 300.0, "auto right margin");
    assert_close(box_.dimensions.content.x, 300.0, "centered x");
}

#[test]
fn test_min_max_height_clamp_used_height() {
    // height: 10px raised by min-height: 50px.
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Auto, Length::Px(10.0));
    style.min_height = Length::Px(50.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_close(
        tree.children[0].dimensions.content.height,
        50.0,
        "min-height",
    );

    // Auto height of a 100px-tall child clamped by max-height: 30px.
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);
    let mut wrapper_style = make_block_style(Length::Auto, Length::Auto);
    wrapper_style.max_height = Length::Px(30.0);
    styles.insert(wrapper, wrapper_style);
    styles.insert(child, make_block_style(Length::Auto, Length::Px(100.0)));

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_close(
        tree.children[0].dimensions.content.height,
        30.0,
        "auto height clamped by max-height",
    );

    // border-box min-height constrains the border box.
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Auto, Length::Px(10.0));
    style.box_sizing = BoxSizing::BorderBox;
    style.padding = RectEdges::all(Length::Px(5.0));
    style.min_height = Length::Px(40.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let box_ = &tree.children[0];
    assert_close(
        box_.dimensions.content.height,
        30.0,
        "border-box min-height content",
    );
    assert_close(
        box_.dimensions.border_box().height,
        40.0,
        "border-box min-height border box",
    );
}

#[test]
fn test_overconstrained_ltr_ignores_margin_right() {
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Px(400.0), Length::Auto);
    style.margin.left = Length::Px(20.0);
    style.margin.right = Length::Px(100.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let box_ = &tree.children[0];
    // 20 + 400 + margin-right = 800 => margin-right is solved as 380 (LTR).
    assert_close(box_.dimensions.margin.left, 20.0, "margin-left");
    assert_close(box_.dimensions.margin.right, 380.0, "margin-right solved");
    assert_close(box_.dimensions.content.x, 20.0, "content x");
    assert_close(box_.dimensions.content.width, 400.0, "content width");
}

// ---------------------------------------------------------------------------
// `<br>` forced line breaks (CSS 2.2 §16.6.1)
// ---------------------------------------------------------------------------

#[test]
fn test_br_generates_forced_line_break() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_p(&mut doc, body);
    doc.append_text(p, "line1");
    let br = make_br(&mut doc, p);
    doc.append_text(p, "line2");

    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);
    styles.insert(p, p_style);
    styles.insert(br, make_inline_style());

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let p_box = &tree.children[0].children[0];

    // `<br>` creates a second line box instead of an invisible inline box.
    assert_eq!(
        p_box.children.len(),
        2,
        "br must split the paragraph in two lines"
    );
    let first_line = &p_box.children[0];
    let second_line = &p_box.children[1];
    assert_close(
        first_line.dimensions.content.height,
        20.0,
        "first line height",
    );
    assert_close(first_line.dimensions.content.y, 0.0, "first line y");
    assert_close(
        second_line.dimensions.content.height,
        20.0,
        "second line height",
    );
    assert_close(second_line.dimensions.content.y, 20.0, "second line y");
    assert_close(p_box.dimensions.content.height, 40.0, "paragraph height");

    let first_text = first_line
        .children
        .iter()
        .find_map(|child| match &child.box_type {
            BoxType::TextNode(text) => Some(text.as_str()),
            _ => None,
        })
        .expect("first line text");
    assert_eq!(first_text, "line1");
    let second_text = second_line
        .children
        .iter()
        .find_map(|child| match &child.box_type {
            BoxType::TextNode(text) => Some(text.as_str()),
            _ => None,
        })
        .expect("second line text");
    assert_eq!(second_text, "line2");

    // `<br>` never generates an element box in the layout tree.
    assert!(
        !p_box.children.iter().any(|line| line
            .children
            .iter()
            .any(|child| matches!(child.box_type, BoxType::InlineNode(_)))),
        "br must not produce an inline element box"
    );
}

#[test]
fn test_br_inside_inline_element_forces_line_break() {
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_p(&mut doc, body);
    let span = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(p, span);
    doc.append_text(span, "alpha");
    let br = make_br(&mut doc, span);
    doc.append_text(span, "beta");

    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);
    styles.insert(p, p_style);
    let mut span_style = make_inline_style();
    span_style.font_size = 10.0;
    span_style.line_height = Some(20.0);
    styles.insert(span, span_style);
    styles.insert(br, make_inline_style());

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let p_box = &tree.children[0].children[0];
    assert_eq!(
        p_box.children.len(),
        2,
        "br inside span must split the line"
    );
    // Both line fragments keep the inline box open/closed around the break.
    assert_eq!(p_box.children[0].box_type, BoxType::LineBox);
    assert_eq!(p_box.children[1].box_type, BoxType::LineBox);
    assert_close(p_box.dimensions.content.height, 40.0, "paragraph height");
}

#[test]
fn test_nbsp_text_is_not_treated_as_collapsible_whitespace() {
    // Only ASCII collapsible whitespace may be discarded between blocks: a text node made
    // of NO-BREAK SPACE is visible content and must generate an anonymous block.
    let (mut doc, mut styles, _html, body) = setup_document();
    let div = make_div(&mut doc, body);
    doc.append_text(body, "\u{00A0}");

    styles.insert(div, make_block_style(Length::Auto, Length::Px(10.0)));
    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let body_box = &tree.children[0];
    assert_eq!(
        body_box.children.len(),
        2,
        "nbsp text must keep its anonymous block"
    );
    assert_eq!(body_box.children[1].box_type, BoxType::AnonymousBlock);
    assert!(
        !body_box.children[1].children.is_empty(),
        "nbsp text must generate a line box"
    );
}
