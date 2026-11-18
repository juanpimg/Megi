use super::common::*;

#[test]
fn test_border_collapse_resolves_shared_border_width() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    let mut collapsed_style = table_style(Length::Px(200.0));
    collapsed_style.border_collapse = BorderCollapse::Collapse;
    styles.insert(table, collapsed_style);
    let row = add_row(&mut doc, &mut styles, table);
    let cell0 = add_cell(&mut doc, &mut styles, row, "aa", None);
    let mut style0 = collapse_cell_style();
    style0.border_width.right = 2.0;
    styles.insert(cell0, style0);
    let cell1 = add_cell(&mut doc, &mut styles, row, "bb", None);
    let mut style1 = collapse_cell_style();
    style1.border_width.left = 4.0;
    styles.insert(cell1, style1);

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    // Collapse ignores border-spacing: the row starts at the table content top.
    let row_box = &table_box.children[0];
    assert!(
        (row_box.dimensions.content.y - table_box.dimensions.content.y).abs() < 0.01,
        "collapse must ignore border-spacing"
    );
    let cell0_box = &row_box.children[0];
    let cell1_box = &row_box.children[1];
    // Shared edge: max(2, 4) = 4 on both cells.
    assert!(
        (cell0_box.dimensions.border.right - 4.0).abs() < 0.01,
        "adjacent borders collapse to the widest one"
    );
    assert!((cell1_box.dimensions.border.left - 4.0).abs() < 0.01);
    // Outer edges have no table border: cells keep their own (0) border.
    assert!((cell0_box.dimensions.border.left - 0.0).abs() < 0.01);
    assert!((cell1_box.dimensions.border.right - 0.0).abs() < 0.01);
}

#[test]
fn test_border_collapse_hidden_wins() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    let mut collapsed_style = table_style(Length::Px(200.0));
    collapsed_style.border_collapse = BorderCollapse::Collapse;
    styles.insert(table, collapsed_style);
    let row = add_row(&mut doc, &mut styles, table);
    let cell0 = add_cell(&mut doc, &mut styles, row, "aa", None);
    let mut style0 = collapse_cell_style();
    style0.border_width.right = 8.0;
    style0.border_style.right = BorderStyle::Hidden;
    styles.insert(cell0, style0);
    let cell1 = add_cell(&mut doc, &mut styles, row, "bb", None);
    let mut style1 = collapse_cell_style();
    style1.border_width.left = 2.0;
    styles.insert(cell1, style1);

    let tree = layout_tree(&doc, &styles);
    let row_box = &tree.children[0].children[0].children[0];
    // hidden wins over any width: the shared edge resolves to 0 on both sides.
    assert!((row_box.children[0].dimensions.border.right - 0.0).abs() < 0.01);
    assert!((row_box.children[1].dimensions.border.left - 0.0).abs() < 0.01);
}

#[test]
fn test_border_collapse_ignores_spacing_vs_separate() {
    // Same table border-spacing: separate inserts 4px between cells, collapse does not.
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    let mut separate_table_style = table_style(Length::Px(200.0));
    separate_table_style.border_collapse = BorderCollapse::Separate;
    styles.insert(table, separate_table_style);
    let row = add_row(&mut doc, &mut styles, table);
    add_cell(&mut doc, &mut styles, row, "aa", None);
    add_cell(&mut doc, &mut styles, row, "bb", None);
    let separate = layout_tree(&doc, &styles);
    let separate_row = &separate.children[0].children[0].children[0];
    assert!((separate_row.children[0].dimensions.content.x - 4.0).abs() < 0.01);

    let mut collapsed_doc = Document::new();
    let html = collapsed_doc.create_element(Namespace::Html, "html", vec![]);
    let collapsed_body = collapsed_doc.create_element(Namespace::Html, "body", vec![]);
    collapsed_doc.append_child(collapsed_doc.root(), html);
    collapsed_doc.append_child(html, collapsed_body);
    let table2 = mk(&mut collapsed_doc, collapsed_body, "table");
    let mut styles2 = HashMap::new();
    styles2.insert(html, block_style(Length::Auto));
    styles2.insert(collapsed_body, block_style(Length::Auto));
    let mut collapsed_table_style = table_style(Length::Px(200.0));
    collapsed_table_style.border_collapse = BorderCollapse::Collapse;
    styles2.insert(table2, collapsed_table_style);
    let row2 = mk(&mut collapsed_doc, table2, "tr");
    styles2.insert(row2, row_style());
    for word in ["aa", "bb"] {
        let cell = mk(&mut collapsed_doc, row2, "td");
        styles2.insert(cell, cell_style());
        collapsed_doc.append_text(cell, word);
    }
    let collapsed = layout_tree(&collapsed_doc, &styles2);
    let collapsed_row = &collapsed.children[0].children[0].children[0];
    assert!(
        (collapsed_row.children[0].dimensions.content.x - 0.0).abs() < 0.01,
        "collapse removes border-spacing"
    );
}
