//! # Integration Tests for Inline Formatting Context (IFC) and Line Breaking

use std::collections::HashMap;

use css::{ComputedStyle, Display, Length, TextAlign, VerticalAlign, WhiteSpace};
use dom::{Document, Namespace};
use layout::{BoxType, Rect, layout_document};

fn make_block_style(width: Length, height: Length) -> ComputedStyle {
    let mut s = ComputedStyle::initial();
    s.display = Display::Block;
    s.width = width;
    s.height = height;
    s
}

fn make_inline_style() -> ComputedStyle {
    let mut s = ComputedStyle::initial();
    s.display = Display::Inline;
    s
}

#[test]
fn test_single_line_text_layout_exact_box_width_and_height() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_text(p, "Hello World"); // 11 characters

    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_size = 16.0;
    p_style.font_family = "sans-serif".to_string(); // proportional: 0.5 * 16 = 8px per char
    p_style.line_height = Some(20.0);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let body_box = &layout_tree.children[0];
    let p_box = &body_box.children[0];

    // Single line box generated
    assert_eq!(p_box.children.len(), 1);
    let line_box = &p_box.children[0];
    assert_eq!(line_box.box_type, BoxType::LineBox);
    assert_eq!(line_box.dimensions.content.width, 400.0);
    assert_eq!(line_box.dimensions.content.height, 20.0);

    // Text node inside: 11 chars * 8.0 = 88.0px
    assert_eq!(line_box.children.len(), 1);
    let text_box = &line_box.children[0];
    assert_eq!(text_box.dimensions.content.width, 88.0);

    // Auto-height of paragraph equals the line box height (20.0px)
    assert_eq!(p_box.dimensions.content.height, 20.0);
}

#[test]
fn test_multiline_text_wrapping_exceeding_container() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    // 3 words: 5 chars + 5 chars + 5 chars
    doc.append_text(p, "word1 word2 word3");

    let mut p_style = make_block_style(Length::Px(60.0), Length::Auto);
    p_style.font_family = "monospace".to_string(); // 0.6 * 10 = 6px per char
    p_style.font_size = 10.0;
    p_style.line_height = Some(15.0);
    // Container width = 60.0px: fits 10 chars per line (60px).
    // "word1" = 30px, " " = 6px, "word2" = 30px -> 66px > 60px! Must wrap!

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];

    // Must wrap into 3 line boxes: word1 on line 1, word2 on line 2, word3 on line 3
    assert_eq!(p_box.children.len(), 3);
    for line in &p_box.children {
        assert_eq!(line.box_type, BoxType::LineBox);
        assert_eq!(line.dimensions.content.height, 15.0);
    }

    // Total accumulated auto height = 3 * 15.0 = 45.0px
    assert_eq!(p_box.dimensions.content.height, 45.0);
}

#[test]
fn test_white_space_nowrap_and_pre() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p_nowrap = doc.create_element(Namespace::Html, "p", vec![]);
    let pre = doc.create_element(Namespace::Html, "pre", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p_nowrap);
    doc.append_child(body, pre);

    doc.append_text(p_nowrap, "First Second Third Fourth Fifth");
    doc.append_text(pre, "Line 1\nLine 2\nLine 3");

    let mut nowrap_style = make_block_style(Length::Px(50.0), Length::Auto);
    nowrap_style.white_space = WhiteSpace::Nowrap;
    nowrap_style.font_size = 16.0;
    nowrap_style.line_height = Some(20.0);

    let mut pre_style = make_block_style(Length::Px(400.0), Length::Auto);
    pre_style.white_space = WhiteSpace::Pre;
    pre_style.font_family = "monospace".to_string();
    pre_style.font_size = 10.0;
    pre_style.line_height = Some(15.0);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p_nowrap, nowrap_style);
    styles.insert(pre, pre_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let body_box = &layout_tree.children[0];
    let nowrap_box = &body_box.children[0];
    let pre_box = &body_box.children[1];

    // Under nowrap: exactly 1 LineBox despite width 50px
    assert_eq!(nowrap_box.children.len(), 1);

    // Under pre: exactly 3 LineBoxes from the 3 newline-separated lines
    assert_eq!(pre_box.children.len(), 3);
    assert_eq!(pre_box.dimensions.content.height, 45.0);
}

#[test]
fn test_inline_box_splitting_with_borders_and_padding() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, span);
    // Two words: "First Second"
    doc.append_text(span, "First Second");

    // Container width = 60px with 10px monospace chars (6px per char).
    // "First" = 30px + left border 5px + left padding 10px = 45px <= 60px.
    // "Second" cannot fit on line 1, so span splits across line 1 and line 2!
    let mut p_style = make_block_style(Length::Px(60.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let mut span_style = make_inline_style();
    span_style.font_family = "monospace".to_string();
    span_style.font_size = 10.0;
    span_style.border_width.left = 5.0;
    span_style.border_width.right = 8.0;
    span_style.padding.left = Length::Px(10.0);
    span_style.padding.right = Length::Px(12.0);
    span_style.margin.left = Length::Px(2.0);
    span_style.margin.right = Length::Px(4.0);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span, span_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];
    assert_eq!(p_box.children.len(), 2);

    let line1 = &p_box.children[0];
    let line2 = &p_box.children[1];

    let span_frag1 = &line1.children[0];
    let span_frag2 = &line2.children[0];

    assert_eq!(span_frag1.box_type, BoxType::InlineNode(span));
    assert_eq!(span_frag2.box_type, BoxType::InlineNode(span));

    // Fragment 1 (Line 1): Left border/padding/margin applied, right edges are 0.0!
    assert_eq!(span_frag1.dimensions.border.left, 5.0);
    assert_eq!(span_frag1.dimensions.padding.left, 10.0);
    assert_eq!(span_frag1.dimensions.margin.left, 2.0);
    assert_eq!(span_frag1.dimensions.border.right, 0.0);
    assert_eq!(span_frag1.dimensions.padding.right, 0.0);
    assert_eq!(span_frag1.dimensions.margin.right, 0.0);

    // Fragment 2 (Line 2): Left edges are 0.0, right border/padding/margin applied!
    assert_eq!(span_frag2.dimensions.border.left, 0.0);
    assert_eq!(span_frag2.dimensions.padding.left, 0.0);
    assert_eq!(span_frag2.dimensions.margin.left, 0.0);
    assert_eq!(span_frag2.dimensions.border.right, 8.0);
    assert_eq!(span_frag2.dimensions.padding.right, 12.0);
    assert_eq!(span_frag2.dimensions.margin.right, 4.0);
}

#[test]
fn test_text_align_center_and_right() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p_center = doc.create_element(Namespace::Html, "p", vec![]);
    let p_right = doc.create_element(Namespace::Html, "p", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p_center);
    doc.append_child(body, p_right);

    // 5 chars monospace: 5 * 6.0 = 30.0px width
    doc.append_text(p_center, "Hello");
    doc.append_text(p_right, "Hello");

    let mut center_style = make_block_style(Length::Px(200.0), Length::Auto);
    center_style.text_align = TextAlign::Center;
    center_style.font_family = "monospace".to_string();
    center_style.font_size = 10.0;
    center_style.line_height = Some(20.0);

    let mut right_style = make_block_style(Length::Px(200.0), Length::Auto);
    right_style.text_align = TextAlign::Right;
    right_style.font_family = "monospace".to_string();
    right_style.font_size = 10.0;
    right_style.line_height = Some(20.0);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p_center, center_style);
    styles.insert(p_right, right_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let body_box = &layout_tree.children[0];
    let center_box = &body_box.children[0];
    let right_box = &body_box.children[1];

    let center_text = &center_box.children[0].children[0];
    let right_text = &right_box.children[0].children[0];

    // Under center: remaining = 200 - 30 = 170 -> offset = 85.0px
    assert_eq!(center_text.dimensions.content.x, 85.0);

    // Under right: remaining = 200 - 30 = 170 -> offset = 170.0px
    assert_eq!(right_text.dimensions.content.x, 170.0);
}

#[test]
fn test_strut_minimum_height_and_empty_lines() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p_strut = doc.create_element(Namespace::Html, "p", vec![]);
    let span_tiny = doc.create_element(Namespace::Html, "span", vec![]);
    let p_empty = doc.create_element(Namespace::Html, "p", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p_strut);
    doc.append_child(p_strut, span_tiny);
    doc.append_text(span_tiny, "Hi");
    doc.append_child(body, p_empty);

    // Strut: font-size 20px, line-height 40px
    let mut strut_style = make_block_style(Length::Px(200.0), Length::Auto);
    strut_style.font_size = 20.0;
    strut_style.line_height = Some(40.0);

    // Child span: font-size 10px, line-height 10px
    let mut tiny_style = make_inline_style();
    tiny_style.font_size = 10.0;
    tiny_style.line_height = Some(10.0);

    let empty_style = make_block_style(Length::Px(200.0), Length::Auto);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p_strut, strut_style);
    styles.insert(span_tiny, tiny_style);
    styles.insert(p_empty, empty_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let body_box = &layout_tree.children[0];
    let strut_box = &body_box.children[0];
    let empty_box = &body_box.children[1];

    // Line height must be at least 40.0px guaranteed by the strut
    assert_eq!(strut_box.children[0].dimensions.content.height, 40.0);
    assert_eq!(strut_box.dimensions.content.height, 40.0);

    // Empty paragraph produces 0 height
    assert_eq!(empty_box.dimensions.content.height, 0.0);
}

#[test]
fn test_vertical_align_positioning() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span_raised = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_text(p, "Text ");
    doc.append_child(p, span_raised);
    doc.append_text(span_raised, "Raised");

    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_size = 16.0;
    p_style.line_height = Some(20.0);

    let mut raised_style = make_inline_style();
    raised_style.font_size = 16.0;
    raised_style.line_height = Some(20.0);
    raised_style.vertical_align = VerticalAlign::Length(10.0); // 10px shift upwards

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span_raised, raised_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];
    let line_box = &p_box.children[0];

    // Strut: font 16, line-height 20 -> ascent 14.8, descent 5.2.
    // Raised text shifted up by 10px -> ascent 24.8.
    // Total line height = 24.8 + 5.2 = 30.0px!
    assert_eq!(line_box.dimensions.content.height, 30.0);
    assert_eq!(p_box.dimensions.content.height, 30.0);
}
