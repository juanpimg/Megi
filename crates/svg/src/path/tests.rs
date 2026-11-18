use super::*;
use crate::Limits;

fn points(raw: &str) -> Vec<SubPath> {
    let mut used = 0usize;
    parse_path_data(raw, &Limits::default(), &mut used).expect("valid path")
}

#[test]
fn test_move_line_and_close() {
    let subpaths = points("M0 0 L10 0 L10 10 Z");
    assert_eq!(subpaths.len(), 1);
    assert_eq!(subpaths[0].points.len(), 4);
    assert!(subpaths[0].closed);
    assert_eq!(subpaths[0].points[3], (0.0, 0.0));
}

#[test]
fn test_relative_horizontal_vertical_and_implicit_repetition() {
    let subpaths = points("M1 1 h2 v2 l1 1");
    assert_eq!(subpaths[0].points.last().copied(), Some((4.0, 4.0)));
    let repeated = points("M0 0 L1 0 2 0 3 0");
    assert_eq!(repeated[0].points.len(), 4);
}

#[test]
fn test_cubic_quadratic_and_smooth_commands_flatten() {
    for raw in [
        "M0 0 C1 0 2 1 3 0",
        "M0 0 Q1 1 2 0",
        "M0 0 C1 0 2 1 3 0 S4 2 5 0",
        "M0 0 Q1 1 2 0 T3 0",
    ] {
        let subpaths = points(raw);
        assert!(subpaths[0].points.len() >= 3, "{raw}");
    }
}

#[test]
fn test_arc_flags_compact_and_units() {
    let subpaths = points("M0 0a5 5 0 0110 10");
    assert!(subpaths[0].points.len() >= 3);
    assert_eq!(subpaths[0].points.last().copied(), Some((10.0, 10.0)));
}

#[test]
fn test_malformed_paths_error() {
    let mut used = 0usize;
    assert!(parse_path_data("30 30", &Limits::default(), &mut used).is_err());
    assert!(parse_path_data("M0", &Limits::default(), &mut used).is_err());
    assert!(parse_path_data("M0 0 X1 1", &Limits::default(), &mut used).is_err());
    assert!(parse_path_data("M0 0 A5 5 0 2 0 10 10", &Limits::default(), &mut used).is_err());
}
