use super::*;

#[test]
fn test_opacity_and_antialiasing_intermediate_values() {
    let image = render_at(
        &svg_doc(
            8.0,
            8.0,
            "<rect width='8' height='8' fill='red' fill-opacity='0.5'/>",
        ),
        8,
        8,
    );
    let center = pixel(&image, 4, 4);
    assert_eq!(center[0], 255);
    assert_near(center[3], 128, 1);
    // A diagonal edge yields partially covered pixels.
    let diagonal = render_at(
        &svg_doc(8.0, 8.0, "<polygon points='0,0 8,0 0,8' fill='black'/>"),
        8,
        8,
    );
    let partial = (0..8)
        .flat_map(|y| (0..8).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let alpha = pixel(&diagonal, x, y)[3];
            alpha > 0 && alpha < 255
        })
        .count();
    assert!(partial > 0, "expected antialiased pixels on the diagonal");
}

#[test]
fn test_element_and_group_opacity_multiply() {
    let image = render_at(
        &svg_doc(
            8.0,
            8.0,
            "<g opacity='0.5'><rect width='8' height='8' fill='black' opacity='0.5'/></g>",
        ),
        8,
        8,
    );
    assert_near(pixel(&image, 4, 4)[3], 64, 2);
}
