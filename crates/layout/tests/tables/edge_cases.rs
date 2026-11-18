use super::common::*;

#[test]
fn test_table_inside_inline_is_laid_out() {
    let (mut doc, mut styles, body) = setup();
    let span = mk(&mut doc, body, "span");
    let mut inline_style = ComputedStyle::initial();
    inline_style.display = Display::Inline;
    styles.insert(span, inline_style);

    let table = mk(&mut doc, span, "table");
    styles.insert(table, table_style(Length::Px(200.0)));
    let row = add_row(&mut doc, &mut styles, table);
    let _cell = add_cell(&mut doc, &mut styles, row, "hello", None);

    let tree = layout_tree(&doc, &styles);
    let mut texts = Vec::new();
    collect_text_boxes(&tree, &mut texts);
    assert_eq!(texts.len(), 1, "table inside inline must expose its text");
    assert!(texts[0].1.width > 0.0 && texts[0].1.height > 0.0);
}

/// CSS 2.1 §17.5.2: an empty cell contributes zero intrinsic width, so a table whose cells
/// hold nothing must not grow past its borders/spacing (#695).
#[test]
fn empty_cells_contribute_no_intrinsic_width_floor() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Auto));
    let row = mk(&mut doc, table, "tr");
    styles.insert(row, row_style());
    for _ in 0..2 {
        let cell = mk(&mut doc, row, "td");
        styles.insert(cell, cell_style());
    }
    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    // border-spacing separate: 4px left + 4px gap + 4px right = 12px of spacing, and each
    // empty cell used to add its own 1px floor on top of that.
    let width = table_box.dimensions.border_box().width;
    assert!(
        width <= 12.5,
        "empty cells must not add a 1px floor per column, got {width}"
    );
}

#[test]
fn a_single_character_still_gives_the_column_a_width() {
    // Guard against over-correcting: real content must still produce a track.
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Auto));
    let row = mk(&mut doc, table, "tr");
    styles.insert(row, row_style());
    let cell = mk(&mut doc, row, "td");
    styles.insert(cell, cell_style());
    doc.append_text(cell, "x");
    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let cell_box = &table_box.children[0].children[0];
    assert!(
        (cell_box.dimensions.content.width - 6.0).abs() < 0.01,
        "a single character 'x' at 10px mono should give ~6px width, got {}",
        cell_box.dimensions.content.width
    );
}

#[test]
fn huge_colspans_are_capped_to_a_bounded_column_count() {
    let huge = layout_single_colspan("1000");
    let capped = layout_single_colspan("512");
    assert_eq!(
        huge, capped,
        "colspan=1000 must clamp to the 512-column cap, not allocate 1000 tracks"
    );
    // Sanity: a span that fits under the cap is honoured as-is.
    let small = layout_single_colspan("3");
    assert!(
        small.0 > 0.0 && small.1 > 0.0 && small.1.is_finite(),
        "ordinary colspans keep their geometry: {small:?}"
    );
}

/// An absolutely positioned child of a table must be laid out, not left at its provisional
/// zero size with an unlaid subtree (#694).
#[test]
fn absolutely_positioned_table_child_is_laid_out() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Px(200.0)));
    let row = mk(&mut doc, table, "tr");
    styles.insert(row, row_style());
    let cell = mk(&mut doc, row, "td");
    styles.insert(cell, cell_style());
    doc.append_text(cell, "cell");

    let overlay = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(table, overlay);
    let mut overlay_style = block_style(Length::Auto);
    overlay_style.position = Position::Absolute;
    overlay_style.width = Length::Px(60.0);
    overlay_style.height = Length::Px(30.0);
    styles.insert(overlay, overlay_style.clone());
    let inner = mk(&mut doc, overlay, "div");
    let mut inner_style = block_style(Length::Auto);
    inner_style.width = Length::Px(20.0);
    inner_style.height = Length::Px(10.0);
    styles.insert(inner, inner_style);

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let overlay_box = find_box(table_box, overlay);
    assert_eq!(
        overlay_box.dimensions.content.width, 60.0,
        "abspos table child must get its used width"
    );
    assert_eq!(overlay_box.dimensions.content.height, 30.0);
    let inner_box = find_box(overlay_box, inner);
    assert_eq!(
        inner_box.dimensions.content.width, 20.0,
        "the abspos subtree must be laid out, not just positioned"
    );
    assert_eq!(inner_box.dimensions.content.height, 10.0);
}
