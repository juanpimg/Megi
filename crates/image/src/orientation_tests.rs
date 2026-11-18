use super::{RgbaImage, apply_orientation};

/// 4x2 image whose red channel encodes the source index, so every transform
/// can be checked against its exact destination pixel.
fn indexed_image() -> RgbaImage {
    let mut data = Vec::new();
    for y in 0..2u8 {
        for x in 0..4u8 {
            let index = y * 4 + x;
            data.extend_from_slice(&[index, index, index, 255]);
        }
    }
    RgbaImage::new(4, 2, data).expect("valid image")
}

fn reds(image: &RgbaImage) -> Vec<u8> {
    image.data.chunks(4).map(|pixel| pixel[0]).collect()
}

#[test]
fn applies_all_eight_orientations() {
    // Source indices: [0 1 2 3 / 4 5 6 7]
    let cases: &[(u8, u32, u32, &[u8])] = &[
        (1, 4, 2, &[0, 1, 2, 3, 4, 5, 6, 7]),
        (2, 4, 2, &[3, 2, 1, 0, 7, 6, 5, 4]),
        (3, 4, 2, &[7, 6, 5, 4, 3, 2, 1, 0]),
        (4, 4, 2, &[4, 5, 6, 7, 0, 1, 2, 3]),
        (5, 2, 4, &[0, 4, 1, 5, 2, 6, 3, 7]),
        (6, 2, 4, &[4, 0, 5, 1, 6, 2, 7, 3]),
        (7, 2, 4, &[7, 3, 6, 2, 5, 1, 4, 0]),
        (8, 2, 4, &[3, 7, 2, 6, 1, 5, 0, 4]),
    ];
    for &(orientation, width, height, expected) in cases {
        let image = apply_orientation(indexed_image(), Some(orientation)).expect("transform");
        assert_eq!(
            (image.width, image.height),
            (width, height),
            "orientation {orientation} size"
        );
        assert_eq!(reds(&image), expected, "orientation {orientation}");
    }
    let identity = apply_orientation(indexed_image(), None).expect("identity");
    assert_eq!(reds(&identity), [0, 1, 2, 3, 4, 5, 6, 7]);
}
