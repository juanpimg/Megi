use super::common::*;

#[test]
fn test_rowspan_distributes_row_heights() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Px(200.0)));

    // Row 0: tall cell with rowspan=3 plus a normal cell.
    let row0 = add_row(&mut doc, &mut styles, table);
    let tall = doc.create_element(Namespace::Html, "td", vec![Attribute::new("rowspan", "3")]);
    doc.append_child(row0, tall);
    let mut tall_style = cell_style();
    tall_style.height = Length::Px(100.0);
    styles.insert(tall, tall_style);
    add_cell(&mut doc, &mut styles, row0, "aa", None);
    let row1 = add_row(&mut doc, &mut styles, table);
    add_cell(&mut doc, &mut styles, row1, "bb", None);
    let row2 = add_row(&mut doc, &mut styles, table);
    add_cell(&mut doc, &mut styles, row2, "cc", None);

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let rows: Vec<&LayoutBox> = table_box.children.iter().collect();
    assert_eq!(rows.len(), 3);
    for row in &rows {
        // 100px rowspan cell over 3 rows of 20px + two 4px gaps = 68px: the 40px
        // deficit is shared equally (13.333 each) => every row is 33.333px tall.
        assert!(
            (row.dimensions.content.height - 33.3333).abs() < 0.01,
            "rowspan must distribute the row heights, got {}",
            row.dimensions.content.height
        );
    }
    // The rowspan cell is not duplicated and its border box spans the three rows.
    assert_eq!(rows[0].children.len(), 2, "row 0 keeps its two cells");
    assert_eq!(
        rows[1].children.len(),
        1,
        "rowspan cell belongs to row 0 only"
    );
    // Column 0 holds only the empty rowspan cell, so its intrinsic width is 0 rather than
    // the old 1px floor (#695): the 200px table splits the free space evenly (88 + 100)
    // and the second column now starts at 96 instead of 96.5.
    assert_eq!(rows[1].children[0].dimensions.content.x, 96.0);
    let tall_box = &rows[0].children[0];
    let expected_height = 3.0 * 33.3333 + 2.0 * 4.0;
    assert!(
        (tall_box.dimensions.content.height - expected_height).abs() < 0.01,
        "rowspan cell height: expected ~{}, got {}",
        expected_height,
        tall_box.dimensions.content.height
    );
    // Table height = 4 (top spacing) + 3 rows + 2 inner spacings + 4 (bottom spacing).
    let expected_table_height = 4.0 + 3.0 * 33.3333 + 2.0 * 4.0 + 4.0;
    assert!((table_box.dimensions.content.height - expected_table_height).abs() < 0.05);
}

#[test]
fn test_colspan_and_rowspan_combined_cell() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Px(200.0)));
    let row0 = add_row(&mut doc, &mut styles, table);
    let cell = doc.create_element(
        Namespace::Html,
        "td",
        vec![
            Attribute::new("colspan", "2"),
            Attribute::new("rowspan", "2"),
        ],
    );
    doc.append_child(row0, cell);
    styles.insert(cell, cell_style());
    doc.append_text(cell, "x");

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    // Only one real row exists in the DOM; rowspan is clamped to the available rows.
    assert_eq!(table_box.children.len(), 1);
    let row = &table_box.children[0];
    assert_eq!(row.children.len(), 1);
    let grid_cell = &row.children[0];
    // Two columns: (200 - 3*4)/2 = 94 each; the cell spans 94 + 4 + 94 = 192px.
    assert!((grid_cell.dimensions.content.width - 192.0).abs() < 0.01);
    // The grid cell is the only content of the only row (no phantom rows created).
    assert!(
        (table_box.dimensions.content.height - 28.0).abs() < 0.01,
        "one row of 20px + two spacings of 4px"
    );
}

#[test]
fn test_table_layout_fixed_uses_declared_and_equal_widths() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    let mut fixed_table_style = table_style(Length::Px(200.0));
    fixed_table_style.table_layout = TableLayout::Fixed;
    styles.insert(table, fixed_table_style);
    let row0 = add_row(&mut doc, &mut styles, table);
    let specified = add_cell(&mut doc, &mut styles, row0, "aaaaaa aaaaaa", None);
    let mut specified_style = cell_style();
    specified_style.width = Length::Px(50.0);
    styles.insert(specified, specified_style);
    add_cell(&mut doc, &mut styles, row0, "bb", None);
    add_cell(&mut doc, &mut styles, row0, "cc", None);

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    assert!((table_box.dimensions.content.width - 200.0).abs() < 0.01);
    let row = &table_box.children[0];
    // col0: explicit 50px; remaining columns share (200 - 4*4 - 50) / 2 = 67px.
    assert!(
        (row.children[0].dimensions.content.width - 50.0).abs() < 0.01,
        "explicit first-row width wins, content (72px) does not widen it"
    );
    assert!((row.children[1].dimensions.content.width - 67.0).abs() < 0.01);
    assert!((row.children[2].dimensions.content.width - 67.0).abs() < 0.01);
    // The content wraps inside the 50px column: two 20px line boxes.
    assert!((row.dimensions.content.height - 40.0).abs() < 0.01);
}

#[test]
fn test_table_layout_auto_lets_content_widen_columns() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Px(300.0)));
    let row0 = add_row(&mut doc, &mut styles, table);
    add_cell(&mut doc, &mut styles, row0, "aaaaaaaaaaaa", None); // 72px of text
    add_cell(&mut doc, &mut styles, row0, "bb", None);

    let tree = layout_tree(&doc, &styles);
    let row = &tree.children[0].children[0].children[0];
    // Auto layout measures content: the long cell gets more than the short one.
    assert!(row.children[0].dimensions.content.width > row.children[1].dimensions.content.width);
}

#[test]
fn test_inline_table_is_atomic_with_first_row_baseline() {
    let (mut doc, mut styles, body) = setup();
    let p = mk(&mut doc, body, "p");
    let mut p_style = block_style(Length::Px(400.0));
    p_style.font_size = 16.0;
    styles.insert(p, p_style);
    doc.append_text(p, "A");
    let inline_table = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(p, inline_table);
    let mut inline_table_style = table_style(Length::Auto);
    inline_table_style.display = Display::InlineTable;
    styles.insert(inline_table, inline_table_style);
    let row = mk(&mut doc, inline_table, "tr");
    styles.insert(row, row_style());
    add_cell(&mut doc, &mut styles, row, "aa", None);
    add_cell(&mut doc, &mut styles, row, "bb", None);

    let tree = layout_tree(&doc, &styles);
    let p_box = &tree.children[0].children[0];
    let line = &p_box.children[0];
    let atomic = line
        .children
        .iter()
        .find(|child| matches!(child.box_type, BoxType::InlineTable(_)))
        .expect("inline-table must be an atomic inline-level box");
    // Auto inline-table width: two 12px columns + three 4px spacings = 36px.
    assert!((atomic.dimensions.content.width - 36.0).abs() < 0.01);
    assert!(
        (atomic.dimensions.content.x - 8.0).abs() < 0.01,
        "after the 8px 'A'"
    );
    // Height = 4 + 20 (row) + 4 = 28; baseline = first row baseline (4 + 13 = 17).
    assert!((atomic.dimensions.content.height - 28.0).abs() < 0.01);
    assert!(
        (atomic.baseline - 17.0).abs() < 0.01,
        "inline-table baseline is its first row baseline, got {}",
        atomic.baseline
    );
    // Line box must be tall enough for the inline-table.
    assert!(line.dimensions.content.height >= 28.0);
}
