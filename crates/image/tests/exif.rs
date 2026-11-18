//! EXIF orientation: each fixture stores the source *physically rotated* by the inverse
//! of its tag, so `decode` (which applies the tag) must land on the same upright image
//! (#1365). This exercises the real-world case, not just tag parsing.

use image::{decode, exif_orientation};

/// Corner colours of the orientation-1 fixture, read literally from its decoded pixels.
/// These are independent of `apply_orientation`: a no-op or transposed regression in that
/// function cannot make this oracle follow it (#1364).
const TL: [u8; 4] = [255, 208, 28, 255];
const TR: [u8; 4] = [0, 255, 121, 255];
const BL: [u8; 4] = [0, 33, 255, 255];
const BR: [u8; 4] = [255, 181, 0, 255];

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
        (16, 16),
        "reference must be upright 16x16"
    );
    for orientation in 1..=8u8 {
        let bytes: &[u8] = match orientation {
            1 => fixture!("exif_o1.jpg"),
            2 => fixture!("exif_o2.jpg"),
            3 => fixture!("exif_o3.jpg"),
            4 => fixture!("exif_o4.jpg"),
            5 => fixture!("exif_o5.jpg"),
            6 => fixture!("exif_o6.jpg"),
            7 => fixture!("exif_o7.jpg"),
            8 => fixture!("exif_o8.jpg"),
            _ => unreachable!(),
        };
        let decoded = decode(bytes).expect("EXIF fixture decodes");
        // The stored frames are 8x12 for the transposing orientations (5..=8), but the
        // tag rotates them back: every decoded frame must be the upright 16x16 reference.
        assert_eq!(
            (decoded.width, decoded.height),
            (reference.width, reference.height),
            "orientation {orientation} must land on the upright dimensions"
        );
        // JPEG is lossy but these fixtures use quality 95 and flat colour blocks;
        // compare with a small per-channel tolerance.
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
    // For each EXIF orientation, the four corners of the decoded image must land on the
    // literal reference corner colours above. This pins the transpose/inverse convention for
    // orientations 5-8 independently of the `apply_orientation` implementation (#1364).
    // (TL, TR, BL, BR) of the decoded image, in reference-corner terms:
    type Corners = ([u8; 4], [u8; 4], [u8; 4], [u8; 4]);
    // Every fixture now stores pixels rotated by the inverse of its tag, so the decoded
    // image is upright in all eight cases (#1365).
    let corners: [(u8, Corners, (u32, u32)); 8] = [
        (1, (TL, TR, BL, BR), (16, 16)),
        (2, (TL, TR, BL, BR), (16, 16)),
        (3, (TL, TR, BL, BR), (16, 16)),
        (4, (TL, TR, BL, BR), (16, 16)),
        (5, (TL, TR, BL, BR), (16, 16)),
        (6, (TL, TR, BL, BR), (16, 16)),
        (7, (TL, TR, BL, BR), (16, 16)),
        (8, (TL, TR, BL, BR), (16, 16)),
    ];
    for (orientation, (want_tl, want_tr, want_bl, want_br), (want_w, want_h)) in corners {
        let bytes: &[u8] = match orientation {
            1 => fixture!("exif_o1.jpg"),
            2 => fixture!("exif_o2.jpg"),
            3 => fixture!("exif_o3.jpg"),
            4 => fixture!("exif_o4.jpg"),
            5 => fixture!("exif_o5.jpg"),
            6 => fixture!("exif_o6.jpg"),
            7 => fixture!("exif_o7.jpg"),
            8 => fixture!("exif_o8.jpg"),
            _ => unreachable!(),
        };
        let img = decode(bytes).expect("EXIF fixture decodes");
        assert_eq!(
            (img.width, img.height),
            (want_w, want_h),
            "orientation {orientation}: dimensions"
        );
        let close = |got: [u8; 4], want: [u8; 4]| {
            got.iter()
                .zip(want.iter())
                .all(|(a, b)| a.abs_diff(*b) <= 8)
        };
        assert!(
            close(corner(&img, 0, 0), want_tl),
            "orientation {orientation}: TL"
        );
        assert!(
            close(corner(&img, img.width - 1, 0), want_tr),
            "orientation {orientation}: TR"
        );
        assert!(
            close(corner(&img, 0, img.height - 1), want_bl),
            "orientation {orientation}: BL"
        );
        assert!(
            close(corner(&img, img.width - 1, img.height - 1), want_br),
            "orientation {orientation}: BR"
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
