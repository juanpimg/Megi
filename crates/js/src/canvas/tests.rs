use super::*;

#[test]
fn parses_css_colors() {
    assert_eq!(
        Rgba::parse("#f00"),
        Some(Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        })
    );
    assert_eq!(
        Rgba::parse("#00ff00"),
        Some(Rgba {
            r: 0,
            g: 255,
            b: 0,
            a: 255
        })
    );
    assert_eq!(
        Rgba::parse("rgb(1, 2, 3)"),
        Some(Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 255
        })
    );
    assert_eq!(
        Rgba::parse("rgba(0,0,0,0.5)"),
        Some(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 128
        })
    );
    assert_eq!(Rgba::parse("not-a-color"), None);
    // #1772: byte indexing must not split a multi-byte UTF-8 character.
    assert_eq!(Rgba::parse("#éa"), None);
    assert_eq!(Rgba::parse("#fffé"), None);
}

#[test]
fn image_data_rejects_overflowing_coordinates() {
    // #1773: saturating i64 conversions from JS must not wrap past the guard.
    let surface = CanvasSurface::new(2, 2).expect("surface");
    assert_eq!(surface.get_image_data(i64::MAX, 0, 1, 1), None);
    assert_eq!(surface.get_image_data(0, i64::MAX, 1, 1), None);
    assert_eq!(surface.get_image_data(1, 1, u32::MAX, 1), None);
    let mut surface = CanvasSurface::new(2, 2).expect("surface");
    assert!(!surface.put_image_data(i64::MAX, 0, 1, 1, &[0; 4]));
    assert!(!surface.put_image_data(0, 0, u32::MAX, u32::MAX, &[0; 4]));
}

#[test]
fn rejects_oversized_surfaces() {
    assert!(CanvasSurface::new(0, 10).is_none());
    assert!(CanvasSurface::new(MAX_CANVAS_DIM + 1, 10).is_none());
    assert!(CanvasSurface::new(5000, 5000).is_none());
    assert!(CanvasSurface::new(300, 150).is_some());
}

#[test]
fn fill_clear_and_stroke_rect_manipulate_pixels() {
    let mut surface = CanvasSurface::new(4, 4).expect("surface");
    surface.fill_style = Rgba {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    surface.fill_rect(0.0, 0.0, 2.0, 2.0);
    assert_eq!(&surface.pixels[0..4], &[255, 0, 0, 255], "top-left filled");
    assert_eq!(
        &surface.pixels[(3 * 4 * 4)..(3 * 4 * 4 + 4)],
        &[0, 0, 0, 0],
        "outside untouched"
    );
    surface.clear_rect(0.0, 0.0, 1.0, 1.0);
    assert_eq!(&surface.pixels[0..4], &[0, 0, 0, 0], "cleared");
    surface.stroke_style = Rgba {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    };
    surface.stroke_rect(0.0, 0.0, 4.0, 4.0);
    assert_eq!(&surface.pixels[0..4], &[0, 0, 255, 255], "stroke top-left");
}

#[test]
fn put_image_data_replaces_including_transparent_pixels() {
    // #1848: WHATWG HTML §4.12.5.1.3 — putImageData writes raw pixels, it must not
    // source-over blend with the existing contents.
    let mut surface = CanvasSurface::new(2, 2).expect("surface");
    surface.fill_style = Rgba {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    surface.fill_rect(0.0, 0.0, 2.0, 2.0);
    assert_eq!(&surface.pixels[0..4], &[255, 0, 0, 255]);
    // Fully transparent red must replace, leaving the pixel transparent.
    assert!(surface.put_image_data(0, 0, 1, 1, &[0, 0, 0, 0]));
    assert_eq!(
        &surface.pixels[0..4],
        &[0, 0, 0, 0],
        "a fully transparent source must clear the target pixel"
    );
    // An opaque blue must replace, not blend with the red underneath.
    assert!(surface.put_image_data(1, 1, 1, 1, &[0, 0, 255, 255]));
    assert_eq!(&surface.pixels[12..16], &[0, 0, 255, 255]);
}

#[test]
fn surface_owns_paint_and_line_width_mutations() {
    // WHATWG HTML §4.12.5.1: fill/stroke style and line width are drawing-state
    // of the surface, so the surface — not its callers — owns their mutation.
    let mut surface = CanvasSurface::new(2, 2).expect("surface");
    let color = Rgba {
        r: 1,
        g: 2,
        b: 3,
        a: 255,
    };
    surface.set_paint(true, PaintStyle::Gradient(7));
    assert_eq!(surface.fill_gradient, Some(7));
    surface.set_paint(true, PaintStyle::Color(color));
    assert_eq!(surface.fill_style, color);
    assert_eq!(
        surface.fill_gradient, None,
        "a solid paint clears the gradient"
    );
    surface.set_paint(false, PaintStyle::Gradient(9));
    assert_eq!(surface.stroke_gradient, Some(9));
    assert_eq!(
        surface.fill_gradient, None,
        "stroke paint leaves fill untouched"
    );
    surface.set_line_width(2.5);
    assert_eq!(surface.line_width, 2.5);
}

#[test]
fn get_put_image_data_round_trips() {
    let mut surface = CanvasSurface::new(2, 2).expect("surface");
    surface.fill_style = Rgba {
        r: 10,
        g: 20,
        b: 30,
        a: 255,
    };
    surface.fill_rect(0.0, 0.0, 1.0, 1.0);
    let data = surface.get_image_data(0, 0, 1, 1).expect("data");
    assert_eq!(data, vec![10, 20, 30, 255]);
    let mut other = CanvasSurface::new(2, 2).expect("surface");
    assert!(other.put_image_data(1, 1, 1, 1, &data));
    assert_eq!(
        &other.pixels[12..16],
        &[10, 20, 30, 255],
        "bottom-right pixel"
    );
    assert!(
        !other.put_image_data(1, 1, 4, 4, &data),
        "out of bounds rejected"
    );
}
