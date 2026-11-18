pub(crate) use std::collections::HashMap;

pub(crate) use megi_core::{Attribute, Document, Namespace, NodeId};

pub(crate) use megi_css::{
    BorderCollapse, BorderStyle, ComputedStyle, Display, Length, Position, TableLayout,
    VerticalAlign,
};

pub(crate) use megi_layout::{BoxType, LayoutBox, Rect, layout_document};

pub(crate) fn block_style(width: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style
}

pub(crate) fn table_style(width: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Table;
    style.width = width;
    style.border_spacing = (4.0, 4.0);
    style.border_collapse = BorderCollapse::Separate;
    style
}

pub(crate) fn row_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::TableRow;
    style
}

pub(crate) fn cell_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::TableCell;
    style.font_family = "monospace".to_string();
    style.font_size = 10.0;
    style.line_height = Some(20.0);
    style
}

pub(crate) fn setup() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
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

pub(crate) fn mk(doc: &mut Document, parent: NodeId, tag: &str) -> NodeId {
    let id = doc.create_element(Namespace::Html, tag, vec![]);
    doc.append_child(parent, id);
    id
}

pub(crate) fn layout_tree(doc: &Document, styles: &HashMap<NodeId, ComputedStyle>) -> LayoutBox {
    layout_document(doc, styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout")
}

/// Builds a 2x2 table with cells "aa" (12px), "bb", "cc", "dd".
pub(crate) fn build_2x2(width: Length) -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
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

/// Adds a row to `table` and returns it.
pub(crate) fn add_row(
    doc: &mut Document,
    styles: &mut HashMap<NodeId, ComputedStyle>,
    table: NodeId,
) -> NodeId {
    let row = mk(doc, table, "tr");
    styles.insert(row, row_style());
    row
}

/// Adds a cell to `row` with `word` text and optional intrinsic height.
pub(crate) fn add_cell(
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

pub(crate) fn collapse_cell_style() -> ComputedStyle {
    let mut style = cell_style();
    style.border_width = megi_css::RectEdges::all(0.0);
    style.border_style = megi_css::RectEdges::all(BorderStyle::Solid);
    style
}

pub(crate) use megi_layout::Rect as LayoutRect;

/// Recursively collects `(text, content rect)` of every TextNode under `node`.
pub(crate) fn collect_text_boxes(node: &LayoutBox, out: &mut Vec<(String, LayoutRect)>) {
    if let BoxType::TextNode(text) = &node.box_type {
        out.push((text.clone(), node.dimensions.content));
    }
    for child in &node.children {
        collect_text_boxes(child, out);
    }
}

/// Finds the `index`-th row of a table (direct rows or inside row groups).
pub(crate) fn table_row(table: &LayoutBox, index: usize) -> &LayoutBox {
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

pub(crate) fn rect_contains(outer: LayoutRect, inner: LayoutRect) -> bool {
    inner.x >= outer.x - 0.01
        && inner.y >= outer.y - 0.01
        && inner.x + inner.width <= outer.x + outer.width + 0.01
        && inner.y + inner.height <= outer.y + outer.height + 0.01
}

/// `colspan` amplification (#690): the effective span is capped, so a `colspan=1000` cell and
/// a `colspan=512` cell lay out identically instead of allocating 1000 tracks.
pub(crate) fn layout_single_colspan(colspan: &str) -> (f32, f32, f32) {
    let (mut doc, mut styles, body) = setup();
    let table = mk(&mut doc, body, "table");
    styles.insert(table, table_style(Length::Auto));
    let row = mk(&mut doc, table, "tr");
    styles.insert(row, row_style());
    let cell = doc.create_element(
        Namespace::Html,
        "td",
        vec![Attribute::new("colspan", colspan)],
    );
    doc.append_child(row, cell);
    styles.insert(cell, cell_style());
    doc.append_text(cell, "x");
    let tree = layout_tree(&doc, &styles);
    let table_box = &tree.children[0].children[0];
    let cell_box = &table_box.children[0].children[0];
    (
        table_box.dimensions.border_box().width,
        cell_box.dimensions.border_box().width,
        cell_box.dimensions.border_box().x,
    )
}

pub(crate) fn find_box(root: &LayoutBox, id: NodeId) -> &LayoutBox {
    if matches!(
        root.box_type,
        BoxType::BlockNode(node) | BoxType::InlineBlock(node) if node == id
    ) {
        return root;
    }
    for child in &root.children {
        if let Some(found) = find_box_opt(child, id) {
            return found;
        }
    }
    panic!("box {id:?} not found");
}

pub(crate) fn find_box_opt(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
    if matches!(
        root.box_type,
        BoxType::BlockNode(node) | BoxType::InlineBlock(node) if node == id
    ) {
        return Some(root);
    }
    root.children
        .iter()
        .find_map(|child| find_box_opt(child, id))
}
