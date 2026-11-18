use super::*;

#[test]
fn test_fill_rule_nonzero_vs_evenodd() {
    let path = "M0 0 H10 V10 H0 Z M2 2 H8 V8 H2 Z";
    let nonzero = render_at(
        &svg_doc(10.0, 10.0, &format!("<path d='{path}' fill='black'/>")),
        10,
        10,
    );
    assert_eq!(pixel(&nonzero, 5, 5), [0, 0, 0, 255]);
    let evenodd = render_at(
        &svg_doc(
            10.0,
            10.0,
            &format!("<path d='{path}' fill='black' fill-rule='evenodd'/>"),
        ),
        10,
        10,
    );
    assert_eq!(pixel(&evenodd, 5, 5), [0, 0, 0, 0]);
    assert_eq!(pixel(&evenodd, 1, 1), [0, 0, 0, 255]);
}

#[test]
fn test_curves_and_arcs_are_filled() {
    let cubic = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<path d='M0 5 C 2 0 8 10 10 5 Z' fill='black'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&cubic, 2, 4), [0, 0, 0, 255]);
    let arc = render_at(
        &svg_doc(10.0, 10.0, "<path d='M2 5 A3 3 0 0 1 8 5 Z' fill='black'/>"),
        10,
        10,
    );
    assert_eq!(pixel(&arc, 5, 4), [0, 0, 0, 255]);
    assert_eq!(pixel(&arc, 5, 7), [0, 0, 0, 0]);
}

#[test]
fn test_stroke_width_caps_and_joins() {
    let butt = render_at(
        &svg_doc(
            12.0,
            12.0,
            "<line x1='2' y1='6' x2='8' y2='6' stroke='black' stroke-width='4' stroke-linecap='butt'/>",
        ),
        12,
        12,
    );
    assert_eq!(pixel(&butt, 4, 6)[3], 255);
    assert_eq!(pixel(&butt, 1, 6)[3], 0);
    assert_eq!(pixel(&butt, 10, 6)[3], 0);
    let square = render_at(
        &svg_doc(
            12.0,
            12.0,
            "<line x1='2' y1='6' x2='8' y2='6' stroke='black' stroke-width='4' stroke-linecap='square'/>",
        ),
        12,
        12,
    );
    assert_eq!(pixel(&square, 9, 6)[3], 255);
    let round = render_at(
        &svg_doc(
            12.0,
            12.0,
            "<line x1='2' y1='6' x2='8' y2='6' stroke='black' stroke-width='4' stroke-linecap='round'/>",
        ),
        12,
        12,
    );
    // A round cap covers most of (9,6) but its outer corner is partially covered.
    assert!(pixel(&round, 9, 6)[3] >= 230);
    let miter = render_at(
        &svg_doc(
            12.0,
            12.0,
            "<polyline points='2,10 6,2 10,10' fill='none' stroke='black' stroke-width='2' stroke-linejoin='miter'/>",
        ),
        12,
        12,
    );
    assert!(miter.data.iter().any(|byte| *byte != 0));
}
