//! CSS 2.1 §17 table layout conformance tests (practical subset).

use std::collections::HashMap;

use css::{BorderCollapse, ComputedStyle, Display, Length, VerticalAlign};
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
