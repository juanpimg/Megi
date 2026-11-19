use super::*;

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
    float_style.float = megi_css::Float::Left;
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
    span_style.calc_lengths.margin_left = Some(megi_css::calc::CalcValue {
        percent: 10.0,
        px: 5.0,
    });
    span_style.calc_lengths.padding_left = Some(megi_css::calc::CalcValue {
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
    table_style.border_spacing = (2.0, 2.0);
    // calc(50% + 20px) = 220px
    table_style.calc_lengths.width = Some(megi_css::calc::CalcValue {
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
