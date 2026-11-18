//! PNG encoder workloads.

use super::*;

/// Deterministic RGBA pattern for the PNG-phase benchmarks.
///
/// `flat` mimics a UI page (repeating rows/bands, long runs); the other mode is a
/// photo-like gradient plus deterministic LCG noise, which stresses the deflate match search.
/// `alpha` varies the alpha channel (straight RGBA) for the alpha-encode row.
fn png_pattern(width: u32, height: u32, photo: bool, alpha: bool) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    let mut state = 0x9E37_79B9u32;
    for y in 0..height {
        for x in 0..width {
            let (r, g, b) = if photo {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (state >> 24) as u8;
                let gradient = ((x * 255 / width.max(1)) + (y * 127 / height.max(1))) as u8;
                (
                    gradient.wrapping_add(noise / 4),
                    gradient.rotate_left(3).wrapping_add(noise / 8),
                    gradient.rotate_left(6).wrapping_add(noise / 16),
                )
            } else if y < height / 8 {
                (0x20, 0x21, 0x24)
            } else if y < height / 4 {
                (0xF1, 0xF3, 0xF4)
            } else if (x / 40 + y / 24) % 3 == 0 {
                (0xE8, 0xF0, 0xFE)
            } else {
                (0xFF, 0xFF, 0xFF)
            };
            let a = if alpha { (state >> 8) as u8 } else { 0xFF };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    rgba
}

/// Registers the isolated PNG encode-phase rows (`filter`, `deflate`, full `encode`).
///
/// The filter/deflate rows let the round report separate the two dominant phases; the
/// deflate row captures the already-filtered scanlines so only `zlib_compress` runs inside
/// the measurement window. Rows cover the two render viewports (800×600, 1024×768) with a
/// flat UI pattern and the full encoder also covers a photo-like alpha pattern.
pub(super) fn register_png_rows(suite: &mut Suite) {
    for (index, width, height) in [(0usize, 800u32, 600u32), (1, 1024, 768)] {
        let (filter_name, deflate_name, encode_name) = if index == 0 {
            (
                "paint/png-filter-800x600",
                "paint/png-deflate-800x600",
                "paint/png-encode-800x600",
            )
        } else {
            (
                "paint/png-filter-1024x768",
                "paint/png-deflate-1024x768",
                "paint/png-encode-1024x768",
            )
        };
        let rgba = Arc::new(png_pattern(width, height, false, false));
        let filtered = Arc::new(megi_paint::filter_scanlines(width, height, &rgba));
        let filter_rgba = Arc::clone(&rgba);
        suite.add(
            BenchEntry::new(
                filter_name,
                "paint",
                format!("filter_scanlines {width}x{height} (flat UI)"),
                move || {
                    black_box(megi_paint::filter_scanlines(width, height, &filter_rgba));
                },
            )
            .heavy(),
        );
        let deflate_filtered = Arc::clone(&filtered);
        suite.add(
            BenchEntry::new(
                deflate_name,
                "paint",
                format!("zlib_compress {width}x{height} (pre-filtered flat UI)"),
                move || {
                    black_box(megi_paint::zlib_compress(&deflate_filtered));
                },
            )
            .heavy(),
        );
        let encode_rgba = Arc::clone(&rgba);
        suite.add(
            BenchEntry::new(
                encode_name,
                "paint",
                format!("encode_rgba {width}x{height} (flat UI)"),
                move || {
                    black_box(megi_paint::encode_rgba(width, height, &encode_rgba));
                },
            )
            .heavy(),
        );
        let alpha = Arc::new(png_pattern(width, height, true, true));
        let alpha_name = if index == 0 {
            "paint/png-encode-800x600-alpha"
        } else {
            "paint/png-encode-1024x768-alpha"
        };
        suite.add(
            BenchEntry::new(
                alpha_name,
                "paint",
                format!("encode_rgba {width}x{height} (photo alpha)"),
                move || {
                    black_box(megi_paint::encode_rgba(width, height, &alpha));
                },
            )
            .heavy(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_image_fixtures_decode_to_rgba() {
        for (label, bytes, width, height) in [
            ("png", PNG_800X600, 800, 600),
            ("jpeg", JPEG_800X600, 800, 600),
            ("gif", GIF_800X600, 800, 600),
            ("webp-photo", WEBP_PHOTO, 64, 48),
            ("webp-alpha", WEBP_ALPHA, 40, 32),
            ("webp-palette", WEBP_PALETTE, 32, 24),
            ("webp-lossy-1024", WEBP_LOSSY_1024, 1024, 768),
            ("webp-lossy-q75", WEBP_LOSSY_Q75, 64, 48),
            ("webp-lossy-q90", WEBP_LOSSY_Q90, 64, 48),
        ] {
            let decoded = megi_image::decode(bytes).expect("committed fixture decodes");
            assert_eq!((decoded.width, decoded.height), (width, height), "{label}");
            assert_eq!(decoded.data.len(), (width * height * 4) as usize, "{label}");
        }
    }

    /// The VP8 lossy bench rows must measure a deterministic decode: two decodes of
    /// the same fixture produce identical RGBA bytes (and opaque alpha).
    #[test]
    fn webp_lossy_bench_fixtures_decode_deterministically() {
        for (label, bytes) in [
            ("webp-lossy-1024", WEBP_LOSSY_1024),
            ("webp-lossy-q75", WEBP_LOSSY_Q75),
            ("webp-lossy-q90", WEBP_LOSSY_Q90),
        ] {
            let first = megi_image::decode(bytes).expect("first decode");
            let second = megi_image::decode(bytes).expect("second decode");
            assert_eq!(
                (first.width, first.height),
                (second.width, second.height),
                "{label}"
            );
            assert_eq!(
                first.data, second.data,
                "{label}: decode must be deterministic"
            );
            assert!(
                first
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| pixel[3] == 255),
                "{label}: VP8 lossy has no alpha, pixels must be opaque"
            );
        }
    }
}
