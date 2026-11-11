//! Round 29: `<textPath>` and SMIL values/keyTimes/keySplines + colour animation.

use svg::{SvgImage, SvgOptions, render_svg_with_time};

fn render(bytes: &[u8], width: u32, height: u32, time: f32) -> SvgImage {
    render_svg_with_time(
        bytes,
        &SvgOptions {
            width: Some(width),
            height: Some(height),
        },
        time,
    )
    .expect("render")
}

fn dark_count(image: &SvgImage, x0: u32, y0: u32, x1: u32, y1: u32) -> usize {
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some([r, g, b, a]) = image.pixel(x, y)
                && a > 128
                && r < 128
                && g < 128
                && b < 128
            {
                count += 1;
            }
        }
    }
    count
}

fn ink_bounds(image: &SvgImage) -> Option<(u32, u32, u32, u32)> {
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..image.height {
        for x in 0..image.width {
            if let Some([r, g, b, a]) = image.pixel(x, y)
                && a > 128
                && r < 128
                && g < 128
                && b < 128
            {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    (min_x != u32::MAX).then_some((min_x, min_y, max_x, max_y))
}

#[test]
fn text_path_straight_line_ink_near_path() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
      <defs><path id="line" d="M 10 50 L 190 50"/></defs>
      <text font-family="sans-serif" font-size="20" fill="#000">
        <textPath href="#line">ABC</textPath>
      </text></svg>"##;
    let image = render(svg, 200, 100, 0.0);
    assert!(
        dark_count(&image, 0, 30, 200, 70) > 20,
        "ink should follow the path"
    );
    assert_eq!(
        dark_count(&image, 0, 0, 200, 25),
        0,
        "no ink far above path"
    );
}

#[test]
fn text_path_start_offset_moves_ink() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
      <defs><path id="line" d="M 10 50 L 190 50"/></defs>
      <text font-family="sans-serif" font-size="20" fill="#000">
        <textPath href="#line" startOffset="120">ABC</textPath>
      </text></svg>"##;
    let image = render(svg, 200, 100, 0.0);
    assert_eq!(
        dark_count(&image, 0, 30, 110, 70),
        0,
        "startOffset should skip the left part"
    );
    assert!(dark_count(&image, 110, 30, 200, 70) > 10);
}

#[test]
fn text_path_anchor_middle_and_end_shift_left() {
    let make = |anchor: &str| {
        format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='100'>\
             <defs><path id='line' d='M 10 50 L 190 50'/></defs>\
             <text font-family='sans-serif' font-size='20' fill='#000' text-anchor='{anchor}'>\
             <textPath href='#line' startOffset='50%'>ABC</textPath></text></svg>"
        )
    };
    let start = render(make("start").as_bytes(), 200, 100, 0.0);
    let middle = render(make("middle").as_bytes(), 200, 100, 0.0);
    let end = render(make("end").as_bytes(), 200, 100, 0.0);
    let (sx, _, _, _) = ink_bounds(&start).expect("start ink");
    let (mx, _, _, _) = ink_bounds(&middle).expect("middle ink");
    let (ex, _, _, _) = ink_bounds(&end).expect("end ink");
    assert!(
        mx < sx && ex < mx,
        "anchor shifts: start {sx}, middle {mx}, end {ex}"
    );
}

#[test]
fn text_path_curved_path_ink_follows_curve() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120">
      <defs><path id="curve" d="M 20 80 Q 100 0 180 80"/></defs>
      <text font-family="sans-serif" font-size="16" fill="#000">
        <textPath href="#curve">CURVE</textPath>
      </text></svg>"##;
    let image = render(svg, 200, 120, 0.0);
    // Ink follows the quadratic arc's early section (path start at x=20).
    assert!(
        dark_count(&image, 15, 35, 90, 95) > 5,
        "bounds {:?}",
        ink_bounds(&image)
    );
    assert_eq!(dark_count(&image, 0, 100, 30, 120), 0);
}

#[test]
fn animate_values_linear_key_times() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="20">
      <rect width="10" height="10" fill="#f00">
        <animate attributeName="x" values="0;100" dur="2s"/>
      </rect></svg>"##;
    let at = |time: f32| render(svg, 120, 20, time);
    assert_eq!(at(0.0).pixel(2, 2).expect("x0")[0], 255);
    assert_eq!(at(0.5).pixel(2, 2).expect("x0 still")[3], 0);
    assert_eq!(at(0.5).pixel(27, 2).expect("x25")[0], 255);
    assert_eq!(at(1.0).pixel(52, 2).expect("x50")[0], 255);
}

#[test]
fn animate_values_key_times_three_values() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="20">
      <rect width="10" height="10" fill="#f00">
        <animate attributeName="x" values="0;10;20" keyTimes="0;0.5;1" dur="2s"/>
      </rect></svg>"##;
    let image = render(svg, 120, 20, 0.5); // progress 0.25 -> x=5
    assert_eq!(image.pixel(7, 2).expect("x5")[0], 255);
    assert_eq!(image.pixel(2, 2).expect("x0 clear")[3], 0);
}

#[test]
fn animate_calc_mode_discrete() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="20">
      <rect width="10" height="10" fill="#f00">
        <animate attributeName="x" values="0;100" keyTimes="0;0.5" calcMode="discrete" dur="2s"/>
      </rect></svg>"##;
    let early = render(svg, 120, 20, 0.8); // progress 0.4 -> value 0
    let late = render(svg, 120, 20, 1.2); // progress 0.6 -> value 100
    assert_eq!(early.pixel(2, 2).expect("x0")[0], 255);
    assert_eq!(late.pixel(2, 2).expect("x0 clear")[3], 0);
    assert_eq!(late.pixel(102, 2).expect("x100")[0], 255);
}

#[test]
fn animate_spline_easing() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="20">
      <rect width="10" height="10" fill="#f00">
        <animate attributeName="x" values="0;100" keyTimes="0;1" calcMode="spline"
                 keySplines="0.42 0 0.58 1" dur="2s"/>
      </rect></svg>"##;
    let mid = render(svg, 120, 20, 1.0); // symmetric ease -> ~50
    assert_eq!(mid.pixel(52, 2).expect("x50")[0], 255);
}

#[test]
fn animate_fill_colour_interpolates_srgb() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
      <rect width="20" height="20" fill="#ff0000">
        <animate attributeName="fill" from="#ff0000" to="#0000ff" dur="2s"/>
      </rect></svg>"##;
    let mid = render(svg, 20, 20, 1.0);
    let [r, g, b, a] = mid.pixel(10, 10).expect("pixel");
    assert_eq!(a, 255);
    assert!(
        (r as i32 - 128).abs() <= 2 && (b as i32 - 128).abs() <= 2,
        "mid colour {r},{g},{b}"
    );
}

#[test]
fn animate_opacity_interpolates() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
      <rect width="20" height="20" fill="#ff0000">
        <animate attributeName="opacity" from="0" to="1" dur="2s"/>
      </rect></svg>"##;
    let mid = render(svg, 20, 20, 1.0);
    let [r, g, b, a] = mid.pixel(10, 10).expect("pixel");
    assert!(
        (120..=135).contains(&a),
        "half opacity alpha {a} (rgb {r},{g},{b})"
    );
}

#[test]
fn animate_stroke_colour_interpolates() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
      <rect x="2" y="2" width="16" height="16" fill="none" stroke="#ff0000" stroke-width="4">
        <animate attributeName="stroke" from="#ff0000" to="#0000ff" dur="2s"/>
      </rect></svg>"##;
    let mid = render(svg, 20, 20, 1.0);
    let [r, _, b, _] = mid.pixel(2, 10).expect("stroke pixel");
    assert!(
        (r as i32 - 128).abs() <= 4 && (b as i32 - 128).abs() <= 4,
        "stroke colour {r},{b}"
    );
}

#[test]
fn invalid_values_are_ignored_and_render_unchanged() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
      <rect x="3" y="3" width="10" height="10" fill="#f00">
        <animate attributeName="x" values="oops;bad" keyTimes="0;1" dur="2s"/>
      </rect></svg>"##;
    let zero = render(svg, 20, 20, 0.0);
    let half = render(svg, 20, 20, 1.0);
    assert_eq!(zero.data, half.data);
}
