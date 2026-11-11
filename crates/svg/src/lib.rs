//! # `svg` — dependency-free SVG parser and antialiased software rasterizer
//!
//! Megi renders SVG images from scratch (`std` + the internal `image` crate only):
//! a small XML subset parser, an SVG document model, path-data parsing (including
//! elliptical arcs), transformation handling and a deterministic scanline
//! rasterizer with 4×4 sub-pixel antialiasing.
//!
//! ## Supported
//!
//! * XML: prolog/`DOCTYPE`/processing instructions/comments/CDATA (ignored),
//!   elements, quoted attributes, XML entities (named + numeric), namespaces
//!   (prefixes are ignored and only the local name is used).
//! * Elements: `svg`, `g`, `defs`, `use`, `rect` (`rx`/`ry`), `circle`,
//!   `ellipse`, `line`, `polyline`, `polygon`, `path`, `text` (per-glyph advance
//!   positioning; `<textPath>` is not supported), `defs`/`use`, and the effect
//!   containers `clipPath`, `mask` and `filter`; `title`/`desc` and other unknown
//!   elements are ignored.
//! * Path data: `M/m L/l H/h V/v C/c S/s Q/q T/t A/a Z/z`, implicit repeated
//!   commands, compact numbers, elliptical arcs converted to cubic Béziers.
//! * Painting: `fill` (non-zero and even-odd), `stroke` with width, line cap
//!   (`butt`/`round`/`square`), line join (`miter`/`round`/`bevel`),
//!   `stroke-miterlimit`, `fill-opacity`, `stroke-opacity`, `opacity`,
//!   `transform` (`matrix`/`translate`/`scale`/`rotate`/`skewX`/`skewY`),
//!   `viewBox` + `preserveAspectRatio` (`none`/`meet`/`slice` + alignment),
//!   colours (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`, the
//!   16 basic named colours plus `transparent`/`currentColor`), the
//!   presentation attributes or an inline `style="fill:...;stroke:..."`, and
//!   **effects**: `clip-path="url(#id)"`, `mask="url(#id)"` and
//!   `filter="url(#id)"` on shapes and groups (inherited by descendants).
//! * **CSS inside the SVG**: `<style>` blocks with type/class/id compound
//!   selectors, descendant combinators and selector lists; cascade by simple
//!   specificity and source order (inline style beats CSS, which beats
//!   presentation attributes).
//! * **Gradients**: `<linearGradient>` and `<radialGradient>` with stops,
//!   `gradientUnits`, `spreadMethod`, `gradientTransform` and `href`
//!   inheritance, referenced by `fill`/`stroke` through `url(#id)`.
//!
//! ## Effects and documented limitations
//!
//! * **`clipPath`**: shapes/paths/`use` children, `clipPathUnits`
//!   (`userSpaceOnUse`/`objectBoundingBox`), applied to shapes and groups
//!   (nested clips intersect). Clip geometry uses each child's **fill**
//!   geometry (stroke-only clip children are not included); cyclic references
//!   are dropped without panic.
//! * **`mask`**: `maskUnits`/`maskContentUnits`, luminance mask type (alpha
//!   masks are not selectable), content with solid/gradient paints; the mask
//!   region is multiplied into the layer alpha. Group masks are applied per
//!   flattened descendant shape (group compositing approximation).
//! * **Filters**: `feGaussianBlur` (deterministic separable box blur ×3),
//!   `feOffset`, and the `feFlood` + `feComposite` subset
//!   (`over`/`in`/`out`/`atop`); `filterUnits` and the x/y/width/height region
//!   are honoured. `feColorMatrix`, `feBlend`, `feMerge`, `feTile`, `feImage`
//!   and other primitives are **ignored** (documented subset).
//! * Also not supported: `<textPath>`, `@` rules and child/sibling/pseudo-class
//!   selectors, markers, `stroke-dasharray`, `vector-effect`, script, animation
//!   and external references (no network/FS).
//!
//! ## Supported CSS, gradients and limits
//!
//! * **CSS inside the SVG**: `<style>` blocks (`type="text/css"`, including
//!   `<![CDATA[…]]>`) with type/class/id compound selectors, descendant
//!   combinators and selector lists, applied with simple specificity (inline
//!   style > id > class > type > presentation attribute) and source order.
//!   Supported properties: `fill`, `stroke`, `fill-opacity`, `stroke-opacity`,
//!   `stroke-width`, `stroke-linecap`, `stroke-linejoin`,
//!   `stroke-miterlimit` and `fill-rule`.
//! * **Gradients**: `<linearGradient>`/`<radialGradient>` with stops
//!   (`offset`, `stop-color`, `stop-opacity`), `gradientUnits`
//!   (`objectBoundingBox`/`userSpaceOnUse`), `spreadMethod`
//!   (`pad`/`reflect`/`repeat`), `gradientTransform`, geometry attributes and
//!   `href`/`xlink:href` inheritance, usable from `fill` and `stroke`.
//!   Cyclic or broken reference chains degrade to **no paint** (no panic).
//!
//! ## Safety and limits
//!
//! No `unsafe`. All input goes through bounded parsing with explicit anti-DoS
//! limits ([`Limits`]); the CSS/gradient layers add internal caps too:
//! **1 MiB of stylesheet, 4,096 rules, 64 selectors per rule, 16 descendant
//! compounds, 256 stops per gradient and 32 gradient-reference hops**, plus
//! **4,096 shapes per clipPath/mask, 16 filter primitives, blur radius 64 px and
//! a 64M-sample blur budget (radius shrinks for large layers)**. Clip-path/use
//! cycles and filter/mask reference cycles are dropped without panicking. Input
//! bytes, element depth/count, attribute count, path segments, image side and
//! pixel count stay bounded by [`Limits`]. Arbitrary or hostile input can only
//! produce an [`SvgError`], never a panic, an abort, unbounded recursion or an
//! allocation bomb.

//! ## Dependencies
//!
//! The parser and rasterizer are dependency-free apart from `image` (PNG export helper)
//! and the workspace `fonts` crate, used **only** for `<text>` glyph rasterization and
//! metrics (bundled deterministic DejaVu Sans; fonts' own dependencies are ab_glyph,
//! fontdb, rustybuzz and ttf-parser, all permissively licensed). `<text>` uses per-glyph
//! advance positioning: no kerning/ligatures/complex shaping, no bidi (documented).
//! `<pattern>` tiles are rendered per repetition with a 10 000-tile cap and a depth limit
//! for pattern→pattern references (cycles are dropped, never panic).

#![forbid(unsafe_code)]

mod color;
mod css;
mod document;
mod gradient;
mod path;
pub(crate) mod pattern;
mod raster;
pub(crate) mod text;
mod transform;
mod xml;

pub use color::Color;
pub use transform::Transform;

use std::fmt;

/// Anti-DoS limits applied while parsing and rasterizing an SVG image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest accepted input document, in bytes.
    pub max_input_bytes: usize,
    /// Maximum XML element nesting depth.
    pub max_depth: usize,
    /// Maximum number of XML elements in the document.
    pub max_elements: usize,
    /// Maximum number of attributes per element.
    pub max_attributes: usize,
    /// Maximum number of flattened path segments (across the document).
    pub max_path_segments: usize,
    /// Maximum raster width, in pixels.
    pub max_dimension: u32,
    /// Maximum raster height, in pixels.
    pub max_height: u32,
    /// Maximum total raster pixels (`width * height`).
    pub max_pixels: u64,
    /// Maximum `<use>` expansion depth (cycle guard).
    pub max_use_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 8 * 1024 * 1024,
            max_depth: 256,
            max_elements: 100_000,
            max_attributes: 1024,
            max_path_segments: 1_000_000,
            max_dimension: 16_384,
            max_height: 16_384,
            max_pixels: 64 * 1024 * 1024,
            max_use_depth: 16,
        }
    }
}

/// Options controlling SVG rasterization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SvgOptions {
    /// Force the output width (in pixels). When only one of width/height is set
    /// the other one is derived from the intrinsic aspect ratio.
    pub width: Option<u32>,
    /// Force the output height (in pixels).
    pub height: Option<u32>,
}

/// A rasterized SVG image with straight (non-premultiplied) 8-bit RGBA pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvgImage {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes of straight RGBA pixels, row-major.
    pub data: Vec<u8>,
}

impl SvgImage {
    /// Returns the straight RGBA pixel at `(x, y)`, or `None` when out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let offset = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        self.data
            .get(offset..offset + 4)
            .map(|p| [p[0], p[1], p[2], p[3]])
    }

    /// Converts the image into an [`image::RgbaImage`].
    pub fn into_rgba_image(self) -> Result<image::RgbaImage, SvgError> {
        image::RgbaImage::new(self.width, self.height, self.data)
            .map_err(|error| SvgError::Internal(format!("invalid raster buffer: {error}")))
    }
}

/// Errors produced while parsing or rendering an SVG image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SvgError {
    /// The input is empty or contains no root `<svg>` element.
    Empty,
    /// Malformed XML (truncated tags, unclosed elements, bad entities, ...).
    Xml(String),
    /// A value (length, colour, transform, path data, ...) could not be parsed.
    Invalid(String),
    /// A configured anti-DoS limit was exceeded.
    Limit(String),
    /// A supported feature was requested with unsupported syntax.
    Unsupported(String),
    /// Internal invariant violation (never expected for arbitrary input).
    Internal(String),
}

impl fmt::Display for SvgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty SVG document"),
            Self::Xml(message) => write!(f, "malformed XML: {message}"),
            Self::Invalid(message) => write!(f, "invalid SVG value: {message}"),
            Self::Limit(message) => write!(f, "SVG limit exceeded: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported SVG feature: {message}"),
            Self::Internal(message) => write!(f, "internal SVG error: {message}"),
        }
    }
}

impl std::error::Error for SvgError {}

/// Renders an SVG document to a straight-RGBA image.
///
/// The output size follows `options.width`/`options.height` when given, otherwise
/// the document's intrinsic size (`width`/`height` attributes, `viewBox` or the
/// SVG default of 300×150). `viewBox` and `preserveAspectRatio` are honoured.
pub fn render_svg(bytes: &[u8], options: &SvgOptions) -> Result<SvgImage, SvgError> {
    render_svg_with_limits(bytes, options, &Limits::default())
}

/// Like [`render_svg`] but evaluates the deterministic SMIL subset at `svg_time`
/// seconds (default rendering uses `t = 0`; `t = 0.5 * dur` is the documented
/// midpoint of `from`/`to` animations). Without animation elements the output is
/// byte-identical to [`render_svg`].
pub fn render_svg_with_time(
    bytes: &[u8],
    options: &SvgOptions,
    svg_time: f32,
) -> Result<SvgImage, SvgError> {
    render_svg_with_limits_and_time(bytes, options, &Limits::default(), svg_time)
}

/// Like [`render_svg`] but with explicit anti-DoS [`Limits`].
pub fn render_svg_with_limits(
    bytes: &[u8],
    options: &SvgOptions,
    limits: &Limits,
) -> Result<SvgImage, SvgError> {
    render_svg_with_limits_and_time(bytes, options, limits, 0.0)
}

/// Like [`render_svg_with_limits`] but with an explicit deterministic SMIL time.
pub fn render_svg_with_limits_and_time(
    bytes: &[u8],
    options: &SvgOptions,
    limits: &Limits,
    svg_time: f32,
) -> Result<SvgImage, SvgError> {
    if bytes.is_empty() {
        return Err(SvgError::Empty);
    }
    if bytes.len() > limits.max_input_bytes {
        return Err(SvgError::Limit(format!(
            "input is {} bytes (limit {})",
            bytes.len(),
            limits.max_input_bytes
        )));
    }
    let root = xml::parse_document(bytes, limits)?;
    let doc = document::Document::build(&root, limits, svg_time)?;
    let (width, height) = resolve_output_size(&doc, options, limits)?;
    raster::rasterize(&doc, width, height, limits)
}

/// Returns the intrinsic size of an SVG document in CSS pixels.
pub fn intrinsic_size(bytes: &[u8]) -> Result<(f32, f32), SvgError> {
    intrinsic_size_with_limits(bytes, &Limits::default())
}

/// Like [`intrinsic_size`] but with explicit anti-DoS [`Limits`].
pub fn intrinsic_size_with_limits(bytes: &[u8], limits: &Limits) -> Result<(f32, f32), SvgError> {
    if bytes.is_empty() {
        return Err(SvgError::Empty);
    }
    if bytes.len() > limits.max_input_bytes {
        return Err(SvgError::Limit("input too large".to_owned()));
    }
    let root = xml::parse_document(bytes, limits)?;
    let doc = document::Document::build(&root, limits, 0.0)?;
    Ok((doc.width, doc.height))
}

/// Resolves the output pixel size from explicit options and the document size.
fn resolve_output_size(
    doc: &document::Document,
    options: &SvgOptions,
    limits: &Limits,
) -> Result<(u32, u32), SvgError> {
    let intrinsic_w = doc.width.max(1.0);
    let intrinsic_h = doc.height.max(1.0);
    let (width, height) = match (options.width, options.height) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) => {
            let height = (width as f32 * intrinsic_h / intrinsic_w).round().max(1.0);
            (width, height as u32)
        }
        (None, Some(height)) => {
            let width = (height as f32 * intrinsic_w / intrinsic_h).round().max(1.0);
            (width as u32, height)
        }
        (None, None) => {
            let width = intrinsic_w.round().max(1.0);
            let height = intrinsic_h.round().max(1.0);
            (width as u32, height as u32)
        }
    };
    if width == 0 || height == 0 {
        return Err(SvgError::Invalid("zero-sized viewport".to_owned()));
    }
    if width > limits.max_dimension || height > limits.max_height {
        return Err(SvgError::Limit(format!(
            "output size {width}x{height} exceeds the maximum {}x{}",
            limits.max_dimension, limits.max_height
        )));
    }
    if u64::from(width) * u64::from(height) > limits.max_pixels {
        return Err(SvgError::Limit(format!(
            "output size {width}x{height} exceeds the maximum pixel count {}",
            limits.max_pixels
        )));
    }
    Ok((width, height))
}
