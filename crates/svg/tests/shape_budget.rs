//! The documented per-document segment budget must also cover `<polygon>`/`<polyline>`
//! `points` and every flattened curve inside one `d` attribute. Both used to be remote
//! CPU/memory DoS vectors: `points` produced unbounded edges for the rasterizer, and a
//! single path could allocate gigabytes before the budget was checked on flush.

use svg::{Limits, SvgError, SvgOptions, render_svg_with_limits};

fn svg_doc(body: &str) -> String {
    format!("<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'>{body}</svg>")
}

#[test]
fn points_attribute_respects_the_segment_budget() {
    let points = (0..10)
        .map(|index| format!("{index},{}", index % 2))
        .collect::<Vec<_>>()
        .join(" ");
    let source = svg_doc(&format!("<polygon points='{points}' fill='red'/>"));
    let limits = Limits {
        max_path_segments: 8,
        ..Limits::default()
    };
    let error = render_svg_with_limits(source.as_bytes(), &SvgOptions::default(), &limits)
        .expect_err("points over the budget");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");
}

#[test]
fn flattened_curves_are_budgeted_within_a_single_subpath() {
    // Each `C` flattens to 4,097 points (p0 == p3, control points far away), so one
    // sub-path of two curves already exceeds a 256-point budget.
    let mut d = String::from("M0 0");
    for _ in 0..2 {
        d.push_str(" C1e38 1e38 -1e38 -1e38 0 0");
    }
    let source = svg_doc(&format!("<path d='{d}' fill='none' stroke='black'/>"));
    let limits = Limits {
        max_path_segments: 256,
        ..Limits::default()
    };
    let error = render_svg_with_limits(source.as_bytes(), &SvgOptions::default(), &limits)
        .expect_err("curves over the budget");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");
}
