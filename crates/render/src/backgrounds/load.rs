//! Fetching and decoding referenced background images into the paint map.

use std::collections::HashMap;
use std::sync::Arc;

use megi_core::{Document, NodeId};
use megi_css::ComputedStyle;
use megi_net::{ResourceLoader, Scheme, Url};

use crate::svg_assets;

use super::collect::{collect_background_references, push_style_background_references};
use super::types::{BackgroundLimits, LoadedBackgrounds};

/// Loads every referenced background image and builds the [`megi_paint::BackgroundImages`] map.
///
/// `loader` should be the same cached [`megi_net::ResourceLoader`] used for stylesheets/images so
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
    let empty = megi_css::PseudoStyles::new();
    load_backgrounds_with_pseudos(
        document, styles, &empty, base_url, loader, no_network, limits, warnings,
    )
}

/// Loads background/list/content images referenced on elements and `::before`/`::after` pseudo-elements.
#[allow(clippy::too_many_arguments)]
pub fn load_backgrounds_with_pseudos(
    document: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    pseudo_styles: &megi_css::PseudoStyles,
    base_url: &Url,
    loader: &mut ResourceLoader,
    no_network: bool,
    limits: BackgroundLimits,
    warnings: &mut Vec<String>,
) -> LoadedBackgrounds {
    let mut result = LoadedBackgrounds {
        images: megi_paint::BackgroundImages::new(),
        references: 0,
        loaded: 0,
        failed: 0,
        svgs_rendered: 0,
        downloaded_bytes: 0,
        decoded_pixels: 0,
        warnings: Vec::new(),
    };
    let has_style_image = |style: &ComputedStyle, include_content: bool| {
        style.background_image.is_some()
            || style.list_style_image.is_some()
            || style
                .background_layers
                .iter()
                .any(|layer| layer.image.is_some() || layer.mask_image.is_some())
            || (include_content && style.content.image_url().is_some())
    };
    let has_any_image = styles.values().any(|s| has_style_image(s, false))
        || pseudo_styles.values().any(|s| has_style_image(s, true));
    if !has_any_image {
        return result;
    }
    let mut references = collect_background_references(document, styles);
    for (&(node_id, _pseudo), style) in pseudo_styles {
        push_style_background_references(node_id, style, true, &mut references);
    }
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
        let resource = match loader.load_no_cors_checked(url, false) {
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
        let mut was_svg = false;
        let bitmap = if matches!(
            megi_image::detect_format(&resource.bytes),
            megi_image::ImageFormat::Png
                | megi_image::ImageFormat::Jpeg
                | megi_image::ImageFormat::Gif
                | megi_image::ImageFormat::WebP
                | megi_image::ImageFormat::Avif
        ) {
            let decoded = match limits.max_image_dim {
                Some(max_dim) => megi_image::decode_scaled(&resource.bytes, max_dim),
                None => megi_image::decode(&resource.bytes),
            };
            match decoded {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    result.failed += 1;
                    if matches!(
                        megi_image::detect_format(&resource.bytes),
                        megi_image::ImageFormat::WebP
                    ) && error.to_string().contains("VP8")
                    {
                        result.warnings.push(format!(
                            "skipping background image {url}: invalid WebP VP8 lossy payload ({error})"
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
                    was_svg = true;
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
            let format = megi_image::detect_format(&resource.bytes);
            result.failed += 1;
            result.warnings.push(format!(
                "skipping background image {url}: unsupported format {format:?} (PNG, JPEG, GIF, WebP, AVIF and SVG are decoded)"
            ));
            continue;
        };
        let (width, height) = bitmap.dimensions();
        let pixels = u64::from(width).saturating_mul(u64::from(height));
        if result.decoded_pixels.saturating_add(pixels) > limits.max_total_decoded_pixels {
            result.failed += 1;
            result.warnings.push(format!(
                "skipping background image {url}: decoded pixel budget exhausted ({} + {} > {} pixels)",
                result.decoded_pixels, pixels, limits.max_total_decoded_pixels
            ));
            continue;
        }
        result.decoded_pixels = result.decoded_pixels.saturating_add(pixels);
        if was_svg {
            result.svgs_rendered += 1;
        }
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
