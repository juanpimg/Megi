//! Responsive images integration tests (`srcset` / `sizes` / `<picture>` / `<source>`, #1543).

use css::Color;
use render::images::{
    SrcsetCandidate, SrcsetDescriptor, parse_srcset, resolve_sizes, select_image_candidate,
};
use render::{RenderOptions, Viewport, render_url};

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn solid_png_data_url(width: u32, height: u32, rgb: [u8; 3]) -> String {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    let png = paint::encode_rgba(width, height, &rgba);
    format!("data:image/png;base64,{}", encode_base64(&png))
}

fn html_data_url(html: &str) -> String {
    let mut encoded = String::with_capacity(html.len());
    for byte in html.bytes() {
        match byte {
            b'#' => encoded.push_str("%23"),
            b'%' => encoded.push_str("%25"),
            b'?' => encoded.push_str("%3F"),
            b'\n' => encoded.push_str("%0A"),
            b'\r' => encoded.push_str("%0D"),
            b' ' => encoded.push_str("%20"),
            _ if byte.is_ascii() => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    format!("data:text/html;charset=utf-8,{encoded}")
}

fn count_color(pixmap: &paint::Pixmap, color: Color) -> usize {
    pixmap.pixels.iter().filter(|p| **p == color).count()
}

fn find_img_box(layout: &layout::LayoutBox) -> Option<&layout::LayoutBox> {
    if matches!(layout.box_type, layout::BoxType::Replaced(_)) {
        return Some(layout);
    }
    for child in &layout.children {
        if let Some(found) = find_img_box(child) {
            return Some(found);
        }
    }
    None
}

#[test]
fn srcset_parser_handles_data_urls_with_commas_and_mixed_descriptors() {
    let parsed = parse_srcset("data:image/png;base64,AAAA 1x, data:image/png;base64,BBBB 2x");
    assert_eq!(
        parsed,
        vec![
            SrcsetCandidate {
                url: "data:image/png;base64,AAAA".into(),
                descriptor: SrcsetDescriptor::Density(1.0),
            },
            SrcsetCandidate {
                url: "data:image/png;base64,BBBB".into(),
                descriptor: SrcsetDescriptor::Density(2.0),
            },
        ]
    );

    // Mixing `w` and `x` drops the non-`w` entries cleanly per HTML §4.8.4.2.
    let mixed = parse_srcset("small.png 300w, density.png 2x, large.png 900w, broken 0w");
    assert_eq!(
        mixed,
        vec![
            SrcsetCandidate {
                url: "small.png".into(),
                descriptor: SrcsetDescriptor::Width(300),
            },
            SrcsetCandidate {
                url: "large.png".into(),
                descriptor: SrcsetDescriptor::Width(900),
            },
        ]
    );
}

#[test]
fn sizes_and_width_descriptors_select_according_to_viewport_and_dpr() {
    let narrow = css::MediaContext::with_viewport(400.0, 300.0);
    let wide = css::MediaContext::with_viewport(1000.0, 600.0);
    let sizes = "(max-width: 500px) 200px, 50vw";

    assert!((resolve_sizes(Some(sizes), &narrow) - 200.0).abs() < 0.1);
    assert!((resolve_sizes(Some(sizes), &wide) - 500.0).abs() < 0.1);

    let srcset = "s.png 200w, m.png 400w, l.png 1000w";
    assert_eq!(
        select_image_candidate(Some(srcset), None, Some(sizes), &narrow, 1.0).map(|(u, _)| u),
        Some("s.png".into())
    );
    assert_eq!(
        select_image_candidate(Some(srcset), None, Some(sizes), &wide, 1.0).map(|(u, _)| u),
        Some("l.png".into())
    );
    assert_eq!(
        select_image_candidate(Some(srcset), None, Some(sizes), &narrow, 2.0).map(|(u, _)| u),
        Some("m.png".into())
    );
}

#[test]
fn srcset_only_img_paints_and_scales_hidpi_intrinsic_size() {
    let red_1x = solid_png_data_url(20, 10, [255, 0, 0]);
    let green_2x = solid_png_data_url(40, 20, [0, 128, 0]);
    let html = format!(
        "<!doctype html><html><body style='margin:0'><img style='display:block' srcset='{red_1x} 1x, {green_2x} 2x'></body></html>"
    );

    let out_1x = render_url(
        &html_data_url(&html),
        &RenderOptions {
            viewport: Viewport::new(100, 100).expect("vp"),
            device_pixel_ratio: 1.0,
            ..RenderOptions::default()
        },
    )
    .expect("render 1x");
    let red_px = count_color(&out_1x.pixmap, Color::RED);
    let green_px = count_color(&out_1x.pixmap, Color::GREEN);
    assert!(
        red_px >= 200,
        "1x expected >=200 red px, got red={red_px}, green={green_px}, warnings={:?}, stats={:?}",
        out_1x.stats.warnings,
        out_1x.stats
    );
    assert_eq!(green_px, 0);
    let box_1x = find_img_box(&out_1x.layout).expect("img box 1x");
    assert!((box_1x.dimensions.content.width - 20.0).abs() < 0.1);
    assert!((box_1x.dimensions.content.height - 10.0).abs() < 0.1);

    let out_2x = render_url(
        &html_data_url(&html),
        &RenderOptions {
            viewport: Viewport::new(100, 100).expect("vp"),
            device_pixel_ratio: 2.0,
            ..RenderOptions::default()
        },
    )
    .expect("render 2x");
    let red_2x = count_color(&out_2x.pixmap, Color::RED);
    let green_2x = count_color(&out_2x.pixmap, Color::GREEN);
    assert!(
        green_2x >= 200,
        "2x expected >=200 green px, got red={red_2x}, green={green_2x}, warnings={:?}",
        out_2x.stats.warnings
    );
    assert_eq!(red_2x, 0);
    // 40x20 bitmap at 2x density must preserve 20x10 CSS px intrinsic layout size.
    let box_2x = find_img_box(&out_2x.layout).expect("img box 2x");
    assert!((box_2x.dimensions.content.width - 20.0).abs() < 0.1);
    assert!((box_2x.dimensions.content.height - 10.0).abs() < 0.1);
}

#[test]
fn picture_source_matches_media_skips_unsupported_mime_and_falls_back_to_img() {
    let avif_dummy = solid_png_data_url(16, 16, [255, 0, 255]);
    let blue_wide = solid_png_data_url(30, 30, [0, 0, 255]);
    let red_fallback = solid_png_data_url(30, 30, [255, 0, 0]);
    let html = format!(
        r#"<!doctype html><html><body style="margin:0"><picture style="display:block">
            <source type="image/avif" srcset="{avif_dummy}">
            <source media="(min-width: 500px)" type="image/png" srcset="{blue_wide}">
            <img style="display:block" src="{red_fallback}">
        </picture></body></html>"#
    );
    let url = html_data_url(&html);

    let wide = render_url(
        &url,
        &RenderOptions {
            viewport: Viewport::new(600, 100).expect("vp"),
            ..RenderOptions::default()
        },
    )
    .expect("wide render");
    let blue_w = count_color(&wide.pixmap, Color::BLUE);
    let red_w = count_color(&wide.pixmap, Color::RED);
    assert!(
        blue_w >= 900,
        "wide expected >=900 blue px, got blue={blue_w}, red={red_w}, warnings={:?}",
        wide.stats.warnings
    );
    assert_eq!(red_w, 0);

    let narrow = render_url(
        &url,
        &RenderOptions {
            viewport: Viewport::new(320, 100).expect("vp"),
            ..RenderOptions::default()
        },
    )
    .expect("narrow render");
    let red_n = count_color(&narrow.pixmap, Color::RED);
    let blue_n = count_color(&narrow.pixmap, Color::BLUE);
    assert!(
        red_n >= 900,
        "narrow expected >=900 red px, got red={red_n}, blue={blue_n}, warnings={:?}",
        narrow.stats.warnings
    );
    assert_eq!(blue_n, 0);
}
