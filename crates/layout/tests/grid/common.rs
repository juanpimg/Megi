pub(crate) use std::collections::HashMap;

pub(crate) use megi_core::{Document, Namespace, NodeId};

pub(crate) use megi_css::{
    AlignItems, AlignSelf, ComputedStyle, Display, GridLine, GridTemplate, Length, TrackSize,
};

pub(crate) use megi_layout::{BoxType, LayoutBox, Rect, layout_document};

pub(crate) fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

pub(crate) fn grid_container_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = block_style();
    style.display = Display::Grid;
    style.width = width;
    style.height = height;
    style
}

pub(crate) fn item_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = block_style();
    style.width = width;
    style.height = height;
    style
}

pub(crate) fn template(tracks: Vec<TrackSize>) -> GridTemplate {
    GridTemplate {
        tracks,
        auto_repeat: None,
        line_names: Vec::new(),
    }
}

pub(crate) fn px(value: f32) -> TrackSize {
    TrackSize::Fixed(Length::Px(value))
}

pub(crate) fn fr(value: f32) -> TrackSize {
    TrackSize::Fraction(value)
}

pub(crate) fn setup() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let mut styles = HashMap::new();
    styles.insert(html, block_style());
    (doc, styles, html)
}

pub(crate) fn add_element(
    doc: &mut Document,
    parent: NodeId,
    styles: &mut HashMap<NodeId, ComputedStyle>,
    style: ComputedStyle,
) -> NodeId {
    let id = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(parent, id);
    styles.insert(id, style);
    id
}

pub(crate) fn find_box_opt(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
    if matches!(root.box_type, BoxType::BlockNode(node) | BoxType::InlineBlock(node) if node == id)
    {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_box_opt(child, id) {
            return Some(found);
        }
    }
    None
}

pub(crate) fn find_box(root: &LayoutBox, id: NodeId) -> &LayoutBox {
    find_box_opt(root, id).unwrap_or_else(|| panic!("box {id:?} not found"))
}

pub(crate) struct Case {
    pub(crate) root: LayoutBox,
    pub(crate) container: NodeId,
    pub(crate) ids: Vec<NodeId>,
}

pub(crate) fn layout_case(container: ComputedStyle, items: Vec<ComputedStyle>) -> Case {
    let (mut doc, mut styles, html) = setup();
    let container_id = add_element(&mut doc, html, &mut styles, container);
    let mut ids = Vec::new();
    for item in items {
        ids.push(add_element(&mut doc, container_id, &mut styles, item));
    }
    let root =
        layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout result");
    Case {
        root,
        container: container_id,
        ids,
    }
}

impl Case {
    pub(crate) fn item(&self, index: usize) -> &LayoutBox {
        find_box(&self.root, self.ids[index])
    }
}

pub(crate) fn with_placement(
    mut style: ComputedStyle,
    column: (megi_css::GridLine, megi_css::GridLine),
    row: (megi_css::GridLine, megi_css::GridLine),
) -> ComputedStyle {
    style.grid_column_start = column.0;
    style.grid_column_end = column.1;
    style.grid_row_start = row.0;
    style.grid_row_end = row.1;
    style
}
