//! EXIF orientation: the eight fixtures store physically transformed bitmaps
//! with their corresponding EXIF Orientation tags, so decoding must apply the
//! tag and yield the same upright 12x8 image for every orientation (#1365).

use image::{decode, exif_orientation};

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!("fixtures/jpeg/", $name))
    };
}

fn exif_fixture_bytes(orientation: u8) -> &'static [u8] {
    match orientation {
        1 => fixture!("exif_o1.jpg"),
        2 => fixture!("exif_o2.jpg"),
        3 => fixture!("exif_o3.jpg"),
        4 => fixture!("exif_o4.jpg"),
        5 => fixture!("exif_o5.jpg"),
        6 => fixture!("exif_o6.jpg"),
        7 => fixture!("exif_o7.jpg"),
        8 => fixture!("exif_o8.jpg"),
        _ => unreachable!(),
    }
}

fn corner(image: &image::RgbaImage, x: u32, y: u32) -> [u8; 4] {
    image
        .pixel(x, y)
        .expect("corner must be within decoded image")
}

#[test]
fn parses_orientation_tags_from_pil_fixtures() {
    for orientation in 1..=8u8 {
        assert_eq!(
            exif_orientation(exif_fixture_bytes(orientation)),
            Some(orientation),
            "tag {orientation}"
        );
    }
    assert_eq!(exif_orientation(fixture!("baseline_ref.jpg")), None);
}

#[test]
fn decodes_all_orientations_to_the_same_upright_pixels() {
    let reference = decode(fixture!("exif_o1.jpg")).expect("reference decodes");
    assert_eq!(
        (reference.width, reference.height),
        (12, 8),
        "reference must be upright 12x8"
    );
    for orientation in 1..=8u8 {
        let decoded = decode(exif_fixture_bytes(orientation)).expect("EXIF fixture decodes");
        assert_eq!(
            (decoded.width, decoded.height),
            (reference.width, reference.height),
            "orientation {orientation} must decode to upright dimensions"
        );
        let max_delta = decoded
            .data
            .iter()
            .zip(reference.data.iter())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        if max_delta > 8 {
            let tl = corner(&decoded, 0, 0);
            let tr = corner(&decoded, decoded.width - 1, 0);
            let bl = corner(&decoded, 0, decoded.height - 1);
            let br = corner(&decoded, decoded.width - 1, decoded.height - 1);
            let rtl = corner(&reference, 0, 0);
            let rtr = corner(&reference, reference.width - 1, 0);
            let rbl = corner(&reference, 0, reference.height - 1);
            let rbr = corner(&reference, reference.width - 1, reference.height - 1);
            panic!(
                "orientation {orientation}: max delta {max_delta} corners got TL={tl:?} TR={tr:?} BL={bl:?} BR={br:?} ref TL={rtl:?} TR={rtr:?} BL={rbl:?} BR={rbr:?}"
            );
        }
    }
}

#[test]
fn orientation_corners_match_literal_pixels() {
    let reference = decode(fixture!("exif_o1.jpg")).expect("reference decodes");
    let tl = corner(&reference, 0, 0);
    let tr = corner(&reference, reference.width - 1, 0);
    let bl = corner(&reference, 0, reference.height - 1);
    let br = corner(&reference, reference.width - 1, reference.height - 1);
    for orientation in 1..=8u8 {
        let img = decode(exif_fixture_bytes(orientation)).expect("EXIF fixture decodes");
        assert_eq!(
            (img.width, img.height),
            (reference.width, reference.height),
            "orientation {orientation}: dimensions"
        );
        let close = |got: [u8; 4], want: [u8; 4]| {
            got.iter()
                .zip(want.iter())
                .all(|(a, b)| a.abs_diff(*b) <= 8)
        };
        assert!(
            close(corner(&img, 0, 0), tl),
            "orientation {orientation}: TL got={:?} want={tl:?}",
            corner(&img, 0, 0)
        );
        assert!(
            close(corner(&img, img.width - 1, 0), tr),
            "orientation {orientation}: TR got={:?} want={tr:?}",
            corner(&img, img.width - 1, 0)
        );
        assert!(
            close(corner(&img, 0, img.height - 1), bl),
            "orientation {orientation}: BL got={:?} want={bl:?}",
            corner(&img, 0, img.height - 1)
        );
        assert!(
            close(corner(&img, img.width - 1, img.height - 1), br),
            "orientation {orientation}: BR got={:?} want={br:?}",
            corner(&img, img.width - 1, img.height - 1)
        );
    }
}

#[test]
fn single_component_progressive_jpeg_matches_baseline() {
    let progressive =
        decode(include_bytes!("fixtures/jpeg/gray_prog.jpg")).expect("gray progressive");
    let baseline = decode(include_bytes!("fixtures/jpeg/gray_base.jpg")).expect("gray baseline");
    assert_eq!(
        (progressive.width, progressive.height),
        (baseline.width, baseline.height)
    );
    let max_delta = progressive
        .data
        .iter()
        .zip(baseline.data.iter())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert_eq!(max_delta, 0, "gray progressive must match baseline exactly");
}

#[test]
fn multi_component_progressive_refinement_matches_baseline() {
    let progressive =
        decode(include_bytes!("fixtures/jpeg/color_progressive.jpg")).expect("colour progressive");
    let baseline =
        decode(include_bytes!("fixtures/jpeg/color_baseline.jpg")).expect("colour baseline");
    assert_eq!(
        (progressive.width, progressive.height),
        (baseline.width, baseline.height)
    );
    let max_delta = progressive
        .data
        .iter()
        .zip(baseline.data.iter())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert!(
        max_delta <= 2,
        "colour progressive vs baseline max delta {max_delta} (documented tolerance 2)"
    );
}
