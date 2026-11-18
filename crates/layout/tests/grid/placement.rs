use super::common::*;

#[test]
fn test_explicit_column_placement_line_slash_line() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0), px(100.0)]);
    let case = layout_case(
        container,
        vec![
            with_placement(
                item_style(Length::Auto, Length::Px(20.0)),
                (megi_css::GridLine::Line(2), megi_css::GridLine::Line(4)),
                (megi_css::GridLine::Auto, megi_css::GridLine::Auto),
            ),
            item_style(Length::Auto, Length::Px(20.0)),
        ],
    );
    // `2 / 4` occupies the second and third columns (lines 2..=4).
    assert_eq!(case.item(0).dimensions.content.x, 100.0);
    assert_eq!(case.item(0).dimensions.content.width, 200.0);
}

#[test]
fn test_span_columns_auto_placement_fills_around() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0), px(100.0)]);
    let case = layout_case(
        container,
        vec![
            with_placement(
                item_style(Length::Auto, Length::Px(20.0)),
                (megi_css::GridLine::Auto, megi_css::GridLine::Span(2)),
                (megi_css::GridLine::Auto, megi_css::GridLine::Auto),
            ),
            item_style(Length::Auto, Length::Px(20.0)),
            item_style(Length::Auto, Length::Px(20.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.x, 0.0);
    assert_eq!(case.item(0).dimensions.content.width, 200.0);
    assert_eq!(case.item(1).dimensions.content.x, 200.0);
    assert_eq!(case.item(2).dimensions.content.x, 0.0);
    assert!((case.item(2).dimensions.content.y - 20.0).abs() < 0.01);
}

#[test]
fn test_row_span_over_template_rows() {
    let mut container = grid_container_style(Length::Px(100.0), Length::Auto);
    container.grid_template_columns = template(vec![fr(1.0)]);
    container.grid_template_rows = template(vec![px(30.0), px(40.0)]);
    let case = layout_case(
        container,
        vec![
            with_placement(
                item_style(Length::Auto, Length::Auto),
                (megi_css::GridLine::Auto, megi_css::GridLine::Auto),
                (megi_css::GridLine::Line(1), megi_css::GridLine::Span(2)),
            ),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    assert_eq!(case.item(0).dimensions.content.y, 0.0);
    assert!((case.item(0).dimensions.content.height - 70.0).abs() < 0.01);
    // The single auto item is placed in the next free row (row 3, implicit).
    assert!((case.item(1).dimensions.content.y - 70.0).abs() < 0.01);
}

#[test]
fn test_negative_line_placement() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0), px(100.0)]);
    let case = layout_case(
        container,
        vec![with_placement(
            item_style(Length::Auto, Length::Px(20.0)),
            (megi_css::GridLine::Line(-2), megi_css::GridLine::Line(-1)),
            (megi_css::GridLine::Auto, megi_css::GridLine::Auto),
        )],
    );
    // `-2 / -1` is the last column (line 3 = start, line 4 = end).
    assert_eq!(case.item(0).dimensions.content.x, 200.0);
    assert_eq!(case.item(0).dimensions.content.width, 100.0);
}

#[test]
fn test_placement_longhands_start_and_span_end() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0), px(100.0)]);
    let mut style = item_style(Length::Auto, Length::Px(20.0));
    style.grid_column_start = megi_css::GridLine::Line(2);
    style.grid_column_end = megi_css::GridLine::Span(2);
    let case = layout_case(container, vec![style]);
    assert_eq!(case.item(0).dimensions.content.x, 100.0);
    assert_eq!(case.item(0).dimensions.content.width, 200.0);
}

#[test]
fn test_auto_placement_skips_explicitly_occupied_cells() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0), px(100.0)]);
    let case = layout_case(
        container,
        vec![
            with_placement(
                item_style(Length::Auto, Length::Px(10.0)),
                (megi_css::GridLine::Line(3), megi_css::GridLine::Auto),
                (megi_css::GridLine::Line(1), megi_css::GridLine::Auto),
            ),
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    // Explicit item occupies the third column; auto items fill columns 1 and 2.
    assert_eq!(case.item(0).dimensions.content.x, 200.0);
    assert_eq!(case.item(1).dimensions.content.x, 0.0);
    assert_eq!(case.item(2).dimensions.content.x, 100.0);
}

#[test]
fn test_repeat_auto_fill_minmax_creates_fitting_tracks() {
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.column_gap = Length::Px(10.0);
    container.grid_template_columns = GridTemplate {
        tracks: Vec::new(),
        auto_repeat: Some(megi_css::AutoRepeat {
            kind: megi_css::AutoRepeatKind::Fill,
            tracks: vec![TrackSize::MinMax {
                min: megi_css::TrackMin::Fixed(Length::Px(60.0)),
                max: megi_css::TrackMax::Fraction(1.0),
            }],
        }),
        line_names: Vec::new(),
    };
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    // floor((200 + 10) / (60 + 10)) = 3 tracks of 60px with 10px gaps.
    assert!((case.item(0).dimensions.content.width - 60.0).abs() < 0.01);
    assert_eq!(case.item(1).dimensions.content.x, 70.0);
    assert_eq!(case.item(2).dimensions.content.x, 140.0);
}

#[test]
fn test_repeat_auto_fill_fixed_tracks_wrap_items() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = GridTemplate {
        tracks: Vec::new(),
        auto_repeat: Some(megi_css::AutoRepeat {
            kind: megi_css::AutoRepeatKind::Fill,
            tracks: vec![px(80.0)],
        }),
        line_names: Vec::new(),
    };
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    // floor((300 + 0) / 80) = 3 tracks; the fourth item wraps to the next row.
    assert_eq!(case.item(0).dimensions.content.x, 0.0);
    assert_eq!(case.item(1).dimensions.content.x, 80.0);
    assert_eq!(case.item(2).dimensions.content.x, 160.0);
    assert_eq!(case.item(3).dimensions.content.x, 0.0);
    assert!((case.item(3).dimensions.content.y - 10.0).abs() < 0.01);
}

#[test]
fn test_minmax_fixed_max_and_min_clamp() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![TrackSize::MinMax {
        min: megi_css::TrackMin::Fixed(Length::Px(100.0)),
        max: megi_css::TrackMax::Fixed(Length::Px(150.0)),
    }]);
    let case = layout_case(container, vec![item_style(Length::Auto, Length::Px(10.0))]);
    assert!((case.item(0).dimensions.content.width - 150.0).abs() < 0.01);

    let mut clamp = grid_container_style(Length::Px(300.0), Length::Auto);
    clamp.grid_template_columns = template(vec![TrackSize::MinMax {
        min: megi_css::TrackMin::Fixed(Length::Px(120.0)),
        max: megi_css::TrackMax::Fixed(Length::Px(50.0)),
    }]);
    let case = layout_case(clamp, vec![item_style(Length::Auto, Length::Px(10.0))]);
    assert!((case.item(0).dimensions.content.width - 120.0).abs() < 0.01);
}

#[test]
fn test_order_reorders_grid_auto_placement() {
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    let mut first = item_style(Length::Auto, Length::Px(10.0));
    first.order = 2;
    let mut second = item_style(Length::Auto, Length::Px(10.0));
    second.order = 1;
    let case = layout_case(container, vec![first, second]);
    // `order: 1` is placed first (column 0), `order: 2` second (column 1).
    assert_eq!(case.item(0).dimensions.content.x, 100.0);
    assert_eq!(case.item(1).dimensions.content.x, 0.0);
}

#[test]
fn test_span_content_width_uses_all_spanned_tracks() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0), px(100.0)]);
    container.column_gap = Length::Px(10.0);
    let mut spanning = with_placement(
        item_style(Length::Auto, Length::Px(10.0)),
        (megi_css::GridLine::Line(1), megi_css::GridLine::Span(2)),
        (megi_css::GridLine::Auto, megi_css::GridLine::Auto),
    );
    spanning.justify_self = megi_css::AlignSelf::Stretch;
    let case = layout_case(container, vec![spanning]);
    // Two 100px columns plus the 10px inner gap, minus container gap layout:
    // spans columns 1-2 => 100 + 10 + 100 = 210.
    assert!((case.item(0).dimensions.content.width - 210.0).abs() < 0.01);
}

#[test]
fn test_auto_fit_is_treated_like_auto_fill() {
    let mut container = grid_container_style(Length::Px(220.0), Length::Auto);
    container.column_gap = Length::Px(10.0);
    container.grid_template_columns = GridTemplate {
        tracks: Vec::new(),
        auto_repeat: Some(megi_css::AutoRepeat {
            kind: megi_css::AutoRepeatKind::Fit,
            tracks: vec![px(60.0)],
        }),
        line_names: Vec::new(),
    };
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    // floor((220 + 10) / (60 + 10)) = 3 tracks: x = 0, 70, 140.
    assert_eq!(case.item(0).dimensions.content.x, 0.0);
    assert_eq!(case.item(1).dimensions.content.x, 70.0);
    assert_eq!(case.item(2).dimensions.content.x, 140.0);
}

#[test]
fn test_order_tiebreak_keeps_document_order() {
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    let mut first = item_style(Length::Auto, Length::Px(10.0));
    first.order = 1;
    let mut second = item_style(Length::Auto, Length::Px(10.0));
    second.order = 1;
    let case = layout_case(container, vec![first, second]);
    assert_eq!(case.item(0).dimensions.content.x, 0.0);
    assert_eq!(case.item(1).dimensions.content.x, 100.0);
}

#[test]
fn test_start_only_span_places_item_over_n_columns() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0), px(100.0)]);
    let mut style = item_style(Length::Auto, Length::Px(10.0));
    style.grid_column_start = megi_css::GridLine::Span(2);
    let case = layout_case(container, vec![style]);
    assert_eq!(case.item(0).dimensions.content.width, 200.0);
}
