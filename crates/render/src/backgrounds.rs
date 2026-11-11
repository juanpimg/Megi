//! # CSS background images & `list-style-image` loading (WP-5.9)
//!
//! After the cascade, [`css::ComputedStyle`] keeps the **raw, unresolved** URL text of
//! `background-image` and `list-style-image` (`Option<String>`). This module:
//!
//! 1. collects those URLs from every styled element, in document order;
//! 2. resolves them against the **document base URL** (see the limitation below),
//!    deduplicating by resolved URL so each bitmap is fetched/decoded once;
//! 3. fetches the bytes with the shared [`net::ResourceLoader`] (honoring
//!    `--no-network`, the HTTP cache, redirects and TLS) under defensive caps:
//!    [`DEFAULT_MAX_BACKGROUNDS`] distinct images, [`DEFAULT_MAX_BACKGROUND_BYTES`] per
//!    image and [`DEFAULT_MAX_TOTAL_BACKGROUND_BYTES`] per page;
//! 4. decodes the bitmap with the in-tree `image` crate — PNG, JPEG, GIF and lossless
//!    WebP (VP8L); VP8 lossy produces a warning and no image — and
//! 5. builds the [`paint::BackgroundImages`] map that
//!    [`paint::build_display_list_with_images_and_backgrounds`] consumes: key = the raw
//!    CSS URL string (so both `background-image` and `list-style-image` share the same
//!    entry), value = `Arc<image::RgbaImage>`.
//!
//! Failures are **non-fatal**: the URL is simply absent from the map, so paint draws the
//! background color (or the fallback `list-style-type` marker) and the render records a
//! warning. The same `Arc` is shared when the same URL is used as a background and as a
//! list marker.
//!
//! ## Base-URL limitation (documented)
//!
//! URLs are resolved against the **document** base URL (`<base href>` or the document
//! URL), not against the stylesheet that declared them: `@import` does not propagate its
//! own base. This matches the current stylesheet loader and is called out in the crate
//! docs.
//!
//! ## Paint policy (owned by `paint`)
//!
//! The bitmap is stretched into the element's padding box (no `background-repeat`,
//! `background-size`, `background-position` or `background-clip` yet); the background
//! color paints first, the image on top, then borders/content. `list-style-image`
//! replaces the default marker when present. Canvas propagation (`html`/`body`) only
//! propagates the color.

use std::collections::HashMap;
use std::sync::Arc;

use css::ComputedStyle;
use dom::{Document, NodeId};
use net::{ResourceLoader, Scheme, Url};

use crate::svg_assets;

/// Default maximum number of distinct background/list-style images per page.
pub const DEFAULT_MAX_BACKGROUNDS: usize = 32;
/// Default maximum downloaded size of one background image (8 MiB).
pub const DEFAULT_MAX_BACKGROUND_BYTES: usize = 8 * 1024 * 1024;
/// Default maximum total downloaded background bytes per page (16 MiB).
pub const DEFAULT_MAX_TOTAL_BACKGROUND_BYTES: usize = 16 * 1024 * 1024;

/// Defensive caps applied while loading background images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackgroundLimits {
    /// Maximum number of distinct (deduplicated) background URLs fetched.
    pub max_backgrounds: usize,
    /// Maximum downloaded bytes for a single background image.
    pub max_background_bytes: usize,
    /// Maximum total downloaded background bytes for the page.
    pub max_total_bytes: usize,
    /// Optional decode downscale cap (`RenderOptions::max_image_dim`).
    pub max_image_dim: Option<u32>,
}

impl Default for BackgroundLimits {
    fn default() -> Self {
        Self {
            max_backgrounds: DEFAULT_MAX_BACKGROUNDS,
            max_background_bytes: DEFAULT_MAX_BACKGROUND_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_BACKGROUND_BYTES,
            max_image_dim: None,
        }
    }
}

/// One `background-image`/`list-style-image` reference found in the computed styles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundReference {
    /// Element carrying the declaration.
    pub node: NodeId,
    /// Raw CSS URL string exactly as stored in [`ComputedStyle`] (the paint map key).
    pub raw_url: String,
    /// Which property produced the reference.
    pub kind: BackgroundKind,
}

/// Property that referenced a background image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundKind {
    /// `background-image` on the element's padding box.
    Background,
    /// `list-style-image` marker for `display: list-item` boxes.
    ListStyleImage,
}

/// Result of the background-loading stage.
pub struct LoadedBackgrounds {
    /// Paint-ready map: raw CSS URL → decoded RGBA bitmap (shared between both kinds).
    pub images: paint::BackgroundImages,
    /// References found in the cascade (before deduplication).
    pub references: usize,
    /// Distinct URLs successfully fetched and decoded.
    pub loaded: usize,
    /// Distinct URLs that failed (invalid, blocked, too large, unsupported format, ...).
    pub failed: usize,
    /// Background images that were SVGs rasterized through the `svg` crate.
    pub svgs_rendered: usize,
    /// Downloaded (encoded) bytes accepted by the caps.
    pub downloaded_bytes: usize,
    /// Non-fatal warnings produced by this stage.
    pub warnings: Vec<String>,
}

impl LoadedBackgrounds {
    /// Returns `true` when no background image is available for paint.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

/// Collects `background-image`/`list-style-image` references in document order.
///
/// Only elements present in `styles` (i.e. styled) are considered. Elements that are not
/// in the live document tree are skipped (styles are keyed by arena handle).
pub fn collect_background_references(
    document: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
) -> Vec<BackgroundReference> {
    let mut references = Vec::new();
    for node_id in document.descendants(document.root()) {
        let Some(style) = styles.get(&node_id) else {
            continue;
        };
        // Multi-layer backgrounds: one reference per layer image (top-first order, which
        // is what paint expects). `background_image` remains the mirror of the first layer
        // for compatibility; it is only used when the layer list has no image at all so a
        // singular declaration is not counted twice.
        let mut mirrored_first_layer = false;
        for (index, layer) in style.background_layers.iter().enumerate() {
            let Some(raw_url) = layer.image.as_deref() else {
                continue;
            };
            if !mirrored_first_layer
                && style.background_image.as_deref() == Some(raw_url)
                && index == 0
            {
                mirrored_first_layer = true;
            }
            references.push(BackgroundReference {
                node: node_id,
                raw_url: raw_url.to_owned(),
                kind: BackgroundKind::Background,
            });
        }
        if !mirrored_first_layer && let Some(raw_url) = style.background_image.as_deref() {
            references.push(BackgroundReference {
                node: node_id,
                raw_url: raw_url.to_owned(),
                kind: BackgroundKind::Background,
            });
        }
        if let Some(raw_url) = style.list_style_image.as_deref() {
            references.push(BackgroundReference {
                node: node_id,
                raw_url: raw_url.to_owned(),
                kind: BackgroundKind::ListStyleImage,
            });
        }
    }
    references
}

/// Loads every referenced background image and builds the [`paint::BackgroundImages`] map.
///
/// `loader` should be the same cached [`net::ResourceLoader`] used for stylesheets/images so
/// repeated URLs share the in-memory cache. `no_network` blocks `http(s)` images with a
/// warning (offline rendering). Warnings are appended to `warnings`.
pub fn load_backgrounds(
    document: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    base_url: &Url,
    loader: &mut ResourceLoader,
    no_network: bool,
    limits: BackgroundLimits,
    warnings: &mut Vec<String>,
) -> LoadedBackgrounds {
    let mut result = LoadedBackgrounds {
        images: paint::BackgroundImages::new(),
        references: 0,
        loaded: 0,
        failed: 0,
        svgs_rendered: 0,
        downloaded_bytes: 0,
        warnings: Vec::new(),
    };
    let references = collect_background_references(document, styles);
    result.references = references.len();
    if references.is_empty() {
        return result;
    }

    // Resolve + deduplicate by resolved URL while remembering every raw CSS spelling that
    // points at it, because paint looks images up by the raw (unresolved) string.
    let mut resolved: HashMap<String, Url> = HashMap::new(); // cache key -> URL
    let mut raw_aliases: HashMap<String, Vec<String>> = HashMap::new(); // cache key -> raw strings
    let mut order: Vec<String> = Vec::new();
    let mut cap_warned = false;
    for reference in &references {
        let url = match base_url.join(&reference.raw_url) {
            Ok(url) => url,
            Err(error) => {
                result.failed += 1;
                result.warnings.push(format!(
                    "skipping background image {:?}: invalid URL ({error})",
                    reference.raw_url
                ));
                continue;
            }
        };
        if no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
            result.failed += 1;
            result.warnings.push(format!(
                "skipping background image {url} (--no-network is set)"
            ));
            continue;
        }
        let cache_key = url.cache_key();
        if !resolved.contains_key(&cache_key) {
            if resolved.len() >= limits.max_backgrounds {
                if !cap_warned {
                    cap_warned = true;
                    result.warnings.push(format!(
                        "page references more than {} background images; loading only the first {} (cap)",
                        limits.max_backgrounds, limits.max_backgrounds
                    ));
                }
                result.failed += 1;
                continue;
            }
            resolved.insert(cache_key.clone(), url);
            order.push(cache_key.clone());
        }
        raw_aliases
            .entry(cache_key)
            .or_default()
            .push(reference.raw_url.clone());
    }

    for cache_key in order {
        let Some(url) = resolved.get(&cache_key) else {
            continue;
        };
        let resource = match loader.load(url) {
            Ok(resource) => resource,
            Err(error) => {
                result.failed += 1;
                result
                    .warnings
                    .push(format!("skipping background image {url}: {error}"));
                continue;
            }
        };
        if resource.bytes.len() > limits.max_background_bytes {
            result.failed += 1;
            result.warnings.push(format!(
                "skipping background image {url}: {} bytes exceeds max_background_bytes={}",
                resource.bytes.len(),
                limits.max_background_bytes
            ));
            continue;
        }
        if result.downloaded_bytes + resource.bytes.len() > limits.max_total_bytes {
            result.failed += 1;
            result.warnings.push(format!(
                "skipping background image {url}: total background bytes would exceed max_total_background_bytes={}",
                limits.max_total_bytes
            ));
            continue;
        }
        let bitmap = if matches!(
            image::detect_format(&resource.bytes),
            image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::Gif
                | image::ImageFormat::WebP
        ) {
            let decoded = match limits.max_image_dim {
                Some(max_dim) => image::decode_scaled(&resource.bytes, max_dim),
                None => image::decode(&resource.bytes),
            };
            match decoded {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    result.failed += 1;
                    if matches!(
                        image::detect_format(&resource.bytes),
                        image::ImageFormat::WebP
                    ) && error.to_string().contains("VP8")
                    {
                        result.warnings.push(format!(
                            "skipping background image {url}: WebP VP8 lossy is experimental ({error})"
                        ));
                    } else {
                        result.warnings.push(format!(
                            "skipping background image {url}: image decode failed ({error})"
                        ));
                    }
                    continue;
                }
            }
        } else if svg_assets::looks_like_svg(&resource.content_type, &resource.bytes) {
            match svg_assets::render(&resource.bytes) {
                Ok(rendered) => {
                    result.svgs_rendered += 1;
                    rendered.raster
                }
                Err(error) => {
                    result.failed += 1;
                    result.warnings.push(format!(
                        "skipping background image {url}: SVG render failed ({error})"
                    ));
                    continue;
                }
            }
        } else {
            let format = image::detect_format(&resource.bytes);
            result.failed += 1;
            result.warnings.push(format!(
                "skipping background image {url}: unsupported format {format:?} (PNG, JPEG, GIF, WebP-lossless and SVG are decoded)"
            ));
            continue;
        };
        result.downloaded_bytes += resource.bytes.len();
        result.loaded += 1;
        let bitmap = Arc::new(bitmap);
        for raw_url in raw_aliases.get(&cache_key).into_iter().flatten() {
            result.images.insert(raw_url.clone(), Arc::clone(&bitmap));
        }
    }
    // Mirror the stage warnings into the shared pipeline warning list as well.
    warnings.extend(result.warnings.iter().cloned());
    result
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use net::FetchOptions;
    use std::time::Duration;

    fn options() -> FetchOptions {
        FetchOptions {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..FetchOptions::default()
        }
    }

    fn styled_divs(
        backgrounds: &[&str],
        list_images: &[&str],
    ) -> (Document, HashMap<NodeId, ComputedStyle>) {
        let mut doc = Document::new();
        let body = doc.create_element(dom::Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), body);
        let mut styles = HashMap::new();
        for raw_url in backgrounds {
            let node = doc.create_element(dom::Namespace::Html, "div", vec![]);
            doc.append_child(body, node);
            let mut style = ComputedStyle::initial();
            style.background_image = Some((*raw_url).to_owned());
            styles.insert(node, style);
        }
        for raw_url in list_images {
            let node = doc.create_element(dom::Namespace::Html, "ul", vec![]);
            doc.append_child(body, node);
            let mut style = ComputedStyle::initial();
            style.list_style_image = Some((*raw_url).to_owned());
            styles.insert(node, style);
        }
        (doc, styles)
    }

    #[test]
    fn collects_references_in_document_order() {
        let (doc, styles) = styled_divs(&["a.png"], &["b.png"]);
        let references = collect_background_references(&doc, &styles);
        assert_eq!(references.len(), 2);
        assert_eq!(references[0].kind, BackgroundKind::Background);
        assert_eq!(references[0].raw_url, "a.png");
        assert_eq!(references[1].kind, BackgroundKind::ListStyleImage);
        assert_eq!(references[1].raw_url, "b.png");
        assert!(references[0].node.index() < references[1].node.index());
    }

    #[test]
    fn invalid_and_blocked_urls_are_non_fatal() {
        let (doc, styles) = styled_divs(
            &["http://exa mple.com/bg.png", "http://127.0.0.1:9/a.png"],
            &[],
        );
        let base = Url::parse("file:///tmp/index.html").expect("file URL");
        let mut loader = ResourceLoader::with_options(options());
        let mut warnings = Vec::new();
        let loaded = load_backgrounds(
            &doc,
            &styles,
            &base,
            &mut loader,
            true, // --no-network: the http URL is skipped without connecting
            BackgroundLimits::default(),
            &mut warnings,
        );
        assert_eq!(loaded.references, 2);
        assert_eq!(loaded.loaded, 0);
        assert_eq!(loaded.failed, 2);
        assert_eq!(loaded.images.len(), 0);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("--no-network"))
        );
    }

    #[test]
    fn background_cap_counts_distinct_urls() {
        let (doc, styles) = styled_divs(&["a.png", "b.png", "c.png"], &[]);
        // Unique base directory so unrelated files in /tmp (from other tools or parallel
        // test runs) cannot make the "file not found" assertions flaky.
        let base = Url::parse(&format!(
            "file:///tmp/megi-render-backgrounds-cap-{}/index.html",
            std::process::id()
        ))
        .expect("file URL");
        let mut loader = ResourceLoader::with_options(options());
        let mut warnings = Vec::new();
        let limits = BackgroundLimits {
            max_backgrounds: 2,
            ..BackgroundLimits::default()
        };
        let loaded = load_backgrounds(
            &doc,
            &styles,
            &base,
            &mut loader,
            false,
            limits,
            &mut warnings,
        );
        // The third distinct URL is refused before any I/O; the first two fail with
        // file-not-found (still counted as failed attempts).
        assert_eq!(loaded.references, 3);
        assert_eq!(loaded.loaded, 0);
        assert_eq!(loaded.failed, 3);
        assert!(warnings.iter().any(|warning| warning.contains("(cap)")));
    }

    #[test]
    fn missing_files_are_warnings_not_errors() {
        let (doc, styles) = styled_divs(&["does-not-exist.png"], &[]);
        let base =
            Url::parse("file:///tmp/megi-render-backgrounds-test/index.html").expect("file URL");
        let mut loader = ResourceLoader::with_options(options());
        let mut warnings = Vec::new();
        let loaded = load_backgrounds(
            &doc,
            &styles,
            &base,
            &mut loader,
            false,
            BackgroundLimits::default(),
            &mut warnings,
        );
        assert_eq!(loaded.loaded, 0);
        assert_eq!(loaded.failed, 1);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("skipping background image"))
        );
    }

    #[test]
    fn empty_styles_produce_empty_map() {
        let doc = Document::new();
        let styles = HashMap::new();
        let base = Url::parse("file:///tmp/index.html").expect("file URL");
        let mut loader = ResourceLoader::with_options(options());
        let mut warnings = Vec::new();
        let loaded = load_backgrounds(
            &doc,
            &styles,
            &base,
            &mut loader,
            false,
            BackgroundLimits::default(),
            &mut warnings,
        );
        assert!(loaded.is_empty());
        assert_eq!(loaded.references, 0);
        assert!(warnings.is_empty());
    }
}
