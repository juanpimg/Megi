//! Mixed `calc()` (`<percentage> + <length>`) side values on `ComputedStyle`.

use std::collections::HashMap;

use css::ComputedStyle;
use dom::{Attribute, Document, Namespace, NodeId};

fn append(doc: &mut Document, parent: NodeId, name: &str, attrs: &[(&str, &str)]) -> NodeId {
    let attributes = attrs
        .iter()
        .map(|(key, value)| Attribute::new(*key, *value))
        .collect();
    let id = doc.create_element(Namespace::Html, name, attributes);
    doc.append_child(parent, id);
    id
}

fn computed(css_text: &str) -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let root = doc.root();
    let html = append(&mut doc, root, "html", &[]);
    let body = append(&mut doc, html, "body", &[]);
    let target = append(&mut doc, body, "div", &[("id", "target")]);
    let styles = css::resolve_document_styles(&doc, Some(css_text));
    (doc, styles, target)
}

fn style_of(styles: &HashMap<NodeId, ComputedStyle>, target: NodeId) -> &ComputedStyle {
    styles.get(&target).expect("computed style")
}

#[test]
fn width_height_store_mixed_calc_components() {
    let (_doc, styles, target) =
        computed("#target { width: calc(100% - 20px); height: calc(50% + 1em); }");
    let style = style_of(&styles, target);
    let width = style.calc_lengths.width.expect("mixed width");
    assert_eq!(width.percent, 100.0);
    assert_eq!(width.px, -20.0);
    let height = style.calc_lengths.height.expect("mixed height");
    assert_eq!(height.percent, 50.0);
    assert_eq!(height.px, 16.0);
    // The plain `Length` fallback keeps the absolute component for legacy consumers.
    assert!(matches!(style.width, css::Length::Px(px) if px == -20.0));
}

#[test]
fn min_max_store_mixed_calc_components() {
    let (_doc, styles, target) = computed(
        "#target { min-width: calc(25% + 4px); max-width: calc(75% - 6px); \
         min-height: calc(10% + 2px); max-height: calc(90% - 3px); }",
    );
    let style = style_of(&styles, target);
    assert_eq!(
        style.calc_lengths.min_width.expect("min-width").percent,
        25.0
    );
    assert_eq!(style.calc_lengths.max_width.expect("max-width").px, -6.0);
    assert_eq!(style.calc_lengths.min_height.expect("min-height").px, 2.0);
    assert_eq!(
        style.calc_lengths.max_height.expect("max-height").percent,
        90.0
    );
}

#[test]
fn margins_paddings_and_offsets_store_mixed_calc() {
    let (_doc, styles, target) = computed(
        "#target { margin-left: calc(50% - 20px); margin-top: calc(2% + 1em); \
         padding-right: calc(10% + 3px); padding-bottom: calc(2em - 1%); \
         top: calc(100% - 30px); right: calc(5% + 6px); bottom: calc(4% + 1em); \
         left: calc(33% + 2px); }",
    );
    let style = style_of(&styles, target);
    assert_eq!(
        style.calc_lengths.margin_left.expect("margin-left").percent,
        50.0
    );
    assert_eq!(
        style.calc_lengths.margin_left.expect("margin-left").px,
        -20.0
    );
    assert_eq!(
        style.calc_lengths.margin_top.expect("margin-top").percent,
        2.0
    );
    assert_eq!(style.calc_lengths.margin_top.expect("margin-top").px, 16.0);
    assert_eq!(
        style.calc_lengths.padding_right.expect("padding-right").px,
        3.0
    );
    assert_eq!(
        style
            .calc_lengths
            .padding_bottom
            .expect("padding-bottom")
            .px,
        32.0
    );
    assert_eq!(style.calc_lengths.top.expect("top").percent, 100.0);
    assert_eq!(style.calc_lengths.right.expect("right").px, 6.0);
    assert_eq!(style.calc_lengths.bottom.expect("bottom").percent, 4.0);
    assert_eq!(style.calc_lengths.bottom.expect("bottom").px, 16.0);
    assert_eq!(style.calc_lengths.left.expect("left").percent, 33.0);
}

#[test]
fn gaps_store_mixed_calc_from_longhands_and_shorthand() {
    let (_doc, styles, target) =
        computed("#target { row-gap: calc(10% + 2px); gap: calc(5% + 1px) calc(3% + 2em); }");
    let style = style_of(&styles, target);
    assert_eq!(style.calc_lengths.row_gap.expect("row-gap").percent, 10.0);
    assert_eq!(
        style.calc_lengths.column_gap.expect("column-gap").percent,
        3.0
    );
    assert_eq!(style.calc_lengths.column_gap.expect("column-gap").px, 32.0);

    let (_doc, styles, target) = computed("#target { gap: calc(5% + 1em); }");
    let style = style_of(&styles, target);
    let row = style.calc_lengths.row_gap.expect("gap row");
    let column = style.calc_lengths.column_gap.expect("gap column");
    assert_eq!(row.percent, 5.0);
    assert_eq!(row.px, 16.0);
    assert_eq!(column.percent, 5.0);
    assert_eq!(column.px, 16.0);
}

#[test]
fn background_position_size_and_border_radius_store_mixed_calc() {
    let (_doc, styles, target) = computed(
        "#target { background-position: calc(10% + 5px) calc(20% - 3px); \
         background-size: calc(50% + 4px) calc(25% - 2px); \
         border-radius: calc(10% + 2px) calc(20% - 4px); }",
    );
    let style = style_of(&styles, target);
    assert_eq!(
        style
            .calc_lengths
            .background_position_x
            .expect("position x")
            .px,
        5.0
    );
    assert_eq!(
        style
            .calc_lengths
            .background_position_y
            .expect("position y")
            .percent,
        20.0
    );
    assert_eq!(
        style
            .calc_lengths
            .background_size_width
            .expect("size w")
            .percent,
        50.0
    );
    assert_eq!(
        style
            .calc_lengths
            .background_size_height
            .expect("size h")
            .px,
        -2.0
    );
    let radius = style.calc_lengths.border_radius.expect("radius");
    assert_eq!(radius[0].px, 2.0);
    assert_eq!(radius[2].px, -4.0);
}

#[test]
fn plain_values_leave_calc_sides_empty() {
    let (_doc, styles, target) =
        computed("#target { width: 100px; height: 50%; margin: 4px; padding: 2%; gap: 8px; }");
    let style = style_of(&styles, target);
    assert_eq!(style.calc_lengths, css::CalcLengths::default());
    assert_eq!(style.calc_lengths.width, None);
    assert_eq!(style.calc_lengths.height, None);
}

#[test]
fn margin_and_padding_shorthands_expand_mixed_calc() {
    let (_doc, styles, target) =
        computed("#target { margin: calc(10% - 2px); padding: calc(1em + 3px) calc(2% + 1px); }");
    let style = style_of(&styles, target);
    // 1-value margin shorthand: all four sides identical.
    for side in [
        style.calc_lengths.margin_top,
        style.calc_lengths.margin_right,
        style.calc_lengths.margin_bottom,
        style.calc_lengths.margin_left,
    ] {
        let side = side.expect("margin side");
        assert_eq!((side.percent, side.px), (10.0, -2.0));
    }
    // 2-value padding shorthand: top/bottom first (pure `calc(1em + 3px)` stays in
    // the plain `Length`), right/left second (mixed, stored in the side table).
    assert_eq!(style.calc_lengths.padding_top, None);
    assert_eq!(style.calc_lengths.padding_bottom, None);
    let right = style.calc_lengths.padding_right.expect("padding right");
    assert_eq!((right.percent, right.px), (2.0, 1.0));
    let left = style.calc_lengths.padding_left.expect("padding left");
    assert_eq!((left.percent, left.px), (2.0, 1.0));
}

#[test]
fn border_radius_slash_stores_horizontal_and_vertical_components() {
    let (_doc, styles, target) =
        computed("#target { border-radius: calc(10% + 2px) / calc(20% - 4px); }");
    let style = style_of(&styles, target);
    let radius = style.calc_lengths.border_radius.expect("radius");
    assert_eq!((radius[0].percent, radius[0].px), (10.0, 2.0));
    assert_eq!((radius[1].percent, radius[1].px), (20.0, -4.0));
    assert_eq!((radius[6].percent, radius[6].px), (10.0, 2.0));
}

#[test]
fn pure_calc_values_do_not_populate_calc_sides() {
    let (_doc, styles, target) = computed("#target { width: calc(50% + 0px); height: calc(2em); }");
    let style = style_of(&styles, target);
    assert_eq!(style.calc_lengths.width, None);
    assert_eq!(style.calc_lengths.height, None);
    assert!(matches!(style.width, css::Length::Percent(50.0)));
    assert!(matches!(style.height, css::Length::Px(32.0)));
}

#[test]
fn single_background_position_mixed_calc_is_stored_on_x() {
    let (_doc, styles, target) = computed("#target { background-position: calc(25% + 3px); }");
    let style = style_of(&styles, target);
    assert_eq!(
        style
            .calc_lengths
            .background_position_x
            .expect("position x")
            .percent,
        25.0
    );
    assert_eq!(style.calc_lengths.background_position_y, None);
}

#[test]
fn unlisted_properties_do_not_break_calc_side_computation() {
    let (_doc, styles, target) = computed(
        "#target { width: calc(100% - 20px); color: red; font-size: 20px;          border-left: 2px solid black; transform: translateX(calc(10% + 1px)); }",
    );
    let style = style_of(&styles, target);
    let width = style.calc_lengths.width.expect("width");
    assert_eq!((width.percent, width.px), (100.0, -20.0));
    assert_eq!(style.font_size, 20.0);
}

#[test]
fn border_radius_longhand_mixed_calc_is_stored() {
    let (_doc, styles, target) = computed(
        "#target { border-top-left-radius: calc(10% + 2px) calc(20% - 4px);          border-bottom-right-radius: calc(5% + 1px); }",
    );
    let style = style_of(&styles, target);
    let radius = style.calc_lengths.border_radius.expect("radius");
    assert_eq!((radius[0].percent, radius[0].px), (10.0, 2.0));
    assert_eq!((radius[1].percent, radius[1].px), (20.0, -4.0));
    assert_eq!((radius[4].percent, radius[4].px), (5.0, 1.0));
    assert_eq!(
        radius[6],
        css::calc::CalcValue {
            percent: 0.0,
            px: 0.0
        }
    );
}
