use super::common::*;

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
