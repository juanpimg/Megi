use super::*;

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
