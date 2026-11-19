use super::*;

#[test]
fn test_row_items_are_placed_in_document_order() {
    let case = layout_case(
        flex_container_style(Length::Px(300.0), Length::Px(100.0)),
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.x, 0.0);
    assert_eq!(case.item(1).dimensions.content.x, 50.0);
    assert_eq!(case.item(2).dimensions.content.x, 100.0);
    assert_eq!(case.item(0).dimensions.content.width, 50.0);
}

#[test]
fn test_justify_content_space_between_center_and_evenly() {
    let space_between = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(40.0));
            style.justify_content = JustifyContent::SpaceBetween;
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert_eq!(space_between.item(0).dimensions.content.x, 0.0);
    assert_eq!(space_between.item(1).dimensions.content.x, 125.0);
    assert_eq!(space_between.item(2).dimensions.content.x, 250.0);

    let centered = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(40.0));
            style.justify_content = JustifyContent::Center;
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert_eq!(centered.item(0).dimensions.content.x, 100.0);
    assert_eq!(centered.item(1).dimensions.content.x, 150.0);

    let evenly = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(40.0));
            style.justify_content = JustifyContent::SpaceEvenly;
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    // free = 150; leading = 150 / 4 = 37.5, gaps also 37.5.
    assert!((evenly.item(0).dimensions.content.x - 37.5).abs() < 0.01);
    assert!((evenly.item(1).dimensions.content.x - 125.0).abs() < 0.01);
    assert!((evenly.item(2).dimensions.content.x - 212.5).abs() < 0.01);
}

#[test]
fn test_flex_grow_and_shrink_with_basis() {
    let grown = layout_case(
        flex_container_style(Length::Px(400.0), Length::Px(40.0)),
        vec![
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(0.0);
                style.flex_grow = 1.0;
                style
            },
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(0.0);
                style.flex_grow = 2.0;
                style
            },
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(0.0);
                style.flex_grow = 1.0;
                style
            },
        ],
    );
    assert!((grown.item(0).dimensions.content.width - 100.0).abs() < 0.01);
    assert!((grown.item(1).dimensions.content.width - 200.0).abs() < 0.01);
    assert!((grown.item(2).dimensions.content.width - 100.0).abs() < 0.01);

    let shrunk = layout_case(
        flex_container_style(Length::Px(300.0), Length::Px(40.0)),
        vec![
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(200.0);
                style.flex_shrink = 1.0;
                style
            },
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(200.0);
                style.flex_shrink = 1.0;
                style
            },
        ],
    );
    assert!((shrunk.item(0).dimensions.content.width - 150.0).abs() < 0.01);
    assert!((shrunk.item(1).dimensions.content.width - 150.0).abs() < 0.01);
}

#[test]
fn test_flex_shrink_freezes_items_at_min_size_and_redistributes() {
    // #1704: CSS Flexbox §9.7(c) — an item that reaches its minimum main size
    // is frozen, and the remaining deficit is redistributed among the others.
    let case = layout_case(
        flex_container_style(Length::Px(200.0), Length::Px(40.0)),
        vec![
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(200.0);
                style.flex_shrink = 1.0;
                style.min_width = Length::Px(150.0);
                style
            },
            {
                let mut style = item_style(Length::Auto, Length::Px(20.0));
                style.flex_basis = Length::Px(200.0);
                style.flex_shrink = 1.0;
                style
            },
        ],
    );
    assert!(
        (case.item(0).dimensions.content.width - 150.0).abs() < 0.01,
        "item at min-width must be frozen"
    );
    assert!(
        (case.item(1).dimensions.content.width - 50.0).abs() < 0.01,
        "the remaining deficit must go to the unfrozen item"
    );
}

#[test]
fn test_column_direction_and_stretch() {
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(200.0), Length::Px(200.0));
            style.flex_direction = FlexDirection::Column;
            style
        },
        vec![
            item_style(Length::Auto, Length::Px(40.0)),
            item_style(Length::Auto, Length::Px(40.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.y, 0.0);
    assert_eq!(case.item(1).dimensions.content.y, 40.0);
    // align-items defaults to stretch: items fill the 200px container width.
    assert_eq!(case.item(0).dimensions.content.width, 200.0);
}

#[test]
fn test_align_items_center_and_self_override() {
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(100.0));
            style.align_items = AlignItems::Center;
            style
        },
        vec![item_style(Length::Px(50.0), Length::Px(20.0)), {
            let mut style = item_style(Length::Px(50.0), Length::Px(20.0));
            style.align_self = AlignSelf::FlexEnd;
            style
        }],
    );
    assert!((case.item(0).dimensions.content.y - 40.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.y - 80.0).abs() < 0.01);
}

#[test]
fn test_gap_and_wrap_create_multiple_lines() {
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(120.0), Length::Auto);
            style.flex_wrap = FlexWrap::Wrap;
            style.row_gap = Length::Px(10.0);
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    // Two items fit on the first line (50 + 50), the third goes to a second line
    // separated by the 10px row gap.
    assert_eq!(case.item(0).dimensions.content.y, 0.0);
    assert_eq!(case.item(1).dimensions.content.y, 0.0);
    assert!(
        (case.item(2).dimensions.content.y - 30.0).abs() < 0.01,
        "{:?}",
        case.item(2).dimensions.content
    );
}

#[test]
fn test_row_gap_between_items() {
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(40.0));
            style.column_gap = Length::Px(10.0);
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert_eq!(case.item(1).dimensions.content.x, 60.0);
}

#[test]
fn test_reverse_directions_mirror_placement() {
    let case = layout_case(
        {
            let mut style = flex_container_style(Length::Px(300.0), Length::Px(40.0));
            style.flex_direction = FlexDirection::RowReverse;
            style
        },
        vec![
            item_style(Length::Px(50.0), Length::Px(20.0)),
            item_style(Length::Px(50.0), Length::Px(20.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.x, 250.0);
    assert_eq!(case.item(1).dimensions.content.x, 200.0);
}
