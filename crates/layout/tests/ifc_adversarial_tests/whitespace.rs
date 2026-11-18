use super::common::*;

#[test]
fn test_adversarial_pathological_whitespace_normal_collapsing() {
    // 100 consecutive spaces between words, plus leading/trailing mixed whitespace
    let pathological_text = "   \t\r\n   Alpha     \t\t\t\n\r   Beta   \n\r\t   Gamma   \t\n\r   ";

    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_family = "monospace".to_string(); // 10px * 0.6 = 6px per char
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);
    p_style.white_space = WhiteSpace::Normal;

    let (doc, styles) = setup_simple_doc(pathological_text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");

    let p_box = &tree.children[0].children[0];
    assert_eq!(p_box.children.len(), 1, "Should collapse into single line");

    let line_box = &p_box.children[0];
    assert_eq!(line_box.children.len(), 1, "Text should be coalesced");

    // Collapsed text should be "Alpha Beta Gamma"
    // 5 chars ("Alpha") + 1 space + 4 chars ("Beta") + 1 space + 5 chars ("Gamma") = 16 chars * 6.0px = 96.0px
    let text_box = &line_box.children[0];
    if let BoxType::TextNode(ref txt) = text_box.box_type {
        assert_eq!(txt, "Alpha Beta Gamma");
    } else {
        panic!("Expected TextNode");
    }
    assert_eq!(text_box.dimensions.content.width, 96.0);
}

#[test]
fn test_adversarial_pure_whitespace_inputs() {
    for ws_mode in [WhiteSpace::Normal, WhiteSpace::Nowrap, WhiteSpace::PreLine] {
        let ws_only = "   \t\t\r\n   \n\r\t   ";
        let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
        p_style.font_size = 16.0;
        p_style.line_height = Some(24.0);
        p_style.white_space = ws_mode;

        let (doc, styles) = setup_simple_doc(ws_only, p_style);
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
        let p_box = &tree.children[0].children[0];

        if ws_mode == WhiteSpace::PreLine {
            // PreLine preserves newlines, so forced breaks will exist
            assert!(
                p_box.dimensions.content.height > 0.0,
                "PreLine should preserve forced breaks"
            );
        } else {
            // Normal & Nowrap collapse pure whitespace with no words -> 0 lines and 0 height
            assert_eq!(
                p_box.children.len(),
                0,
                "Pure whitespace under {:?} should produce 0 lines",
                ws_mode
            );
            assert_eq!(
                p_box.dimensions.content.height, 0.0,
                "Pure whitespace under {:?} should have 0 height",
                ws_mode
            );
        }
    }
}

#[test]
fn test_adversarial_pre_consecutive_newlines_and_spaces() {
    // 3 consecutive newlines with spaces on some lines
    let pre_text = "Line 1\n\n\n   Indented Line 4   ";
    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);
    p_style.white_space = WhiteSpace::Pre;

    let (doc, styles) = setup_simple_doc(pre_text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    // Exactly 4 line boxes: Line 1, empty line 2, empty line 3, Indented Line 4
    assert_eq!(
        p_box.children.len(),
        4,
        "Pre should produce exactly 4 line boxes"
    );
    assert_eq!(
        p_box.dimensions.content.height, 80.0,
        "4 lines * 20px = 80px"
    );

    // Empty lines (index 1 and 2) must have line-height 20px
    assert_eq!(p_box.children[1].dimensions.content.height, 20.0);
    assert_eq!(p_box.children[2].dimensions.content.height, 20.0);
}

#[test]
fn test_adversarial_pre_wrap_space_preservation_and_soft_wrap() {
    // Under pre-wrap, consecutive spaces are preserved but words can wrap
    let pre_wrap_text = "One     Two     Three";
    let mut p_style = make_block_style(Length::Px(100.0), Length::Auto);
    p_style.font_family = "monospace".to_string(); // 10px * 0.6 = 6px per char
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);
    p_style.white_space = WhiteSpace::PreWrap;

    let (doc, styles) = setup_simple_doc(pre_wrap_text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    // "One     " is 3 chars + 5 spaces = 8 chars * 6px = 48px <= 100px.
    // "Two" is 3 chars * 6 = 18px -> total 48 + 18 = 66px <= 100px.
    // "     Three" is 5 spaces + 5 chars = 10 chars * 6 = 60px -> 66 + 60 = 126px > 100px! Must wrap!
    assert!(p_box.children.len() >= 2, "Must wrap across multiple lines");
}
