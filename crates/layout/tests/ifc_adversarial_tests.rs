//! # Adversarial Verification Test Suite for Inline Formatting Context (IFC)
//!
//! Empirical stress tests challenging:
//! 1. Pathological whitespace inputs (consecutive spaces, mixed tabs/spaces/newlines across normal/nowrap/pre/pre-wrap/pre-line).
//! 2. Massive strings (10,000+ chars) and single massive words exceeding container width.
//! 3. Zero-width containers (content.width == 0.0), negative widths, and floating-point edge cases (NaN, subnormals).
//! 4. Boundary breaks (punctuation, hyphens, slashes, box boundaries).
//! 5. Deeply nested inline box fragmentation and extreme vertical alignments.

use std::collections::HashMap;

use css::{ComputedStyle, Display, Length, VerticalAlign, WhiteSpace};
use dom::{Document, Namespace, NodeId};
use layout::{BoxType, Rect, layout_document};

fn make_block_style(width: Length, height: Length) -> ComputedStyle {
    let mut s = ComputedStyle::initial();
    s.display = Display::Block;
    s.width = width;
    s.height = height;
    s
}

fn make_inline_style() -> ComputedStyle {
    let mut s = ComputedStyle::initial();
    s.display = Display::Inline;
    s
}

fn setup_simple_doc(
    text: &str,
    container_style: ComputedStyle,
) -> (Document, HashMap<NodeId, ComputedStyle>) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_text(p, text);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, container_style);

    (doc, styles)
}

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

#[test]
fn test_adversarial_massive_single_word_unbreakable() {
    // 10,000 character single word in a 100px container
    let massive_word = "A".repeat(10_000);
    let mut p_style = make_block_style(Length::Px(100.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(20.0);

    let start = std::time::Instant::now();
    let (doc, styles) = setup_simple_doc(&massive_word, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let elapsed = start.elapsed();

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

    println!("Massive word (10,000 chars) processed in {:?}", elapsed);
    assert!(elapsed.as_millis() < 500, "Must terminate in under 500ms");
}

#[test]
fn test_adversarial_massive_document_5000_words() {
    // 5,000 words in 100px container
    let words: Vec<String> = (0..5_000).map(|i| format!("w{}", i)).collect();
    let text = words.join(" ");

    let mut p_style = make_block_style(Length::Px(100.0), Length::Auto);
    p_style.font_family = "monospace".to_string();
    p_style.font_size = 10.0;
    p_style.line_height = Some(15.0);

    let start = std::time::Instant::now();
    let (doc, styles) = setup_simple_doc(&text, p_style);
    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, viewport).expect("layout should succeed");
    let elapsed = start.elapsed();

    let p_box = &tree.children[0].children[0];
    assert!(p_box.children.len() > 100, "Should generate many lines");
    println!(
        "5,000 words processed into {} lines in {:?}",
        p_box.children.len(),
        elapsed
    );
    assert!(elapsed.as_secs() < 2, "Must terminate in under 2 seconds");
}

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
}

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
