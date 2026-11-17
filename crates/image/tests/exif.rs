//! EXIF orientation: the eight PIL-generated fixtures decode with the applied
//! orientation (dimensions and pixels match the orientation-1 reference).

use image::{apply_orientation, decode, exif_orientation};

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

#[test]
fn parses_orientation_tags_from_pil_fixtures() {
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
        assert_eq!(
            exif_orientation(bytes),
            Some(orientation),
            "tag {orientation}"
        );
    }
    assert_eq!(exif_orientation(fixture!("baseline_ref.jpg")), None);
}

#[test]
fn decodes_all_orientations_to_the_same_upright_pixels() {
    let reference = decode(fixture!("exif_o1.jpg")).expect("reference decodes");
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
        let expected = apply_orientation(
            image::RgbaImage::new(reference.width, reference.height, reference.data.clone())
                .expect("clone reference"),
            Some(orientation),
        )
        .expect("reference transform");
        assert_eq!(
            (decoded.width, decoded.height),
            (expected.width, expected.height),
            "orientation {orientation}"
        );
        // JPEG is lossy but these fixtures use quality 95 and flat colour blocks;
        // compare with a small per-channel tolerance.
        let max_delta = decoded
            .data
            .iter()
            .zip(expected.data.iter())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(
            max_delta <= 8,
            "orientation {orientation}: max delta {max_delta}"
        );
    }
}

#[test]
fn orientation_corners_match_literal_pixels() {
    // For each EXIF orientation, the four corners of the decoded image must land on the
    // literal reference corner colours above. This pins the transpose/inverse convention for
    // orientations 5-8 independently of the `apply_orientation` implementation (#1364).
    // (TL, TR, BL, BR) of the decoded image, in reference-corner terms:
    type Corners = ([u8; 4], [u8; 4], [u8; 4], [u8; 4]);
    let corners: [(u8, Corners, (u32, u32)); 8] = [
        (1, (TL, TR, BL, BR), (12, 8)), // identity
        (2, (TR, TL, BR, BL), (12, 8)), // mirror horizontal
        (3, (BR, BL, TR, TL), (12, 8)), // rotate 180
        (4, (BL, BR, TL, TR), (12, 8)), // mirror vertical
        (5, (TL, BL, TR, BR), (8, 12)), // transpose
        (6, (BL, TL, BR, TR), (8, 12)), // rotate 90 CW
        (7, (BR, TR, BL, TL), (8, 12)), // transverse
        (8, (TR, BR, TL, BL), (8, 12)), // rotate 270 CW
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
        let px = |x: u32, y: u32| -> [u8; 4] {
            let o = ((y as usize) * (img.width as usize) + (x as usize)) * 4;
            [
                img.data[o],
                img.data[o + 1],
                img.data[o + 2],
                img.data[o + 3],
            ]
        };
        // JPEG is lossy (quality 95), so allow a small per-channel tolerance.
        let close = |got: [u8; 4], want: [u8; 4]| {
            got.iter()
                .zip(want.iter())
                .all(|(a, b)| a.abs_diff(*b) <= 8)
        };
        assert!(close(px(0, 0), want_tl), "orientation {orientation}: TL");
        assert!(
            close(px(img.width - 1, 0), want_tr),
            "orientation {orientation}: TR"
        );
        assert!(
            close(px(0, img.height - 1), want_bl),
            "orientation {orientation}: BL"
        );
        assert!(
            close(px(img.width - 1, img.height - 1), want_br),
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
    // Full colour progressive: DC/AC first scans plus successive-approximation refinement
    // scans over the three components. JPEG quantizes once, so a correct decoder
    // reconstructs the same coefficients as the sequential baseline; the documented
    // tolerance covers the IDCT rounding/subsampling order between the two encodings.
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
