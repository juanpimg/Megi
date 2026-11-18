//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;


#[cfg(test)]
pub(crate) mod round19_tests {
    use super::*;
    use crate::rule::Declaration;

    fn declaration(name: &str, raw: &str) -> Declaration {
        Declaration {
            name: name.to_owned(),
            value: Vec::new(),
            important: false,
            raw_value: raw.to_owned(),
        }
    }

    fn compute(pairs: &[(&str, &str)]) -> ComputedGrid {
        let declarations: Vec<Declaration> = pairs
            .iter()
            .map(|(name, value)| declaration(name, value))
            .collect();
        let entries: Vec<(&str, &Declaration)> = pairs
            .iter()
            .zip(declarations.iter())
            .map(|((name, _), declaration)| (*name, declaration))
            .collect();
        compute_grid(&entries, 16.0, 16.0)
    }

    #[test]
    fn test_parse_minmax_and_auto_fill_repeat() {
        let grid = compute(&[(
            "grid-template-columns",
            "repeat(auto-fill, minmax(60px, 1fr))",
        )]);
        assert!(grid.template_columns.tracks.is_empty());
        let auto = grid.template_columns.auto_repeat.expect("auto repeat");
        assert_eq!(auto.kind, AutoRepeatKind::Fill);
        assert_eq!(
            auto.tracks,
            vec![TrackSize::MinMax {
                min: TrackMin::Fixed(Length::Px(60.0)),
                max: TrackMax::Fraction(1.0),
            }]
        );
        let fixed = compute(&[(
            "grid-template-columns",
            "minmax(100px, 150px) minmax(120px, 50px)",
        )]);
        assert_eq!(
            fixed.template_columns.tracks,
            vec![
                TrackSize::MinMax {
                    min: TrackMin::Fixed(Length::Px(100.0)),
                    max: TrackMax::Fixed(Length::Px(150.0)),
                },
                TrackSize::MinMax {
                    min: TrackMin::Fixed(Length::Px(120.0)),
                    max: TrackMax::Fixed(Length::Px(50.0)),
                },
            ]
        );
    }

    #[test]
    fn test_parse_explicit_placement_and_order() {
        let grid = compute(&[
            ("grid-column", "2 / span 3"),
            ("grid-row", "1 / 4"),
            ("order", "2"),
        ]);
        assert_eq!(grid.column_start, GridLine::Line(2));
        assert_eq!(grid.column_end, GridLine::Span(3));
        assert_eq!(grid.row_start, GridLine::Line(1));
        assert_eq!(grid.row_end, GridLine::Line(4));
        assert_eq!(grid.order, 2);

        let longhands = compute(&[
            ("grid-column-start", "-2"),
            ("grid-column-end", "span 2"),
            ("grid-row", "2"),
        ]);
        assert_eq!(longhands.column_start, GridLine::Line(-2));
        assert_eq!(longhands.column_end, GridLine::Span(2));
        assert_eq!(longhands.row_start, GridLine::Line(2));
        assert_eq!(longhands.row_end, GridLine::Auto);
    }
}


#[cfg(test)]
pub(crate) mod round22_tests {
    use super::*;
    use crate::rule::Declaration;

    fn declaration(name: &str, raw: &str) -> Declaration {
        Declaration {
            name: name.to_owned(),
            value: Vec::new(),
            important: false,
            raw_value: raw.to_owned(),
        }
    }

    fn compute(pairs: &[(&str, &str)]) -> ComputedGrid {
        let declarations: Vec<Declaration> = pairs
            .iter()
            .map(|(name, value)| declaration(name, value))
            .collect();
        let entries: Vec<(&str, &Declaration)> = pairs
            .iter()
            .zip(declarations.iter())
            .map(|((name, _), declaration)| (*name, declaration))
            .collect();
        compute_grid(&entries, 16.0, 16.0)
    }

    #[test]
    fn test_auto_flow_and_content_distribution() {
        let grid = compute(&[
            ("grid-auto-flow", "column dense"),
            ("justify-content", "space-between"),
            ("align-content", "center"),
        ]);
        assert_eq!(grid.auto_flow, GridAutoFlow::ColumnDense);
        assert_eq!(grid.justify_content, GridContentDistribution::SpaceBetween);
        assert_eq!(grid.align_content, GridContentDistribution::Center);

        let row = compute(&[("grid-auto-flow", "dense")]);
        assert_eq!(row.auto_flow, GridAutoFlow::RowDense);
    }

    #[test]
    fn test_template_areas_and_grid_area_name() {
        let grid = compute(&[
            ("grid-template-areas", "'header header' 'main side'"),
            ("grid-area", "header"),
        ]);
        let areas = grid.template_areas.expect("areas");
        assert_eq!((areas.rows, areas.columns), (2, 2));
        assert_eq!(areas.cells[0], Some("header".to_owned()));
        assert_eq!(areas.cells[3], Some("side".to_owned()));
        assert_eq!(grid.grid_area.as_deref(), Some("header"));

        let invalid = compute(&[("grid-template-areas", "'a a' 'b'")]);
        assert!(invalid.template_areas.is_none());
    }

    #[test]
    fn test_named_template_lines_and_named_placement() {
        let grid = compute(&[
            ("grid-template-columns", "[left] 100px [mid] 100px [right]"),
            ("grid-column", "mid / right"),
        ]);
        assert_eq!(
            grid.template_columns.line_names,
            vec![
                vec!["left".to_owned()],
                vec!["mid".to_owned()],
                vec!["right".to_owned()],
            ]
        );
        assert_eq!(grid.column_start, GridLine::Name("mid".to_owned()));
        assert_eq!(grid.column_end, GridLine::Name("right".to_owned()));
    }

    #[test]
    fn test_grid_area_line_placement_syntax() {
        let grid = compute(&[("grid-area", "2 / 1 / 4 / 3")]);
        assert_eq!(grid.row_start, GridLine::Line(2));
        assert_eq!(grid.column_start, GridLine::Line(1));
        assert_eq!(grid.row_end, GridLine::Line(4));
        assert_eq!(grid.column_end, GridLine::Line(3));
    }
}
