//! # IFC conformance tests (CSS 2.2 §9.4.2, §10.8, §16; CSS Text 3 §4–§5)
//!
//! Covers the second IFC conformance milestone (WP-4.5):
//!
//! 1. line breaking only at valid soft wrap opportunities (audit F-09: adjacent
//!    inline boxes without a space form one unbreakable run);
//! 2. soft hyphen (U+00AD) invisible unless the line breaks at it (audit F-15);
//! 3. `line-height: <number>` inherited as a factor and resolved per element
//!    (audit F-12);
//! 4. `vertical-align: top|bottom` aligned subtrees (audit F-19);
//! 5. `text-align: justify` for soft-wrapped lines.
//!
//! The DOM is built by hand, as in the rest of the layout test suites.

use std::collections::HashMap;

use css::{ComputedStyle, Display, Length, TextAlign, VerticalAlign, WhiteSpace};
use dom::{Document, Namespace, NodeId};
use layout::{BoxType, LayoutBox, Rect, layout_document};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_block_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style.height = height;
    style
}

fn make_inline_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Inline;
    style
}

/// Deterministic monospace metrics: 10px font-size → 6px per character, line-height 20.
fn mono_style(is_block: bool, width: Length) -> ComputedStyle {
    let mut style = if is_block {
        make_block_style(width, Length::Auto)
    } else {
        make_inline_style()
    };
    style.font_family = "monospace".to_string();
    style.font_size = 10.0;
    style.line_height = Some(20.0);
    style
}

/// Creates `<html><body>` with zeroed styles.
fn setup_document() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    (doc, styles, html, body)
}

fn make_element(doc: &mut Document, parent: NodeId, tag: &str) -> NodeId {
    let id = doc.create_element(Namespace::Html, tag, vec![]);
    doc.append_child(parent, id);
    id
}

fn layout_doc(doc: &Document, styles: &HashMap<NodeId, ComputedStyle>) -> LayoutBox {
    layout_document(doc, styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout must succeed")
}

/// Returns the `<p>` box for a document built as `<html><body><p>…</p></body></html>`.
fn paragraph_box(tree: &LayoutBox) -> &LayoutBox {
    &tree.children[0].children[0]
}

/// Concatenates the text of every `TextNode` descendant of a line box, in visual order.
fn line_text(line: &LayoutBox) -> String {
    let mut text = String::new();
    collect_text(line, &mut text);
    text
}

fn collect_text(node: &LayoutBox, out: &mut String) {
    match &node.box_type {
        BoxType::TextNode(value) => out.push_str(value),
        _ => {
            for child in &node.children {
                collect_text(child, out);
            }
        }
    }
}

/// Collects every `TextNode` box of a line box, in visual order.
fn text_fragments<'a>(node: &'a LayoutBox, out: &mut Vec<&'a LayoutBox>) {
    if matches!(node.box_type, BoxType::TextNode(_)) {
        out.push(node);
        return;
    }
    for child in &node.children {
        text_fragments(child, out);
    }
}

/// Collects only the visible (non-space) text fragments of a line box, in visual order.
fn word_fragments<'a>(node: &'a LayoutBox, out: &mut Vec<&'a LayoutBox>) {
    let mut all = Vec::new();
    text_fragments(node, &mut all);
    out.extend(all.into_iter().filter(|fragment| match &fragment.box_type {
        BoxType::TextNode(text) => !text.trim().is_empty(),
        _ => false,
    }));
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 0.001,
        "{what}: expected {expected}, got {actual}"
    );
}

// ---------------------------------------------------------------------------
// 1. F-09: line breaks only at valid soft wrap opportunities
// ---------------------------------------------------------------------------

#[test]
fn test_adjacent_inline_boxes_without_space_are_unbreakable() {
    // p width 40px; mono 10px (6px/char); each span holds 5 chars = 30px.
    // "aaaaabbbbb" is a single word: there is no soft wrap opportunity between the
    // two spans, so it must stay on one (overflowing) line box (F-09).
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    let span1 = make_element(&mut doc, p, "span");
    doc.append_text(span1, "aaaaa");
    let span2 = make_element(&mut doc, p, "span");
    doc.append_text(span2, "bbbbb");

    styles.insert(p, mono_style(true, Length::Px(40.0)));
    styles.insert(span1, mono_style(false, Length::Auto));
    styles.insert(span2, mono_style(false, Length::Auto));

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    assert_eq!(
        p_box.children.len(),
        1,
        "adjacent inline boxes without a space must not be split into two lines"
    );
    let line = &p_box.children[0];
    assert_eq!(line_text(line), "aaaaabbbbb");
    // Both inline fragments stay on the same line.
    assert_eq!(
        line.children.len(),
        2,
        "both span fragments on the same line"
    );
    assert_close(
        line.children[0].dimensions.content.width,
        30.0,
        "span1 width",
    );
    assert_close(
        line.children[1].dimensions.content.width,
        30.0,
        "span2 width",
    );
    // The line overflows the 40px container instead of breaking (60px of text).
    let mut fragments = Vec::new();
    text_fragments(line, &mut fragments);
    assert_close(fragments[0].dimensions.content.x, 0.0, "first span x");
    assert_close(fragments[1].dimensions.content.x, 30.0, "second span x");
    assert_close(
        fragments[1].dimensions.content.x + fragments[1].dimensions.content.width,
        60.0,
        "unbreakable run width",
    );
}

#[test]
fn test_line_break_at_space_between_inline_boxes() {
    // Same content but with a real space: the space is a soft wrap opportunity, so the
    // line breaks there (only there; the words themselves are not split).
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    let span1 = make_element(&mut doc, p, "span");
    doc.append_text(span1, "aaaaa");
    doc.append_text(p, " ");
    let span2 = make_element(&mut doc, p, "span");
    doc.append_text(span2, "bbbbb");

    styles.insert(p, mono_style(true, Length::Px(40.0)));
    styles.insert(span1, mono_style(false, Length::Auto));
    styles.insert(span2, mono_style(false, Length::Auto));

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    assert_eq!(
        p_box.children.len(),
        2,
        "space must provide a break opportunity"
    );
    assert_eq!(line_text(&p_box.children[0]), "aaaaa");
    assert_eq!(line_text(&p_box.children[1]), "bbbbb");
}

#[test]
fn test_word_split_across_nested_inline_boxes_is_not_broken() {
    // <b>aaa</b><i>bbb</i>: word split by inline markup must wrap only at spaces.
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    let bold = make_element(&mut doc, p, "b");
    doc.append_text(bold, "aaa");
    let italic = make_element(&mut doc, p, "i");
    doc.append_text(italic, "bbb");
    doc.append_text(p, " ccc");

    styles.insert(p, mono_style(true, Length::Px(60.0)));
    styles.insert(bold, mono_style(false, Length::Auto));
    styles.insert(italic, mono_style(false, Length::Auto));

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    // "aaabbb" (36px) + space (6px) + "ccc" (18px) = 60px: fits exactly on one line.
    assert_eq!(p_box.children.len(), 1);
    assert_eq!(line_text(&p_box.children[0]), "aaabbb ccc");
}

// ---------------------------------------------------------------------------
// 2. F-15: soft hyphen is invisible unless the line breaks at it
// ---------------------------------------------------------------------------

#[test]
fn test_soft_hyphen_invisible_when_line_is_not_broken() {
    // Wide container: the soft hyphen must contribute zero width and no glyph.
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    doc.append_text(p, "super\u{00AD}cali");
    styles.insert(p, mono_style(true, Length::Px(200.0)));

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    assert_eq!(p_box.children.len(), 1);
    let line = &p_box.children[0];
    assert_eq!(
        line_text(line),
        "supercali",
        "soft hyphen must not be painted when the line is not broken"
    );
    // 9 visible characters * 6px = 54px (no extra width for the SHY).
    let mut fragments = Vec::new();
    text_fragments(line, &mut fragments);
    let total_width: f32 = fragments.iter().map(|f| f.dimensions.content.width).sum();
    assert_close(total_width, 54.0, "width without soft hyphen");
}

#[test]
fn test_soft_hyphen_renders_hyphen_only_when_broken() {
    // 40px container: "super" + a rendered "-" is 6 chars = 36px (fits), "cali" = 24px.
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    doc.append_text(p, "super\u{00AD}cali");
    styles.insert(p, mono_style(true, Length::Px(40.0)));

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    assert_eq!(
        p_box.children.len(),
        2,
        "line must break at the soft hyphen"
    );
    assert_eq!(
        line_text(&p_box.children[0]),
        "super-",
        "the broken line must render a real hyphen"
    );
    assert_eq!(line_text(&p_box.children[1]), "cali");
    assert_close(
        p_box.children[0].children[0].dimensions.content.width,
        36.0,
        "broken line width (6 chars, including the rendered hyphen)",
    );
}

#[test]
fn test_soft_hyphen_invisible_under_nowrap() {
    // `white-space: nowrap` disables wrapping: the SHY cannot produce a hyphen.
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    doc.append_text(p, "super\u{00AD}cali");
    let mut p_style = mono_style(true, Length::Px(40.0));
    p_style.white_space = WhiteSpace::Nowrap;
    styles.insert(p, p_style);

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    assert_eq!(p_box.children.len(), 1, "nowrap must not break the word");
    assert_eq!(line_text(&p_box.children[0]), "supercali");
}

#[test]
fn test_soft_hyphen_invisible_under_pre() {
    // `white-space: pre` preserves text but disables wrapping: the SHY is invisible.
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    doc.append_text(p, "a\u{00AD}b");
    let mut p_style = mono_style(true, Length::Px(200.0));
    p_style.white_space = WhiteSpace::Pre;
    styles.insert(p, p_style);

    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    assert_eq!(p_box.children.len(), 1);
    assert_eq!(line_text(&p_box.children[0]), "ab");
}

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

// ---------------------------------------------------------------------------
// 5. `text-align: justify` (CSS 2.2 §16.2)
// ---------------------------------------------------------------------------

fn setup_justify_document(width: f32, text: &str) -> (Document, HashMap<NodeId, ComputedStyle>) {
    let (mut doc, mut styles, _html, body) = setup_document();
    let p = make_element(&mut doc, body, "p");
    // Three differently-coloured spans so the text runs do not merge; spaces between.
    let words: Vec<&str> = text.split(' ').collect();
    for (index, word) in words.iter().enumerate() {
        if index > 0 {
            doc.append_text(p, " ");
        }
        let span = make_element(&mut doc, p, "span");
        doc.append_text(span, word);
        let mut span_style = mono_style(false, Length::Auto);
        span_style.color = css::Color::new(0, 0, (index as u8 + 1) * 40, 255);
        styles.insert(span, span_style);
    }
    let mut p_style = mono_style(true, Length::Px(width));
    p_style.text_align = TextAlign::Justify;
    styles.insert(p, p_style);
    (doc, styles)
}

#[test]
fn test_text_align_justify_single_line_is_not_justified() {
    // A single-line paragraph is its own last line, so `text-align: justify` leaves it
    // left-aligned (CSS 2.2 §16.2). Natural positions are used instead of stretching.
    let (doc, styles) = setup_justify_document(60.0, "aa bb cc");
    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    assert_eq!(p_box.children.len(), 1, "should fit on a single line");
    let line = &p_box.children[0];

    let mut fragments = Vec::new();
    word_fragments(line, &mut fragments);
    assert_eq!(fragments.len(), 3);
    assert_close(fragments[0].dimensions.content.x, 0.0, "word 1 x");
    assert_close(fragments[1].dimensions.content.x, 18.0, "word 2 x (12 + 6)");
    assert_close(
        fragments[2].dimensions.content.x,
        36.0,
        "word 3 x (18 + 12 + 6)",
    );
    let last = fragments.last().expect("fragments");
    assert_close(
        last.dimensions.content.x + last.dimensions.content.width,
        48.0,
        "single (last) line keeps its natural width",
    );
}

#[test]
fn test_text_align_justify_does_not_touch_last_line() {
    // 40px container: line 1 is the soft-wrapped "aa bb" (justified to 40px),
    // line 2 is the last line "cc" and stays left-aligned.
    let (doc, styles) = setup_justify_document(40.0, "aa bb cc");
    let tree = layout_doc(&doc, &styles);
    let p_box = paragraph_box(&tree);
    assert_eq!(p_box.children.len(), 2, "must soft-wrap into two lines");

    let mut first_line_fragments = Vec::new();
    word_fragments(&p_box.children[0], &mut first_line_fragments);
    assert_eq!(first_line_fragments.len(), 2);
    assert_close(
        first_line_fragments[0].dimensions.content.x,
        0.0,
        "line 1 word 1",
    );
    // Line 1: 12 + (6 + 10) => word 2 at 28px and the line reaches 40px.
    assert_close(
        first_line_fragments[1].dimensions.content.x,
        28.0,
        "line 1 word 2",
    );
    let last = first_line_fragments.last().expect("line 1 fragments");
    assert_close(
        last.dimensions.content.x + last.dimensions.content.width,
        40.0,
        "line 1 fully justified",
    );

    let mut last_line_fragments = Vec::new();
    word_fragments(&p_box.children[1], &mut last_line_fragments);
    assert_eq!(
        last_line_fragments.len(),
        1,
        "second line has a single word"
    );
    assert_close(
        last_line_fragments[0].dimensions.content.x,
        0.0,
        "last line stays left-aligned",
    );
}
