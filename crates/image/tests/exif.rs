//! EXIF orientation: the eight PIL-generated fixtures decode with the applied
//! orientation (dimensions and pixels match the orientation-1 reference).

use image::{apply_orientation, decode, exif_orientation};

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
