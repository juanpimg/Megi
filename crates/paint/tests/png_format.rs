//! PNG format tests for the dependency-free encoder (WP-4.1).
//!
//! These tests treat `Pixmap::to_png` as a black box: they parse the produced
//! stream with an independent CRC-32/zlib implementation (see `common`), so a
//! bug in the encoder cannot be masked by reusing the encoder's own helpers
//! for validation.

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
fn test_png_large_enough_for_multiple_stored_blocks() {
    // 120x120 RGBA = 57 600 raw pixel bytes + 120 filter bytes > 65 535 once
    // scanline padding is taken into account? Keep it clearly above one block
    // by using a bigger image: 160x160 -> 103 040 bytes of image data.
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
