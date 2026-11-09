//! Empirical Adversarial Test Harness for Milestone 1 IFC Layout
//!
//! Tests:
//! 1. Strut minimum height invariant (empty line boxes in pre, empty paragraphs, whitespace)
//! 2. Extreme vertical-align combinations (middle, sub, super, text-top, text-bottom, lengths)
//! 3. Multi-fragment inline box splitting across 3+ lines with nested elements
//! 4. text-align right & center with fractional widths, multiple lines, and styled spans

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
fn test_adversarial_strut_empty_line_boxes_in_pre() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let pre = doc.create_element(Namespace::Html, "pre", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, pre);
    // 3 lines: line1, empty line, line3
    doc.append_text(pre, "line1\n\nline3");

    let mut pre_style = make_block_style(Length::Px(400.0), Length::Auto);
    pre_style.white_space = WhiteSpace::Pre;
    pre_style.font_family = "monospace".to_string();
    pre_style.font_size = 10.0;
    pre_style.line_height = Some(20.0);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(pre, pre_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let pre_box = &layout_tree.children[0].children[0];
    println!("Pre children count: {}", pre_box.children.len());
    for (i, line) in pre_box.children.iter().enumerate() {
        println!(
            "Line {}: y={}, h={}",
            i, line.dimensions.content.y, line.dimensions.content.height
        );
    }

    assert_eq!(pre_box.children.len(), 3);
    let line0 = &pre_box.children[0];
    let line1 = &pre_box.children[1];
    let line2 = &pre_box.children[2];

    // Empty line box (line1) MUST maintain the strut height (20.0px)
    assert_eq!(line0.dimensions.content.height, 20.0);
    assert_eq!(line1.dimensions.content.height, 20.0);
    assert_eq!(line2.dimensions.content.height, 20.0);

    // Line positions must stack consecutively
    assert_eq!(line0.dimensions.content.y, 0.0);
    assert_eq!(line1.dimensions.content.y, 20.0);
    assert_eq!(line2.dimensions.content.y, 40.0);
    assert_eq!(pre_box.dimensions.content.height, 60.0);
}

#[test]
fn test_adversarial_strut_empty_paragraph_behaviors() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p_empty_text = doc.create_element(Namespace::Html, "p", vec![]);
    let p_spaces = doc.create_element(Namespace::Html, "p", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p_empty_text);
    doc.append_text(p_empty_text, "");
    doc.append_child(body, p_spaces);
    doc.append_text(p_spaces, "    ");

    let mut p_style = make_block_style(Length::Px(200.0), Length::Auto);
    p_style.font_size = 16.0;
    p_style.line_height = Some(24.0);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p_empty_text, p_style.clone());
    styles.insert(p_spaces, p_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let body_box = &layout_tree.children[0];
    let box_empty = &body_box.children[0];
    let box_spaces = &body_box.children[1];

    println!(
        "Empty text p height: {}",
        box_empty.dimensions.content.height
    );
    println!(
        "Spaces-only p height: {}",
        box_spaces.dimensions.content.height
    );

    // CSS 2.1 §9.4.2: Line boxes that contain no text and no preserved newline do not exist
    assert_eq!(box_empty.dimensions.content.height, 0.0);
    assert_eq!(box_spaces.dimensions.content.height, 0.0);
}

#[test]
fn test_adversarial_vertical_align_combinations() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span_base = doc.create_element(Namespace::Html, "span", vec![]);
    let span_sub = doc.create_element(Namespace::Html, "span", vec![]);
    let span_super = doc.create_element(Namespace::Html, "span", vec![]);
    let span_len_pos = doc.create_element(Namespace::Html, "span", vec![]);
    let span_len_neg = doc.create_element(Namespace::Html, "span", vec![]);
    let span_ttop = doc.create_element(Namespace::Html, "span", vec![]);
    let span_tbot = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);

    doc.append_child(p, span_base);
    doc.append_text(span_base, "A ");
    doc.append_child(p, span_sub);
    doc.append_text(span_sub, "B ");
    doc.append_child(p, span_super);
    doc.append_text(span_super, "C ");
    doc.append_child(p, span_len_pos);
    doc.append_text(span_len_pos, "D ");
    doc.append_child(p, span_len_neg);
    doc.append_text(span_len_neg, "E ");
    doc.append_child(p, span_ttop);
    doc.append_text(span_ttop, "F ");
    doc.append_child(p, span_tbot);
    doc.append_text(span_tbot, "G");

    let mut p_style = make_block_style(Length::Px(800.0), Length::Auto);
    p_style.font_size = 20.0;
    p_style.line_height = Some(24.0);

    let mut style_base = make_inline_style();
    style_base.font_size = 20.0;
    style_base.line_height = Some(24.0);
    style_base.vertical_align = VerticalAlign::Baseline;

    let mut style_sub = style_base.clone();
    style_sub.vertical_align = VerticalAlign::Sub;

    let mut style_super = style_base.clone();
    style_super.vertical_align = VerticalAlign::Super;

    let mut style_len_pos = style_base.clone();
    style_len_pos.vertical_align = VerticalAlign::Length(30.0); // shifted up 30px

    let mut style_len_neg = style_base.clone();
    style_len_neg.vertical_align = VerticalAlign::Length(-25.0); // shifted down 25px

    let mut style_ttop = style_base.clone();
    style_ttop.vertical_align = VerticalAlign::TextTop;

    let mut style_tbot = style_base.clone();
    style_tbot.vertical_align = VerticalAlign::TextBottom;

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span_base, style_base);
    styles.insert(span_sub, style_sub);
    styles.insert(span_super, style_super);
    styles.insert(span_len_pos, style_len_pos);
    styles.insert(span_len_neg, style_len_neg);
    styles.insert(span_ttop, style_ttop);
    styles.insert(span_tbot, style_tbot);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];
    assert_eq!(p_box.children.len(), 1);
    let line_box = &p_box.children[0];

    // Strut: font 20, line_height 24 -> ascent 16, descent 4, leading 4. strut_ascent = 18, strut_descent = 6.
    // Length(+30): a_prime = 18 + 30 = 48. max_ascent = 48.
    // Length(-25): d_prime = 6 - (-25) = 31. max_descent = 31.
    // Total line height = 48 + 31 = 79.0px!
    println!("Line box height: {}", line_box.dimensions.content.height);
    assert_eq!(line_box.dimensions.content.height, 79.0);
    assert_eq!(p_box.dimensions.content.height, 79.0);

    // Verify all child fragments lie within [line_y, line_y + line_height]
    let line_y = line_box.dimensions.content.y;
    let line_h = line_box.dimensions.content.height;
    for child in &line_box.children {
        // Child can be InlineNode containing TextNode
        let text_child = if !child.children.is_empty() {
            &child.children[0]
        } else {
            child
        };
        let top = text_child.dimensions.content.y;
        let bottom = top + text_child.dimensions.content.height;
        println!(
            "Fragment top={}, bottom={}, bounds=[{}, {}]",
            top,
            bottom,
            line_y,
            line_y + line_h
        );
        assert!(
            top >= line_y - 0.001,
            "Top {} should be >= line_y {}",
            top,
            line_y
        );
        assert!(
            bottom <= line_y + line_h + 0.001,
            "Bottom {} should be <= line_y + h {}",
            bottom,
            line_y + line_h
        );
    }
}

#[test]
fn test_adversarial_vertical_align_middle_bounds() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span_mid = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, span_mid);
    doc.append_text(span_mid, "Middle");

    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_size = 16.0;
    p_style.line_height = Some(20.0);

    let mut mid_style = make_inline_style();
    mid_style.font_size = 16.0;
    mid_style.line_height = Some(20.0);
    mid_style.vertical_align = VerticalAlign::Middle;

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span_mid, mid_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];
    let line_box = &p_box.children[0];
    let span_box = &line_box.children[0];
    let text_box = &span_box.children[0];

    println!(
        "LineBox: y={}, h={}",
        line_box.dimensions.content.y, line_box.dimensions.content.height
    );
    println!(
        "Text in Middle: y={}, h={}",
        text_box.dimensions.content.y, text_box.dimensions.content.height
    );

    // Check if text is within line box bounds
    let line_y = line_box.dimensions.content.y;
    let line_h = line_box.dimensions.content.height;
    let top = text_box.dimensions.content.y;
    let bottom = top + text_box.dimensions.content.height;
    println!("Middle fragment top={}, line_y={}", top, line_y);
    println!(
        "Middle fragment bottom={}, line_bottom={}",
        bottom,
        line_y + line_h
    );

    assert!(
        top >= line_y - 0.001,
        "Middle fragment top {} must be within line box (line_y = {})",
        top,
        line_y
    );
    assert!(
        bottom <= line_y + line_h + 0.001,
        "Middle fragment bottom {} must be within line box (line_bottom = {})",
        bottom,
        line_y + line_h
    );
}

#[test]
fn test_adversarial_deeply_nested_box_splitting_across_lines() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span = doc.create_element(Namespace::Html, "span", vec![]);
    let b = doc.create_element(Namespace::Html, "b", vec![]);
    let i = doc.create_element(Namespace::Html, "i", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, span);
    doc.append_child(span, b);
    doc.append_child(b, i);
    // 4 words, each 5 chars (30px monospace at 10px font).
    // Word1 Word2 Word3 Word4
    doc.append_text(i, "word1 word2 word3 word4");

    // Container width = 60px.
    // Monospace 10px -> 6px per char. "word1" = 30px.
    // Span left edges: margin 2, border 3, padding 4 = 9px.
    // B left edges: margin 1, border 2, padding 3 = 6px.
    // I left edges: margin 1, border 1, padding 1 = 3px.
    // Total left edges on line 1 = 18px.
    // 18 + 30 = 48px <= 60px -> word1 fits on line 1.
    // word2 cannot fit on line 1 -> wraps to line 2.
    // word3 cannot fit on line 2 -> wraps to line 3.
    // word4 cannot fit on line 3 -> wraps to line 4.
    let mut p_style = make_block_style(Length::Px(60.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(15.0);

    let mut span_style = make_inline_style();
    span_style.font_family = "monospace".to_string();
    span_style.font_size = 10.0;
    span_style.margin.left = Length::Px(2.0);
    span_style.margin.right = Length::Px(5.0);
    span_style.border_width.left = 3.0;
    span_style.border_width.right = 4.0;
    span_style.padding.left = Length::Px(4.0);
    span_style.padding.right = Length::Px(6.0);

    let mut b_style = make_inline_style();
    b_style.font_family = "monospace".to_string();
    b_style.font_size = 10.0;
    b_style.margin.left = Length::Px(1.0);
    b_style.margin.right = Length::Px(2.0);
    b_style.border_width.left = 2.0;
    b_style.border_width.right = 3.0;
    b_style.padding.left = Length::Px(3.0);
    b_style.padding.right = Length::Px(4.0);

    let mut i_style = make_inline_style();
    i_style.font_family = "monospace".to_string();
    i_style.font_size = 10.0;
    i_style.margin.left = Length::Px(1.0);
    i_style.margin.right = Length::Px(1.0);
    i_style.border_width.left = 1.0;
    i_style.border_width.right = 1.0;
    i_style.padding.left = Length::Px(1.0);
    i_style.padding.right = Length::Px(2.0);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span, span_style);
    styles.insert(b, b_style);
    styles.insert(i, i_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];
    println!("Line boxes count: {}", p_box.children.len());
    assert_eq!(p_box.children.len(), 4);

    for (line_idx, line) in p_box.children.iter().enumerate() {
        println!("Checking line {}", line_idx);
        // Each line should have span -> b -> i -> text
        assert_eq!(line.children.len(), 1);
        let span_frag = &line.children[0];
        assert_eq!(span_frag.box_type, BoxType::InlineNode(span));
        assert_eq!(span_frag.children.len(), 1);
        let b_frag = &span_frag.children[0];
        assert_eq!(b_frag.box_type, BoxType::InlineNode(b));
        assert_eq!(b_frag.children.len(), 1);
        let i_frag = &b_frag.children[0];
        assert_eq!(i_frag.box_type, BoxType::InlineNode(i));

        if line_idx == 0 {
            // Line 1: Left edges only
            assert_eq!(span_frag.dimensions.margin.left, 2.0);
            assert_eq!(span_frag.dimensions.border.left, 3.0);
            assert_eq!(span_frag.dimensions.padding.left, 4.0);
            assert_eq!(span_frag.dimensions.margin.right, 0.0);
            assert_eq!(span_frag.dimensions.border.right, 0.0);
            assert_eq!(span_frag.dimensions.padding.right, 0.0);

            assert_eq!(b_frag.dimensions.margin.left, 1.0);
            assert_eq!(b_frag.dimensions.border.left, 2.0);
            assert_eq!(b_frag.dimensions.padding.left, 3.0);
            assert_eq!(b_frag.dimensions.margin.right, 0.0);
            assert_eq!(b_frag.dimensions.border.right, 0.0);
            assert_eq!(b_frag.dimensions.padding.right, 0.0);

            assert_eq!(i_frag.dimensions.margin.left, 1.0);
            assert_eq!(i_frag.dimensions.border.left, 1.0);
            assert_eq!(i_frag.dimensions.padding.left, 1.0);
            assert_eq!(i_frag.dimensions.margin.right, 0.0);
            assert_eq!(i_frag.dimensions.border.right, 0.0);
            assert_eq!(i_frag.dimensions.padding.right, 0.0);
        } else if line_idx == 3 {
            // Line 4 (last line): Right edges only
            assert_eq!(span_frag.dimensions.margin.left, 0.0);
            assert_eq!(span_frag.dimensions.border.left, 0.0);
            assert_eq!(span_frag.dimensions.padding.left, 0.0);
            assert_eq!(span_frag.dimensions.margin.right, 5.0);
            assert_eq!(span_frag.dimensions.border.right, 4.0);
            assert_eq!(span_frag.dimensions.padding.right, 6.0);

            assert_eq!(b_frag.dimensions.margin.left, 0.0);
            assert_eq!(b_frag.dimensions.border.left, 0.0);
            assert_eq!(b_frag.dimensions.padding.left, 0.0);
            assert_eq!(b_frag.dimensions.margin.right, 2.0);
            assert_eq!(b_frag.dimensions.border.right, 3.0);
            assert_eq!(b_frag.dimensions.padding.right, 4.0);

            assert_eq!(i_frag.dimensions.margin.left, 0.0);
            assert_eq!(i_frag.dimensions.border.left, 0.0);
            assert_eq!(i_frag.dimensions.padding.left, 0.0);
            assert_eq!(i_frag.dimensions.margin.right, 1.0);
            assert_eq!(i_frag.dimensions.border.right, 1.0);
            assert_eq!(i_frag.dimensions.padding.right, 2.0);
        } else {
            // Intermediate lines (1 and 2): ZERO border, margin, padding on both sides!
            assert_eq!(span_frag.dimensions.margin.left, 0.0);
            assert_eq!(span_frag.dimensions.border.left, 0.0);
            assert_eq!(span_frag.dimensions.padding.left, 0.0);
            assert_eq!(span_frag.dimensions.margin.right, 0.0);
            assert_eq!(span_frag.dimensions.border.right, 0.0);
            assert_eq!(span_frag.dimensions.padding.right, 0.0);

            assert_eq!(b_frag.dimensions.margin.left, 0.0);
            assert_eq!(b_frag.dimensions.border.left, 0.0);
            assert_eq!(b_frag.dimensions.padding.left, 0.0);
            assert_eq!(b_frag.dimensions.margin.right, 0.0);
            assert_eq!(b_frag.dimensions.border.right, 0.0);
            assert_eq!(b_frag.dimensions.padding.right, 0.0);

            assert_eq!(i_frag.dimensions.margin.left, 0.0);
            assert_eq!(i_frag.dimensions.border.left, 0.0);
            assert_eq!(i_frag.dimensions.padding.left, 0.0);
            assert_eq!(i_frag.dimensions.margin.right, 0.0);
            assert_eq!(i_frag.dimensions.border.right, 0.0);
            assert_eq!(i_frag.dimensions.padding.right, 0.0);
        }
    }
}

#[test]
fn test_adversarial_text_align_right_center_fractional_multiline() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p_right = doc.create_element(Namespace::Html, "p", vec![]);
    let p_center = doc.create_element(Namespace::Html, "p", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p_right);
    doc.append_child(body, p_center);

    // 3 words: "AAAA BBBB CCCC"
    doc.append_text(p_right, "AAAA BBBB CCCC");
    doc.append_text(p_center, "AAAA BBBB CCCC");

    // Fractional container width = 125.5px
    // Monospace 10px -> 6px per char. "AAAA" = 24px.
    // Line 1: "AAAA" (24px). Cannot fit "AAAA BBBB" (24 + 6 + 24 = 54 <= 125.5).
    // Let's make container width = 50.5px so "AAAA" (24px) fits, but "AAAA BBBB" (54px) wraps!
    let mut right_style = make_block_style(Length::Px(50.5), Length::Auto);
    right_style.text_align = TextAlign::Right;
    right_style.font_family = "monospace".to_string();
    right_style.font_size = 10.0;
    right_style.line_height = Some(15.0);

    let mut center_style = make_block_style(Length::Px(50.5), Length::Auto);
    center_style.text_align = TextAlign::Center;
    center_style.font_family = "monospace".to_string();
    center_style.font_size = 10.0;
    center_style.line_height = Some(15.0);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p_right, right_style);
    styles.insert(p_center, center_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let body_box = &layout_tree.children[0];
    let right_box = &body_box.children[0];
    let center_box = &body_box.children[1];

    assert_eq!(right_box.children.len(), 3);
    assert_eq!(center_box.children.len(), 3);

    // Container width = 50.5. Each word = 24.0px.
    // Remaining = 50.5 - 24.0 = 26.5px.
    // For right align: offset = 26.5px. Right edge = 26.5 + 24.0 = 50.5px!
    for (i, line) in right_box.children.iter().enumerate() {
        let text_box = &line.children[0];
        println!(
            "Right line {}: x={}, w={}",
            i, text_box.dimensions.content.x, text_box.dimensions.content.width
        );
        assert_eq!(text_box.dimensions.content.x, 26.5);
        assert_eq!(text_box.dimensions.content.width, 24.0);
    }

    // For center align: remaining = 26.5px. Offset = 26.5 / 2.0 = 13.25px!
    for (i, line) in center_box.children.iter().enumerate() {
        let text_box = &line.children[0];
        println!(
            "Center line {}: x={}, w={}",
            i, text_box.dimensions.content.x, text_box.dimensions.content.width
        );
        assert_eq!(text_box.dimensions.content.x, 13.25);
        assert_eq!(text_box.dimensions.content.width, 24.0);
    }
}

#[test]
fn test_adversarial_text_align_right_with_styled_span() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, span);
    doc.append_text(span, "Hello");

    // Container width = 200.0px.
    // "Hello" monospace 10px = 30.0px.
    // Span: border-left 10px, border-right 20px. Total span width = 10 + 30 + 20 = 60.0px.
    // Expected right alignment offset: 200 - 60 = 140.0px.
    // Span content.x should be 140 + 10 = 150.0px.
    // Right border edge = 150 + 30 + 20 = 200.0px (flush with right container edge).
    let mut p_style = make_block_style(Length::Px(200.0), Length::Auto);
    p_style.text_align = TextAlign::Right;
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let mut span_style = make_inline_style();
    span_style.font_family = "monospace".to_string();
    span_style.font_size = 10.0;
    span_style.border_width.left = 10.0;
    span_style.border_width.right = 20.0;

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span, span_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];
    let line_box = &p_box.children[0];
    let span_box = &line_box.children[0];
    let text_box = &span_box.children[0];

    let right_edge = span_box.dimensions.content.x
        + span_box.dimensions.content.width
        + span_box.dimensions.border.right;
    println!("Span content.x = {}", span_box.dimensions.content.x);
    println!("Span content.w = {}", span_box.dimensions.content.width);
    println!("Span border.left = {}", span_box.dimensions.border.left);
    println!("Span border.right = {}", span_box.dimensions.border.right);
    println!("Text content.x = {}", text_box.dimensions.content.x);
    println!("Span right edge = {}, container_width = 200.0", right_edge);

    // If right_edges are properly included in line_content_width, right_edge == 200.0.
    // If right_edges were omitted from line_content_width, right_edge == 220.0 (overflow!).
    assert_eq!(
        right_edge, 200.0,
        "Span right edge should be flush with container width (200.0), but got {}",
        right_edge
    );
}
