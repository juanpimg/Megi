//! Fetching, decoding and measuring every `<img>` (the load stage).

use std::collections::HashMap;
use std::sync::Arc;

use megi_core::Document;
use megi_net::{ResourceLoader, Scheme, Url};

use crate::svg_assets;

use super::sources::collect_responsive_image_sources;
use super::types::{ImageLimits, LoadedImages};

/// Loads, decodes and measures every `<img>` of the document with default media context.
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
    load_images_with_context(
        document,
        base_url,
        loader,
        no_network,
        &megi_css::MediaContext::default(),
        limits,
        None,
        warnings,
    )
}

/// Like [`load_images`] but enforces CSP (`img-src`, #1526) and resolves responsive images
/// (`<picture>` / `srcset` / `sizes`) using `media` and `limits.device_pixel_ratio` (#1543).
#[allow(clippy::too_many_arguments)]
pub fn load_images_with_context(
    document: &Document,
    base_url: &Url,
    loader: &mut ResourceLoader,
    no_network: bool,
    media: &megi_css::MediaContext,
    limits: ImageLimits,
    csp: Option<&megi_net::csp::ContentSecurityPolicy>,
    warnings: &mut Vec<String>,
) -> LoadedImages {
    let mut result = LoadedImages {
        store: megi_paint::ImageStore::new(),
        sizes: HashMap::new(),
        loaded: 0,
        failed: 0,
        downloaded_bytes: 0,
        decoded_pixels: 0,
        svgs_rendered: 0,
        warnings: Vec::new(),
    };
    // Identical URLs are fetched and decoded once: the decoded bitmap is shared through an
    // `Arc` across every `<img>` that references it (backgrounds already deduplicate this
    // way). The raw `(intrinsic_size, scale_with_density)` tuple lets each `<img srcset>`
    // apply its own selected density divisor while still sharing the bitmap buffer.
    type CachedImageEntry = ((f32, f32), bool, Arc<megi_image::RgbaImage>);
    let mut cache: HashMap<String, CachedImageEntry> = HashMap::new();
    let sources = collect_responsive_image_sources(document, media, limits.device_pixel_ratio);
    if sources.len() > limits.max_images {
        warnings.push(format!(
            "page references {} images; loading only the first {} (cap)",
            sources.len(),
            limits.max_images
        ));
    }

    let mut resolved = Vec::new();
    for (node_id, src, density) in sources.into_iter().take(limits.max_images) {
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
        if let Some(policy) = csp
            && !policy.allows_image(&url, base_url)
        {
            result.failed += 1;
            result.warnings.push(format!(
                "skipping image {url}: blocked by Content-Security-Policy"
            ));
            continue;
        }
        if no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
            result.failed += 1;
            result
                .warnings
                .push(format!("skipping image {url} (--no-network is set)"));
            continue;
        }
        let use_cors = document
            .node(node_id)
            .as_element()
            .is_some_and(|element| element.get_attribute("crossorigin").is_some());
        resolved.push((node_id, url, use_cors, density));
    }

    let requests: Vec<(&Url, bool, bool)> = resolved
        .iter()
        .map(|(_, url, use_cors, _)| (url, *use_cors, false))
        .collect();
    // Enforce the image byte cap while fetching, not after every response has been
    // buffered (#1213). Restore the caller's cap afterwards so unrelated later fetches
    // keep their original limit.
    let previous_cap = loader.set_max_response_bytes(limits.max_image_bytes);
    let loaded_resources = loader.load_many_checked(&requests);
    loader.set_max_response_bytes(previous_cap);

    let css_size = |raw: (f32, f32), scale_density: bool, density: f32| -> (f32, f32) {
        if scale_density && density.is_finite() && density > 0.0 {
            ((raw.0 / density).max(1.0), (raw.1 / density).max(1.0))
        } else {
            raw
        }
    };

    for ((node_id, url, _use_cors, density), loaded_res) in
        resolved.into_iter().zip(loaded_resources)
    {
        let cache_key = url.to_string();
        if let Some((raw_size, scale_density, bitmap)) = cache.get(&cache_key) {
            result
                .sizes
                .insert(node_id, css_size(*raw_size, *scale_density, density));
            result.store.insert(node_id, Arc::clone(bitmap));
            result.loaded += 1;
            continue;
        }
        let resource = match loaded_res {
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
            // #1666: count every skipped image as failed instead of abandoning the loop, so
            // telemetry reflects all images the page referenced.
            result.failed += 1;
            result.warnings.push(format!(
                "image byte budget exhausted at {url} ({} bytes total, max {})",
                result.downloaded_bytes + resource.bytes.len(),
                limits.max_total_bytes
            ));
            continue;
        }
        if matches!(
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
                Ok(decoded) => {
                    let (width, height) = decoded.dimensions();
                    let pixels = u64::from(width).saturating_mul(u64::from(height));
                    if result.decoded_pixels.saturating_add(pixels)
                        > limits.max_total_decoded_pixels
                    {
                        result.failed += 1;
                        result.warnings.push(format!(
                            "skipping image {url}: decoded pixel budget exhausted ({} + {} > {} pixels)",
                            result.decoded_pixels, pixels, limits.max_total_decoded_pixels
                        ));
                        continue;
                    }
                    result.decoded_pixels = result.decoded_pixels.saturating_add(pixels);
                    result.downloaded_bytes += resource.bytes.len();
                    let size = (width as f32, height as f32);
                    let bitmap = Arc::new(decoded);
                    result.sizes.insert(node_id, css_size(size, true, density));
                    result.store.insert(node_id, Arc::clone(&bitmap));
                    cache.insert(cache_key.clone(), (size, true, bitmap));
                    result.loaded += 1;
                }
                Err(error) => {
                    result.failed += 1;
                    let message = format!("skipping image {url}: {error}");
                    // Broken/unsupported VP8 lossy payloads (e.g. VP8X containers or a
                    // truncated keyframe) keep an explicit WebP-lossy warning.
                    if matches!(
                        megi_image::detect_format(&resource.bytes),
                        megi_image::ImageFormat::WebP
                    ) && error.to_string().contains("VP8")
                    {
                        result.warnings.push(format!(
                            "skipping image {url}: invalid WebP VP8 lossy payload ({error})"
                        ));
                    } else {
                        result.warnings.push(message);
                    }
                }
            }
        } else if svg_assets::looks_like_svg(&resource.content_type, &resource.bytes) {
            match svg_assets::render(&resource.bytes) {
                Ok(rendered) => {
                    let (width, height) = rendered.raster.dimensions();
                    let pixels = u64::from(width).saturating_mul(u64::from(height));
                    if result.decoded_pixels.saturating_add(pixels)
                        > limits.max_total_decoded_pixels
                    {
                        result.failed += 1;
                        result.warnings.push(format!(
                            "skipping image {url}: decoded pixel budget exhausted ({} + {} > {} pixels)",
                            result.decoded_pixels, pixels, limits.max_total_decoded_pixels
                        ));
                        continue;
                    }
                    result.decoded_pixels = result.decoded_pixels.saturating_add(pixels);
                    result.downloaded_bytes += resource.bytes.len();
                    let bitmap = Arc::new(rendered.raster);
                    result.sizes.insert(node_id, rendered.intrinsic);
                    result.store.insert(node_id, Arc::clone(&bitmap));
                    cache.insert(cache_key.clone(), (rendered.intrinsic, false, bitmap));
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
            let format = megi_image::detect_format(&resource.bytes);
            result.failed += 1;
            result.warnings.push(format!(
                "skipping image {url}: unsupported format {format:?} (PNG, JPEG, GIF, WebP, AVIF and SVG are decoded)"
            ));
        }
    }

    warnings.append(&mut result.warnings);
    result
}
