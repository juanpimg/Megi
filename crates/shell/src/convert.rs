//! Framebuffer conversion between `paint::Pixmap` (RGBA) and the softbuffer surface
//! format (`0RGB`, `u32` per pixel).
//!
//! softbuffer expects each pixel as a native-endian `u32` with the layout
//! `0x00RRGGBB`; the render canvas is always opaque white-backed, so the alpha channel
//! is ignored here. This conversion is pure and unit-tested (no display/event loop).

use paint::Pixmap;

/// Softbuffer pixel format produced by [`pixmap_to_0rgb`].
pub const PIXEL_FORMAT: &str = "0RGB";

/// Opaque white pixel in `0RGB` format (used for blank/error frames).
pub const WHITE_0RGB: u32 = 0x00FF_FFFF;

/// Number of `u32` pixels required for a `width * height` framebuffer, or `None` on
/// overflow (hostile resize values must not panic).
pub fn expected_buffer_len(width: u32, height: u32) -> Option<usize> {
    (width as usize).checked_mul(height as usize)
}

/// Converts every RGBA pixel of `pixmap` into a `0RGB` framebuffer word.
///
/// Returns exactly `width * height` entries (the `Pixmap` invariant).
pub fn pixmap_to_0rgb(pixmap: &Pixmap) -> Vec<u32> {
    pixmap
        .pixels
        .iter()
        .map(|pixel| (u32::from(pixel.r) << 16) | (u32::from(pixel.g) << 8) | u32::from(pixel.b))
        .collect()
}

/// Returns a blank opaque-white framebuffer of `len` pixels.
pub fn blank_frame(len: usize) -> Vec<u32> {
    vec![WHITE_0RGB; len]
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use paint::Color;

    #[test]
    fn converts_rgba_to_0rgb() {
        let mut pixmap = Pixmap::new(2, 1);
        pixmap.set_pixel(0, 0, Color::new(0x12, 0x34, 0x56, 0xFF));
        pixmap.set_pixel(1, 0, Color::new(0xFF, 0x00, 0x00, 0x80));
        let converted = pixmap_to_0rgb(&pixmap);
        assert_eq!(converted, vec![0x0012_3456, 0x00FF_0000]);
    }

    #[test]
    fn converts_a_filled_frame() {
        let mut pixmap = Pixmap::new(4, 3);
        for y in 0..3 {
            for x in 0..4 {
                pixmap.set_pixel(x, y, Color::WHITE);
            }
        }
        let converted = pixmap_to_0rgb(&pixmap);
        assert_eq!(converted.len(), 12);
        assert!(converted.iter().all(|&pixel| pixel == WHITE_0RGB));
    }

    #[test]
    fn buffer_lengths_are_checked() {
        assert_eq!(expected_buffer_len(800, 600), Some(480_000));
        // On 64-bit hosts the u32 square still fits in usize; the checked path exists
        // for 32-bit targets (and for future callers passing usize sizes).
        if cfg!(target_pointer_width = "64") {
            assert_eq!(
                expected_buffer_len(u32::MAX, u32::MAX),
                Some((u32::MAX as usize) * (u32::MAX as usize))
            );
        } else {
            assert_eq!(expected_buffer_len(u32::MAX, u32::MAX), None);
        }
        assert_eq!(blank_frame(3), vec![WHITE_0RGB; 3]);
    }

    #[test]
    fn format_is_documented_as_0rgb() {
        assert_eq!(PIXEL_FORMAT, "0RGB");
    }
}
