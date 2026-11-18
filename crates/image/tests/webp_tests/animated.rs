use super::*;

#[test]
fn trailing_bytes_and_wrong_riff_size_are_rejected() {
    let original = fixture("palette_transform.webp");
    decode(&original).expect("the fixture decodes");
    let mut padded = original.clone();
    padded.extend_from_slice(b"trailing-bytes");
    match decode(&padded) {
        Err(ImageError::InvalidData { reason }) => {
            assert!(reason.contains("RIFF size"), "{reason}");
        }
        other => panic!("expected trailing-bytes rejection, got {other:?}"),
    }
    let mut wrong_size = original.clone();
    let size = u32::from_le_bytes(wrong_size[4..8].try_into().unwrap());
    wrong_size[4..8].copy_from_slice(&(size + 1).to_le_bytes());
    match decode(&wrong_size) {
        Err(ImageError::InvalidData { reason }) => {
            assert!(reason.contains("RIFF size"), "{reason}");
        }
        other => panic!("expected RIFF-size rejection, got {other:?}"),
    }
}

#[test]
fn decodes_animated_webp_frames_blending_and_disposal() {
    let solid_vp8l = extract_vp8l_chunk(&fixture("v8l_two_color.webp"));
    let alpha_vp8l = extract_vp8l_chunk(&fixture("v8l_alpha.webp"));

    // Frame 0: 8x8 opaque two-color, 100ms, keep canvas (flags=0).
    // Frame 1: 8x8 half-transparent alpha-blended over frame 0, 250ms, dispose to bg (flags=1).
    let webp = build_animated_webp(
        8,
        8,
        [0, 255, 0, 255], // BGRA green
        3,
        &[
            (0, 0, 8, 8, 100, 0x00, &solid_vp8l),
            (0, 0, 8, 8, 250, 0x01, &alpha_vp8l),
        ],
    );

    // Static decode() returns frame 0.
    let first = decode(&webp).expect("animated webp static decode");
    assert_eq!(first.dimensions(), (8, 8));
    assert_eq!(first.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(first.pixel(7, 7), Some([0, 0, 255, 255]));

    // Full animated decode returns both composited frames with metadata.
    let anim = megi_image::decode_animated(&webp).expect("animated webp frames");
    assert_eq!((anim.width, anim.height, anim.loop_count), (8, 8, 3));
    assert_eq!(anim.frames.len(), 2);
    assert_eq!(anim.frames[0].delay_ms, 100);
    assert_eq!(anim.frames[1].delay_ms, 250);
    // Transparent half of v8l_alpha preserves frame 0's blue pixel at (7, 7).
    assert_eq!(anim.frames[1].image.pixel(7, 7), Some([0, 0, 255, 255]));
}

#[test]
fn rejects_out_of_bounds_anmf_frame_and_missing_anim() {
    let solid_vp8l = extract_vp8l_chunk(&fixture("v8l_two_color.webp"));
    // Frame 8x8 at x=4 on 8x8 canvas overflows.
    let oob = build_animated_webp(
        8,
        8,
        [0, 0, 0, 0],
        0,
        &[(4, 0, 8, 8, 50, 0x00, &solid_vp8l)],
    );
    assert!(matches!(
        megi_image::decode_animated(&oob),
        Err(ImageError::InvalidData { .. })
    ));

    // VP8X with animation flag but no ANIM chunk.
    let mut body = Vec::from(*b"WEBP");
    let mut vp8x = [0u8; 10];
    vp8x[0] = 0x02;
    vp8x[4..7].copy_from_slice(&encode_u24_le(7));
    vp8x[7..10].copy_from_slice(&encode_u24_le(7));
    push_riff_chunk(&mut body, b"VP8X", &vp8x);
    let mut no_anim = Vec::from(*b"RIFF");
    no_anim.extend_from_slice(&(body.len() as u32).to_le_bytes());
    no_anim.extend_from_slice(&body);
    assert!(matches!(
        megi_image::decode_animated(&no_anim),
        Err(ImageError::InvalidData { .. })
    ));
}

#[test]
fn decodes_real_animated_webp_fixture_with_compositing() {
    // Real VP8X/ANIM/ANMF container (not synthesized in memory) exercising frame
    // compositing, alpha blending and disposal (#1436). Oracles are per-frame
    // composited RGBA stored by crates/image/tests/fixtures/generate_webp.py.
    let webp = fixture("animated_blend.webp");
    let first = decode(&webp).expect("animated fixture static decode");
    assert_eq!(first.dimensions(), (8, 8));
    assert_eq!(first.data, fixture("animated_blend.frame0.rgba"));

    let anim = megi_image::decode_animated(&webp).expect("animated fixture frames");
    assert_eq!((anim.width, anim.height, anim.loop_count), (8, 8, 3));
    assert_eq!(anim.frames.len(), 2);
    assert_eq!(anim.frames[0].delay_ms, 100);
    assert_eq!(anim.frames[1].delay_ms, 250);
    assert_eq!(
        anim.frames[0].image.data,
        fixture("animated_blend.frame0.rgba")
    );
    assert_eq!(
        anim.frames[1].image.data,
        fixture("animated_blend.frame1.rgba")
    );
    // Frame 1 blends a transparent-right-half alpha frame over frame 0 (dispose none).
    assert_eq!(anim.frames[1].image.pixel(7, 7), Some([0, 0, 255, 255]));
}

#[test]
fn decodes_vp8x_uncompressed_alph_lossy_fixture_exactly() {
    // Static VP8X + VP8 lossy with an uncompressed ALPH plane (method 0, filter 0)
    // committed as a real fixture (#1436).
    let image = decode(&fixture("vp8x_alpha_lossy.webp")).expect("VP8X+ALPH fixture");
    assert_eq!(image.dimensions(), (16, 16));
    assert_eq!(image.data, fixture("vp8x_alpha_lossy.rgba"));
    assert_eq!(image.pixel(0, 0).map(|pixel| pixel[3]), Some(0));
    assert_eq!(image.pixel(15, 0).map(|pixel| pixel[3]), Some(255));
}

#[test]
fn alph_filtering_methods_2_and_3_decode() {
    // #1660: WebP ALPH filtering methods 2 (vertical) and 3 (gradient) are valid.
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/audit/findings/wave2/repro-w2-assets");
    for fixture in ["alph-f1.webp", "alph-f2.webp", "alph-f3.webp"] {
        let path = base.join(fixture);
        if !path.exists() {
            continue;
        }
        let bytes = std::fs::read(&path).expect("read fixture");
        let decoded = megi_image::decode(&bytes)
            .unwrap_or_else(|error| panic!("{fixture} must decode: {error}"));
        assert_eq!((decoded.width, decoded.height), (16, 16), "{fixture} size");
    }
}
