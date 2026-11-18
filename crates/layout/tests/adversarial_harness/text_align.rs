use super::common::*;

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

#[test]
fn test_adversarial_text_align_right_nested_styled_spans() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let outer = doc.create_element(Namespace::Html, "span", vec![]);
    let inner = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, outer);
    doc.append_child(outer, inner);
    doc.append_text(inner, "Nested");

    let mut p_style = make_block_style(Length::Px(200.0), Length::Auto);
    p_style.text_align = TextAlign::Right;
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let mut outer_style = make_inline_style();
    outer_style.font_family = "monospace".to_string();
    outer_style.font_size = 10.0;
    outer_style.margin.left = Length::Px(5.0);
    outer_style.margin.right = Length::Px(5.0);
    outer_style.border_width.left = 3.0;
    outer_style.border_width.right = 3.0;
    outer_style.padding.left = Length::Px(7.0);
    outer_style.padding.right = Length::Px(7.0); // outer edges = 15.0 left, 15.0 right

    let mut inner_style = make_inline_style();
    inner_style.font_family = "monospace".to_string();
    inner_style.font_size = 10.0;
    inner_style.margin.left = Length::Px(4.0);
    inner_style.margin.right = Length::Px(4.0);
    inner_style.border_width.left = 2.0;
    inner_style.border_width.right = 2.0;
    inner_style.padding.left = Length::Px(6.0);
    inner_style.padding.right = Length::Px(6.0); // inner edges = 12.0 left, 12.0 right

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(outer, outer_style);
    styles.insert(inner, inner_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

    let p_box = &layout_tree.children[0].children[0];
    let line_box = &p_box.children[0];
    let outer_box = &line_box.children[0];
    let inner_box = &outer_box.children[0];
    let text_box = &inner_box.children[0];

    // "Nested" monospace 10px = 6 chars * 6.0 = 36.0px.
    // Total content width = 15.0 (outer left) + 12.0 (inner left) + 36.0 + 12.0 (inner right) + 15.0 (outer right) = 90.0px.
    // Remaining width = 200.0 - 90.0 = 110.0px.
    // Outer content.x = 110.0 + 15.0 = 125.0px.
    // Inner content.x = 125.0 + 12.0 = 137.0px.
    // Text content.x = 137.0px.
    // Outer right border edge = 125.0 + (12.0 + 36.0 + 12.0) + 3.0 = 188.0 + 7.0 (padding) + 5.0 (margin) = 200.0px.
    let outer_right_edge = outer_box.dimensions.content.x
        + outer_box.dimensions.content.width
        + outer_box.dimensions.padding.right
        + outer_box.dimensions.border.right
        + outer_box.dimensions.margin.right;

    println!("Outer content.x = {}", outer_box.dimensions.content.x);
    println!("Inner content.x = {}", inner_box.dimensions.content.x);
    println!("Text content.x = {}", text_box.dimensions.content.x);
    println!("Outer right edge = {}", outer_right_edge);

    assert_eq!(outer_box.dimensions.content.x, 125.0);
    assert_eq!(inner_box.dimensions.content.x, 137.0);
    assert_eq!(text_box.dimensions.content.x, 137.0);
    assert_eq!(outer_right_edge, 200.0);
}
