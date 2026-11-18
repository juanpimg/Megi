use super::*;

#[test]
fn test_viewbox_scaling_and_default_intrinsic_size() {
    let image = render_at(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'>\
         <rect width='10' height='10' fill='red'/></svg>",
        20,
        20,
    );
    assert_eq!((image.width, image.height), (20, 20));
    assert_eq!(pixel(&image, 19, 19), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 0, 0), [255, 0, 0, 255]);
    assert_eq!(
        intrinsic_size("<svg viewBox='0 0 10 10'/>".as_bytes()),
        Ok((10.0, 10.0))
    );
    assert_eq!(
        intrinsic_size("<svg width='40' height='20'/>".as_bytes()),
        Ok((40.0, 20.0))
    );
    assert_eq!(intrinsic_size("<svg/>".as_bytes()), Ok((300.0, 150.0)));
}

#[test]
fn test_preserve_aspect_ratio_meet_and_none() {
    // meet (default): 10×10 viewBox inside 20×10 viewport stays square and centred.
    let meet = render_at(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'>\
         <rect width='10' height='10' fill='black'/></svg>",
        20,
        10,
    );
    assert_eq!(pixel(&meet, 5, 5), [0, 0, 0, 255]);
    assert_eq!(pixel(&meet, 1, 5), [0, 0, 0, 0]);
    assert_eq!(pixel(&meet, 19, 5), [0, 0, 0, 0]);
    // none: stretches to the full viewport.
    let stretch = render_at(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' preserveAspectRatio='none'>\
         <rect width='10' height='10' fill='black'/></svg>",
        20,
        10,
    );
    assert_eq!(pixel(&stretch, 1, 5), [0, 0, 0, 255]);
    assert_eq!(pixel(&stretch, 19, 5), [0, 0, 0, 255]);
}

#[test]
fn test_transforms_translate_scale_rotate() {
    let translated = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<rect x='0' y='0' width='2' height='2' transform='translate(4,4) scale(2)' fill='black'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&translated, 5, 5), [0, 0, 0, 255]);
    assert_eq!(pixel(&translated, 1, 1), [0, 0, 0, 0]);
    let rotated = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<rect x='0' y='4' width='10' height='2' transform='rotate(90 5 5)' fill='black'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&rotated, 5, 5), [0, 0, 0, 255]);
    assert_eq!(pixel(&rotated, 1, 1), [0, 0, 0, 0]);
}
