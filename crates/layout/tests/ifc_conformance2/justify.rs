use super::*;

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
        span_style.color = megi_css::Color::new(0, 0, (index as u8 + 1) * 40, 255);
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
