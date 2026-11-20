//! CSS `transform` parsing (#1539): translate/scale/rotate resolve into a 2D matrix
//! on `ComputedStyle`; `none` and malformed values stay unset.

use megi_core::{Document, Namespace};
use megi_css::resolve_document_styles;

fn style_for(css_text: &str) -> megi_css::ComputedStyle {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let div = doc.create_element(
        Namespace::Html,
        "div",
        vec![megi_core::Attribute::new("id", "target")],
    );
    doc.append_child(html, div);
    let styles = resolve_document_styles(&doc, Some(css_text));
    styles.get(&div).cloned().expect("target style is resolved")
}

#[test]
fn transform_parses_into_computed_style() {
    let style = style_for("#target { transform: translate(10px, 20px) scale(2); }");
    let transform = style.transform.expect("transform is parsed");
    let (x, y) = transform.apply(3.0, 4.0);
    assert!((x - 16.0).abs() < 0.001, "x={x}");
    assert!((y - 28.0).abs() < 0.001, "y={y}");
}

#[test]
fn transform_rotate_parses_and_serializes() {
    let style = style_for("#target { transform: rotate(90deg); }");
    let transform = style.transform.expect("transform is parsed");
    let (x, y) = transform.apply(1.0, 0.0);
    assert!(x.abs() < 0.001, "x={x}");
    assert!((y - 1.0).abs() < 0.001, "y={y}");
    assert!(transform.to_css().starts_with("matrix("));
}

#[test]
fn transform_none_and_absent_are_unset() {
    let none = style_for("#target { transform: none; }");
    assert!(none.transform.is_none());
    let absent = style_for("#target { color: red; }");
    assert!(absent.transform.is_none());
    let invalid = style_for("#target { transform: bogus(1); }");
    assert!(invalid.transform.is_none(), "malformed value ignored");
}

#[test]
fn transform_is_not_inherited() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let parent = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(html, parent);
    let child = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(parent, child);
    let css_text = "div { transform: translate(5px, 0); }";
    let styles = resolve_document_styles(&doc, Some(css_text));
    assert!(styles.get(&parent).and_then(|s| s.transform).is_some());
    assert!(
        styles.get(&child).and_then(|s| s.transform).is_none(),
        "transform must not inherit"
    );
}

#[test]
fn transform_function_names_are_case_insensitive() {
    // #1668: CSS function names are ASCII case-insensitive.
    let upper = megi_css::transform::Transform::parse("TRANSLATE(10px) ROTATE(90deg)");
    let lower = megi_css::transform::Transform::parse("translate(10px) rotate(90deg)");
    assert!(upper.is_some(), "uppercase function names must parse");
    assert_eq!(upper, lower);
}

#[test]
fn transform_validates_units_and_arity() {
    use megi_css::transform::Transform;
    // #1667: typed units and strict arity; garbage suffixes and extra args are invalid.
    assert!(
        Transform::parse("translate(10%)").is_none(),
        "percent not supported in 2D subset"
    );
    assert!(
        Transform::parse("translate(10qpx)").is_none(),
        "garbage unit must be rejected"
    );
    assert!(
        Transform::parse("translate(1px,2px,3px)").is_none(),
        "translate takes 1-2 args"
    );
    assert!(
        Transform::parse("rotate(90deg,1)").is_none(),
        "rotate takes exactly 1 arg"
    );
    assert!(
        Transform::parse("scale(2,3,4)").is_none(),
        "scale takes 1-2 args"
    );
    // rad is a valid angle: 1.5707963rad ≈ 90deg.
    let rad = Transform::parse("rotate(1.5707963rad)").expect("rad angle");
    let deg = Transform::parse("rotate(90deg)").expect("deg angle");
    let p = rad.apply(1.0, 0.0);
    let q = deg.apply(1.0, 0.0);
    assert!(
        (p.0 - q.0).abs() < 1e-5 && (p.1 - q.1).abs() < 1e-5,
        "rad vs deg: {p:?} {q:?}"
    );
}

// #1933: matrix() is the canonical form emitted by animation libraries.
#[test]
fn matrix_function_parses_with_strict_arity() {
    use megi_css::transform::Transform;
    let m = Transform::parse("matrix(1,0.5,0,1,20,0)").expect("matrix must parse");
    let (x, y) = m.apply(10.0, 0.0);
    assert!((x - 30.0).abs() < 1e-4, "x={x}");
    assert!((y - 5.0).abs() < 1e-4, "y={y}");
    assert!(
        Transform::parse("matrix(1,0,0,1,0)").is_none(),
        "5 args invalid"
    );
    assert!(
        Transform::parse("matrix(1,0,0,1,0,0,0)").is_none(),
        "7 args invalid"
    );
    assert!(
        Transform::parse("matrix(1px,0,0,1,0,0)").is_none(),
        "matrix arguments are unitless numbers"
    );
    assert_eq!(
        Transform::parse("MATRIX(1,0,0,1,20,0)"),
        Transform::parse("matrix(1,0,0,1,20,0)"),
        "function names are case-insensitive"
    );
}

#[test]
fn skew_functions_parse_and_compose() {
    use megi_css::transform::Transform;
    let sx = Transform::parse("skewX(45deg)").expect("skewX must parse");
    let p = sx.apply(0.0, 1.0);
    assert!(
        (p.0 - 1.0).abs() < 1e-4 && (p.1 - 1.0).abs() < 1e-4,
        "{p:?}"
    );
    let sy = Transform::parse("skewY(45deg)").expect("skewY must parse");
    let q = sy.apply(1.0, 0.0);
    assert!(
        (q.0 - 1.0).abs() < 1e-4 && (q.1 - 1.0).abs() < 1e-4,
        "{q:?}"
    );
    assert!(
        Transform::parse("skewX(45deg,1)").is_none(),
        "skew takes 1 arg"
    );
    assert!(
        Transform::parse("skewX(45)").is_none(),
        "angle requires a unit"
    );
    // matrix() composes with the existing functions (last function applied first).
    let composed = Transform::parse("translate(5px) matrix(1,0,0,1,20,0)").expect("compose");
    let (x, y) = composed.apply(0.0, 0.0);
    assert!(
        (x - 25.0).abs() < 1e-4 && y.abs() < 1e-4,
        "compose: {x},{y}"
    );
}

// #1934: T(origin) · M · T(-origin) — the transform must act about the given point.
#[test]
fn transform_rotates_around_the_requested_origin() {
    use megi_css::transform::Transform;
    let rotate = Transform::parse("rotate(90deg)").expect("rotate");
    let about = rotate.around_origin(100.0, 0.0);
    let p = about.apply(100.0, 0.0);
    assert!(
        (p.0 - 100.0).abs() < 1e-4 && p.1.abs() < 1e-4,
        "origin stays: {p:?}"
    );
    let q = about.apply(160.0, 0.0);
    assert!(
        (q.0 - 100.0).abs() < 1e-4 && (q.1 - 60.0).abs() < 1e-4,
        "right edge: {q:?}"
    );
}

#[test]
fn transform_origin_parses_lengths_percentages_and_keywords() {
    use megi_css::{Length, TransformOrigin};
    // CSS Transforms 1 §3: initial value is 50% 50% for HTML elements.
    assert_eq!(
        style_for("#target { color: red; }").transform_origin,
        TransformOrigin::INITIAL
    );
    assert_eq!(
        style_for("#target { transform-origin: 0 0; }").transform_origin,
        TransformOrigin {
            x: Length::Px(0.0),
            y: Length::Px(0.0),
        }
    );
    assert_eq!(
        style_for("#target { transform-origin: 10px 20px; }").transform_origin,
        TransformOrigin {
            x: Length::Px(10.0),
            y: Length::Px(20.0),
        }
    );
    assert_eq!(
        style_for("#target { transform-origin: 25% 75%; }").transform_origin,
        TransformOrigin {
            x: Length::Percent(25.0),
            y: Length::Percent(75.0),
        }
    );
    assert_eq!(
        style_for("#target { transform-origin: left top; }").transform_origin,
        TransformOrigin {
            x: Length::Percent(0.0),
            y: Length::Percent(0.0),
        }
    );
    assert_eq!(
        style_for("#target { transform-origin: right bottom; }").transform_origin,
        TransformOrigin {
            x: Length::Percent(100.0),
            y: Length::Percent(100.0),
        }
    );
    // One component: the second defaults to `center` (50%).
    assert_eq!(
        style_for("#target { transform-origin: left; }").transform_origin,
        TransformOrigin {
            x: Length::Percent(0.0),
            y: Length::Percent(50.0),
        }
    );
    assert_eq!(
        style_for("#target { transform-origin: center 10px; }").transform_origin,
        TransformOrigin {
            x: Length::Percent(50.0),
            y: Length::Px(10.0),
        }
    );
    // Malformed values keep the initial value.
    assert_eq!(
        style_for("#target { transform-origin: 10px 20px 30px; }").transform_origin,
        TransformOrigin::INITIAL
    );
    assert_eq!(
        style_for("#target { transform-origin: bogus; }").transform_origin,
        TransformOrigin::INITIAL
    );
}

#[test]
fn transform_origin_is_not_inherited() {
    use megi_css::TransformOrigin;
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let parent = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(html, parent);
    let child = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(parent, child);
    let styles = resolve_document_styles(&doc, Some("div { transform-origin: 0 0; }"));
    assert_eq!(
        styles.get(&parent).map(|s| s.transform_origin),
        Some(TransformOrigin {
            x: megi_css::Length::Px(0.0),
            y: megi_css::Length::Px(0.0),
        })
    );
    assert_eq!(
        styles.get(&child).map(|s| s.transform_origin),
        Some(TransformOrigin::INITIAL),
        "transform-origin must not inherit"
    );
}
