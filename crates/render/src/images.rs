//! # `<img>` discovery, loading, decoding and intrinsic sizes (WP-5.7)
//!
//! `render` resolves every `<img src>` against the document base URL, fetches it
//! with the shared [`megi_net::ResourceLoader`] (HTTP cache, redirects, TLS, caps and
//! `--no-network`) and decodes it with the dependency-free `image` crate.
//!
//! ## Policy (mirrors the stylesheet loader)
//!
//! - Missing/blocked/failed subresources are **non-fatal**: the render yields a
//!   gray placeholder box (painted by `megi_paint::IMAGE_PLACEHOLDER`) and a warning.
//! - PNG, baseline/progressive JPEG, GIF (first frame), **lossless WebP (VP8L)** and
//!   **lossy WebP (VP8 keyframe)** are decoded with the in-tree `image` crate;
//!   unsupported JPEG variants and corrupt/unsupported WebP payloads produce a
//!   non-fatal warning and a placeholder, never a hard error.
//! - Defensive caps, configurable through [`crate::RenderOptions`]:
//!   [`DEFAULT_MAX_IMAGES`] images, [`DEFAULT_MAX_IMAGE_BYTES`] per image and
//!   [`DEFAULT_MAX_TOTAL_IMAGE_BYTES`] of downloaded image bytes per page.
//! - Remote images require network access: under `--no-network`, `http(s)`
//!   images are skipped with a warning (offline tests use `file:`/`data:`).
//!
//! The decoded RGBA pixels and the intrinsic `(width, height)` sizes are handed
//! to `paint` ([`megi_paint::ImageStore`]) and `layout`
//! (`layout_document_with_measurer_and_replaced_sizes`), respectively.

use std::collections::HashMap;
use std::sync::Arc;

use megi_core::{Document, NodeId};
use megi_net::{ResourceLoader, Scheme, Url};

use crate::svg_assets;

/// Default maximum number of `<img>` elements loaded per page.
pub const DEFAULT_MAX_IMAGES: usize = 64;
/// Default maximum downloaded size of one image (8 MiB).
pub const DEFAULT_MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Default maximum total downloaded image bytes per page (32 MiB).
pub const DEFAULT_MAX_TOTAL_IMAGE_BYTES: usize = 32 * 1024 * 1024;
/// Default maximum total decoded pixels across `<img>` and CSS background images (64 Mpx ≈ 256 MiB RGBA).
pub const DEFAULT_MAX_TOTAL_DECODED_PIXELS: u64 = 64 * 1024 * 1024;

/// Defensive caps and display metrics applied while loading images.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageLimits {
    /// Maximum number of `<img>` elements fetched.
    pub max_images: usize,
    /// Maximum downloaded bytes for a single image.
    pub max_image_bytes: usize,
    /// Maximum total downloaded image bytes for the page.
    pub max_total_bytes: usize,
    /// Maximum total decoded pixels across all distinct `<img>` bitmaps.
    pub max_total_decoded_pixels: u64,
    /// Optional decode downscale cap (`RenderOptions::max_image_dim`).
    pub max_image_dim: Option<u32>,
    /// Device pixel ratio used when selecting `srcset` / `<picture>` candidates (default `1.0`).
    pub device_pixel_ratio: f32,
}

impl Default for ImageLimits {
    fn default() -> Self {
        Self {
            max_images: DEFAULT_MAX_IMAGES,
            max_image_bytes: DEFAULT_MAX_IMAGE_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_IMAGE_BYTES,
            max_total_decoded_pixels: DEFAULT_MAX_TOTAL_DECODED_PIXELS,
            max_image_dim: None,
            device_pixel_ratio: 1.0,
        }
    }
}

/// Result of the image-loading stage.
pub struct LoadedImages {
    /// Decoded images keyed by their `<img>` DOM node.
    pub store: megi_paint::ImageStore,
    /// Intrinsic `(width, height)` in CSS px, ready for layout's replaced sizes.
    pub sizes: HashMap<NodeId, (f32, f32)>,
    /// Images successfully fetched and decoded.
    pub loaded: usize,
    /// Images that failed (fetch error, too large, unsupported format, ...).
    pub failed: usize,
    /// Downloaded (compressed) image bytes actually accepted.
    pub downloaded_bytes: usize,
    /// Total decoded pixels (`width * height`) across distinct decoded bitmaps.
    pub decoded_pixels: u64,
    /// `<img>` images that were SVGs rasterized through the `svg` crate.
    pub svgs_rendered: usize,
    /// Non-fatal warnings produced by this stage.
    pub warnings: Vec<String>,
}

/// Parsed descriptor on a single `srcset` image candidate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SrcsetDescriptor {
    /// Pixel density multiplier (`1x`, `2x`, or default `1x`).
    Density(f32),
    /// Physical width descriptor (`400w`, `800w`, ...).
    Width(u32),
}

/// One parsed candidate entry from an HTML `srcset` attribute.
#[derive(Clone, Debug, PartialEq)]
pub struct SrcsetCandidate {
    /// Raw (unjoined) URL string for the candidate.
    pub url: String,
    /// Parsed `w` or `x` descriptor.
    pub descriptor: SrcsetDescriptor,
}

/// Sanitizes device pixel ratio (finite and positive; falls back to `1.0`).
pub fn sanitize_dpr(dpr: f32) -> f32 {
    if dpr.is_finite() && dpr > 0.0 {
        dpr
    } else {
        1.0
    }
}

/// Parses an HTML `srcset` microsyntax string (WHATWG HTML §4.8.4.2).
///
/// Supports commas inside `data:` URLs cleanly by splitting URL tokens on ASCII
/// whitespace (stripping trailing `,` directly attached to a descriptor-less URL)
/// and scanning descriptor lists up to the next top-level comma.
/// Rejects malformed descriptors and keeps the candidate set homogeneous
/// (cannot mix `w` and `x` / default descriptors).
pub fn parse_srcset(input: &str) -> Vec<SrcsetCandidate> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;
    let mut raw_entries: Vec<(String, Option<SrcsetDescriptor>)> = Vec::new();

    while pos < len {
        // 1. Skip leading ASCII whitespace and commas.
        while pos < len && (bytes[pos].is_ascii_whitespace() || bytes[pos] == b',') {
            pos += 1;
        }
        if pos >= len {
            break;
        }

        // 2. Scan candidate URL (non-ASCII-whitespace).
        let url_start = pos;
        while pos < len && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let raw_url = &input[url_start..pos];

        // Per HTML §4.8.4.2: if the URL ends with `,`, trailing commas delimit the
        // candidate and there are zero descriptors.
        if raw_url.ends_with(',') {
            let trimmed_url = raw_url.trim_end_matches(',');
            if !trimmed_url.is_empty() {
                raw_entries.push((trimmed_url.to_string(), None));
            }
            continue;
        }

        // Skip whitespace before descriptors.
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }

        // 3. Scan descriptor region up to next top-level comma (respecting parens).
        let desc_start = pos;
        let mut depth = 0usize;
        while pos < len {
            let b = bytes[pos];
            if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth = depth.saturating_sub(1);
            } else if b == b',' && depth == 0 {
                break;
            }
            pos += 1;
        }
        let desc_str = input[desc_start..pos].trim();
        if pos < len && bytes[pos] == b',' {
            pos += 1;
        }

        if desc_str.is_empty() {
            raw_entries.push((raw_url.to_string(), None));
            continue;
        }

        // HTML spec allows at most one width (`w`) or pixel density (`x`) descriptor token.
        let tokens: Vec<&str> = desc_str.split_ascii_whitespace().collect();
        if tokens.len() != 1 {
            continue;
        }
        let tok = tokens[0];
        if let Some(num) = tok.strip_suffix(['w', 'W']) {
            if let Ok(width) = num.parse::<u32>()
                && width > 0
                && !num.starts_with('+')
            {
                raw_entries.push((raw_url.to_string(), Some(SrcsetDescriptor::Width(width))));
            }
        } else if let Some(num) = tok.strip_suffix(['x', 'X'])
            && let Ok(density) = num.parse::<f32>()
            && density.is_finite()
            && density > 0.0
        {
            raw_entries.push((
                raw_url.to_string(),
                Some(SrcsetDescriptor::Density(density)),
            ));
        }
    }

    if raw_entries.is_empty() {
        return Vec::new();
    }

    // Spec §4.8.4.2.1: if any valid candidate specifies a width descriptor, all valid
    // candidates must specify a width descriptor (mixed x/omitted descriptors are dropped).
    let uses_width = raw_entries
        .iter()
        .any(|(_, d)| matches!(d, Some(SrcsetDescriptor::Width(_))));

    let mut out = Vec::with_capacity(raw_entries.len());
    for (url, desc) in raw_entries {
        if uses_width {
            if let Some(SrcsetDescriptor::Width(w)) = desc
                && !out.iter().any(|c: &SrcsetCandidate| {
                    matches!(c.descriptor, SrcsetDescriptor::Width(prev) if prev == w)
                })
            {
                out.push(SrcsetCandidate {
                    url,
                    descriptor: SrcsetDescriptor::Width(w),
                });
            }
        } else {
            let density = match desc {
                Some(SrcsetDescriptor::Density(d)) => d,
                None => 1.0,
                Some(SrcsetDescriptor::Width(_)) => continue,
            };
            if !out.iter().any(|c: &SrcsetCandidate| {
                matches!(c.descriptor, SrcsetDescriptor::Density(prev) if (prev - density).abs() < 1e-4)
            }) {
                out.push(SrcsetCandidate {
                    url,
                    descriptor: SrcsetDescriptor::Density(density),
                });
            }
        }
    }
    out
}

/// Parses and evaluates a `<source-size-value>` CSS length against the viewport metrics.
fn parse_size_length_px(token: &str, media: &megi_css::MediaContext) -> Option<f32> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (vw, vh) = media.viewport.unwrap_or((800.0, 600.0));
    let lower = trimmed.to_ascii_lowercase();
    let parse_unit = |suffix: &str, scale: f32| -> Option<f32> {
        let num_str = lower.strip_suffix(suffix)?.trim();
        let val = num_str.parse::<f32>().ok()?;
        if val.is_finite() && val >= 0.0 {
            Some(val * scale)
        } else {
            None
        }
    };
    if lower == "0" {
        return Some(0.0);
    }
    parse_unit("px", 1.0)
        .or_else(|| parse_unit("vw", vw / 100.0))
        .or_else(|| parse_unit("vh", vh / 100.0))
        .or_else(|| parse_unit("vmin", vw.min(vh) / 100.0))
        .or_else(|| parse_unit("vmax", vw.max(vh) / 100.0))
        .or_else(|| parse_unit("em", 16.0))
        .or_else(|| parse_unit("rem", 16.0))
        .or_else(|| parse_unit("pt", 96.0 / 72.0))
        .or_else(|| parse_unit("pc", 16.0))
        .or_else(|| parse_unit("in", 96.0))
        .or_else(|| parse_unit("cm", 96.0 / 2.54))
        .or_else(|| parse_unit("mm", 96.0 / 25.4))
}

/// Evaluates an HTML `sizes` attribute against `media`, returning the target CSS width in px.
///
/// Falls back to the viewport width (`100vw`, default `800px` when viewport is unknown).
pub fn resolve_sizes(sizes_attr: Option<&str>, media: &megi_css::MediaContext) -> f32 {
    let default_width = media.viewport.map_or(800.0, |(w, _)| w.max(1.0));
    let Some(raw) = sizes_attr.map(str::trim).filter(|s| !s.is_empty()) else {
        return default_width;
    };

    // Split on top-level commas (outside parentheses).
    let mut entries = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (idx, ch) in raw.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                entries.push(raw[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }
    entries.push(raw[start..].trim());

    for entry in entries {
        if entry.is_empty() {
            continue;
        }
        // Separate optional `<media-condition>` from trailing `<source-size-value>` at the
        // last top-level ASCII whitespace outside parentheses.
        let mut split_at = None;
        let mut p_depth = 0usize;
        for (idx, ch) in entry.char_indices() {
            match ch {
                '(' => p_depth += 1,
                ')' => p_depth = p_depth.saturating_sub(1),
                c if c.is_ascii_whitespace() && p_depth == 0 => {
                    split_at = Some(idx);
                }
                _ => {}
            }
        }
        let (cond_opt, len_str) = match split_at {
            Some(idx) => {
                let cond = entry[..idx].trim();
                let len = entry[idx..].trim();
                (if cond.is_empty() { None } else { Some(cond) }, len)
            }
            None => (None, entry),
        };
        let Some(px) = parse_size_length_px(len_str, media) else {
            continue;
        };
        if let Some(cond) = cond_opt {
            let list = megi_css::media::parse_media_query_list(cond);
            if !megi_css::media::evaluate(&list, media) {
                continue;
            }
        }
        return px;
    }

    default_width
}

/// Returns whether a `<source type="...">` MIME type is decodable by the engine.
pub fn is_supported_image_mime(mime: &str) -> bool {
    let essential = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if essential.is_empty() {
        return true;
    }
    matches!(
        essential.as_str(),
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp" | "image/svg+xml"
    )
}

/// Selects the best image `(url, effective_density)` from `srcset` / `src` / `sizes`.
pub fn select_image_candidate(
    srcset: Option<&str>,
    fallback_src: Option<&str>,
    sizes_attr: Option<&str>,
    media: &megi_css::MediaContext,
    dpr: f32,
) -> Option<(String, f32)> {
    let dpr = sanitize_dpr(dpr);
    let mut candidates = srcset.map(parse_srcset).unwrap_or_default();

    let fallback = fallback_src.map(str::trim).filter(|s| !s.is_empty());
    let uses_width = candidates
        .iter()
        .any(|c| matches!(c.descriptor, SrcsetDescriptor::Width(_)));

    // When using density descriptors (or empty srcset), include `<img src>` as `1x`
    // unless `srcset` already defines a `1x` candidate (WHATWG HTML §4.8.4.3.5).
    if !uses_width
        && let Some(src) = fallback
        && !candidates
            .iter()
            .any(|c| matches!(c.descriptor, SrcsetDescriptor::Density(d) if (d - 1.0).abs() < 1e-4))
    {
        candidates.push(SrcsetCandidate {
            url: src.to_string(),
            descriptor: SrcsetDescriptor::Density(1.0),
        });
    }

    if candidates.is_empty() {
        return fallback.map(|s| (s.to_string(), 1.0));
    }

    let target_width = if uses_width {
        resolve_sizes(sizes_attr, media).max(1.0)
    } else {
        1.0
    };

    let mut weighted: Vec<(String, f32)> = candidates
        .into_iter()
        .map(|c| {
            let density = match c.descriptor {
                SrcsetDescriptor::Density(d) => d,
                SrcsetDescriptor::Width(w) => (w as f32 / target_width).max(0.01),
            };
            (c.url, density)
        })
        .collect();

    // Sort ascending by effective density (stable tie-breaker preserves source order).
    weighted.sort_by(|a, b| a.1.total_cmp(&b.1));

    // Pick the smallest density >= dpr, or fall back to the highest density available.
    if let Some(best) = weighted.iter().find(|(_, d)| *d + 1e-4 >= dpr) {
        Some(best.clone())
    } else {
        weighted.last().cloned()
    }
}

/// Resolves the selected `(url, effective_density)` for an `<img>` element, taking into
/// account any enclosing `<picture>` container's `<source media/type/srcset/sizes>` children.
pub fn resolve_element_image_source(
    document: &Document,
    img_node: NodeId,
    media: &megi_css::MediaContext,
    dpr: f32,
) -> Option<(String, f32)> {
    let img_el = document.node(img_node).as_element()?;
    let img_sizes = img_el.get_attribute("sizes");

    // Check enclosing `<picture>` for preceding matching `<source>` siblings.
    if let Some(parent_id) = document.node(img_node).parent
        && let Some(parent_el) = document.node(parent_id).as_element()
        && parent_el.local_name.eq_ignore_ascii_case("picture")
    {
        for sibling_id in document.children(parent_id) {
            if sibling_id == img_node {
                break;
            }
            let Some(source_el) = document.node(sibling_id).as_element() else {
                continue;
            };
            if !source_el.local_name.eq_ignore_ascii_case("source") {
                continue;
            }
            if let Some(media_attr) = source_el.get_attribute("media").map(str::trim)
                && !media_attr.is_empty()
            {
                let list = megi_css::media::parse_media_query_list(media_attr);
                if !megi_css::media::evaluate(&list, media) {
                    continue;
                }
            }
            if let Some(type_attr) = source_el.get_attribute("type").map(str::trim)
                && !type_attr.is_empty()
                && !is_supported_image_mime(type_attr)
            {
                continue;
            }
            let Some(source_srcset) = source_el
                .get_attribute("srcset")
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let sizes = source_el.get_attribute("sizes").or(img_sizes);
            if let Some(chosen) =
                select_image_candidate(Some(source_srcset), None, sizes, media, dpr)
            {
                return Some(chosen);
            }
        }
    }

    select_image_candidate(
        img_el.get_attribute("srcset"),
        img_el.get_attribute("src"),
        img_sizes,
        media,
        dpr,
    )
}

/// Resolves only the chosen URL string for an `<img>` element (shared with shell callers).
pub fn resolve_img_source(
    document: &Document,
    img_node: NodeId,
    media: &megi_css::MediaContext,
    dpr: f32,
) -> Option<String> {
    resolve_element_image_source(document, img_node, media, dpr).map(|(url, _)| url)
}

/// Collects `(NodeId, src, effective_density)` triples for every `<img>` using responsive
/// selection (`<picture>` / `<source>`, `srcset`, `sizes`, `dpr`).
pub fn collect_responsive_image_sources(
    document: &Document,
    media: &megi_css::MediaContext,
    dpr: f32,
) -> Vec<(NodeId, String, f32)> {
    let mut sources = Vec::new();
    let mut stack = vec![document.root()];
    while let Some(node_id) = stack.pop() {
        if sources.len() >= 1024 {
            break;
        }
        let node = document.node(node_id);
        if let Some(element) = node.as_element()
            && element.local_name.eq_ignore_ascii_case("img")
            && let Some((src, density)) =
                resolve_element_image_source(document, node_id, media, dpr)
        {
            sources.push((node_id, src, density));
        }
        // Push children reversed so the stack pops them in document order.
        let children: Vec<NodeId> = document.children(node_id).collect();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    sources
}

/// Collects `(NodeId, src)` pairs for every `<img>` element (including `srcset`/`<picture>`),
/// in tree order, evaluated with default media context and `1.0` DPR.
pub fn collect_image_sources(document: &Document) -> Vec<(NodeId, String)> {
    collect_responsive_image_sources(document, &megi_css::MediaContext::default(), 1.0)
        .into_iter()
        .map(|(id, src, _)| (id, src))
        .collect()
}

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
                "skipping image {url}: unsupported format {format:?} (PNG, JPEG, GIF, WebP-lossless and SVG are decoded)"
            ));
        }
    }

    warnings.append(&mut result.warnings);
    result
}

#[cfg(test)]
mod dedup_tests {
    use super::*;
    use megi_net::FetchOptions;

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
