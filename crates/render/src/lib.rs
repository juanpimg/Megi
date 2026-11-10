//! # Megi Render (`render`) — first real browser pipeline
//!
//! `URL → net → HTML decode → DOM → author CSS (inline + external) → computed styles →
//! layout with real fonts → display list → deterministic PNG`.
//!
//! This crate is the integration point of the engine: it consumes `net` (HTTP/HTTPS/
//! file/data), `html`, `css`, `layout` (with `fonts`) and `paint`, and produces a PNG
//! that can be written to disk. [`render_url`] is the library entry point; the
//! `render` binary adds a CLI (`--width/--height/--base/--no-network/--json`).
//!
//! ```no_run
//! use render::{RenderOptions, Viewport, render_url};
//!
//! let options = RenderOptions { viewport: Viewport::new(800, 600)?, ..RenderOptions::default() };
//! let output = render_url("data:text/html,<h1>Hello Megi</h1>", &options)?;
//! std::fs::write("/tmp/hello.png", &output.png)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Pipeline stages and observability
//!
//! [`RenderOutput::stats`] exposes per-stage timings (fetch, decode, styles, layout,
//! paint, raster), the resolved charset, HTTP status (for `http(s)` documents), author
//! CSS volume, external stylesheet count, display-item count, HTML parser anti-DoS limit
//! violations and non-fatal warnings (e.g. a 404 stylesheet).
//!
//! ## Deliberate limitations (this WP)
//!
//! - **No JavaScript** (scripts are parsed but never executed).
//! - **Images**: `<img src>` is fetched (HTTP cache/redirects/TLS from `net`,
//!   honoring `--no-network` and the [`RenderOptions`] image caps) and decoded
//!   with the in-tree PNG decoder; it is laid out with its intrinsic size and
//!   painted stretched into its content box (`object-fit: fill`). Missing /
//!   too-large / non-PNG images become gray placeholders plus warnings. CSS
//!   `background-image`/`url(...)`, SVG images, `srcset` and `object-fit`
//!   modes other than `fill` remain out of scope.
//! - **No `@import`**: only direct `<style>` and `<link rel="stylesheet">` sheets are
//!   loaded; the `media` attribute is only screened for `screen`/`all`, not evaluated.
//! - No forms, no iframes, no web fonts, no `srcset`, no CORS/cookies policies.
//! - Encodings: UTF-8 (with BOM) and Windows-1252/Latin-1 only; other declared encodings
//!   fail with a typed error (see [`charset`]).
//! - HTTP cache is the shared `net` loader cache (freshness/revalidation apply per
//!   resource); redirects are followed by `net`.
//! - Canvas is opaque white, the PNG encoder is the deterministic paint writer.

pub mod charset;
pub mod error;
pub mod images;
pub mod stylesheets;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use css::ComputedStyle;
use dom::{Document, NodeId};
use net::{FetchOptions, ResourceLoader, Scheme, Url};

use images::{
    DEFAULT_MAX_IMAGE_BYTES, DEFAULT_MAX_IMAGES, DEFAULT_MAX_TOTAL_IMAGE_BYTES, ImageLimits,
};

pub use error::RenderError;

/// Largest accepted viewport dimension for a single render.
pub const MAX_VIEWPORT_DIMENSION: u32 = 8192;
/// Default maximum document size (16 MiB, aligned with the `net` fetch cap).
pub const DEFAULT_MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
/// Default maximum number of external stylesheets per page.
pub const DEFAULT_MAX_STYLESHEETS: usize = 32;
/// Default maximum size of one external stylesheet (1 MiB).
pub const DEFAULT_MAX_CSS_BYTES_PER_SHEET: usize = 1024 * 1024;
/// Default total author-CSS budget (4 MiB, inline + external).
pub const DEFAULT_MAX_TOTAL_CSS_BYTES: usize = 4 * 1024 * 1024;

/// Output viewport in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    /// Width in CSS pixels (1..=8192).
    pub width: u32,
    /// Height in CSS pixels (1..=8192).
    pub height: u32,
}

impl Viewport {
    /// Creates a viewport, validating the render caps.
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        if !(1..=MAX_VIEWPORT_DIMENSION).contains(&width)
            || !(1..=MAX_VIEWPORT_DIMENSION).contains(&height)
        {
            return Err(RenderError::InvalidViewport { width, height });
        }
        Ok(Self { width, height })
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
        }
    }
}

/// Configuration for [`render_url`].
#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// Output viewport.
    pub viewport: Viewport,
    /// Refuse all `http:`/`https:` document and stylesheet URLs (`--no-network`).
    pub no_network: bool,
    /// Override the document base URL for relative resources (`--base URL`).
    ///
    /// Accepts an absolute URL or one resolved against the document URL.
    pub base_override: Option<String>,
    /// Maximum accepted main-document size.
    pub max_document_bytes: usize,
    /// Maximum number of external `<link rel="stylesheet">` sheets.
    pub max_stylesheets: usize,
    /// Maximum size of a single external stylesheet.
    pub max_css_bytes_per_sheet: usize,
    /// Maximum concatenated author CSS size (inline + external).
    pub max_total_css_bytes: usize,
    /// Maximum number of `<img>` elements fetched and decoded.
    pub max_images: usize,
    /// Maximum downloaded size of a single image (bytes).
    pub max_image_bytes: usize,
    /// Maximum total downloaded image bytes for the page.
    pub max_total_image_bytes: usize,
    /// HTML parser anti-DoS limits (depth/nodes/attributes/input size).
    pub parse_options: html::ParseOptions,
    /// Networking options (timeouts, caps, TLS configuration, user agent).
    pub fetch_options: FetchOptions,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            viewport: Viewport::default(),
            no_network: false,
            base_override: None,
            max_document_bytes: DEFAULT_MAX_DOCUMENT_BYTES,
            max_stylesheets: DEFAULT_MAX_STYLESHEETS,
            max_css_bytes_per_sheet: DEFAULT_MAX_CSS_BYTES_PER_SHEET,
            max_total_css_bytes: DEFAULT_MAX_TOTAL_CSS_BYTES,
            max_images: DEFAULT_MAX_IMAGES,
            max_image_bytes: DEFAULT_MAX_IMAGE_BYTES,
            max_total_image_bytes: DEFAULT_MAX_TOTAL_IMAGE_BYTES,
            parse_options: html::ParseOptions::DEFAULT,
            fetch_options: FetchOptions::default(),
        }
    }
}

/// Wall-clock breakdown of one render, per pipeline stage.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderTimings {
    /// Main-document fetch (network, file or data URL).
    pub fetch: Duration,
    /// Charset detection/decoding + HTML parsing.
    pub decode_parse: Duration,
    /// Stylesheet collection, external loading and cascade/UA resolution.
    pub styles: Duration,
    /// Layout (box tree + BFC/IFC with real metrics).
    pub layout: Duration,
    /// Image discovery, fetching and decoding (`<img>`).
    pub images: Duration,
    /// Display-list construction.
    pub paint: Duration,
    /// Rasterization (and PNG encoding happens after this measurement).
    pub raster: Duration,
    /// Whole pipeline.
    pub total: Duration,
}

/// Observability data for one render.
#[derive(Clone, Debug)]
pub struct RenderStats {
    /// Main-document size in bytes.
    pub document_bytes: usize,
    /// Encoding actually used (`utf-8` or `windows-1252`).
    pub charset: String,
    /// HTTP status of the document, when loaded over `http(s)`.
    pub status: Option<u16>,
    /// Number of external stylesheets successfully loaded.
    pub external_stylesheets: usize,
    /// Number of `<style>` elements processed.
    pub inline_stylesheets: usize,
    /// Total author CSS bytes (inline + external).
    pub css_bytes: usize,
    /// Number of `<img>` images successfully loaded and decoded.
    pub images_loaded: usize,
    /// Number of `<img>` images that failed (missing, too large, bad format, ...).
    pub images_failed: usize,
    /// Downloaded (encoded) image bytes accepted by the caps.
    pub image_bytes: usize,
    /// Number of display-list items painted.
    pub display_items: usize,
    /// HTML parser anti-DoS limits that fired (should be empty for normal pages).
    pub limit_violations: html::LimitViolations,
    /// Per-stage timings.
    pub timings: RenderTimings,
    /// Non-fatal warnings (missing stylesheet, invalid base, skipped media, ...).
    pub warnings: Vec<String>,
}

/// Result of a successful render.
pub struct RenderOutput {
    /// Deterministic PNG bytes.
    pub png: Vec<u8>,
    /// Rasterized pixel buffer (opaque white canvas).
    pub pixmap: paint::Pixmap,
    /// Parsed DOM document.
    pub document: Document,
    /// Computed styles per element.
    pub styles: HashMap<NodeId, ComputedStyle>,
    /// Laid-out root box.
    pub layout: layout::LayoutBox,
    /// Flat display list consumed by the rasterizer.
    pub display_list: paint::DisplayList,
    /// Final document URL (after HTTP redirects).
    pub document_url: Url,
    /// Pipeline observability data.
    pub stats: RenderStats,
}

impl RenderOutput {
    /// Convenience accessor for the PNG dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.pixmap.width, self.pixmap.height)
    }
}

impl std::fmt::Debug for RenderOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderOutput")
            .field("width", &self.pixmap.width)
            .field("height", &self.pixmap.height)
            .field("png_bytes", &self.png.len())
            .field("document_url", &self.document_url.as_str())
            .field("display_items", &self.display_list.items.len())
            .field("stats", &self.stats)
            .finish()
    }
}

/// Loads `input` (an absolute URL: `http(s)://`, `file://` or `data:`) and renders it.
pub fn render_url(input: &str, options: &RenderOptions) -> Result<RenderOutput, RenderError> {
    let total_start = Instant::now();
    let url = Url::parse(input)?;
    stylesheets::ensure_supported_scheme(&url)?;
    if options.no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
        return Err(RenderError::NetworkDisabled { url: url.as_str() });
    }

    let fetch_start = Instant::now();
    let mut loader = ResourceLoader::with_options(options.fetch_options.clone());
    let loaded = load_document(&url, &mut loader, options)?;
    let fetch_time = fetch_start.elapsed();
    if loaded.bytes.len() > options.max_document_bytes {
        return Err(RenderError::DocumentTooLarge {
            bytes: loaded.bytes.len(),
            max: options.max_document_bytes,
        });
    }

    let decode_start = Instant::now();
    let decoded = charset::decode_html(&loaded.bytes, &loaded.content_type)?;
    if !content_looks_like_html(&loaded.content_type, &loaded.bytes) {
        return Err(RenderError::UnsupportedContentType(
            loaded.content_type.clone(),
        ));
    }
    let parse = html::parse_document_with_options(&decoded.text, false, &options.parse_options);
    let document = parse.document;
    let decode_parse_time = decode_start.elapsed();

    let styles_start = Instant::now();
    let document_url = loaded.url.clone();
    // `<base href>` (or `--base`) sets the URL used for every relative resource.
    let mut warnings = Vec::new();
    let override_base = match &options.base_override {
        Some(base) => Some(document_url.join(base).map_err(RenderError::Url)?),
        None => None,
    };
    let (mut base_url, base_warnings) =
        stylesheets::find_base_url(&document, override_base.as_ref().unwrap_or(&document_url));
    warnings.extend(base_warnings);
    if override_base.is_some() && base_url == document_url {
        // `--base` should win over a missing `<base>`; keep the explicit value.
        base_url = override_base.clone().unwrap_or(document_url.clone());
    }
    let collected = stylesheets::collect_stylesheets(&document, &base_url);
    warnings.extend(collected.warnings.iter().cloned());

    let (author_css, external_stylesheets, inline_stylesheets, css_bytes) =
        load_author_css(&collected, &mut loader, options, &mut warnings)?;
    let styles = if author_css.trim().is_empty() {
        css::resolve_document_styles_with_viewport(
            &document,
            None,
            options.viewport.width as f32,
            options.viewport.height as f32,
        )
    } else {
        css::resolve_document_styles_with_viewport(
            &document,
            Some(&author_css),
            options.viewport.width as f32,
            options.viewport.height as f32,
        )
    };
    let styles_time = styles_start.elapsed();

    let images_start = Instant::now();
    let loaded_images = images::load_images(
        &document,
        &base_url,
        &mut loader,
        options.no_network,
        ImageLimits {
            max_images: options.max_images,
            max_image_bytes: options.max_image_bytes,
            max_total_bytes: options.max_total_image_bytes,
        },
        &mut warnings,
    );
    let images_time = images_start.elapsed();

    let layout_start = Instant::now();
    let measurer = layout::FontTextMeasurer::bundled()
        .map_err(|error| RenderError::FontEngine(error.to_string()))?;
    let viewport = layout::Rect::new(
        0.0,
        0.0,
        options.viewport.width as f32,
        options.viewport.height as f32,
    );
    let layout_root = layout::layout_document_with_measurer_and_replaced_sizes(
        &document,
        &styles,
        viewport,
        &measurer,
        Some(&loaded_images.sizes),
    )
    .ok_or(RenderError::LayoutFailed)?;
    let layout_time = layout_start.elapsed();

    let paint_start = Instant::now();
    let display_list =
        paint::build_display_list_with_images(&layout_root, viewport, &loaded_images.store);
    let display_items = display_list.items.len();
    let paint_time = paint_start.elapsed();

    let raster_start = Instant::now();
    let pixmap = paint::rasterize_display_list(
        &display_list,
        options.viewport.width,
        options.viewport.height,
    );
    let raster_time = raster_start.elapsed();
    let png = pixmap.to_png();
    if png.is_empty() {
        return Err(RenderError::LayoutFailed);
    }

    let total = total_start.elapsed();
    Ok(RenderOutput {
        png,
        pixmap,
        document,
        styles,
        layout: layout_root,
        display_list,
        document_url,
        stats: RenderStats {
            document_bytes: loaded.bytes.len(),
            charset: decoded.charset,
            status: loaded.status,
            external_stylesheets,
            inline_stylesheets,
            css_bytes,
            images_loaded: loaded_images.loaded,
            images_failed: loaded_images.failed,
            image_bytes: loaded_images.downloaded_bytes,
            display_items,
            limit_violations: parse.limit_violations,
            timings: RenderTimings {
                fetch: fetch_time,
                decode_parse: decode_parse_time,
                styles: styles_time,
                images: images_time,
                layout: layout_time,
                paint: paint_time,
                raster: raster_time,
                total,
            },
            warnings,
        },
    })
}

/// Main document loaded by [`render_url`], before decoding.
struct LoadedDocument {
    bytes: Vec<u8>,
    url: Url,
    content_type: String,
    status: Option<u16>,
}

/// Loads the main document, using `net::fetch` for `http(s)` (to observe the HTTP status)
/// and the cached [`ResourceLoader`] for `file:`/`data:` URLs.
fn load_document(
    url: &Url,
    loader: &mut ResourceLoader,
    options: &RenderOptions,
) -> Result<LoadedDocument, RenderError> {
    match url.scheme() {
        Scheme::Http | Scheme::Https => {
            let response = net::fetch(url, &options.fetch_options)?;
            if response.status >= 400 {
                return Err(RenderError::HttpStatus {
                    url: response.final_url.as_str(),
                    status: response.status,
                });
            }
            let content_type = net::resolve_content_type(&response.headers, &response.body);
            Ok(LoadedDocument {
                bytes: response.body,
                url: response.final_url,
                content_type,
                status: Some(response.status),
            })
        }
        Scheme::File | Scheme::Data => {
            let resource = loader.load(url)?;
            Ok(LoadedDocument {
                bytes: resource.bytes.clone(),
                url: resource.url.clone(),
                content_type: resource.content_type.clone(),
                status: None,
            })
        }
    }
}

/// Returns `true` when the resource should be parsed as HTML.
///
/// Accepts a `text/html` / `application/xhtml+xml` content type, or a resource that the
/// MIME sniffer identifies as HTML (browsers sniff `text/plain` navigations the same way;
/// this keeps `file://` and `data:` pages working when the server omits `Content-Type`).
fn content_looks_like_html(content_type: &str, bytes: &[u8]) -> bool {
    let lower = content_type.to_ascii_lowercase();
    if lower.starts_with("text/html") || lower.starts_with("application/xhtml+xml") {
        return true;
    }
    net::sniff(bytes) == net::MimeHint::Html
}

/// Loads external stylesheets and concatenates the author CSS in document order.
///
/// Returns `(css, external_count, inline_count, css_bytes)`.
fn load_author_css(
    collected: &stylesheets::CollectedStyles,
    loader: &mut ResourceLoader,
    options: &RenderOptions,
    warnings: &mut Vec<String>,
) -> Result<(String, usize, usize, usize), RenderError> {
    let mut css = String::new();
    let mut external = 0usize;
    let mut inline = 0usize;
    let mut css_bytes = 0usize;
    for source in &collected.sources {
        let text = match source {
            stylesheets::CssSource::Inline(text) => {
                inline += 1;
                text.clone()
            }
            stylesheets::CssSource::External(url) => {
                external += 1;
                if external > options.max_stylesheets {
                    return Err(RenderError::TooManyStylesheets {
                        count: external,
                        max: options.max_stylesheets,
                    });
                }
                if options.no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
                    warnings.push(format!(
                        "skipping external stylesheet {url} (--no-network is set)"
                    ));
                    continue;
                }
                match loader.load(url) {
                    Ok(resource) => {
                        if resource.bytes.len() > options.max_css_bytes_per_sheet {
                            return Err(RenderError::StylesheetTooLarge {
                                url: url.as_str(),
                                bytes: resource.bytes.len(),
                                max: options.max_css_bytes_per_sheet,
                            });
                        }
                        String::from_utf8_lossy(&resource.bytes).into_owned()
                    }
                    Err(error) => {
                        // Missing/blocked subresources are non-fatal in browsers.
                        warnings.push(format!("skipping stylesheet {url}: {error}"));
                        continue;
                    }
                }
            }
        };
        css_bytes += text.len();
        if css_bytes > options.max_total_css_bytes {
            return Err(RenderError::CssBudgetExceeded {
                bytes: css_bytes,
                max: options.max_total_css_bytes,
            });
        }
        css.push_str(&text);
        css.push('\n');
    }
    Ok((css, external, inline, css_bytes))
}
