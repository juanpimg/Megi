use super::*;

// ---------------------------------------------------------------------------
// 3. F-12: numeric line-height inherited as a factor and resolved per element
// ---------------------------------------------------------------------------

#[test]
fn test_numeric_line_height_is_resolved_per_element() {
    // parent: font-size 16px, line-height: 1.5 (factor) => 24px line box.
    // child: font-size 32px inherits the *factor* => 48px line box (F-12).
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    let span = make_element(&mut doc, p, "span");
    doc.append_text(span, "Hi");

    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_size = 16.0;
    p_style.line_height = Some(24.0);
    p_style.line_height_factor = Some(1.5);
    styles.insert(p, p_style);

    let mut span_style = make_inline_style();
    span_style.font_size = 32.0;
    // The CSS crate still carries the legacy computed px value inherited from the
    // parent; the factor is what must drive the used value.
    span_style.line_height = Some(24.0);
    span_style.line_height_factor = Some(1.5);
    styles.insert(span, span_style);

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    assert_close(
        p_box.children[0].dimensions.content.height,
        48.0,
        "line box height (1.5 * 32px child font-size)",
    );
}

#[test]
fn test_px_line_height_is_inherited_verbatim() {
    // Same layout but with `line-height: 24px` (no factor): a child with a larger font
    // inherits the fixed 24px line height, so the line box stays 24px.
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    let span = make_element(&mut doc, p, "span");
    doc.append_text(span, "Hi");

    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_size = 16.0;
    p_style.line_height = Some(24.0);
    styles.insert(p, p_style);

    let mut span_style = make_inline_style();
    span_style.font_size = 32.0;
    span_style.line_height = Some(24.0);
    styles.insert(span, span_style);

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    // The fixed 24px line-height is inherited verbatim, so it must NOT scale to
    // 1.5 * 32 = 48 as the numeric factor does. The line box is the strut ascent
    // (16.8) plus the larger child descent (7.2 + (24-32)/2 = 2.4) — i.e.
    // max_ascent (21.6) + max_descent (7.2) = 28.8 per CSS 2.2 §10.8.1.
    assert_close(
        p_box.children[0].dimensions.content.height,
        28.8,
        "fixed px line-height does not scale with the child font-size",
    );
}

#[test]
fn test_normal_line_height_uses_measurer_default() {
    // No computed line-height ⇒ `normal`: the deterministic measurer uses 1.2 * font-size.
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    doc.append_text(p, "Hi");
    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_size = 20.0;
    styles.insert(p, p_style);

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    assert_close(
        p_box.children[0].dimensions.content.height,
        24.0,
        "normal line-height = 1.2 * 20px",
    );
}

// ---------------------------------------------------------------------------
// 4. F-19: `vertical-align: top|bottom` aligned subtrees (CSS 2.2 §10.8.1)
// ---------------------------------------------------------------------------

#[test]
fn test_vertical_align_top_and_bottom_aligned_subtrees() {
    // Strut: 10px font, 20px line-height => ascent 13, descent 7.
    // Top span: 40px font, 40px line-height => 40px tall subtree, anchored to the line top.
    // Bottom span: 20px font, 20px line-height => 20px tall subtree, anchored to the line bottom.
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    doc.append_text(p, "A");
    let top_span = make_element(&mut doc, p, "span");
    doc.append_text(top_span, "T");
    let bottom_span = make_element(&mut doc, p, "span");
    doc.append_text(bottom_span, "B");

    let mut p_style = mono_style(true, Length::Px(400.0));
    p_style.font_family = "sans-serif".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);
    styles.insert(p, p_style);

    let mut top_style = make_inline_style();
    top_style.font_family = "sans-serif".to_string();
    top_style.font_size = 40.0;
    top_style.line_height = Some(40.0);
    top_style.vertical_align = VerticalAlign::Top;
    styles.insert(top_span, top_style);

    let mut bottom_style = make_inline_style();
    bottom_style.font_family = "sans-serif".to_string();
    bottom_style.font_size = 20.0;
    bottom_style.line_height = Some(20.0);
    bottom_style.vertical_align = VerticalAlign::Bottom;
    styles.insert(bottom_span, bottom_style);

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    let line = &p_box.children[0];
    let line_y = line.dimensions.content.y;
    let line_h = line.dimensions.content.height;

    // The 40px top-aligned subtree makes the line box 40px tall.
    assert_close(
        line_h,
        40.0,
        "line box height driven by the top-aligned subtree",
    );
    assert_close(p_box.dimensions.content.height, 40.0, "paragraph height");

    let top_box = &line.children[1];
    let bottom_box = &line.children[2];
    assert_eq!(top_box.box_type, BoxType::InlineNode(top_span));
    assert_eq!(bottom_box.box_type, BoxType::InlineNode(bottom_span));

    // top: aligned subtree top == line box top.
    assert_close(
        top_box.dimensions.content.y,
        line_y,
        "top-aligned subtree top",
    );
    // bottom: aligned subtree bottom == line box bottom.
    assert_close(
        bottom_box.dimensions.content.y,
        line_y + line_h - 20.0,
        "bottom-aligned subtree top",
    );
    assert_close(
        bottom_box.dimensions.content.y + 20.0,
        line_y + line_h,
        "bottom-aligned subtree bottom",
    );

    // Text inside the anchored subtrees follows the subtree baseline.
    let mut top_fragments = Vec::new();
    text_fragments(top_box, &mut top_fragments);
    assert_close(
        top_fragments[0].dimensions.content.y,
        line_y,
        "text top inside top subtree",
    );
    let mut bottom_fragments = Vec::new();
    text_fragments(bottom_box, &mut bottom_fragments);
    assert_close(
        bottom_fragments[0].dimensions.content.y,
        line_y + line_h - 20.0,
        "text top inside bottom subtree",
    );

    // Baseline-aligned text ("A") is not stretched: its baseline stays at the strut ascent.
    let mut plain_fragments = Vec::new();
    text_fragments(&line.children[0], &mut plain_fragments);
    assert_close(
        plain_fragments[0].dimensions.content.y + plain_fragments[0].baseline,
        line_y + 13.0,
        "baseline-aligned text baseline",
    );
}

#[test]
fn test_vertical_align_top_taller_subtree_wins() {
    // Two top-aligned subtrees: the taller one defines the line box height.
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    let small = make_element(&mut doc, p, "span");
    doc.append_text(small, "s");
    let tall = make_element(&mut doc, p, "span");
    doc.append_text(tall, "T");

    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_family = "sans-serif".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);
    styles.insert(p, p_style);

    let mut small_style = make_inline_style();
    small_style.font_family = "sans-serif".to_string();
    small_style.font_size = 20.0;
    small_style.line_height = Some(20.0);
    small_style.vertical_align = VerticalAlign::Top;
    styles.insert(small, small_style);

    let mut tall_style = make_inline_style();
    tall_style.font_family = "sans-serif".to_string();
    tall_style.font_size = 50.0;
    tall_style.line_height = Some(50.0);
    tall_style.vertical_align = VerticalAlign::Top;
    styles.insert(tall, tall_style);

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    let line = &p_box.children[0];
    assert_close(
        line.dimensions.content.height,
        50.0,
        "tallest top-aligned subtree defines the line box height",
    );
    // Both top-aligned subtrees start at the line box top.
    assert_close(
        line.children[0].dimensions.content.y,
        0.0,
        "small top subtree top",
    );
    assert_close(
        line.children[1].dimensions.content.y,
        0.0,
        "tall top subtree top",
    );
}
