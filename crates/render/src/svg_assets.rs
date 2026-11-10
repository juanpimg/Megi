//! SVG detection and rasterization for `<img>` and `background-image` (WP-5.10).
//!
//! The pipeline accepts SVG images alongside PNGs when the payload is either served with
//! `Content-Type: image/svg+xml` or sniffed as SVG (a document starting with `<svg`, or an
//! XML prolog/doctype whose first 1 KiB contains `<svg`). Decoding/rendering is delegated to
//! the dependency-free [`svg`] crate (parser + antialiased rasterizer), which applies its own
//! anti-DoS limits.
//!
//! ## Sizing rules (documented)
//!
//! - The **layout intrinsic size** is the SVG document's own `width`/`height` (or `viewBox`
//!   aspect) in CSS px as returned by [`svg::intrinsic_size`]; it may be fractional.
//! - The **raster** is produced at `ceil(intrinsic)` pixels in each axis (at least 1×1), so
//!   a fractional intrinsic size is rounded up by less than one CSS pixel; CSS `width`/
//!   `height` on the `<img>` still wins for the box geometry (the bitmap is stretched like
//!   any replaced element).
//! - An SVG with no `width`/`height`/`viewBox` uses the crate's SVG default of **300×150**
//!   CSS px (the browser default).
//! - Rasterization is capped defensively at [`MAX_RENDERED_SVG_PIXELS`] total pixels (a
//!   64 MiB RGBA buffer); larger intrinsic sizes fail with a **warning** instead of
//!   allocating, and the element falls back to the usual placeholder/background color.
//! - CSS-in-SVG, gradients/patterns, filters, `<text>` and animation are not rendered (svg
//!   crate limitations); host CSS does not reach the SVG.
//!
//! `background-image: url(*.svg)` is rasterized at the intrinsic size too and then stretched
//! into the element's padding box by paint (the same rule as PNG backgrounds).

use svg::{Limits, SvgError, SvgOptions};

/// Render-side cap on the rasterized SVG area: 16 Mpx (64 MiB of RGBA8). Intrinsic sizes
/// above this are rejected with a warning rather than risking an allocation spike.
pub const MAX_RENDERED_SVG_PIXELS: u64 = 16 * 1024 * 1024;

/// A rendered SVG image plus its CSS-pixel intrinsic size.
pub(crate) struct RenderedSvg {
    /// Intrinsic size in CSS px (may be fractional), for layout's replaced sizes.
    pub intrinsic: (f32, f32),
    /// Rasterized straight-RGBA bitmap at `ceil(intrinsic)` pixels.
    pub raster: image::RgbaImage,
}

/// Returns `true` when the payload should be handled as an SVG image.
pub(crate) fn looks_like_svg(content_type: &str, bytes: &[u8]) -> bool {
    let essence = content_type.split(';').next().unwrap_or("").trim();
    if essence.eq_ignore_ascii_case("image/svg+xml") {
        return true;
    }
    sniff_svg(bytes)
}

/// Content sniffing for SVG (after an optional UTF-8 BOM and leading ASCII whitespace).
fn sniff_svg(bytes: &[u8]) -> bool {
    let rest = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let start = rest
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0C))
        .unwrap_or(rest.len());
    let head = &rest[start..];
    if starts_with_ignore_ascii_case(head, b"<svg") {
        return true;
    }
    // Only treat XML prologs as SVG when the document body starts with `<svg`.
    if starts_with_ignore_ascii_case(head, b"<?xml") {
        return contains_svg_tag(head);
    }
    // A `<!doctype>` prolog: `<!doctype html>` documents are HTML (they may contain inline
    // `<svg>` elements, which must not classify the whole page as an SVG image). Only an
    // explicit `<!doctype svg>` (or an unknown doctype followed by `<svg`) counts as SVG.
    if starts_with_ignore_ascii_case(head, b"<!doctype") {
        let mut rest = &head[b"<!doctype".len()..];
        while matches!(rest.first(), Some(byte) if byte.is_ascii_whitespace()) {
            rest = &rest[1..];
        }
        let name_len = rest
            .iter()
            .position(|byte| !byte.is_ascii_alphanumeric())
            .unwrap_or(rest.len());
        let name = &rest[..name_len];
        if name.eq_ignore_ascii_case(b"html") {
            return false;
        }
        if name.eq_ignore_ascii_case(b"svg") {
            return true;
        }
        return contains_svg_tag(head);
    }
    false
}

/// Returns `true` when a `<svg` element opener appears in the first 1 KiB of `head`.
fn contains_svg_tag(head: &[u8]) -> bool {
    let window_len = head.len().min(1024);
    head[..window_len]
        .windows(4)
        .any(|window| window.eq_ignore_ascii_case(b"<svg"))
}

fn starts_with_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

/// SVG render limits used by the pipeline: the svg crate defaults plus the stricter
/// render-side pixel cap.
fn render_limits() -> Limits {
    Limits {
        max_pixels: MAX_RENDERED_SVG_PIXELS,
        ..Limits::default()
    }
}

/// Parses the intrinsic size and rasterizes the SVG at `ceil(intrinsic)` pixels.
pub(crate) fn render(bytes: &[u8]) -> Result<RenderedSvg, SvgError> {
    let limits = render_limits();
    let intrinsic = svg::intrinsic_size_with_limits(bytes, &limits)?;
    let width = intrinsic.0.ceil().max(1.0);
    let height = intrinsic.1.ceil().max(1.0);
    if !width.is_finite() || !height.is_finite() {
        return Err(SvgError::Invalid("non-finite intrinsic size".to_owned()));
    }
    if width > limits.max_dimension as f32 || height > limits.max_height as f32 {
        return Err(SvgError::Limit(format!(
            "intrinsic size {width}x{height} px exceeds the raster limits"
        )));
    }
    if u64::from(width as u32).saturating_mul(u64::from(height as u32)) > limits.max_pixels {
        return Err(SvgError::Limit(format!(
            "intrinsic size {width}x{height} px exceeds the {}-pixel render cap",
            limits.max_pixels
        )));
    }
    let raster = svg::render_svg_with_limits(
        bytes,
        &SvgOptions {
            width: Some(width as u32),
            height: Some(height as u32),
        },
        &limits,
    )?
    .into_rgba_image()?;
    Ok(RenderedSvg { intrinsic, raster })
}
