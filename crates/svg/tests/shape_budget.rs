//! The documented per-document segment budget must also cover `<polygon>`/`<polyline>`
//! `points` and every flattened curve inside one `d` attribute. Both used to be remote
//! CPU/memory DoS vectors: `points` produced unbounded edges for the rasterizer, and a
//! single path could allocate gigabytes before the budget was checked on flush.

use svg::{Limits, SvgError, SvgOptions, render_svg, render_svg_with_limits};

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
    // A trailing malformed token makes the test discriminating: the old parser only
    // checked the budget when the sub-path was flushed, so it reached this token and
    // returned `Invalid`; the per-command check returns `Limit` after the first curve.
    d.push_str(" X");
    let source = svg_doc(&format!("<path d='{d}' fill='none' stroke='black'/>"));
    let limits = Limits {
        max_path_segments: 256,
        ..Limits::default()
    };
    let error = render_svg_with_limits(source.as_bytes(), &SvgOptions::default(), &limits)
        .expect_err("curves over the budget");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");
}

#[test]
fn the_segment_budget_is_document_wide() {
    // Two polygons with five pairs each: each one fits a budget of 8, the document does not.
    let source = svg_doc(
        "<polygon points='0,0 1,0 2,0 3,0 4,0' fill='red'/>\
         <polygon points='0,1 1,1 2,1 3,1 4,1' fill='blue'/>",
    );
    let limits = Limits {
        max_path_segments: 8,
        ..Limits::default()
    };
    let error = render_svg_with_limits(source.as_bytes(), &SvgOptions::default(), &limits)
        .expect_err("two polygons exceed the document budget");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");
}

#[test]
fn use_revisits_charge_the_document_budget() {
    // `<use>` re-parses the referenced path: both visits must count, or a page can amplify
    // one large path into thousands of copies while every per-path check passes.
    let source = svg_doc(
        "<defs><path id='p' d='M0 0 L1 0 L2 0 L3 0'/></defs>\
         <use href='#p'/><use href='#p'/><use href='#p'/>",
    );
    let limits = Limits {
        max_path_segments: 6,
        ..Limits::default()
    };
    let error = render_svg_with_limits(source.as_bytes(), &SvgOptions::default(), &limits)
        .expect_err("repeated <use> exceeds the document budget");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");
}

#[test]
fn a_closed_polygon_counts_its_closing_vertex() {
    let source = svg_doc("<polygon points='0,0 1,0 2,0' fill='red'/>");
    let limits = Limits {
        max_path_segments: 3,
        ..Limits::default()
    };
    let error = render_svg_with_limits(source.as_bytes(), &SvgOptions::default(), &limits)
        .expect_err("the closing vertex exceeds a 3-segment budget");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");
}

#[test]
fn close_command_followed_by_data_does_not_hang() {
    // `Z` consumes no input; implicit repetition of it looped forever on `M0 0Z1`.
    let source = svg_doc("<path d='M0 0Z1' stroke='black'/>");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = render_svg(source.as_bytes(), &SvgOptions::default());
        let _ = tx.send(());
    });
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("the path parser must not loop forever on `Z` followed by data");
}
