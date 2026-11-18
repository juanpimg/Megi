//! Image decoding and full render-pipeline workloads.

use super::*;

/// Image decoding workloads (rounds 22/25/31): PNG/JPEG/GIF 800×600, real WebP
/// VP8L lossless fixtures (transforms/alpha/palette) and WebP VP8 **lossy**
/// keyframes (1024×768 + 64×48 q75/q90) → RGBA8.
///
/// Real pages spend a meaningful part of their time decoding images, which the
/// legacy parse/style/layout/paint rows never exercised. These rows use
/// committed fixtures (PNG/WebP from `crates/image/tests/fixtures`, JPEG/GIF from
/// `crates/bench/fixtures`, see the README there) loaded with `include_bytes!`,
/// so the measurement is deterministic and offline. Each row decodes the whole
/// image with the format defaults and drops the resulting RGBA buffer.
///
/// The `image/decode-webp-lossy*` rows exercise the in-tree VP8 keyframe decoder
/// (boolean entropy decoder, intra prediction, IDCT/WHT, loop filter and
/// YUV420→RGB); `image/decode-webp-lossy` is the 1024×768 heavy row, while the
/// 64×48 q75/q90 rows keep the default iteration count for a stable median of
/// the small-image path. Their determinism is asserted by the unit test below.
///
/// Reference medians (release, 5 iterations / heavy cap 3, idle host) are in
/// `docs/PERFORMANCE.md`; treat a reproducible >15 % regression as a finding.
pub(super) fn register_image(suite: &mut Suite) {
    // (name, fixture, label, heavy). Heavy rows get fewer timed iterations; the tiny
    // 64×48 lossy rows keep the default iteration count so their median is stable.
    for (name, bytes, label, heavy) in [
        ("image/decode-png-800x600", PNG_800X600, "PNG", true),
        ("image/decode-jpeg-800x600", JPEG_800X600, "JPEG", true),
        ("image/decode-gif", GIF_800X600, "GIF", true),
        (
            "image/decode-webp-lossless",
            WEBP_PHOTO,
            "WebP VP8L (transforms)",
            false,
        ),
        (
            "image/decode-webp-alpha",
            WEBP_ALPHA,
            "WebP VP8L (alpha)",
            false,
        ),
        (
            "image/decode-webp-palette",
            WEBP_PALETTE,
            "WebP VP8L (palette)",
            false,
        ),
        (
            "image/decode-webp-lossy",
            WEBP_LOSSY_1024,
            "WebP VP8 lossy (1024×768)",
            true,
        ),
        (
            "image/decode-webp-lossy-q75",
            WEBP_LOSSY_Q75,
            "WebP VP8 lossy (q75)",
            false,
        ),
        (
            "image/decode-webp-lossy-q90",
            WEBP_LOSSY_Q90,
            "WebP VP8 lossy (q90)",
            false,
        ),
    ] {
        match megi_image::decode(bytes) {
            Ok(probe) => {
                let pixels = (probe.width as f64) * (probe.height as f64) / 1_000_000.0;
                let detail = format!(
                    "{label} {}x{} ({} B) → RGBA8, decode completo",
                    probe.width,
                    probe.height,
                    bytes.len()
                );
                let entry = BenchEntry::new(name, "image", detail, move || {
                    black_box(megi_image::decode(bytes).expect("committed image fixture decodes"));
                })
                .with_units(pixels, "MPix");
                suite.add(if heavy { entry.heavy() } else { entry });
            }
            Err(error) => suite.skip(name, format!("fixture de imagen inválido: {error}")),
        }
    }

    // Scaled-decode companion row: integer box downscale (800×600 → 400×300) while
    // unfiltering, so the scaled path cannot silently regress against the full decode.
    let scaled_label = "PNG 800x600 → box downscale ×2 (400x300), decode escalado";
    suite.add(
        BenchEntry::new(
            "image/decode-png-scaled-x2",
            "image",
            scaled_label.to_string(),
            move || {
                black_box(
                    megi_image::decode_scaled(PNG_800X600, 400)
                        .expect("committed PNG fixture decodes scaled"),
                );
            },
        )
        .with_units(0.12, "MPix")
        .heavy(),
    );
}

/// Registers the **render crate** end-to-end row (`render/html-to-png`).
///
/// `pipeline/html-to-png` predates the `render` crate and composes html/css/layout/paint
/// manually, so it never exercises the real browser pipeline (loader, resource scanning,
/// style collection, scripts/images scans, render options and the final PNG encode). This
/// row feeds a deterministic data URL through `megi_render::render_url` to measure that path.
pub(super) fn register_render_pipeline(suite: &mut Suite) {
    fn percent_encode_data_url(html: &str) -> String {
        let mut out = String::with_capacity(html.len() + html.len() / 2 + 16);
        out.push_str("data:text/html,");
        for byte in html.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                out.push(byte as char);
            } else {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
        out
    }

    let mut html = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>\
         html,body{margin:0;padding:0;background:#fff}\
         .card{display:inline-block;width:80px;height:24px;margin:2px;background:#ddd}\
         .card:nth-child(3n){background:#9cf}\
         .card:nth-child(4n-1){background:#fc9}\
         .label{color:#123;font:12px sans-serif}\
         </style></head><body>",
    );
    for index in 0..240 {
        html.push_str("<div class=\"card\"><span class=\"label\">item ");
        html.push_str(&index.to_string());
        html.push_str("</span></div>");
    }
    html.push_str("</body></html>");
    let url = percent_encode_data_url(&html);
    let options = megi_render::RenderOptions {
        viewport: megi_render::Viewport::new(800, 600).expect("render viewport"),
        ..megi_render::RenderOptions::default()
    };
    let detail = format!(
        "render crate end-to-end: data URL ({} B HTML) → DOM/styles/layout/paint → PNG",
        html.len()
    );
    suite.add(
        BenchEntry::new("render/html-to-png", "render", detail, move || {
            black_box(megi_render::render_url(&url, &options).expect("render benchmark fixture"));
        })
        .with_units(0.096, "MPix")
        .heavy(),
    );
}
