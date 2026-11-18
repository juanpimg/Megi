use super::*;

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
