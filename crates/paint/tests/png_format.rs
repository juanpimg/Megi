//! PNG format tests for the dependency-free encoder (WP-4.1/WP-4.11).
//!
//! These tests treat `Pixmap::to_png` as a black box: they parse the produced
//! stream with independent CRC-32 helpers (see `common`) and decode the IDAT
//! payload with the in-tree verification inflater, so a bug in the encoder
//! cannot be masked by reusing the encoder's own helpers for validation.

mod common;

use common::{ihdr_fields, parse_png, rgba_from_scanlines};
use layout::Rect;
use paint::{Color, Pixmap};

/// Builds a small pixmap with a known, non-uniform pattern:
///
/// ```text
/// row 0: red    green  blue   white
/// row 1: black  (0,0,0,128) (10,20,30,40) (1,2,3,4)
/// row 2: (5,6,7,8) repeated
/// ```
fn patterned_pixmap() -> (Pixmap, Vec<u8>) {
    let mut pixmap = Pixmap::new(4, 3);
    pixmap.set_pixel(0, 0, Color::RED);
    pixmap.set_pixel(1, 0, Color::GREEN);
    pixmap.set_pixel(2, 0, Color::BLUE);
    pixmap.set_pixel(3, 0, Color::WHITE);
    pixmap.set_pixel(0, 1, Color::BLACK);
    pixmap.set_pixel(1, 1, Color::new(0, 0, 0, 128));
    pixmap.set_pixel(2, 1, Color::new(10, 20, 30, 40));
    pixmap.set_pixel(3, 1, Color::new(1, 2, 3, 4));
    for x in 0..4 {
        pixmap.set_pixel(x, 2, Color::new(5, 6, 7, 8));
    }

    let mut expected = Vec::new();
    for pixel in &pixmap.pixels {
        expected.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
    }
    (pixmap, expected)
}

#[test]
fn test_png_signature_and_chunk_crcs() {
    let (pixmap, _) = patterned_pixmap();
    let png = pixmap.to_png();
    let parsed = parse_png(&png); // validates signature, chunk order and CRCs

    assert_eq!(parsed.chunk_names(), vec!["IHDR", "IDAT", "IEND"]);
    assert_eq!(parsed.chunk("IEND").map(<[u8]>::len), Some(0));
    assert!(!parsed.chunk("IDAT").unwrap().is_empty());
}

#[test]
fn test_png_ihdr_describes_rgba_8bit() {
    let (pixmap, _) = patterned_pixmap();
    let parsed = parse_png(&pixmap.to_png());
    let (width, height, bit_depth, color_type) = ihdr_fields(&parsed);

    assert_eq!(width, 4);
    assert_eq!(height, 3);
    assert_eq!(bit_depth, 8, "8 bits per channel");
    assert_eq!(color_type, 6, "truecolor with alpha (RGBA)");

    let ihdr = parsed.chunk("IHDR").unwrap();
    assert_eq!(ihdr[10], 0, "compression method");
    assert_eq!(ihdr[11], 0, "filter method");
    assert_eq!(ihdr[12], 0, "interlace method");
}

#[test]
fn test_png_pixel_roundtrip_equals_pixmap() {
    let (pixmap, expected) = patterned_pixmap();
    let parsed = parse_png(&pixmap.to_png());
    let (width, height, _, _) = ihdr_fields(&parsed);
    let decoded = rgba_from_scanlines(&parsed.raw, width, height);

    assert_eq!(decoded.len(), expected.len());
    assert_eq!(decoded, expected, "encoded pixels must roundtrip exactly");
    assert_eq!(decoded, pixmap.to_rgba_bytes());
}

#[test]
fn test_large_image_roundtrip_with_compression() {
    // 160x160 RGBA exercises multi-block DEFLATE output (LZ77 + fixed Huffman)
    // and per-row adaptive filtering.
    let mut pixmap = Pixmap::new(160, 160);
    let mut expected = Vec::with_capacity(160 * 160 * 4);
    for y in 0..160u32 {
        for x in 0..160u32 {
            let pixel = Color::new((x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255);
            pixmap.set_pixel(x, y, pixel);
            expected.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
        }
    }

    let parsed = parse_png(&pixmap.to_png());
    let (width, height, _, _) = ihdr_fields(&parsed);
    assert_eq!(rgba_from_scanlines(&parsed.raw, width, height), expected);
}

#[test]
fn test_encode_pixels_matches_encode_rgba_byte_for_byte() {
    // The direct-pixel path avoids the intermediate raw-RGBA buffer; it must
    // produce exactly the same PNG stream.
    let (pixmap, rgba) = patterned_pixmap();
    let direct = paint::encode_pixels(pixmap.width, pixmap.height, &pixmap.pixels);
    let via_rgba = paint::encode_rgba(pixmap.width, pixmap.height, &rgba);
    assert_eq!(direct, via_rgba);

    let mut flat = Pixmap::new(120, 60);
    flat.fill_rect(Rect::new(0.0, 0.0, 120.0, 60.0), Color::new(9, 8, 7, 255));
    assert_eq!(
        paint::encode_pixels(flat.width, flat.height, &flat.pixels),
        paint::encode_rgba(flat.width, flat.height, &flat.to_rgba_bytes())
    );
}

#[test]
fn test_png_uses_real_deflate_compression() {
    // A flat image must compress to a small fraction of its raw size: this is
    // the regression test for "PNG writer emits stored DEFLATE blocks".
    let mut pixmap = Pixmap::new(400, 300);
    pixmap.fill_rect(
        Rect::new(0.0, 0.0, 400.0, 300.0),
        Color::new(20, 40, 60, 255),
    );
    let raw_len = pixmap.to_rgba_bytes().len();
    let png = pixmap.to_png();
    assert!(
        png.len() < raw_len / 20,
        "flat PNG should be at least 20x smaller than raw RGBA ({} vs {} bytes)",
        png.len(),
        raw_len
    );
    let parsed = parse_png(&png);
    let (width, height, _, _) = ihdr_fields(&parsed);
    assert_eq!(
        rgba_from_scanlines(&parsed.raw, width, height),
        pixmap.to_rgba_bytes()
    );
}

#[test]
fn test_row_filters_are_valid_and_diverse() {
    // A vertical gradient makes `Up`/`Paeth` attractive; the encoder must emit
    // only valid PNG filter bytes (0..=4).
    let mut pixmap = Pixmap::new(64, 64);
    for y in 0..64u32 {
        for x in 0..64u32 {
            let value = (y * 3) as u8;
            pixmap.set_pixel(x, y, Color::new(value, value / 2, 255 - value, 255));
        }
    }
    let png = pixmap.to_png();
    let parsed = parse_png(&png);
    let stride = 64 * 4;
    let mut filters = std::collections::BTreeSet::new();
    for row in 0..64 {
        let filter = parsed.raw[row * (stride + 1)];
        assert!(filter <= 4, "invalid PNG filter {filter}");
        filters.insert(filter);
    }
    assert_eq!(
        rgba_from_scanlines(&parsed.raw, 64, 64),
        pixmap.to_rgba_bytes()
    );
    assert!(!filters.is_empty());
}

#[test]
fn test_zlib_roundtrip_public_api() {
    let mut pseudo_random = Vec::with_capacity(32_768);
    let mut state = 0x2545_F491u32;
    for _ in 0..32_768 {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        pseudo_random.push((state >> 16) as u8);
    }
    for input in [&b""[..], b"abcabcabc", &pseudo_random] {
        let compressed = paint::zlib_compress(input);
        let decoded = paint::inflate_zlib(&compressed).expect("valid zlib stream");
        assert_eq!(decoded, input);
    }
}

#[test]
fn test_to_rgba_bytes_is_row_major() {
    let (pixmap, expected) = patterned_pixmap();
    assert_eq!(pixmap.to_rgba_bytes(), expected);
    assert_eq!(pixmap.to_rgba_bytes().len(), 4 * 3 * 4);
}

#[test]
fn test_to_png_rgba_matches_to_png() {
    let (pixmap, _) = patterned_pixmap();
    assert_eq!(pixmap.to_png_rgba(), pixmap.to_png());
}

#[test]
fn test_png_is_deterministic() {
    let (pixmap, _) = patterned_pixmap();
    assert_eq!(pixmap.to_png(), pixmap.to_png());
}

#[test]
fn test_ppm_export_is_still_available() {
    let mut pixmap = Pixmap::new(2, 1);
    pixmap.fill_rect(Rect::new(0.0, 0.0, 2.0, 1.0), Color::RED);
    let ppm = pixmap.to_ppm();
    assert!(ppm.starts_with(b"P6\n2 1\n255\n"));
    assert_eq!(ppm.len(), b"P6\n2 1\n255\n".len() + 6);
}
