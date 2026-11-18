use super::common::*;

#[test]
fn test_adversarial_deeply_nested_inline_spans() {
    // 50 levels of nested spans: <span><span>...<span>Word1 Word2</span>...</span></span>
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);

    let mut current_parent = p;
    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));

    let mut p_style = make_block_style(Length::Px(50.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);
    styles.insert(p, p_style);

    for i in 0..50 {
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(current_parent, span);
        let mut span_style = make_inline_style();
        if i == 0 {
            span_style.border_width.left = 2.0;
            span_style.border_width.right = 3.0;
        }
        styles.insert(span, span_style);
        current_parent = span;
    }
    doc.append_text(current_parent, "First Second");

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("deeply nested spans must layout");
    let p_box = &tree.children[0].children[0];

    // Must wrap into 2 lines cleanly without stack overflow or corrupted frames
    assert_eq!(p_box.children.len(), 2);
    // Line height is 21.4px per line due to span font_size 16px (16 * 1.2 = 19.2, leading 3.2, ascent 14.4 + descent 7.0 = 21.4)
    // 2 * 21.4 = 42.8px
    assert!((p_box.dimensions.content.height - 42.8).abs() < 0.1);
}

#[test]
fn test_adversarial_trailing_space_inside_inline_span() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, span);
    // Span ends with a trailing space, then "Second" wraps
    doc.append_text(span, "First ");
    doc.append_text(p, "Second");

    // Container width = 40px, monospace 10px (6px per char).
    // "First" = 30px <= 40px. "Second" = 36px > 40px - 30px, so "Second" wraps to Line 2!
    let mut p_style = make_block_style(Length::Px(40.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let mut span_style = make_inline_style();
    span_style.font_family = "monospace".to_string();
    span_style.font_size = 10.0;

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span, span_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    assert_eq!(p_box.children.len(), 2, "Must wrap into 2 lines");
    let line1 = &p_box.children[0];
    let span_in_line1 = &line1.children[0];
    let text_in_span = &span_in_line1.children[0];

    if let BoxType::TextNode(ref txt) = text_in_span.box_type {
        println!("DEBUG: Line 1 text inside span is {:?}", txt);
        assert_eq!(
            txt, "First",
            "Trailing collapsible space inside span must be trimmed when line wraps"
        );
    } else {
        panic!("Expected TextNode in span");
    }
}

#[test]
fn test_adversarial_trailing_space_multilevel_nested_box_end() {
    // Trailing space inside deeply nested spans at the end of a line
    // <p><span><span><span>Deeply spaced </span></span></span>Second</p>
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let span1 = doc.create_element(Namespace::Html, "span", vec![]);
    let span2 = doc.create_element(Namespace::Html, "span", vec![]);
    let span3 = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_child(p, span1);
    doc.append_child(span1, span2);
    doc.append_child(span2, span3);
    doc.append_text(span3, "Deeply spaced   ");
    doc.append_text(p, "Second");

    // Container width = 90px (monospace 10px -> 6px/char).
    // "Deeply spaced" = 13 chars * 6 = 78px <= 90px.
    // "Second" = 6 chars * 6 = 36px -> 78 + 36 = 114px > 90px, wraps to line 2.
    let mut p_style = make_block_style(Length::Px(90.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let mut span_style = make_inline_style();
    span_style.font_family = "monospace".to_string();
    span_style.font_size = 10.0;
    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, p_style);
    styles.insert(span1, span_style.clone());
    styles.insert(span2, span_style.clone());
    styles.insert(span3, span_style);

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    assert_eq!(p_box.children.len(), 2, "Must wrap into 2 lines");
    let line1 = &p_box.children[0];

    // Traverse into span hierarchy to verify trimmed text
    let s1 = &line1.children[0];
    let s2 = &s1.children[0];
    let s3 = &s2.children[0];
    let text_node = &s3.children[0];

    if let BoxType::TextNode(ref txt) = text_node.box_type {
        assert_eq!(
            txt, "Deeply spaced",
            "Trailing collapsible spaces across multi-level BoxEnd nesting must be completely trimmed"
        );
    } else {
        panic!("Expected TextNode in span3");
    }
}

#[test]
fn test_adversarial_deeply_nested_spans_in_zero_width_container() {
    // 100 levels of nested spans in a 0px container
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);

    let mut current_parent = p;
    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));

    let mut p_style = make_block_style(Length::Px(0.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);
    styles.insert(p, p_style);

    for _ in 0..100 {
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(current_parent, span);
        let mut span_style = make_inline_style();
        span_style.font_family = "monospace".to_string();
        span_style.font_size = 10.0;
        styles.insert(span, span_style);
        current_parent = span;
    }
    doc.append_text(current_parent, "WordOne WordTwo WordThree");

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("deeply nested zero-width layout");
    let p_box = &tree.children[0].children[0];

    // Must wrap cleanly into 3 lines without stack overflow
    assert_eq!(
        p_box.children.len(),
        3,
        "3 words in 100-deep nested spans within 0px container must wrap into 3 line boxes"
    );
    assert_eq!(p_box.dimensions.content.height, 60.0);
}
