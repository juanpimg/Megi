//! Round 33: textPath auto-rotation/textLength/side and <use> presentation inheritance.

use svg::{SvgImage, SvgOptions, render_svg};

fn render(svg: &[u8]) -> SvgImage {
    render_svg(
        svg,
        &SvgOptions {
            width: Some(200),
            height: Some(120),
        },
    )
    .expect("render")
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

fn pixel(image: &SvgImage, x: u32, y: u32) -> [u8; 4] {
    image.pixel(x, y).expect("pixel")
}

#[test]
fn text_path_auto_rotation_follows_curve() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120">
      <defs><path id="arc" d="M 20 100 Q 100 10 180 100"/></defs>
      <text font-family="sans-serif" font-size="18" fill="#000">
        <textPath href="#arc">ROTATE</textPath></text></svg>"##;
    let image = render(svg);
    let (_, min_y, max_x, max_y) = ink_bounds(&image).expect("ink");
    assert!(max_y - min_y > 8, "rotated glyphs should span vertically");
    assert!(max_x > 60, "text should advance along the curve");
}

#[test]
fn text_path_descending_segment_flips_upright() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120">
      <defs><path id="down" d="M 100 10 L 100 110"/></defs>
      <text font-family="sans-serif" font-size="16" fill="#000">
        <textPath href="#down">FLIP</textPath></text></svg>"##;
    let image = render(svg);
    let (min_x, min_y, max_x, max_y) = ink_bounds(&image).expect("ink");
    assert!(max_y > min_y + 20, "text should descend along the path");
    assert!(
        max_x - min_x > 4,
        "flipped glyphs remain readable (width > 0)"
    );
}

#[test]
fn text_path_text_length_spacing() {
    let make = |text_length: u32| {
        format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='120'>\
         <defs><path id='line' d='M 10 60 L 190 60'/></defs>\
         <text font-family='sans-serif' font-size='20' fill='#000' textLength='{text_length}'>\
         <textPath href='#line'>ABC</textPath></text></svg>"
        )
    };
    let short = render(make(60).as_bytes());
    let long = render(make(160).as_bytes());
    let short_bounds = ink_bounds(&short).expect("short ink");
    let long_bounds = ink_bounds(&long).expect("long ink");
    assert!(
        long_bounds.2 > short_bounds.2,
        "textLength should spread the text: {:?} vs {:?}",
        short_bounds,
        long_bounds
    );
}

#[test]
fn text_path_text_length_spacing_and_glyphs_scales() {
    let make = |adjust: &str| {
        format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='120'>\
         <defs><path id='line' d='M 10 60 L 190 60'/></defs>\
         <text font-family='sans-serif' font-size='20' fill='#000' textLength='10' lengthAdjust='{adjust}'>\
         <textPath href='#line'>ABC</textPath></text></svg>"
        )
    };
    let spacing = render(make("spacing").as_bytes());
    let glyphs = render(make("spacingAndGlyphs").as_bytes());
    let count = |image: &SvgImage| {
        let mut total = 0;
        for y in 0..image.height {
            for x in 0..image.width {
                if let Some([r, g, b, a]) = image.pixel(x, y)
                    && a > 128
                    && r < 128
                    && g < 128
                    && b < 128
                {
                    total += 1;
                }
            }
        }
        total
    };
    assert!(
        count(&glyphs) < count(&spacing),
        "spacingAndGlyphs should scale glyphs down: spacing pixels {} vs glyphs pixels {}",
        count(&spacing),
        count(&glyphs)
    );
}

#[test]
fn text_path_side_right_offsets_baseline() {
    let make = |side: &str| {
        format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='120'>\
         <defs><path id='line' d='M 10 60 L 190 60'/></defs>\
         <text font-family='sans-serif' font-size='20' fill='#000'>\
         <textPath href='#line' side='{side}'>ABC</textPath></text></svg>"
        )
    };
    let left = render(make("left").as_bytes());
    let right = render(make("right").as_bytes());
    let (_, ly0, _, _) = ink_bounds(&left).expect("left ink");
    let (_, ry0, _, _) = ink_bounds(&right).expect("right ink");
    assert!(
        ry0 < ly0,
        "side=right should shift the baseline to the other side: left y {} right y {}",
        ly0,
        ry0
    );
}

#[test]
fn use_inherits_presentation_and_translates() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="60">
      <defs><rect id="r" width="10" height="10"/></defs>
      <use href="#r" x="20" y="30" fill="#ff0000"/>
      <use href="#r" x="40" y="30" fill="#00ff00" opacity="0.5"/></svg>"##;
    let image = render(svg);
    assert_eq!(
        pixel(&image, 24, 34),
        [255, 0, 0, 255],
        "use fill + x/y translation"
    );
    let [r, g, _, a] = pixel(&image, 44, 34);
    assert!(r < 10 && g > 245, "green use style, got {r},{g}");
    assert!(
        (120..=135).contains(&a),
        "use opacity should be 0.5, got {a}"
    );
}

#[test]
fn use_without_styles_uses_target_presentation() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40">
      <defs><rect id="r" width="10" height="10" fill="#0000ff"/></defs>
      <use href="#r" x="10" y="10"/></svg>"##;
    let image = render(svg);
    assert_eq!(pixel(&image, 14, 14), [0, 0, 255, 255]);
}

#[test]
fn use_x_y_translation_is_relative_to_use_transform() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="60">
      <defs><rect id="r" width="8" height="8" fill="#ff00ff"/></defs>
      <g transform="translate(5 5)"><use href="#r" x="10" y="10"/></g></svg>"##;
    let image = render(svg);
    assert_eq!(pixel(&image, 18, 18), [255, 0, 255, 255]);
}
