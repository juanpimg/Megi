use super::*;

#[test]
fn test_determinism_between_runs() {
    let source = svg_doc(
        32.0,
        32.0,
        "<circle cx='16' cy='16' r='10' fill='#3366cc' stroke='black' stroke-width='2'/>\
         <path d='M4 28 C 8 4 24 4 28 28 Z' fill='rgba(255,0,0,0.5)'/>",
    );
    let first = render_at(&source, 32, 32);
    let second = render_at(&source, 32, 32);
    assert_eq!(first.data, second.data);
}

#[test]
fn test_into_rgba_image_and_pixel_accessors() {
    let image = render_at(
        "<svg width='2' height='2'><rect width='2' height='2' fill='#123456'/></svg>",
        2,
        2,
    );
    assert_eq!(pixel(&image, 1, 1), [0x12, 0x34, 0x56, 255]);
    assert!(image.pixel(2, 0).is_none());
    let rgba = image.into_rgba_image().expect("rgba image");
    assert_eq!((rgba.width, rgba.height), (2, 2));
    assert_eq!(rgba.data[0], 0x12);
}

#[test]
fn test_malformed_inputs_return_errors_without_panicking() {
    assert!(render_svg(b"", &SvgOptions::default()).is_err());
    assert!(render_svg(b"<svg><rect", &SvgOptions::default()).is_err());
    assert!(render_svg(b"<svg><rect width='1'/></svgx>", &SvgOptions::default()).is_err());
    assert!(render_svg(b"not an svg", &SvgOptions::default()).is_err());
    assert!(render_svg(b"<html/>", &SvgOptions::default()).is_err());
    assert!(render_svg(b"<svg><path d='M0'/></svg>", &SvgOptions::default()).is_err());
    assert!(render_svg(b"<svg><path d='M0 0 Q'/></svg>", &SvgOptions::default()).is_err());
    // Numeric edge cases are tolerated (ignored) rather than panicking.
    let nan = render_svg(
        b"<svg width='4' height='4'><rect width='4' height='4' fill='red' transform='translate(nan,1)'/></svg>",
        &SvgOptions::default(),
    );
    assert!(nan.is_ok());
    let infinite = render_svg(
        b"<svg width='4' height='4'><rect width='4' height='4' transform='translate(1e40,0)'/></svg>",
        &SvgOptions::default(),
    );
    assert!(infinite.is_ok());
}

#[test]
fn test_limits_are_enforced() {
    let narrow = Limits {
        max_dimension: 32,
        max_height: 32,
        ..Limits::default()
    };
    let source = b"<svg width='64' height='64'><rect width='64' height='64'/></svg>";
    assert!(render_svg_with_limits(source, &SvgOptions::default(), &narrow).is_err());
    let pixel_limit = Limits {
        max_pixels: 10,
        ..Limits::default()
    };
    assert!(render_svg_with_limits(source, &SvgOptions::default(), &pixel_limit).is_err());
    let small_input = Limits {
        max_input_bytes: 8,
        ..Limits::default()
    };
    assert!(render_svg_with_limits(source, &SvgOptions::default(), &small_input).is_err());
    let mut deep = String::from("<svg width='4' height='4'>");
    for _ in 0..20 {
        deep.push_str("<g>");
    }
    for _ in 0..20 {
        deep.push_str("</g>");
    }
    deep.push_str("</svg>");
    let depth_limit = Limits {
        max_depth: 4,
        ..Limits::default()
    };
    assert!(render_svg_with_limits(deep.as_bytes(), &SvgOptions::default(), &depth_limit).is_err());
}

#[test]
fn test_mini_fuzz_never_panics() {
    let base =
        br#"<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 16 16'>
        <rect x='1' y='1' width='10' height='8' fill='#ff0000' rx='2'/>
        <path d='M2 14 L8 2 L14 14 Z' fill='rgba(0,0,255,0.5)' stroke='black' stroke-width='1'/>
        </svg>"#;
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..200 {
        let mut mutated = base.to_vec();
        let mutations = 1 + (next() % 6) as usize;
        for _ in 0..mutations {
            let index = (next() as usize) % mutated.len();
            let byte = (next() % 256) as u8;
            mutated[index] = byte;
        }
        // Must never panic; any Result is acceptable.
        let _ = render_svg(&mutated, &SvgOptions::default());
        let _ = intrinsic_size(&mutated);
    }
}

#[test]
fn huge_stroke_widths_are_clamped_instead_of_panicking() {
    for shape in [
        "<rect x='4' y='4' width='12' height='12' fill='none' stroke='#123456' stroke-width='1e20'/>",
        "<circle cx='10' cy='10' r='5' fill='none' stroke='#123456' stroke-width='1e20'/>",
        "<path d='M2 2 L18 18' fill='none' stroke='#123456' stroke-width='1e20'/>",
    ] {
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='20' height='20' viewBox='0 0 20 20'>{shape}</svg>"
        );
        let image = render_svg(
            source.as_bytes(),
            &SvgOptions {
                width: Some(20),
                height: Some(20),
            },
        )
        .expect("a clamped stroke still renders");
        assert_eq!(image.width, 20);
    }
}

#[test]
fn far_off_canvas_text_does_not_panic() {
    for x in ["1e38", "-1e38"] {
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><text x='{x}' y='20'>A</text></svg>"
        );
        let image = render_svg(
            source.as_bytes(),
            &SvgOptions {
                width: Some(16),
                height: Some(16),
            },
        )
        .expect("far-off-canvas text still renders");
        assert_eq!(image.width, 16);
    }
}
