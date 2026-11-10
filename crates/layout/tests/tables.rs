//! CSS 2.1 §17 table layout conformance tests (practical subset).

use std::collections::HashMap;

use css::{
    BorderCollapse, BorderStyle, ComputedStyle, Display, Length, TableLayout, VerticalAlign,
};
use dom::{Attribute, Document, Namespace, NodeId};
use layout::{BoxType, LayoutBox, Rect, layout_document};

fn block_style(width: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style
}

fn table_style(width: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Table;
    style.width = width;
    style.border_spacing = (4.0, 4.0);
    style.border_collapse = BorderCollapse::Separate;
    style
}

fn row_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::TableRow;
    style
}

fn cell_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::TableCell;
    style.font_family = "monospace".to_string();
    style.font_size = 10.0;
    style.line_height = Some(20.0);
    style
}

fn setup() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    let mut styles = HashMap::new();
    styles.insert(html, block_style(Length::Auto));
    styles.insert(body, block_style(Length::Auto));
    (doc, styles, body)
}

fn mk(doc: &mut Document, parent: NodeId, tag: &str) -> NodeId {
    let id = doc.create_element(Namespace::Html, tag, vec![]);
    doc.append_child(parent, id);
    id
}

fn layout_tree(doc: &Document, styles: &HashMap<NodeId, ComputedStyle>) -> LayoutBox {
    layout_document(doc, styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout")
}

/// Builds a 2x2 table with cells "aa" (12px), "bb", "cc", "dd".
fn build_2x2(width: Length) -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(width));
    let words = ["aa", "bb", "cc", "dd"];
    for row_index in 0..2 {
        let row = mk(&mut doc, table, "tr");
        styles.insert(row, row_style());
        for column_index in 0..2 {
            let cell = mk(&mut doc, row, "td");
            styles.insert(cell, cell_style());
            doc.append_text(cell, words[row_index * 2 + column_index]);
        }
    }
    (doc, styles, table)
}

#[test]
fn test_table_2x2_fixed_width_layout() {
    let (doc, styles, table) = build_2x2(Length::Px(200.0));
    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    assert_eq!(table_box.box_type, BoxType::Table(table));
    // Table width comes from the `width` declaration.
    assert!((table_box.dimensions.content.width - 200.0).abs() < 0.001);
    let row0 = &table_box.children[0];
    let row1 = &table_box.children[1];
    assert!(matches!(row0.box_type, BoxType::TableRow(_)));
    // border-spacing (4px): first row at y = 4, second at 4 + 20 + 4 = 28.
    assert!((row0.dimensions.content.y - 4.0).abs() < 0.001);
    assert!((row1.dimensions.content.y - 28.0).abs() < 0.001);
    // Columns: (200 - 3*4) / 2 = 94 each.
    let cell00 = &row0.children[0];
    let cell01 = &row0.children[1];
    assert!((cell00.dimensions.content.x - 4.0).abs() < 0.001);
    assert!((cell00.dimensions.content.width - 94.0).abs() < 0.001);
    assert!((cell01.dimensions.content.x - 102.0).abs() < 0.001);
    assert!((cell01.dimensions.content.width - 94.0).abs() < 0.001);
    // Row height from the 20px line box; table height = 4 + 20 + 4 + 20 + 4 = 52.
    assert!((row0.dimensions.content.height - 20.0).abs() < 0.001);
    assert!((table_box.dimensions.content.height - 52.0).abs() < 0.001);
}

#[test]
fn test_table_auto_width_shrinks_to_content() {
    let (doc, styles, _table) = build_2x2(Length::Auto);
    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    // Two columns of 12px + 3 * 4px spacing = 36px.
    assert!((table_box.dimensions.content.width - 36.0).abs() < 0.001);
    let row0 = &table_box.children[0];
    assert!((row0.children[0].dimensions.content.x - 4.0).abs() < 0.001);
    assert!((row0.children[1].dimensions.content.x - 20.0).abs() < 0.001);
}

#[test]
fn test_table_colspan_cell_spans_columns() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Px(200.0)));
    // Row 1: a single cell spanning both columns.
    let row0 = mk(&mut doc, table, "tr");
    styles.insert(row0, row_style());
    let spanning = doc.create_element(Namespace::Html, "td", vec![Attribute::new("colspan", "2")]);
    doc.append_child(row0, spanning);
    styles.insert(spanning, cell_style());
    doc.append_text(spanning, "x");
    // Row 2: two normal cells.
    let row1 = mk(&mut doc, table, "tr");
    styles.insert(row1, row_style());
    for word in ["aa", "bb"] {
        let cell = mk(&mut doc, row1, "td");
        styles.insert(cell, cell_style());
        doc.append_text(cell, word);
    }

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let spanning_cell = &table_box.children[0].children[0];
    assert_eq!(spanning_cell.colspan, 2);
    // Spans the whole grid area: (200 - 3*4) + 4 = 192px.
    assert!((spanning_cell.dimensions.content.width - 192.0).abs() < 0.001);
    let second_cell = &table_box.children[1].children[1];
    assert!((second_cell.dimensions.content.x - 102.0).abs() < 0.001);
}

#[test]
fn test_table_row_and_cell_anonymous_fixups() {
    // `<table><td>x</td><td>y</td></table>`: cells directly inside the table get an
    // anonymous row (§17.2.1). Stray block content gets an anonymous row + cell.
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Auto));
    let cell0 = mk(&mut doc, table, "td");
    styles.insert(cell0, cell_style());
    doc.append_text(cell0, "a");
    let cell1 = mk(&mut doc, table, "td");
    styles.insert(cell1, cell_style());
    doc.append_text(cell1, "b");
    let div = mk(&mut doc, table, "div");
    let mut div_style = ComputedStyle::initial();
    div_style.display = Display::Block;
    styles.insert(div, div_style);
    doc.append_text(div, "stray");

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    // First child: anonymous row wrapping the direct cells.
    assert!(matches!(
        table_box.children[0].box_type,
        BoxType::AnonymousTableRow
    ));
    assert!(matches!(
        table_box.children[0].children[0].box_type,
        BoxType::TableCell(_)
    ));
    // The stray block got an anonymous row + cell too.
    let stray_row = &table_box.children[1];
    assert!(matches!(stray_row.box_type, BoxType::AnonymousTableRow));
    assert!(matches!(
        stray_row.children[0].box_type,
        BoxType::AnonymousTableCell
    ));
}

#[test]
fn test_table_row_group_and_caption() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Px(120.0)));
    // Caption (tag inference + explicit display).
    let caption = mk(&mut doc, table, "caption");
    let mut caption_style = ComputedStyle::initial();
    caption_style.display = Display::TableCaption;
    styles.insert(caption, caption_style);
    doc.append_text(caption, "Cap");
    // Row group with one row/cell.
    let group = mk(&mut doc, table, "tbody");
    styles.insert(group, ComputedStyle::initial());
    let row = mk(&mut doc, group, "tr");
    styles.insert(row, row_style());
    let cell = mk(&mut doc, row, "td");
    styles.insert(cell, cell_style());
    doc.append_text(cell, "z");

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let caption_box = &table_box.children[0];
    assert!(matches!(caption_box.box_type, BoxType::TableCaption(_)));
    // Caption is laid out above the grid at the table width.
    assert!((caption_box.dimensions.content.y - table_box.dimensions.content.y).abs() < 0.001);
    assert!((caption_box.dimensions.content.width - 120.0).abs() < 0.001);
    let group_box = &table_box.children[1];
    assert!(matches!(group_box.box_type, BoxType::TableRowGroup(_)));
    let row_box = &group_box.children[0];
    assert!(matches!(row_box.box_type, BoxType::TableRow(_)));
    // Row starts below the caption line box.
    assert!(row_box.dimensions.content.y > caption_box.dimensions.content.y);
    assert!(
        (table_box.dimensions.content.height - (caption_box.dimensions.content.height + 28.0))
            .abs()
            < 1.0
    );
}

#[test]
fn test_table_cell_vertical_align_middle() {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Px(200.0)));
    // Row with a tall empty cell and a short middle-aligned cell.
    let row = mk(&mut doc, table, "tr");
    styles.insert(row, row_style());
    let tall = mk(&mut doc, row, "td");
    let mut tall_style = cell_style();
    tall_style.height = Length::Px(60.0);
    styles.insert(tall, tall_style);
    let short = mk(&mut doc, row, "td");
    let mut short_style = cell_style();
    short_style.vertical_align = VerticalAlign::Middle;
    styles.insert(short, short_style);
    doc.append_text(short, "mid");

    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let row_box = &table_box.children[0];
    let short_cell = &row_box.children[1];
    let text = &short_cell.children[0];
    // Row is 60px tall; the short cell content (20px) is centred: (60-20)/2 = 20px down.
    assert!((row_box.dimensions.content.height - 60.0).abs() < 0.001);
    assert!((text.dimensions.content.y - (row_box.dimensions.content.y + 20.0)).abs() < 0.001);
}

// ---------------------------------------------------------------------------
// rowspan (CSS 2.1 §17.5.1)
// ---------------------------------------------------------------------------

/// Adds a row to `table` and returns it.
fn add_row(
    doc: &mut Document,
    styles: &mut HashMap<NodeId, ComputedStyle>,
    table: NodeId,
) -> NodeId {
    let row = mk(doc, table, "tr");
    styles.insert(row, row_style());
    row
}

/// Adds a cell to `row` with `word` text and optional intrinsic height.
fn add_cell(
    doc: &mut Document,
    styles: &mut HashMap<NodeId, ComputedStyle>,
    row: NodeId,
    word: &str,
    height: Option<f32>,
) -> NodeId {
    let cell = mk(doc, row, "td");
    let mut style = cell_style();
    if let Some(height) = height {
        style.height = Length::Px(height);
    }
    styles.insert(cell, style);
    if !word.is_empty() {
        doc.append_text(cell, word);
    }
    cell
}

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
    assert_eq!(rows[1].children[0].dimensions.content.x, 96.5);
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

// ---------------------------------------------------------------------------
// table-layout: fixed (CSS 2.1 §17.5.2.1)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// inline-table (CSS 2.1 §17.4, §10.8.1)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// border-collapse: collapse (CSS 2.1 §17.6.2)
// ---------------------------------------------------------------------------

fn collapse_cell_style() -> ComputedStyle {
    let mut style = cell_style();
    style.border_width = css::RectEdges::all(0.0);
    style.border_style = css::RectEdges::all(BorderStyle::Solid);
    style
}

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

// ---------------------------------------------------------------------------
// Cell content exposure (paint walks LayoutBox.children and must find the text)
// ---------------------------------------------------------------------------

use layout::Rect as LayoutRect;

/// Recursively collects `(text, content rect)` of every TextNode under `node`.
fn collect_text_boxes(node: &LayoutBox, out: &mut Vec<(String, LayoutRect)>) {
    if let BoxType::TextNode(text) = &node.box_type {
        out.push((text.clone(), node.dimensions.content));
    }
    for child in &node.children {
        collect_text_boxes(child, out);
    }
}

/// Finds the `index`-th row of a table (direct rows or inside row groups).
fn table_row(table: &LayoutBox, index: usize) -> &LayoutBox {
    let mut rows = Vec::new();
    for child in &table.children {
        match child.box_type {
            BoxType::TableRow(_) | BoxType::AnonymousTableRow => rows.push(child),
            BoxType::TableRowGroup(_) => rows.extend(child.children.iter().filter(|row| {
                matches!(
                    row.box_type,
                    BoxType::TableRow(_) | BoxType::AnonymousTableRow
                )
            })),
            _ => {}
        }
    }
    rows[index]
}

fn rect_contains(outer: LayoutRect, inner: LayoutRect) -> bool {
    inner.x >= outer.x - 0.01
        && inner.y >= outer.y - 0.01
        && inner.x + inner.width <= outer.x + outer.width + 0.01
        && inner.y + inner.height <= outer.y + outer.height + 0.01
}

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
