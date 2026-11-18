use super::*;

#[test]
fn vp8x_animated_without_anmf_is_rejected() {
    // An animated VP8X container with ANIM but no ANMF frames is invalid.
    let plain = fixture("photo_lossy_q75.webp");
    let vp8 = vp8_payload(&plain);
    let mut vp8x = [0u8; 10];
    // flags: bit 1 = animation
    vp8x[0] = 0x02;
    let chunks: Vec<(&[u8; 4], &[u8])> =
        vec![(b"VP8X", &vp8x), (b"ANIM", &[0u8; 6]), (b"VP8 ", vp8)];
    let bytes = riff_container(&chunks);
    let error = decode(&bytes).expect_err("animated VP8X without ANMF must be rejected");
    assert!(matches!(error, ImageError::InvalidData { .. }), "{error:?}");
}

#[test]
fn vp8x_static_container_decodes() {
    // A static VP8X (no animation) with a matching canvas decodes the inner VP8 frame.
    let plain = fixture("photo_lossy_q75.webp");
    let vp8 = vp8_payload(&plain);
    let original = decode(&plain).expect("plain decode");
    let mut vp8x = [0u8; 10];
    let w = original.width - 1;
    let h = original.height - 1;
    vp8x[4] = (w & 0xff) as u8;
    vp8x[5] = ((w >> 8) & 0xff) as u8;
    vp8x[6] = ((w >> 16) & 0xff) as u8;
    vp8x[7] = (h & 0xff) as u8;
    vp8x[8] = ((h >> 8) & 0xff) as u8;
    vp8x[9] = ((h >> 16) & 0xff) as u8;
    let chunks: Vec<(&[u8; 4], &[u8])> = vec![(b"VP8X", &vp8x), (b"VP8 ", vp8)];
    let bytes = riff_container(&chunks);
    let decoded = decode(&bytes).expect("static VP8X decodes");
    assert_eq!(decoded.width, original.width);
    assert_eq!(decoded.height, original.height);
}

#[test]
fn vp8x_alpha_plane_is_applied() {
    // An uncompressed ALPH chunk replaces the alpha channel of the decoded frame.
    let plain = fixture("photo_lossy_q75.webp");
    let vp8 = vp8_payload(&plain);
    let original = decode(&plain).expect("plain decode");
    let mut vp8x = [0u8; 10];
    let w = original.width - 1;
    let h = original.height - 1;
    vp8x[0] = 0x10; // alpha flag
    vp8x[4] = (w & 0xff) as u8;
    vp8x[5] = ((w >> 8) & 0xff) as u8;
    vp8x[6] = ((w >> 16) & 0xff) as u8;
    vp8x[7] = (h & 0xff) as u8;
    vp8x[8] = ((h >> 8) & 0xff) as u8;
    vp8x[9] = ((h >> 16) & 0xff) as u8;
    let alpha = vec![128u8; (original.width * original.height) as usize];
    let mut alph = vec![0u8]; // compression 0, filtering 0
    alph.extend_from_slice(&alpha);
    let chunks: Vec<(&[u8; 4], &[u8])> = vec![(b"VP8X", &vp8x), (b"ALPH", &alph), (b"VP8 ", vp8)];
    let bytes = riff_container(&chunks);
    let decoded = decode(&bytes).expect("VP8X with alpha decodes");
    assert_eq!(decoded.data[3], 128);
}

#[test]
fn vp8x_with_vp8l_alpha_in_bitstream_decodes_successfully() {
    // VP8X wrapping VP8L that already carries alpha in its ARGB bitstream: the
    // alpha flag may be set without a separate ALPH chunk.
    let alpha_webp = fixture("alpha_real.webp");
    let vp8l = find_chunk_payload(&alpha_webp, b"VP8L").expect("fixture has VP8L");
    let plain = decode(&alpha_webp).expect("plain VP8L decodes");
    let (width, height) = plain.dimensions();
    let mut vp8x = [0u8; 10];
    vp8x[0] = 0x10; // alpha flag
    let w = width - 1;
    let h = height - 1;
    vp8x[4] = (w & 0xFF) as u8;
    vp8x[5] = ((w >> 8) & 0xFF) as u8;
    vp8x[6] = ((w >> 16) & 0xFF) as u8;
    vp8x[7] = (h & 0xFF) as u8;
    vp8x[8] = ((h >> 8) & 0xFF) as u8;
    vp8x[9] = ((h >> 16) & 0xFF) as u8;
    let chunks: Vec<(&[u8; 4], &[u8])> = vec![(b"VP8X", &vp8x), (b"VP8L", vp8l)];
    let bytes = riff_container(&chunks);
    let image = decode(&bytes).expect("VP8X+VP8L with in-bitstream alpha must decode");
    assert_eq!(image.dimensions(), plain.dimensions());
    assert_eq!(image.data, plain.data);
}
