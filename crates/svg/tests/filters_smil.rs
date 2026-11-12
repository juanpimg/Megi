//! Round 26: SVG filter chains (ColorMatrix/Blend/Merge/Composite arithmetic) and SMIL.

use svg::{SvgOptions, render_svg, render_svg_with_time};

fn render(bytes: &[u8]) -> svg::SvgImage {
    render_svg(
        bytes,
        &SvgOptions {
            width: Some(40),
            height: Some(40),
        },
    )
    .expect("render")
}

fn pixel_at(image: &svg::SvgImage, x: u32, y: u32) -> [u8; 4] {
    image.pixel(x, y).expect("pixel")
}

#[test]
fn color_matrix_saturate_grayscales_red() {
    let image = render(include_bytes!("fixtures/color_matrix.svg"));
    let [r, g, b, a] = pixel_at(&image, 20, 20);
    assert_eq!(a, 255);
    assert!(r == g && g == b, "expected gray, got {:?}", [r, g, b, a]);
    assert!((50..=60).contains(&r), "red luma should be ~54, got {r}");
}

#[test]
fn color_matrix_hue_rotate_and_luminance() {
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <defs><filter id="f" x="0" y="0" width="1" height="1">
        <feColorMatrix type="hueRotate" values="180"/>
      </filter></defs>
      <rect width="20" height="20" fill="#ff0000" filter="url(#f)"/></svg>"##;
    let image = render(svg);
    let [r, g, b, _] = pixel_at(&image, 10, 10);
    assert!(
        g > 100 && b > 100,
        "red rotated 180 should be cyan, got {r},{g},{b}"
    );
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <defs><filter id="f" x="0" y="0" width="1" height="1">
        <feColorMatrix type="luminanceToAlpha"/>
      </filter></defs>
      <rect width="20" height="20" fill="#ff0000" filter="url(#f)"/></svg>"##;
    let image = render(svg);
    let [r, g, b, a] = pixel_at(&image, 10, 10);
    assert_eq!((r, g, b), (0, 0, 0));
    assert!((50..=60).contains(&a), "luminance alpha ~54, got {a}");
}

#[test]
fn color_matrix_explicit_matrix() {
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <defs><filter id="f" x="0" y="0" width="1" height="1">
        <feColorMatrix type="matrix" values="0 0 0 0 0.5  0 0 0 0 0.5  0 0 0 0 0.5  0 0 0 1 0"/>
      </filter></defs>
      <rect width="20" height="20" fill="#123456" filter="url(#f)"/></svg>"##;
    let image = render(svg);
    assert_eq!(pixel_at(&image, 10, 10), [128, 128, 128, 255]);
}

#[test]
fn blend_lighten_with_flood() {
    let image = render(include_bytes!("fixtures/blend_merge.svg"));
    assert_eq!(pixel_at(&image, 20, 20), [255, 0, 255, 255]);
}

#[test]
fn merge_flood_over_source() {
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <defs><filter id="f" x="0" y="0" width="1" height="1">
        <feFlood flood-color="#00ff00" result="g"/>
        <feMerge><feMergeNode in="SourceGraphic"/><feMergeNode in="g"/></feMerge>
      </filter></defs>
      <rect width="20" height="20" fill="#ff0000" filter="url(#f)"/></svg>"##;
    let image = render(svg);
    assert_eq!(pixel_at(&image, 10, 10), [0, 255, 0, 255]);
}

#[test]
fn composite_arithmetic_and_named_chain() {
    let image = render(include_bytes!("fixtures/blend_merge.svg"));
    // The fixture applies lighten; arithmetic is exercised below with a named chain.
    assert_eq!(pixel_at(&image, 20, 20), [255, 0, 255, 255]);
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <defs><filter id="f" x="0" y="0" width="1" height="1">
        <feFlood flood-color="#00ff00" result="g"/>
        <feComposite in="g" in2="SourceGraphic" operator="over" result="done"/>
        <feComposite in="done" in2="done" operator="arithmetic" k1="1" k2="0" k3="0" k4="0"/>
      </filter></defs>
      <rect width="20" height="20" fill="#ff0000" filter="url(#f)"/></svg>"##;
    let image = render(svg);
    let [r, g, b, a] = pixel_at(&image, 10, 10);
    assert_eq!((r, g, b), (0, 255, 0));
    assert_eq!(a, 255);
}

#[test]
fn composite_in_operator_keeps_source_inside_alpha() {
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <defs><filter id="f" x="0" y="0" width="1" height="1">
        <feFlood flood-color="#00ff00" result="g"/>
        <feComposite in="SourceGraphic" in2="g" operator="in"/>
      </filter></defs>
      <rect width="20" height="20" fill="#ff0000" filter="url(#f)"/></svg>"##;
    let image = render(svg);
    assert_eq!(pixel_at(&image, 10, 10), [255, 0, 0, 255]);
}

#[test]
fn smil_animate_from_to_at_times() {
    let bytes = include_bytes!("fixtures/smil.svg");
    let at_zero = render_svg(
        bytes,
        &SvgOptions {
            width: Some(60),
            height: Some(40),
        },
    )
    .expect("t0");
    let at_one = render_svg_with_time(
        bytes,
        &SvgOptions {
            width: Some(60),
            height: Some(40),
        },
        1.0,
    )
    .expect("t1");
    // Red square starts at x=0; at t=1 (half of 2s) it should be at x=20.
    assert_eq!(at_zero.pixel(2, 2).expect("red at 0")[0], 255);
    assert_eq!(at_one.pixel(2, 2).expect("left clear")[3], 0);
    assert_eq!(at_one.pixel(22, 2).expect("red at 20")[0], 255);
}

#[test]
fn smil_animate_transform_translate() {
    let bytes = include_bytes!("fixtures/smil.svg");
    let at_zero = render_svg(
        bytes,
        &SvgOptions {
            width: Some(60),
            height: Some(40),
        },
    )
    .expect("t0");
    let at_one = render_svg_with_time(
        bytes,
        &SvgOptions {
            width: Some(60),
            height: Some(40),
        },
        1.0,
    )
    .expect("t1");
    assert_eq!(at_zero.pixel(2, 22).expect("green at 0")[1], 255);
    assert_eq!(at_one.pixel(2, 22).expect("green moved")[3], 0);
    assert_eq!(at_one.pixel(12, 22).expect("green at 10")[1], 255);
}

#[test]
fn smil_set_and_by() {
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <rect x="0" y="0" width="20" height="20" fill="#ff0000">
        <set attributeName="fill" to="#0000ff" begin="0s"/>
      </rect></svg>"##;
    let image = render(svg);
    assert_eq!(pixel_at(&image, 10, 10), [0, 0, 255, 255]);
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <rect x="0" y="0" width="10" height="10" fill="#ff0000">
        <animate attributeName="x" by="10" dur="2s"/>
      </rect></svg>"##;
    let image = render_svg_with_time(
        svg,
        &SvgOptions {
            width: Some(20),
            height: Some(20),
        },
        1.0,
    )
    .expect("by");
    assert_eq!(image.pixel(12, 2).expect("moved by 5")[0], 255);
}

#[test]
fn smil_invalid_values_are_ignored() {
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <rect x="3" y="3" width="10" height="10" fill="#ff0000">
        <animate attributeName="x" from="oops" to="also-oops" dur="bogus"/>
      </rect></svg>"##;
    let image = render_svg(
        svg,
        &SvgOptions {
            width: Some(20),
            height: Some(20),
        },
    )
    .expect("render");
    assert_eq!(pixel_at(&image, 4, 4), [255, 0, 0, 255]);
}

#[test]
fn no_animation_is_byte_identical() {
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <rect x="2" y="2" width="16" height="16" fill="#336699"/></svg>"##;
    let timed = render_svg_with_time(
        svg,
        &SvgOptions {
            width: Some(40),
            height: Some(40),
        },
        0.5,
    )
    .expect("timed");
    let base = render_svg(
        svg,
        &SvgOptions {
            width: Some(40),
            height: Some(40),
        },
    )
    .expect("base");
    assert_eq!(base.data, timed.data);
}

#[test]
fn unknown_filter_inputs_are_transparent_black() {
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
      <defs><filter id="f" x="0" y="0" width="1" height="1">
        <feComposite in="bogus" in2="SourceGraphic" operator="in"/>
      </filter></defs>
      <rect width="20" height="20" fill="#ff0000" filter="url(#f)"/></svg>"##;
    let image = render(svg);
    let [_, _, _, alpha] = pixel_at(&image, 10, 10);
    assert_eq!(
        alpha, 0,
        "an unknown filter input must be transparent black"
    );
}
