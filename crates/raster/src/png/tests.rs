//! PNG encoder tests.

#[allow(unused_imports)]
use super::*;

use crate::inflate::inflate_zlib;

/// Parsed chunk: `(type, payload)`.
type Chunk = (String, Vec<u8>);

/// Parses a PNG stream, validating every chunk CRC, and returns the chunk
/// list and the concatenated inflated image data.
fn parse_png(png: &[u8]) -> (Vec<Chunk>, Vec<u8>) {
    assert!(png.starts_with(&PNG_SIGNATURE), "missing PNG signature");
    let mut pos = PNG_SIGNATURE.len();
    let mut chunks = Vec::new();
    let mut idat = Vec::new();

    loop {
        assert!(pos + 8 <= png.len(), "truncated chunk header");
        let length = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        let kind = &png[pos + 4..pos + 8];
        let data_start = pos + 8;
        let data_end = data_start + length;
        assert!(data_end + 4 <= png.len(), "truncated chunk payload");
        let data = &png[data_start..data_end];
        let stored_crc = u32::from_be_bytes(png[data_end..data_end + 4].try_into().unwrap());

        let mut computed = crc32_update(0xFFFF_FFFF, kind);
        computed = crc32_update(computed, data);
        computed ^= 0xFFFF_FFFF;
        assert_eq!(stored_crc, computed, "bad CRC for chunk {kind:?}");

        let name = String::from_utf8_lossy(kind).into_owned();
        if name == "IDAT" {
            idat.extend_from_slice(data);
        }
        chunks.push((name, data.to_vec()));
        pos = data_end + 4;
        if kind == b"IEND" {
            break;
        }
    }
    assert_eq!(pos, png.len(), "trailing bytes after IEND");

    (
        chunks,
        inflate_zlib(&idat).expect("valid zlib/IDAT payload"),
    )
}

/// Reverses the PNG scanline filters of an 8-bit RGBA image.
fn rgba_from_scanlines(raw: &[u8], width: u32, height: u32) -> Vec<u8> {
    let stride = width as usize * 4;
    let mut rgba = Vec::with_capacity(stride * height as usize);
    let mut previous = vec![0u8; stride];
    for row_index in 0..height as usize {
        let start = row_index * (stride + 1);
        let filter = raw[start];
        let mut row = raw[start + 1..start + 1 + stride].to_vec();
        for x in 0..stride {
            let left = if x >= 4 { row[x - 4] } else { 0 };
            let above = previous[x];
            let upper_left = if x >= 4 { previous[x - 4] } else { 0 };
            let predicted = match filter {
                0 => 0,
                1 => left,
                2 => above,
                3 => {
                    let sum = u16::from(left) + u16::from(above);
                    (sum / 2) as u8
                }
                4 => paeth_predictor(left, above, upper_left),
                other => panic!("invalid filter {other}"),
            };
            row[x] = row[x].wrapping_add(predicted);
        }
        rgba.extend_from_slice(&row);
        previous = row;
    }
    rgba
}

#[test]
fn test_crc32_and_adler32_reference_vectors() {
    // Well-known PNG CRC of the IEND chunk (type only, empty payload).
    assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    // RFC 1950 reference example.
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    assert_eq!(adler32(b""), 1);
}

#[test]
fn test_png_signature_and_chunk_layout() {
    let rgba = [255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 1, 2, 3, 4];
    let png = encode_rgba(2, 2, &rgba);
    assert!(!png.is_empty());
    assert_eq!(&png[..8], &PNG_SIGNATURE);

    let (chunks, inflated) = parse_png(&png);
    let names: Vec<&str> = chunks.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, vec!["IHDR", "IDAT", "IEND"]);

    let ihdr = &chunks[0].1;
    assert_eq!(ihdr.len(), 13);
    assert_eq!(u32::from_be_bytes(ihdr[0..4].try_into().unwrap()), 2);
    assert_eq!(u32::from_be_bytes(ihdr[4..8].try_into().unwrap()), 2);
    assert_eq!(ihdr[8], 8, "bit depth");
    assert_eq!(ihdr[9], 6, "color type RGBA");
    assert_eq!(ihdr[10], 0, "compression");
    assert_eq!(ihdr[11], 0, "filter");
    assert_eq!(ihdr[12], 0, "interlace");

    // 2 rows × (1 filter byte + 2 pixels × 4 bytes).
    assert_eq!(inflated.len(), 2 * (1 + 8));
    assert_eq!(rgba_from_scanlines(&inflated, 2, 2), rgba);
}

#[test]
fn test_flat_image_compresses_well() {
    let width = 200u32;
    let height = 100u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        rgba.extend_from_slice(&[12, 200, 34, 255]);
    }
    let png = encode_rgba(width, height, &rgba);
    let (_, inflated) = parse_png(&png);
    assert_eq!(rgba_from_scanlines(&inflated, width, height), rgba);
    assert!(
        png.len() < 1024,
        "flat 200x100 image should compress below 1 KiB, got {} bytes",
        png.len()
    );
}

#[test]
fn test_png_pixel_roundtrip_single_pixel() {
    let rgba = [12u8, 34, 56, 78];
    let png = encode_rgba(1, 1, &rgba);
    let (_, inflated) = parse_png(&png);
    assert_eq!(rgba_from_scanlines(&inflated, 1, 1), rgba);
}

#[test]
fn test_encode_rgba_rejects_inconsistent_buffers() {
    assert!(encode_rgba(0, 0, &[]).is_empty());
    assert!(encode_rgba(2, 2, &[0u8; 15]).is_empty());
}
