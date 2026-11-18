use super::*;

// ---------------------------------------------------------------------------
// F-02: anonymous boxes / text nodes must only carry inherited properties
// ---------------------------------------------------------------------------

#[test]
fn test_anonymous_blocks_do_not_inherit_non_inherited_properties() {
    // body has a UA-like 8px margin, a red background and a green 20px font.
    // Its mixed content (text + div) forces an AnonymousBlock wrapper around the text.
    let (mut doc, mut styles, _html, body) = setup_document();
    doc.append_text(body, "Hello");
    let div = make_div(&mut doc, body);

    {
        let body_style = styles.get_mut(&body).expect("body style");
        body_style.margin = RectEdges::all(Length::Px(8.0));
        body_style.background_color = Color::RED;
        body_style.color = Color::GREEN;
        body_style.font_size = 20.0;
    }
    styles.insert(div, make_block_style(Length::Auto, Length::Auto));

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let body_box = &tree.children[0];
    let anon = &body_box.children[0];

    // Mixed inline/block content is wrapped in an anonymous block...
    assert_eq!(anon.box_type, BoxType::AnonymousBlock);
    // ...which must have initial (zero) margins/paddings/borders and no background.
    assert_eq!(anon.dimensions.margin, megi_layout::EdgeSizes::ZERO);
    assert_eq!(anon.dimensions.padding, megi_layout::EdgeSizes::ZERO);
    assert_eq!(anon.dimensions.border, megi_layout::EdgeSizes::ZERO);
    let anon_style = anon.style.as_ref().expect("anonymous style");
    assert_eq!(anon_style.background_color, Color::TRANSPARENT);
    // Inherited properties are still inherited from the parent.
    assert_eq!(anon_style.color, Color::GREEN);
    assert_close(anon_style.font_size, 20.0, "anonymous inherited font-size");

    // The anonymous block spans exactly the body content box: no double indentation.
    assert_close(anon.dimensions.content.x, 8.0, "anonymous block x");
    assert_close(
        anon.dimensions.content.width,
        784.0,
        "anonymous block width",
    );

    // The text line starts at the body content edge, not indented twice.
    let line_box = &anon.children[0];
    assert_eq!(line_box.box_type, BoxType::LineBox);
    assert_close(line_box.dimensions.content.x, 8.0, "line box x");
    assert_close(line_box.dimensions.content.width, 784.0, "line box width");

    // Text boxes are formatting artifacts: inherited font/colour only, no decoration.
    let text_box = &line_box.children[0];
    assert!(matches!(text_box.box_type, BoxType::TextNode(_)));
    let text_style = text_box.style.as_ref().expect("text style");
    assert_eq!(text_style.background_color, Color::TRANSPARENT);
    assert_eq!(text_style.color, Color::GREEN);
    assert_close(text_style.font_size, 20.0, "text font-size");
    assert_eq!(text_box.dimensions.margin, megi_layout::EdgeSizes::ZERO);
}

#[test]
fn test_pretty_printed_blocks_have_no_phantom_vertical_gap() {
    // Pretty-printed HTML: whitespace-only text nodes between block-level siblings.
    let (mut doc, mut styles, _html, body) = setup_document();
    let div1 = make_div(&mut doc, body);
    doc.append_text(body, "\n  ");
    let div2 = make_div(&mut doc, body);
    doc.append_text(body, "\n\t");

    styles.get_mut(&body).expect("body style").margin = RectEdges::all(margin_all(8.0));
    for div in [div1, div2] {
        styles.insert(div, make_block_style(Length::Auto, Length::Px(10.0)));
    }

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let body_box = &tree.children[0];

    // The whitespace-only inline runs are insignificant between blocks.
    assert_eq!(
        body_box.children.len(),
        2,
        "whitespace runs must not create boxes"
    );
    let box1 = &body_box.children[0];
    let box2 = &body_box.children[1];

    // body margin-top 8: content starts at y = 8, the first div is flush with it.
    assert_close(box1.dimensions.content.y, 8.0, "first block y");
    // Second div stacks immediately below the first: 8 + 10 = 18 (no phantom gap).
    assert_close(box2.dimensions.content.y, 18.0, "second block y");
    assert_close(body_box.dimensions.content.height, 20.0, "body height");
    assert_close(
        tree.dimensions.content.height,
        36.0,
        "html height (body margin box)",
    );

    // Both blocks span the body content box: 800 - 2*8 = 784.
    assert_close(box1.dimensions.content.x, 8.0, "first block x");
    assert_close(box1.dimensions.content.width, 784.0, "first block width");
}

// ---------------------------------------------------------------------------
// CSS 2.2 §8.3.1: collapsing margins
// ---------------------------------------------------------------------------

#[test]
fn test_sibling_margins_collapse_to_max() {
    let (mut doc, mut styles, html) = setup_html_only();
    // html > wrapper > div1, div2 (wrapper isolates the test from the root element).
    let wrapper = make_div(&mut doc, html);
    let div1 = make_div(&mut doc, wrapper);
    let div2 = make_div(&mut doc, wrapper);

    let mut s1 = make_block_style(Length::Auto, Length::Px(10.0));
    s1.margin.bottom = Length::Px(20.0);
    let mut s2 = make_block_style(Length::Auto, Length::Px(10.0));
    s2.margin.top = Length::Px(20.0);
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Auto));
    styles.insert(div1, s1);
    styles.insert(div2, s2);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    let box1 = &wrapper_box.children[0];
    let box2 = &wrapper_box.children[1];

    assert_close(box1.dimensions.content.y, 0.0, "first sibling y");
    // max(20, 20) == 20, not 40.
    assert_close(box2.dimensions.content.y, 30.0, "second sibling y");
    let separation =
        box2.dimensions.content.y - (box1.dimensions.content.y + box1.dimensions.content.height);
    assert_close(separation, 20.0, "collapsed sibling separation");
    // Auto height of the wrapper: last child's border box (no bottom margin escape).
    assert_close(
        wrapper_box.dimensions.content.height,
        40.0,
        "wrapper height",
    );
}

#[test]
fn test_parent_child_top_margins_collapse() {
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);

    let mut wrapper_style = make_block_style(Length::Auto, Length::Auto);
    wrapper_style.margin.top = Length::Px(10.0);
    let mut child_style = make_block_style(Length::Auto, Length::Px(10.0));
    child_style.margin.top = Length::Px(20.0);
    styles.insert(wrapper, wrapper_style);
    styles.insert(child, child_style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    let child_box = &wrapper_box.children[0];

    // collapsed margin = max(10, 20) = 20 applied above the wrapper; the child starts at
    // the wrapper's content top (its own 20px margin does not apply twice).
    assert_close(wrapper_box.dimensions.content.y, 20.0, "wrapper y");
    assert_close(child_box.dimensions.content.y, 20.0, "child y");
    assert_close(
        wrapper_box.dimensions.content.height,
        10.0,
        "wrapper height",
    );
}

#[test]
fn test_parent_padding_prevents_top_margin_collapse() {
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);

    let mut wrapper_style = make_block_style(Length::Auto, Length::Auto);
    wrapper_style.margin.top = Length::Px(10.0);
    wrapper_style.padding.top = Length::Px(5.0);
    let mut child_style = make_block_style(Length::Auto, Length::Px(10.0));
    child_style.margin.top = Length::Px(20.0);
    styles.insert(wrapper, wrapper_style);
    styles.insert(child, child_style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    let child_box = &wrapper_box.children[0];

    // The wrapper's padding blocks collapsing: 10px above, then 5px padding, then the
    // child's own 20px margin inside.
    assert_close(
        wrapper_box.dimensions.content.y,
        15.0,
        "wrapper content y (10 margin + 5 padding)",
    );
    assert_close(child_box.dimensions.content.y, 15.0 + 20.0, "child y");
}

#[test]
fn test_negative_margins_collapse_max_positive_plus_min_negative() {
    // Pair 1: +20 and -10 => 10 (max positive + min negative).
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let div1 = make_div(&mut doc, wrapper);
    let div2 = make_div(&mut doc, wrapper);

    let mut s1 = make_block_style(Length::Auto, Length::Px(10.0));
    s1.margin.bottom = Length::Px(-10.0);
    let mut s2 = make_block_style(Length::Auto, Length::Px(10.0));
    s2.margin.top = Length::Px(20.0);
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Auto));
    styles.insert(div1, s1);
    styles.insert(div2, s2);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    assert_close(
        wrapper_box.children[1].dimensions.content.y,
        20.0,
        "mixed-sign collapsed margin (+20, -10)",
    );

    // Pair 2: -5 and -10 => -10 (most negative when all margins are negative).
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let div1 = make_div(&mut doc, wrapper);
    let div2 = make_div(&mut doc, wrapper);

    let mut s1 = make_block_style(Length::Auto, Length::Px(10.0));
    s1.margin.bottom = Length::Px(-5.0);
    let mut s2 = make_block_style(Length::Auto, Length::Px(10.0));
    s2.margin.top = Length::Px(-10.0);
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Auto));
    styles.insert(div1, s1);
    styles.insert(div2, s2);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    assert_close(
        wrapper_box.children[1].dimensions.content.y,
        0.0,
        "negative collapsed margin (-5, -10)",
    );
}

#[test]
fn test_bottom_margin_does_not_collapse_through_padding_or_border() {
    // Parent without bottom padding: the last child's bottom margin escapes and the
    // parent's auto height excludes it.
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);
    let sibling = make_div(&mut doc, html);

    let mut child_style = make_block_style(Length::Auto, Length::Px(10.0));
    child_style.margin.bottom = Length::Px(30.0);
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Auto));
    styles.insert(child, child_style);
    styles.insert(sibling, make_block_style(Length::Auto, Length::Px(5.0)));

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    let sibling_box = &tree.children[1];
    assert_close(
        wrapper_box.dimensions.content.height,
        10.0,
        "wrapper height",
    );
    // The escaped 30px margin separates the wrapper from the next sibling.
    assert_close(sibling_box.dimensions.content.y, 40.0, "sibling y");

    // Parent with bottom padding: no collapsing, the margin is part of the parent height.
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);
    let sibling = make_div(&mut doc, html);

    let mut wrapper_style = make_block_style(Length::Auto, Length::Auto);
    wrapper_style.padding.bottom = Length::Px(5.0);
    let mut child_style = make_block_style(Length::Auto, Length::Px(10.0));
    child_style.margin.bottom = Length::Px(30.0);
    styles.insert(wrapper, wrapper_style);
    styles.insert(child, child_style);
    styles.insert(sibling, make_block_style(Length::Auto, Length::Px(5.0)));

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    let sibling_box = &tree.children[1];
    assert_close(
        wrapper_box.dimensions.content.height,
        40.0,
        "wrapper height includes child margin (10 + 30)",
    );
    // 10 content + 30 margin + 5 padding = 45.
    assert_close(sibling_box.dimensions.content.y, 45.0, "sibling y");
}

// ---------------------------------------------------------------------------
// CSS 2.2 §10.1 / §10.5: initial containing block and percentage heights
// ---------------------------------------------------------------------------

#[test]
fn test_initial_containing_block_uses_viewport_height() {
    let (doc, mut styles, html, body) = setup_document();
    styles.get_mut(&html).expect("html style").height = Length::Percent(100.0);
    styles.get_mut(&body).expect("body style").height = Length::Percent(100.0);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    // html: 100% of the ICB (viewport) height.
    assert_close(
        tree.dimensions.content.height,
        600.0,
        "root percentage height",
    );
    // body: 100% of html's definite content height.
    assert_close(
        tree.children[0].dimensions.content.height,
        600.0,
        "body percentage height",
    );
}

#[test]
fn test_percentage_height_resolves_against_fixed_parent() {
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);

    let mut child_style = make_block_style(Length::Auto, Length::Percent(50.0));
    child_style.display = Display::Block;
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Px(300.0)));
    styles.insert(child, child_style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    assert_close(
        wrapper_box.dimensions.content.height,
        300.0,
        "wrapper height",
    );
    assert_close(
        wrapper_box.children[0].dimensions.content.height,
        150.0,
        "50% height resolves against 300px parent",
    );
}

#[test]
fn test_percentage_height_against_auto_parent_is_auto() {
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);
    styles.insert(wrapper, make_block_style(Length::Auto, Length::Auto));
    styles.insert(child, make_block_style(Length::Auto, Length::Percent(50.0)));

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let wrapper_box = &tree.children[0];
    // The parent height is auto, so the percentage behaves as auto; the empty child has
    // an intrinsic height of 0.
    assert_close(
        wrapper_box.children[0].dimensions.content.height,
        0.0,
        "percentage height against auto parent",
    );
    assert_close(
        wrapper_box.dimensions.content.height,
        0.0,
        "auto wrapper height",
    );
}
