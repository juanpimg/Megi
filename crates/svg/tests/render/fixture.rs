use super::*;

#[test]
fn test_ascii_art_fixture_renders_expected_colours() {
    let image = render_svg(
        include_bytes!("../fixtures/simple.svg"),
        &SvgOptions::default(),
    )
    .expect("fixture renders");
    assert_eq!((image.width, image.height), (16, 16));
    assert_eq!(pixel(&image, 1, 1), [0x20, 0x40, 0x80, 255]);
    assert_eq!(pixel(&image, 8, 8), [0xff, 0xcc, 0x00, 255]);
    assert_eq!(pixel(&image, 8, 0), [0x20, 0x40, 0x80, 255]);
}
