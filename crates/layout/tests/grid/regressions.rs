use super::common::*;

#[test]
fn test_auto_item_span_wider_than_the_implicit_columns_terminates() {
    // One implicit column + `span 2`: the placement loop used to advance the row forever
    // because the column limit never grew; it must widen the implicit grid instead.
    let mut item = item_style(Length::Px(50.0), Length::Px(10.0));
    item.grid_column_start = GridLine::Span(2);
    let case = layout_case(
        grid_container_style(Length::Px(200.0), Length::Auto),
        vec![item],
    );
    let laid_out = case.item(0);
    assert!(laid_out.dimensions.content.width.is_finite());
}

#[test]
fn test_auto_item_with_a_huge_span_is_clamped() {
    // `span 4294967295` used to ask `ensure_occupancy` for a 4e9-wide row.
    let mut item = item_style(Length::Px(50.0), Length::Px(10.0));
    item.grid_column_start = GridLine::Span(u32::MAX);
    let case = layout_case(
        grid_container_style(Length::Px(200.0), Length::Auto),
        vec![item],
    );
    let laid_out = case.item(0);
    assert!(laid_out.dimensions.content.width.is_finite());
    assert!(laid_out.dimensions.content.width <= 200.001);
}

#[test]
fn test_explicit_line_and_end_beyond_the_track_cap_terminate() {
    // `grid-column-start: 2147483647` / `grid-column-end: 2147483647` used to request ~2e9
    // tracks; both are clamped with the span so start + span <= MAX_EXPANDED_TRACKS.
    for line in [GridLine::Line(i32::MAX), GridLine::Line(1_000_000)] {
        let mut item = item_style(Length::Px(20.0), Length::Px(10.0));
        item.grid_column_start = line;
        item.grid_column_end = GridLine::Line(i32::MAX);
        let case = layout_case(
            grid_container_style(Length::Px(200.0), Length::Auto),
            vec![item],
        );
        let laid_out = case.item(0);
        assert!(laid_out.dimensions.content.width.is_finite());
    }
}

#[test]
fn test_grid_item_respects_min_max_width_and_min_wins_over_max() {
    // Item with auto width clamped by max-width inside a 200px track.
    let mut max_clamped = item_style(Length::Auto, Length::Px(20.0));
    max_clamped.max_width = Length::Px(80.0);
    let case_max = layout_case(
        grid_container_style(Length::Px(200.0), Length::Auto),
        vec![max_clamped],
    );
    assert!((case_max.item(0).dimensions.content.width - 80.0).abs() < 0.1);

    // When min-width > max-width, CSS 2.2 §10.4 specifies min-width wins.
    let mut min_over_max = item_style(Length::Px(50.0), Length::Px(20.0));
    min_over_max.min_width = Length::Px(120.0);
    min_over_max.max_width = Length::Px(80.0);
    let case_min_wins = layout_case(
        grid_container_style(Length::Px(200.0), Length::Auto),
        vec![min_over_max],
    );
    assert!((case_min_wins.item(0).dimensions.content.width - 120.0).abs() < 0.1);
}
