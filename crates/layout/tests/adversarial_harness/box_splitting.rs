use super::common::*;

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
fn test_adversarial_text_align_right_multiline_box_splitting_edges() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, span);
    doc.append_text(span, "AAA BBB");

    // Monospace 10px -> 6px/char. "AAA" = 18px. "BBB" = 18px.
    // Span border-left = 10px, border-right = 20px.
    // Container width = 45px.
    // Line 1: left border 10px + "AAA" 18px = 28px <= 45px.
    // With space (6px) + "BBB" (18px) + border-right (20px), total = 72px > 45px -> wraps to Line 2!
    let mut p_style = make_block_style(Length::Px(45.0), Length::Auto);
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
    assert_eq!(p_box.children.len(), 2, "Must break into 2 lines");

    let line1 = &p_box.children[0];
    let line2 = &p_box.children[1];

    let span_frag1 = &line1.children[0];
    let span_frag2 = &line2.children[0];

    // Line 1: Left border = 10.0, Right border = 0.0.
    // line_content_width = 10.0 + 18.0 = 28.0px.
    // Remaining = 45.0 - 28.0 = 17.0px.
    // span content.x = 17.0 + 10.0 = 27.0px.
    // Rightmost point of text on line 1 = 27.0 + 18.0 = 45.0px (flush with container width!).
    assert_eq!(span_frag1.dimensions.border.left, 10.0);
    assert_eq!(span_frag1.dimensions.border.right, 0.0);
    assert_eq!(span_frag1.dimensions.content.x, 27.0);
    assert_eq!(span_frag1.dimensions.content.width, 18.0);
    assert_eq!(
        span_frag1.dimensions.content.x + span_frag1.dimensions.content.width,
        45.0
    );

    // Line 2: Left border = 0.0, Right border = 20.0.
    // line_content_width = 18.0 + 20.0 = 38.0px.
    // Remaining = 45.0 - 38.0 = 7.0px.
    // span content.x = 7.0 + 0.0 = 7.0px.
    // Right border edge = 7.0 + 18.0 + 20.0 = 45.0px (flush with container width!).
    assert_eq!(span_frag2.dimensions.border.left, 0.0);
    assert_eq!(span_frag2.dimensions.border.right, 20.0);
    assert_eq!(span_frag2.dimensions.content.x, 7.0);
    assert_eq!(span_frag2.dimensions.content.width, 18.0);
    assert_eq!(
        span_frag2.dimensions.content.x
            + span_frag2.dimensions.content.width
            + span_frag2.dimensions.border.right,
        45.0
    );
}
