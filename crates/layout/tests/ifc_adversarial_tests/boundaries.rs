use super::common::*;

#[test]
fn test_adversarial_boundary_breaks_punctuation_and_hyphens() {
    // Test if line breaker breaks at hyphens or punctuation
    // Container width = 40px, font monospace 10px (6px per char).
    // Fits max 6 chars (36px).
    // Word "auto-test" has 9 chars (54px > 40px).
    // "auto-" is 5 chars (30px <= 40px).
    // "test" is 4 chars (24px <= 40px).
    let text = "auto-test";
    let mut p_style = make_block_style(Length::Px(40.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let (doc, styles) = setup_simple_doc(text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    println!(
        "DEBUG: 'auto-test' in 40px container produced {} line boxes",
        p_box.children.len()
    );
    assert_eq!(
        p_box.children.len(),
        2,
        "'auto-test' in 40px container must break at hyphen into 2 lines"
    );
}

#[test]
fn test_adversarial_soft_hyphen_wrap_opportunity() {
    // Soft hyphen \u{00AD} should allow line breaking when container width is exceeded
    let text = "super\u{00AD}cali";
    // Monospace 10px -> 6px per char.
    // Container width 40px fits max 6 chars (36px).
    // "super" + hyphen is 6 chars (36px <= 40px), "cali" is 4 chars (24px <= 40px).
    let mut p_style = make_block_style(Length::Px(40.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let (doc, styles) = setup_simple_doc(text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    assert_eq!(
        p_box.children.len(),
        2,
        "'super\\u{{00AD}}cali' in 40px container must break at soft hyphen into 2 lines"
    );
}

#[test]
fn test_adversarial_zero_width_container_with_hyphens_and_shy() {
    // Zero-width container with hyphens and soft hyphens
    let text = "alpha-beta\u{00AD}gamma";
    let mut p_style = make_block_style(Length::Px(0.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let (doc, styles) = setup_simple_doc(text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    // In a 0px container, each soft wrap segment must wrap onto its own line:
    // Line 1: "alpha-"
    // Line 2: "beta\u{00AD}"
    // Line 3: "gamma"
    assert_eq!(
        p_box.children.len(),
        3,
        "Zero-width container with hyphens and soft hyphens must wrap each segment to its own line"
    );
    assert_eq!(p_box.dimensions.content.height, 60.0);
}

#[test]
fn test_adversarial_hyphen_nowrap_mode_preserves_compound() {
    // Under WhiteSpace::Nowrap, hyphens must NOT allow line wrapping
    let text = "auto-test-compound-word";
    let mut p_style = make_block_style(Length::Px(40.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);
    p_style.white_space = WhiteSpace::Nowrap;

    let (doc, styles) = setup_simple_doc(text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    assert_eq!(
        p_box.children.len(),
        1,
        "Nowrap must keep hyphenated compound words on a single overflowing line box"
    );
    assert_eq!(p_box.dimensions.content.height, 20.0);
}

#[test]
fn test_adversarial_hyphen_unbroken_when_sufficient_width() {
    // When container width is ample, hyphenated words must stay on 1 line and coalesce
    let text = "auto-test";
    let mut p_style = make_block_style(Length::Px(200.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let (doc, styles) = setup_simple_doc(text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    assert_eq!(
        p_box.children.len(),
        1,
        "Ample width must keep text on 1 line"
    );
    let line_box = &p_box.children[0];
    assert_eq!(line_box.children.len(), 1, "Must coalesce into 1 text box");
    if let BoxType::TextNode(ref txt) = line_box.children[0].box_type {
        assert_eq!(txt, "auto-test");
    } else {
        panic!("Expected TextNode");
    }
}
