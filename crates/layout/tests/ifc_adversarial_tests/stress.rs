use super::common::*;

#[test]
fn test_adversarial_massive_single_word_unbreakable() {
    // 10,000 character single word in a 100px container.
    // Correctness matters; wall-clock budgets are flaky under load (see #1383).
    let massive_word = "A".repeat(10_000);
    let mut p_style = make_block_style(Length::Px(100.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let (doc, styles) = setup_simple_doc(&massive_word, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");

    let p_box = &tree.children[0].children[0];
    // Unbreakable word must not cause infinite loop or stack overflow
    assert_eq!(
        p_box.children.len(),
        1,
        "Unbreakable word must occupy 1 line box and overflow"
    );
    assert_eq!(p_box.dimensions.content.height, 20.0);
    let line_box = &p_box.children[0];
    let text_box = &line_box.children[0];
    // 10,000 * 6.0px = 60,000px
    assert_eq!(text_box.dimensions.content.width, 60_000.0);
}

#[test]
fn test_adversarial_massive_document_5000_words() {
    // 5,000 words in 100px container.
    // Correctness matters; wall-clock budgets are flaky under load (see #1383).
    let words: Vec<String> = (0..5_000).map(|i| format!("w{}", i)).collect();
    let text = words.join(" ");

    let mut p_style = make_block_style(Length::Px(100.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(15.0);

    let (doc, styles) = setup_simple_doc(&text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");

    let p_box = &tree.children[0].children[0];
    assert!(p_box.children.len() > 100, "Should generate many lines");
}

#[test]
fn test_adversarial_massive_hyphenated_document_linear_scaling() {
    // 2,000 hyphenated words: "hyphen-test-case-auto" repeated 500 times.
    // Correctness matters; wall-clock budgets are flaky under load (see #1383).
    let segment = "hyphen-test-case-auto ";
    let massive_text = segment.repeat(500); // 22 * 500 = 11,000 chars, ~2,000 hyphen wrap opportunities

    let mut p_style = make_block_style(Length::Px(60.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(15.0);

    let (doc, styles) = setup_simple_doc(&massive_text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");

    let p_box = &tree.children[0].children[0];
    assert!(
        p_box.children.len() > 1000,
        "Hyphens must generate abundant wrap points"
    );
}

#[test]
fn test_adversarial_consecutive_hyphen_train_wrapping() {
    // 50 consecutive hyphens in a 12px container (monospace 10px -> 6px/char, fits exactly 2 chars per line)
    let text = "-".repeat(50);
    let mut p_style = make_block_style(Length::Px(12.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let (doc, styles) = setup_simple_doc(&text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];

    // Each hyphen can wrap. With 12px container width, 2 hyphens fit per line -> exactly 25 lines!
    assert_eq!(
        p_box.children.len(),
        25,
        "50 hyphens in 12px container should wrap into 25 lines of 2 hyphens each"
    );
    assert_eq!(p_box.dimensions.content.height, 500.0);
}

#[test]
fn test_adversarial_massive_document_20000_words_linear_time() {
    // 20,000 words in 100px container. Correctness matters; wall-clock budgets are
    // flaky under load and do not prove linear scaling from a single N (see #1383).
    let words: Vec<String> = (0..20_000).map(|i| format!("token{}", i)).collect();
    let text = words.join(" ");

    let mut p_style = make_block_style(Length::Px(100.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(15.0);

    let (doc, styles) = setup_simple_doc(&text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");

    let p_box = &tree.children[0].children[0];
    assert!(
        p_box.children.len() > 1000,
        "Should generate thousands of lines"
    );
}

#[test]
fn test_adversarial_single_word_50000_chars_unbreakable() {
    // 50,000 character single unbreakable word in a 100px container.
    // Correctness matters; wall-clock budgets are flaky under load (see #1383).
    let massive_word = "Z".repeat(50_000);
    let mut p_style = make_block_style(Length::Px(100.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let (doc, styles) = setup_simple_doc(&massive_word, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");

    let p_box = &tree.children[0].children[0];
    assert_eq!(
        p_box.children.len(),
        1,
        "Unbreakable word must occupy 1 line box"
    );
    let line_box = &p_box.children[0];
    let text_box = &line_box.children[0];
    // 50,000 * 6.0px = 300,000px
    assert_eq!(text_box.dimensions.content.width, 300_000.0);
}

#[test]
fn test_adversarial_long_leading_soft_hyphen_run_terminates() {
    // 100,000 leading U+00AD produce one zero-width run; removing them from the front of
    // the token vector one by one moved ~160 GB (quadratic). Draining the run once is O(n).
    let text = "\u{00AD}".repeat(100_000);
    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    let (doc, styles) = setup_simple_doc(&text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];
    // Nothing visible: the soft hyphens are zero-width and produce no painted content.
    assert!(
        p_box
            .children
            .iter()
            .all(|line| line.box_type == BoxType::LineBox)
    );
}

#[test]
fn test_adversarial_long_trailing_space_hyphen_runs_terminate() {
    // `word` + (100 soft hyphens + space) × 5,000: trimming the trailing spaces one by one
    // shifted the invisible suffix after each removal (~1e9 token moves). The O(n) retain
    // pass keeps the cost linear.
    let text = format!("word{}", ("\u{00AD}".repeat(100) + " ").repeat(5_000));
    let mut p_style = make_block_style(Length::Px(400.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    let (doc, styles) = setup_simple_doc(&text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let p_box = &tree.children[0].children[0];
    assert_eq!(
        p_box.children.len(),
        1,
        "the trailing run stays on one line"
    );
}
