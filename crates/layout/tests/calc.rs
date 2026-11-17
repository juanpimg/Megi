//! Mixed `calc()` (`<percentage> + <length>`) resolution in layout.

use std::collections::HashMap;

use css::{ComputedStyle, Display, Length, Position};
use dom::{Document, Namespace, NodeId};
use layout::{BoxType, LayoutBox, Rect, layout_document};

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn block(width: Length, height: Length) -> ComputedStyle {
    let mut style = block_style();
    style.width = width;
    style.height = height;
    style
}

fn setup() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let mut styles = HashMap::new();
    styles.insert(html, block_style());
    (doc, styles, html)
}

fn add(
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

fn find_box_opt(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
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

fn find_box(root: &LayoutBox, id: NodeId) -> &LayoutBox {
    find_box_opt(root, id).unwrap_or_else(|| panic!("box {id:?} not found"))
}

#[derive(Clone, Copy, Debug)]
struct BoxDimensions {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn dimensions(layout_box: &LayoutBox) -> BoxDimensions {
    BoxDimensions {
        x: layout_box.dimensions.content.x,
        y: layout_box.dimensions.content.y,
        width: layout_box.dimensions.content.width,
        height: layout_box.dimensions.content.height,
    }
}

// The helper above is intentionally simple: most tests build the tree inline below.
fn simple(style: ComputedStyle, viewport_width: f32) -> BoxDimensions {
    let (mut doc, mut styles, html) = setup();
    let id = add(&mut doc, html, &mut styles, style);
    let root =
        layout_document(&doc, &styles, Rect::new(0.0, 0.0, viewport_width, 600.0)).expect("layout");
    dimensions(find_box(&root, id))
}

#[test]
fn width_calc_percent_minus_px() {
    let mut style = block(Length::Auto, Length::Px(20.0));
    style.calc_lengths.width = Some(css::calc::CalcValue {
        percent: 100.0,
        px: -40.0,
    });
    let dims = simple(style, 800.0);
    assert!((dims.width - 760.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn width_calc_half_plus_px_inside_parent() {
    let (mut doc, mut styles, html) = setup();
    let parent = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );
    let mut child_style = block(Length::Auto, Length::Px(10.0));
    child_style.calc_lengths.width = Some(css::calc::CalcValue {
        percent: 50.0,
        px: 10.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.width - 210.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn height_calc_percent_minus_px() {
    let (mut doc, mut styles, html) = setup();
    let parent = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(200.0), Length::Px(200.0)),
    );
    let mut child_style = block(Length::Px(50.0), Length::Auto);
    child_style.calc_lengths.height = Some(css::calc::CalcValue {
        percent: 100.0,
        px: -20.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.height - 180.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn margin_left_calc_percent_minus_px() {
    let (mut doc, mut styles, html) = setup();
    let parent = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );
    let mut child_style = block(Length::Px(100.0), Length::Px(10.0));
    child_style.calc_lengths.margin_left = Some(css::calc::CalcValue {
        percent: 50.0,
        px: -20.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.x - 180.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn padding_top_calc_percent_plus_px() {
    let (mut doc, mut styles, html) = setup();
    let parent = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(200.0), Length::Auto),
    );
    let mut child_style = block(Length::Px(50.0), Length::Px(10.0));
    child_style.calc_lengths.padding_top = Some(css::calc::CalcValue {
        percent: 10.0,
        px: 5.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.y - 25.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn absolute_top_calc_percent_minus_px() {
    let (mut doc, mut styles, html) = setup();
    let mut parent_style = block(Length::Px(200.0), Length::Px(200.0));
    parent_style.position = Position::Relative;
    let parent = add(&mut doc, html, &mut styles, parent_style);
    let mut child_style = block(Length::Px(50.0), Length::Px(50.0));
    child_style.position = Position::Absolute;
    child_style.calc_lengths.top = Some(css::calc::CalcValue {
        percent: 100.0,
        px: -30.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.y - 170.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn nested_calc_em_and_multiplication() {
    let mut style = block(Length::Auto, Length::Px(10.0));
    style.calc_lengths.width = Some(css::calc::CalcValue {
        percent: 50.0,
        px: 20.0,
    });
    let dims = simple(style, 300.0);
    assert!((dims.width - 170.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn max_width_calc_clamps_stretch_width() {
    let (mut doc, mut styles, html) = setup();
    let parent = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );
    let mut child_style = block(Length::Auto, Length::Px(10.0));
    child_style.calc_lengths.max_width = Some(css::calc::CalcValue {
        percent: 50.0,
        px: 10.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.width - 210.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn indefinite_height_uses_absolute_part_only() {
    let mut style = block(Length::Px(50.0), Length::Auto);
    style.calc_lengths.height = Some(css::calc::CalcValue {
        percent: 50.0,
        px: 10.0,
    });
    let dims = simple(style, 300.0);
    assert!((dims.height - 10.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn flex_gap_calc_em_plus_px() {
    let (mut doc, mut styles, html) = setup();
    let mut container_style = block(Length::Px(300.0), Length::Px(20.0));
    container_style.display = Display::Flex;
    let container = add(&mut doc, html, &mut styles, container_style);
    let mut first_style = block(Length::Px(50.0), Length::Px(10.0));
    first_style.display = Display::Block;
    let first = add(&mut doc, container, &mut styles, first_style);
    let mut second_style = block(Length::Px(50.0), Length::Px(10.0));
    second_style.display = Display::Block;
    let second = add(&mut doc, container, &mut styles, second_style);
    // Store the mixed gap directly on the container style before layout.
    let mut styles2 = styles;
    let mut s = styles2.get(&container).cloned().expect("style");
    s.calc_lengths.column_gap = Some(css::calc::CalcValue {
        percent: 0.0,
        px: 21.0,
    });
    styles2.insert(container, s);
    let root = layout_document(&doc, &styles2, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let first_x = dimensions(find_box(&root, first)).x;
    let second_x = dimensions(find_box(&root, second)).x;
    assert!((first_x).abs() < 0.01, "{first_x:?}");
    assert!((second_x - 71.0).abs() < 0.01, "{second_x}");
}

#[test]
fn absolute_left_calc_percent_minus_px() {
    let (mut doc, mut styles, html) = setup();
    let mut parent_style = block(Length::Px(200.0), Length::Px(200.0));
    parent_style.position = Position::Relative;
    let parent = add(&mut doc, html, &mut styles, parent_style);
    let mut child_style = block(Length::Px(50.0), Length::Px(50.0));
    child_style.position = Position::Absolute;
    child_style.calc_lengths.left = Some(css::calc::CalcValue {
        percent: 100.0,
        px: -40.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.x - 160.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn grid_column_gap_calc_mixed() {
    let (mut doc, mut styles, html) = setup();
    let mut container_style = block(Length::Px(200.0), Length::Auto);
    container_style.display = Display::Grid;
    container_style.grid_template_columns = css::GridTemplate {
        tracks: vec![
            css::TrackSize::Fixed(Length::Px(100.0)),
            css::TrackSize::Fixed(Length::Px(100.0)),
        ],
        auto_repeat: None,
        line_names: Vec::new(),
    };
    container_style.calc_lengths.column_gap = Some(css::calc::CalcValue {
        percent: 10.0,
        px: 5.0,
    });
    let container = add(&mut doc, html, &mut styles, container_style);
    let first = add(
        &mut doc,
        container,
        &mut styles,
        block(Length::Auto, Length::Px(10.0)),
    );
    let second = add(
        &mut doc,
        container,
        &mut styles,
        block(Length::Auto, Length::Px(10.0)),
    );
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    // Two explicit 100px columns with a 10% + 5px = 25px gap between them.
    let first_x = dimensions(find_box(&root, first)).x;
    let second_x = dimensions(find_box(&root, second)).x;
    assert!((first_x).abs() < 0.01, "{first_x:?}");
    assert!((second_x - 125.0).abs() < 0.01, "{second_x:?}");
}

#[test]
fn flex_item_width_calc_mixed() {
    let (mut doc, mut styles, html) = setup();
    let mut container_style = block(Length::Px(300.0), Length::Px(20.0));
    container_style.display = Display::Flex;
    let container = add(&mut doc, html, &mut styles, container_style);
    let mut item_style = block(Length::Auto, Length::Px(10.0));
    item_style.calc_lengths.width = Some(css::calc::CalcValue {
        percent: 50.0,
        px: -10.0,
    });
    let item = add(&mut doc, container, &mut styles, item_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, item));
    assert!((dims.width - 140.0).abs() < 0.01, "{dims:?}");
}

/// Hostile lengths must not reach box geometry as `inf`/`NaN` (#693).
#[test]
fn infinite_and_nan_lengths_are_clamped_to_finite_geometry() {
    for (width, height, padding) in [
        (Length::Px(f32::INFINITY), Length::Px(f32::INFINITY), 0.0),
        (
            Length::Px(f32::NEG_INFINITY),
            Length::Px(f32::NAN),
            f32::NAN,
        ),
        (Length::Px(1.0e30), Length::Px(-1.0e30), 1.0e30),
        (Length::Percent(1.0e30), Length::Percent(f32::INFINITY), 0.0),
    ] {
        let (mut doc, mut styles, html) = setup();
        let mut hostile = block(width, height);
        hostile.padding.top = Length::Px(padding);
        hostile.padding.left = Length::Px(padding);
        let div = add(&mut doc, html, &mut styles, hostile);
        let tree =
            layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
        let div_box = find_box(&tree, div);
        for rect in [
            div_box.dimensions.content,
            div_box.dimensions.padding_box(),
            div_box.dimensions.border_box(),
            div_box.dimensions.margin_box(),
        ] {
            assert!(
                rect.x.is_finite()
                    && rect.y.is_finite()
                    && rect.width.is_finite()
                    && rect.height.is_finite(),
                "hostile length reached geometry: {rect:?}"
            );
            // Each edge is clamped to 1e8, so a box can reach roughly 3x that once the
            // padding/border/margin layers are added on both sides — still finite and far
            // below the `f32 -> i32` overflow that used to fire in paint.
            assert!(
                rect.width.abs() <= 4.0e8 && rect.height.abs() <= 4.0e8,
                "hostile length was not bounded: {rect:?}"
            );
        }
    }
}

#[test]
fn float_shrink_to_fit_clamps_min_and_max_width() {
    let (mut doc, mut styles, html) = setup();
    let container = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );

    // Float with calc(50% + 50px) = 250px clamped by max-width: 180px.
    let mut float_max_style = block(Length::Auto, Length::Px(20.0));
    float_max_style.float = css::Float::Left;
    float_max_style.calc_lengths.width = Some(css::calc::CalcValue {
        percent: 50.0,
        px: 50.0,
    });
    float_max_style.max_width = Length::Px(180.0);
    let float_max = add(&mut doc, container, &mut styles, float_max_style);

    // Float with width: 40px clamped up by min-width: 120px.
    let mut float_min_style = block(Length::Px(40.0), Length::Px(20.0));
    float_min_style.float = css::Float::Left;
    float_min_style.min_width = Length::Px(120.0);
    let float_min = add(&mut doc, container, &mut styles, float_min_style);

    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let w_max = dimensions(find_box(&root, float_max)).width;
    let w_min = dimensions(find_box(&root, float_min)).width;
    assert!((w_max - 180.0).abs() < 0.01, "expected 180.0, got {w_max}");
    assert!((w_min - 120.0).abs() < 0.01, "expected 120.0, got {w_min}");
}

#[test]
fn replaced_element_respects_calc_and_min_max_constraints() {
    let (mut doc, mut styles, html) = setup();
    let container = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(container, img);
    let mut img_style = ComputedStyle::initial();
    img_style.display = Display::Block;
    // calc(50% + 100px) = 300px, clamped by max-width: 160px -> height should scale to 80px.
    img_style.calc_lengths.width = Some(css::calc::CalcValue {
        percent: 50.0,
        px: 100.0,
    });
    img_style.max_width = Length::Px(160.0);
    styles.insert(img, img_style);
    let mut sizes = HashMap::new();
    sizes.insert(img, (200.0, 100.0));

    let root = layout::layout_document_with_replaced_sizes(
        &doc,
        &styles,
        Rect::new(0.0, 0.0, 800.0, 600.0),
        &sizes,
    )
    .expect("layout");
    fn find_replaced(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
        if matches!(root.box_type, BoxType::Replaced(node) if node == id) {
            return Some(root);
        }
        for child in &root.children {
            if let Some(found) = find_replaced(child, id) {
                return Some(found);
            }
        }
        None
    }
    let img_box = find_replaced(&root, img).expect("replaced box");
    assert!(
        (img_box.dimensions.content.width - 160.0).abs() < 0.01,
        "{:?}",
        img_box.dimensions.content
    );
    assert!(
        (img_box.dimensions.content.height - 80.0).abs() < 0.01,
        "{:?}",
        img_box.dimensions.content
    );
}

#[test]
fn definite_height_threaded_into_float_inline_block_and_grid_item() {
    let (mut doc, mut styles, html) = setup();

    // 1. Float inside a 200px-tall container.
    let container_f = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Px(200.0)),
    );
    let mut float_style = block(Length::Px(100.0), Length::Percent(50.0));
    float_style.float = css::Float::Left;
    let float_node = add(&mut doc, container_f, &mut styles, float_style);

    // 2. Inline-block inside a 200px-tall container.
    let container_ib = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Px(200.0)),
    );
    let mut ib_style = ComputedStyle::initial();
    ib_style.display = Display::InlineBlock;
    ib_style.width = Length::Px(80.0);
    ib_style.height = Length::Percent(25.0);
    let ib_node = add(&mut doc, container_ib, &mut styles, ib_style);

    // 3. Grid item child with percentage height inside a 200px-tall grid.
    let mut grid_style = block(Length::Px(400.0), Length::Px(200.0));
    grid_style.display = Display::Grid;
    let container_g = add(&mut doc, html, &mut styles, grid_style);
    let grid_item = add(
        &mut doc,
        container_g,
        &mut styles,
        block(Length::Auto, Length::Percent(50.0)),
    );
    let grid_grandchild = add(
        &mut doc,
        grid_item,
        &mut styles,
        block(Length::Auto, Length::Percent(50.0)),
    );

    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let fh = dimensions(find_box(&root, float_node)).height;
    let ibh = dimensions(find_box(&root, ib_node)).height;
    let ggh = dimensions(find_box(&root, grid_grandchild)).height;
    assert!((fh - 100.0).abs() < 0.01, "float height {fh}");
    assert!((ibh - 50.0).abs() < 0.01, "inline-block height {ibh}");
    assert!((ggh - 50.0).abs() < 0.01, "grid grandchild height {ggh}");
}

#[test]
fn inline_span_resolves_calc_margin_and_padding() {
    let (mut doc, mut styles, html) = setup();
    let container = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );
    let span = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(container, span);
    let text = doc.create_text_node("hello");
    doc.append_child(span, text);

    let mut span_style = ComputedStyle::initial();
    span_style.display = Display::Inline;
    // 10% of 400 + 5 = 45px margin-left, 5% of 400 + 10 = 30px padding-left.
    span_style.calc_lengths.margin_left = Some(css::calc::CalcValue {
        percent: 10.0,
        px: 5.0,
    });
    span_style.calc_lengths.padding_left = Some(css::calc::CalcValue {
        percent: 5.0,
        px: 10.0,
    });
    styles.insert(span, span_style);

    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    fn find_inline(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
        if matches!(root.box_type, BoxType::InlineNode(node) if node == id) {
            return Some(root);
        }
        for child in &root.children {
            if let Some(found) = find_inline(child, id) {
                return Some(found);
            }
        }
        None
    }
    let span_box = find_inline(&root, span).expect("inline box");
    assert!(
        (span_box.dimensions.margin.left - 45.0).abs() < 0.01,
        "{:?}",
        span_box.dimensions.margin
    );
    assert!(
        (span_box.dimensions.padding.left - 30.0).abs() < 0.01,
        "{:?}",
        span_box.dimensions.padding
    );
}

#[test]
fn table_with_calc_width_uses_specified_width() {
    let (mut doc, mut styles, html) = setup();
    let container = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );

    let table = doc.create_element(Namespace::Html, "table", vec![]);
    doc.append_child(container, table);
    let mut table_style = ComputedStyle::initial();
    table_style.display = Display::Table;
    // calc(50% + 20px) = 220px
    table_style.calc_lengths.width = Some(css::calc::CalcValue {
        percent: 50.0,
        px: 20.0,
    });
    styles.insert(table, table_style);

    let tr = doc.create_element(Namespace::Html, "tr", vec![]);
    doc.append_child(table, tr);
    let mut tr_style = ComputedStyle::initial();
    tr_style.display = Display::TableRow;
    styles.insert(tr, tr_style);

    let td = doc.create_element(Namespace::Html, "td", vec![]);
    doc.append_child(tr, td);
    let mut td_style = ComputedStyle::initial();
    td_style.display = Display::TableCell;
    styles.insert(td, td_style);
    let txt = doc.create_text_node("cell");
    doc.append_child(td, txt);

    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    fn find_table(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
        if matches!(root.box_type, BoxType::Table(node) if node == id) {
            return Some(root);
        }
        for child in &root.children {
            if let Some(found) = find_table(child, id) {
                return Some(found);
            }
        }
        None
    }
    let table_box = find_table(&root, table).expect("table box");
    assert!(
        (table_box.dimensions.content.width - 220.0).abs() < 0.01,
        "{:?}",
        table_box.dimensions.content
    );
}
