//! # `<img>` discovery, loading, decoding and intrinsic sizes (WP-5.7)
//!
//! `render` resolves every `<img src>` against the document base URL, fetches it
//! with the shared [`net::ResourceLoader`] (HTTP cache, redirects, TLS, caps and
//! `--no-network`) and decodes it with the dependency-free `image` crate.
//!
//! ## Policy (mirrors the stylesheet loader)
//!
//! - Missing/blocked/failed subresources are **non-fatal**: the render yields a
//!   gray placeholder box (painted by `paint::IMAGE_PLACEHOLDER`) and a warning.
//! - PNG, baseline/progressive JPEG, GIF (first frame) and **lossless WebP (VP8L)** are
//!   decoded with the in-tree `image` crate; VP8 *lossy* WebP and unsupported JPEG
//!   variants produce a warning and a placeholder, never an error.
//! - Defensive caps, configurable through [`crate::RenderOptions`]:
//!   [`DEFAULT_MAX_IMAGES`] images, [`DEFAULT_MAX_IMAGE_BYTES`] per image and
//!   [`DEFAULT_MAX_TOTAL_IMAGE_BYTES`] of downloaded image bytes per page.
//! - Remote images require network access: under `--no-network`, `http(s)`
//!   images are skipped with a warning (offline tests use `file:`/`data:`).
//!
//! The decoded RGBA pixels and the intrinsic `(width, height)` sizes are handed
//! to `paint` ([`paint::ImageStore`]) and `layout`
//! (`layout_document_with_measurer_and_replaced_sizes`), respectively.

use std::collections::HashMap;
use std::sync::Arc;

use dom::{Document, NodeId};
use net::{ResourceLoader, Scheme, Url};

use crate::svg_assets;

/// Default maximum number of `<img>` elements loaded per page.
pub const DEFAULT_MAX_IMAGES: usize = 64;
/// Default maximum downloaded size of one image (8 MiB).
pub const DEFAULT_MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Default maximum total downloaded image bytes per page (32 MiB).
pub const DEFAULT_MAX_TOTAL_IMAGE_BYTES: usize = 32 * 1024 * 1024;

/// Defensive caps applied while loading images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageLimits {
    /// Maximum number of `<img>` elements fetched.
    pub max_images: usize,
    /// Maximum downloaded bytes for a single image.
    pub max_image_bytes: usize,
    /// Maximum total downloaded image bytes for the page.
    pub max_total_bytes: usize,
    /// Optional decode downscale cap (`RenderOptions::max_image_dim`).
    pub max_image_dim: Option<u32>,
}

impl Default for ImageLimits {
    fn default() -> Self {
        Self {
            max_images: DEFAULT_MAX_IMAGES,
            max_image_bytes: DEFAULT_MAX_IMAGE_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_IMAGE_BYTES,
            max_image_dim: None,
        }
    }
}

/// Result of the image-loading stage.
pub struct LoadedImages {
    /// Decoded images keyed by their `<img>` DOM node.
    pub store: paint::ImageStore,
    /// Intrinsic `(width, height)` in CSS px, ready for layout's replaced sizes.
    pub sizes: HashMap<NodeId, (f32, f32)>,
    /// Images successfully fetched and decoded.
    pub loaded: usize,
    /// Images that failed (fetch error, too large, unsupported format, ...).
    pub failed: usize,
    /// Downloaded (compressed) image bytes actually accepted.
    pub downloaded_bytes: usize,
    /// `<img>` images that were SVGs rasterized through the `svg` crate.
    pub svgs_rendered: usize,
    /// Non-fatal warnings produced by this stage.
    pub warnings: Vec<String>,
}

/// Collects `(NodeId, src)` pairs for every `<img src>` element, in tree order.
pub fn collect_image_sources(document: &Document) -> Vec<(NodeId, String)> {
    let mut sources = Vec::new();
    let mut stack = vec![document.root()];
    while let Some(node_id) = stack.pop() {
        let node = document.node(node_id);
        if let Some(element) = node.as_element()
            && element.local_name.eq_ignore_ascii_case("img")
            && let Some(src) = element
                .get_attribute("src")
                .map(str::trim)
                .filter(|src| !src.is_empty())
        {
            sources.push((node_id, src.to_string()));
        }
        // Push children reversed so the stack pops them in document order.
        let children: Vec<NodeId> = document.children(node_id).collect();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    sources
}

/// Loads, decodes and measures every `<img>` of the document.
///
/// Never fails as a whole: individual problems become warnings/placeholders.
pub fn load_images(
    document: &Document,
    base_url: &Url,
    loader: &mut ResourceLoader,
    no_network: bool,
    limits: ImageLimits,
    warnings: &mut Vec<String>,
) -> LoadedImages {
    let mut result = LoadedImages {
        store: paint::ImageStore::new(),
        sizes: HashMap::new(),
        loaded: 0,
        failed: 0,
        downloaded_bytes: 0,
        svgs_rendered: 0,
        warnings: Vec::new(),
    };
    let sources = collect_image_sources(document);
    if sources.len() > limits.max_images {
        warnings.push(format!(
            "page references {} images; loading only the first {} (cap)",
            sources.len(),
            limits.max_images
        ));
    }

    for (node_id, src) in sources.into_iter().take(limits.max_images) {
        let url = match base_url.join(&src) {
            Ok(url) => url,
            Err(error) => {
                result.failed += 1;
                result
                    .warnings
                    .push(format!("skipping image {src:?}: invalid URL ({error})"));
                continue;
            }
        };
        if no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
            result.failed += 1;
            result
                .warnings
                .push(format!("skipping image {url} (--no-network is set)"));
            continue;
        }
        let resource = match loader.load(&url) {
            Ok(resource) => resource,
            Err(error) => {
                result.failed += 1;
                result
                    .warnings
                    .push(format!("skipping image {url}: {error}"));
                continue;
            }
        };
        if resource.bytes.len() > limits.max_image_bytes {
            result.failed += 1;
            result.warnings.push(format!(
                "skipping image {url}: {} bytes exceeds max_image_bytes ({})",
                resource.bytes.len(),
                limits.max_image_bytes
            ));
            continue;
        }
        if result.downloaded_bytes + resource.bytes.len() > limits.max_total_bytes {
            result.warnings.push(format!(
                "image byte budget exhausted at {url} ({} bytes total, max {})",
                result.downloaded_bytes + resource.bytes.len(),
                limits.max_total_bytes
            ));
            break;
        }
        if matches!(
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
                Ok(decoded) => {
                    result.downloaded_bytes += resource.bytes.len();
                    let (width, height) = decoded.dimensions();
                    result.sizes.insert(node_id, (width as f32, height as f32));
                    result.store.insert(node_id, Arc::new(decoded));
                    result.loaded += 1;
                }
                Err(error) => {
                    result.failed += 1;
                    result
                        .warnings
                        .push(format!("skipping image {url}: {error}"));
                }
            }
        } else if svg_assets::looks_like_svg(&resource.content_type, &resource.bytes) {
            match svg_assets::render(&resource.bytes) {
                Ok(rendered) => {
                    result.downloaded_bytes += resource.bytes.len();
                    result.sizes.insert(node_id, rendered.intrinsic);
                    result.store.insert(node_id, Arc::new(rendered.raster));
                    result.loaded += 1;
                    result.svgs_rendered += 1;
                }
                Err(error) => {
                    result.failed += 1;
                    result
                        .warnings
                        .push(format!("skipping image {url}: SVG render failed ({error})"));
                }
            }
        } else {
            let format = image::detect_format(&resource.bytes);
            result.failed += 1;
            result.warnings.push(format!(
                "skipping image {url}: unsupported format {format:?} (PNG, JPEG, GIF, WebP-lossless and SVG are decoded)"
            ));
        }
    }

    warnings.append(&mut result.warnings);
    result
}
