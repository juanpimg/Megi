use super::common::*;

#[test]
fn test_adversarial_zero_width_container_wrapping() {
    // If container width is 0.0px, what does line breaker do with multiple words?
    let text = "word1 word2 word3";
    let mut p_style = make_block_style(Length::Px(0.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let (doc, styles) = setup_simple_doc(text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    println!(
        "DEBUG: Container width 0.0px produced {} line boxes. Heights: {:?}",
        p_box.children.len(),
        p_box.dimensions.content.height
    );
    assert_eq!(
        p_box.children.len(),
        3,
        "Zero-width container must wrap each word into its own line box"
    );
    assert_eq!(p_box.dimensions.content.height, 60.0);
}

#[test]
fn test_adversarial_nan_and_subnormal_container_width() {
    for width in [f32::NAN, 1e-40_f32, -100.0_f32] {
        let text = "Hello World From Adversarial Challenger";
        let mut p_style = make_block_style(Length::Px(width), Length::Auto);
        p_style.font_size = 16.0;
        p_style.line_height = Some(20.0);

        let (doc, styles) = setup_simple_doc(text, p_style);
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        // Must NOT panic or infinite-loop
        let res = layout_document(&doc, &styles, viewport);
        assert!(
            res.is_some(),
            "Layout must not panic with width {:?}",
            width
        );
        let tree = res.unwrap();
        let p_box = &tree.children[0].children[0];
        println!(
            "Width {:?} resulted in {} lines, height {}",
            width,
            p_box.children.len(),
            p_box.dimensions.content.height
        );
    }
}

#[test]
fn test_adversarial_extreme_vertical_align() {
    // Test extreme vertical align shifts
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span_huge = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_text(p, "Base ");
    doc.append_child(p, span_huge);
    doc.append_text(span_huge, "HugeShift");

    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_size = 16.0;
    p_style.line_height = Some(20.0);

    let mut span_style = make_inline_style();
    span_style.font_size = 16.0;
    span_style.line_height = Some(20.0);
    span_style.vertical_align = VerticalAlign::Length(10_000.0);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span_huge, span_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("extreme vertical align");
    let p_box = &tree.children[0].children[0];
    let line_box = &p_box.children[0];

    // Total line height must expand to accommodate the 10,000px shift
    assert!(line_box.dimensions.content.height >= 10_000.0);
    assert_eq!(
        p_box.dimensions.content.height,
        line_box.dimensions.content.height
    );
}
