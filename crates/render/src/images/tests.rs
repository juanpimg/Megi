//! Unit tests for the `<img>` loading stage.

use super::*;
use megi_net::{FetchOptions, ResourceLoader};

#[cfg(test)]
mod dedup_tests {
    use super::*;

    #[test]
    fn image_byte_cap_is_applied_during_fetch() {
        // A data URL larger than max_image_bytes must be rejected by the fetch layer
        // (response cap), not only by the post-load size check (#1213).
        let payload = "x".repeat(4096);
        let source = format!("<body><img src='data:image/png;base64,{payload}'></body>");
        let document = megi_html::parse_document(&source, false);
        let base = megi_net::Url::parse("file:///tmp/page.html").expect("base URL");
        let mut loader = ResourceLoader::with_options(FetchOptions::default());
        let limits = ImageLimits {
            max_image_bytes: 128,
            ..ImageLimits::default()
        };
        let mut warnings = Vec::new();
        let loaded = load_images(&document, &base, &mut loader, false, limits, &mut warnings);
        assert!(
            loaded.failed >= 1,
            "oversized image must fail: {warnings:?}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("too large")),
            "the fetch-layer response cap must reject it: {warnings:?}"
        );
    }

    #[test]
    fn identical_image_urls_share_one_decode() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/render/layout_aspect_ratio.png");
        let fixture = fixture.canonicalize().expect("image fixture");
        let directory = fixture.parent().expect("fixture dir");
        let source = format!(
            "<body><img src='{}'><img src='{}'></body>",
            fixture.display(),
            fixture.display()
        );
        let document = megi_html::parse_document(&source, false);
        let base = megi_net::Url::parse(&format!("file://{}/page.html", directory.display()))
            .expect("base URL");
        let mut loader = ResourceLoader::with_options(FetchOptions::default());
        let loaded = load_images(
            &document,
            &base,
            &mut loader,
            false,
            ImageLimits::default(),
            &mut Vec::new(),
        );
        assert_eq!(loaded.loaded, 2, "both <img> elements resolve");
        assert_eq!(
            loaded.downloaded_bytes,
            std::fs::metadata(&fixture).expect("metadata").len() as usize,
            "the image is downloaded once"
        );
        let bitmaps: Vec<_> = loaded.store.values().collect();
        assert_eq!(bitmaps.len(), 2);
        assert!(
            std::sync::Arc::ptr_eq(bitmaps[0], bitmaps[1]),
            "the decoded bitmap is shared"
        );
    }
}
