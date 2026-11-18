use super::*;

#[test]
fn test_rect_fill_exact_pixels_and_transparent_background() {
    let image = render(
        &svg_doc(
            10.0,
            10.0,
            "<rect x='2' y='2' width='4' height='4' fill='#ff0000'/>",
        ),
        SvgOptions::default(),
    );
    assert_eq!((image.width, image.height), (10, 10));
    assert_eq!(pixel(&image, 3, 3), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 6, 6), [0, 0, 0, 0]);
    assert_eq!(pixel(&image, 0, 0), [0, 0, 0, 0]);
    assert_eq!(pixel(&image, 9, 9), [0, 0, 0, 0]);
    // Whole image outside the shape stays transparent.
    let transparent = image
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|chunk| **chunk == [0, 0, 0, 0])
        .count();
    assert_eq!(transparent, 10 * 10 - 16);
}

#[test]
fn test_empty_svg_is_fully_transparent() {
    let image = render(&svg_doc(4.0, 4.0, ""), SvgOptions::default());
    assert!(image.data.iter().all(|byte| *byte == 0));
}

#[test]
fn test_circle_and_ellipse_centres_and_antialiased_edges() {
    let image = render_at(
        &svg_doc(
            20.0,
            20.0,
            "<circle cx='10' cy='10' r='6' fill='#00ff00'/>\
             <ellipse cx='4' cy='4' rx='3' ry='1.5' fill='#0000ff'/>",
        ),
        20,
        20,
    );
    assert_eq!(pixel(&image, 10, 10)[1], 255);
    assert_eq!(pixel(&image, 10, 10)[3], 255);
    assert_eq!(pixel(&image, 19, 19), [0, 0, 0, 0]);
    // The leftmost point of the circle is at x = 4; its interior pixel is covered.
    let edge = pixel(&image, 4, 10);
    assert!(edge[1] > 0, "edge pixel {edge:?}");
    assert_eq!(pixel(&image, 4, 4)[2], 255, "ellipse centre must be blue");
}

#[test]
fn test_polygon_and_polyline() {
    let image = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<polygon points='1,9 9,9 5,1' fill='#ff00ff'/>\
             <polyline points='0,0 10,0' fill='none' stroke='black' stroke-width='2'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&image, 5, 8)[0], 255);
    assert_eq!(pixel(&image, 0, 5), [0, 0, 0, 0]);
    let line = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<polyline points='0,1 10,1' fill='none' stroke='black' stroke-width='2'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&line, 5, 1)[3], 255);
}
