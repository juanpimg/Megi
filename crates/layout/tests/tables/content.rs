use super::common::*;

#[test]
fn test_table_cell_text_is_exposed_in_layout_tree() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Px(300.0)));
    let row = add_row(&mut doc, &mut styles, table);
    add_cell(&mut doc, &mut styles, row, "hola", None);
    add_cell(&mut doc, &mut styles, row, "mundo", None);

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let row_box = table_row(table_box, 0);
    assert_eq!(row_box.children.len(), 2);
    for (cell_index, expected) in ["hola", "mundo"].iter().enumerate() {
        let cell = &row_box.children[cell_index];
        assert!(matches!(cell.box_type, BoxType::TableCell(_)));
        // The cell must expose its inline content through `LayoutBox.children`.
        let mut texts = Vec::new();
        collect_text_boxes(cell, &mut texts);
        assert_eq!(texts.len(), 1, "cell {cell_index} must expose one text box");
        assert_eq!(&texts[0].0, expected);
        // ...with geometry inside the cell content box (padding/border respected).
        assert!(
            rect_contains(cell.dimensions.content, texts[0].1),
            "text rect {:?} must lie inside cell content {:?}",
            texts[0].1,
            cell.dimensions.content
        );
        // There is a line box between the cell and the text run.
        assert!(cell.children.iter().any(|child| child.is_line_box()));
    }
}

#[test]
fn test_table_cell_text_boxes_do_not_overlap() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Px(300.0)));
    let row0 = add_row(&mut doc, &mut styles, table);
    add_cell(&mut doc, &mut styles, row0, "aa", None);
    add_cell(&mut doc, &mut styles, row0, "bb", None);
    let row1 = add_row(&mut doc, &mut styles, table);
    add_cell(&mut doc, &mut styles, row1, "cc", None);
    add_cell(&mut doc, &mut styles, row1, "dd", None);

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let mut texts = Vec::new();
    collect_text_boxes(table_box, &mut texts);
    assert_eq!(texts.len(), 4, "every cell text must be in the tree");
    for first in 0..texts.len() {
        for second in (first + 1)..texts.len() {
            let a = texts[first].1;
            let b = texts[second].1;
            let x_overlap = a.x < b.x + b.width - 0.01 && b.x < a.x + a.width - 0.01;
            let y_overlap = a.y < b.y + b.height - 0.01 && b.y < a.y + a.height - 0.01;
            assert!(
                !(x_overlap && y_overlap),
                "text boxes {:?} and {:?} overlap",
                a,
                b
            );
        }
    }
}

#[test]
fn test_fixed_layout_cell_text_fits_cell_content_box() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    let mut fixed = table_style(Length::Px(200.0));
    fixed.table_layout = TableLayout::Fixed;
    styles.insert(table, fixed);
    let row = add_row(&mut doc, &mut styles, table);
    add_cell(&mut doc, &mut styles, row, "one", None);
    add_cell(&mut doc, &mut styles, row, "two", None);

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let row_box = table_row(table_box, 0);
    for (cell_index, expected) in ["one", "two"].iter().enumerate() {
        let cell = &row_box.children[cell_index];
        let mut texts = Vec::new();
        collect_text_boxes(cell, &mut texts);
        assert_eq!(texts.len(), 1);
        assert_eq!(&texts[0].0, expected);
        assert!(rect_contains(cell.dimensions.content, texts[0].1));
        // Text must not spill into the next column.
        if cell_index == 0 {
            let next = &row_box.children[1];
            assert!(texts[0].1.x + texts[0].1.width <= next.dimensions.content.x + 0.01);
        }
    }
}

#[test]
fn test_border_collapse_cell_text_inside_content_box() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    let mut collapsed = table_style(Length::Px(200.0));
    collapsed.border_collapse = BorderCollapse::Collapse;
    styles.insert(table, collapsed);
    let row = add_row(&mut doc, &mut styles, table);
    let cell0 = add_cell(&mut doc, &mut styles, row, "left", None);
    let mut style0 = collapse_cell_style();
    style0.border_width.right = 4.0;
    styles.insert(cell0, style0);
    let cell1 = add_cell(&mut doc, &mut styles, row, "right", None);
    let mut style1 = collapse_cell_style();
    style1.border_width.left = 2.0;
    styles.insert(cell1, style1);

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let row_box = table_row(table_box, 0);
    for cell in row_box.children.iter() {
        let mut texts = Vec::new();
        collect_text_boxes(cell, &mut texts);
        assert_eq!(texts.len(), 1, "collapsed cell must expose its text");
        assert!(
            rect_contains(cell.dimensions.content, texts[0].1),
            "collapsed-border cell text {:?} must be inside content {:?}",
            texts[0].1,
            cell.dimensions.content
        );
    }
    // Shared border resolved to max(2,4)=4 affects both cells' content origin.
    assert!((row_box.children[0].dimensions.border.right - 4.0).abs() < 0.01);
    assert!((row_box.children[1].dimensions.border.left - 4.0).abs() < 0.01);
}
