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
//! CSS volume, external stylesheet count, display-item **and hit-region** counts,
//! `<img>`/background-image load counts/bytes, cookies currently stored in the render jar,
//! the main document's `Content-Encoding` (evidence that `gzip`/`deflate` was decoded),
//! HTML parser anti-DoS limit violations and non-fatal warnings (e.g. a 404 stylesheet, a
//! missing background image or a denied cross-origin resource).
//!
//! ## Hit-testing (WP-7.5b)
//!
//! [`RenderOutput::hit_regions`] carries `paint`'s additive [`paint::HitRegion`] list
//! produced by the same paint pass as [`RenderOutput::display_list`]: element
//! backgrounds/borders, text runs (inheriting their owner element's DOM node), decorations,
//! replaced images and list markers in painting order, clipped at emission time. The PNG
//! bytes are **identical** with or without regions (verified by the golden tests); hosts
//! resolve links with `paint::find_link_at(&output.document, &output.hit_regions, x, y)`.
//!
//! ## Cookies, CORS and compression (Phase 5 advanced networking)
//!
//! A **cookie jar** (per render via [`RenderOptions::cookies`], or caller-provided in
//! [`RenderOptions::cookie_jar`]) is shared by the document fetch, every subresource and
//! redirect hop: `Set-Cookie` from the document is sent to its stylesheets/images/scripts
//! and later navigations. Subresource fetches send the document's origin as `Origin`
//! (overridable with [`RenderOptions::origin`]); a jar switches CORS to credentialed mode
//! and **CORS denials never abort the render** — the resource is skipped with a warning.
//! `net` decodes `gzip`/`deflate` responses transparently (anti-bomb caps included).
//!
//! ## JavaScript (Phase 6, WP-6.2/6.3 host side)
//!
//! When [`RenderOptions::scripting`] is [`ScriptingMode::Enabled`] (the default when the
//! crate is built with its default `js` feature) the page's classic `<script>` elements
//! are discovered in document order after parsing, external sources are fetched with the
//! shared `net` loader (honoring `--no-network`) and executed through `js::JsRuntime` with
//! per-script and total budgets ([`ScriptingLimits`], defaults aligned with
//! `js::Budget::SMALL`). After the scripts, promise microtasks and a bounded number of
//! timer callbacks are pumped, `console.*` output is captured in
//! [`RenderStats::console`] (warn/error also become warnings) and the **mutated DOM** is
//! re-processed by the styles → layout → paint stages, so DOM/CSS changes reach the PNG.
//! Script exceptions and budget exhaustion are non-fatal warnings; the remaining scripts
//! still run until the total budget is spent. With `--no-default-features` the stage is a
//! no-op and the default mode is [`ScriptingMode::Disabled`].
//!
//! Not yet supported: executing scripts *during* parsing (needed for the html5lib
//! `scripted_*` cases and script-driven `document.write` re-tokenization — WP-6.4),
//! `async` ordering, ES modules, event listeners/wall-clock event loops, and
//! `getComputedStyle`.
//!
//! ## Deliberate limitations (this WP)
//!
//! - **No JavaScript** (scripts are parsed but never executed).
//! - **Images**: `<img src>` is fetched (HTTP cache/redirects/TLS from `net`,
//!   honoring `--no-network` and the [`RenderOptions`] image caps) and decoded
//!   with the in-tree PNG decoder; it is laid out with its intrinsic size and
//!   painted stretched into its content box (`object-fit: fill`). Missing /
//!   too-large / non-PNG images become gray placeholders plus warnings.
//! - **CSS backgrounds (WP-5.9)**: `background-image` and `list-style-image` URLs are
//!   collected from the computed styles, resolved against the **document** base URL,
//!   deduplicated, fetched with the shared loader (honoring `--no-network`) and decoded
//!   with the in-tree PNG decoder. They are handed to `paint` as
//!   [`paint::BackgroundImages`] (keyed by the raw CSS URL string): a background bitmap is
//!   stretched into the element's padding box (no `background-repeat`/`size`/`position`
//!   yet), a `list-style-image` replaces the default marker, and the background color
//!   paints behind the bitmap. Caps: [`DEFAULT_MAX_BACKGROUNDS`] distinct images,
//!   [`DEFAULT_MAX_BACKGROUND_BYTES`] each and
//!   [`DEFAULT_MAX_TOTAL_BACKGROUND_BYTES`] per page. External stylesheets resolve their
//!   relative URLs against the document base URL (not the sheet's own URL); `@import` does
//!   not propagate a base. SVG images, `srcset` and `object-fit` modes other than `fill`
//!   remain out of scope.
//! - **`@import` support (WP-5.8)**: imported stylesheets are fetched recursively with the
//!   shared resource loader, resolved against the importing sheet's URL, deduplicated by
//!   URL, cycle-safe, depth-limited (`RenderOptions::max_import_depth`, default 8) and
//!   counted against `RenderOptions::max_stylesheets` / the CSS byte budget. Cascade order
//!   follows CSS: an importing sheet behaves like its own position with its imports
//!   inserted before its rules (`@import` only at the top of a sheet, as parsed by `css`).
//!   Not supported yet: `@import ... layer(...)`, `supports(...)` and `@import` inside
//!   `@media` (the parser only accepts top-of-sheet imports).
//! - **Media queries**: `<link media>`/`<style media>` and `@import ... <media>` are
//!   evaluated against the real viewport (`screen` + width/height/orientation). Features
//!   the CSS engine does not model (`resolution`, `prefers-*`, `hover`, ...) never match.
//! - No forms, no iframes, no web fonts, no `srcset`.
//! - Cookies: one jar per render (or one shared jar per shell session), no cross-process
//!   persistence, `SameSite` ignored and no Public Suffix List (`net::cookies` subset).
//!   CORS is the `net` subset (no preflight cache/`Expose-Headers`); subresource denials are
//!   warnings, and the top-level navigation never sends `Origin` (no navigation CORS).
//! - Compression is transparent: the decoded body is used, the `Content-Encoding` header is
//!   not exposed on `RenderOutput` beyond [`RenderStats::content_encoding`] (main document),
//!   and subresource decoding counts are not observable through `net::Resource` yet.
//! - Encodings: UTF-8 (with BOM) and Windows-1252/Latin-1 only; other declared encodings
//!   fail with a typed error (see [`charset`]).
//! - HTTP cache is the shared `net` loader cache (freshness/revalidation apply per
//!   resource); redirects are followed by `net`.
//! - Canvas is opaque white, the PNG encoder is the deterministic paint writer.

pub mod backgrounds;
pub mod charset;
pub mod error;
pub mod images;
pub mod scripts;
pub mod stylesheets;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use css::ComputedStyle;
use dom::{Document, NodeId};
use net::{FetchOptions, ResourceLoader, Scheme, Url};

use images::{
    DEFAULT_MAX_IMAGE_BYTES, DEFAULT_MAX_IMAGES, DEFAULT_MAX_TOTAL_IMAGE_BYTES, ImageLimits,
};

pub use error::RenderError;
pub use scripts::{ConsoleMessage, ScriptBudget, ScriptingLimits};

/// Largest accepted viewport dimension for a single render.
pub const MAX_VIEWPORT_DIMENSION: u32 = 8192;
/// Default maximum document size (16 MiB, aligned with the `net` fetch cap).
pub const DEFAULT_MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
/// Default maximum number of external stylesheets per page.
pub const DEFAULT_MAX_STYLESHEETS: usize = 32;
/// Default maximum size of one external stylesheet (1 MiB).
pub const DEFAULT_MAX_CSS_BYTES_PER_SHEET: usize = 1024 * 1024;
/// Default total author-CSS budget (4 MiB, inline + external + imports).
pub const DEFAULT_MAX_TOTAL_CSS_BYTES: usize = 4 * 1024 * 1024;
/// Default maximum `@import` nesting depth.
pub const DEFAULT_MAX_IMPORT_DEPTH: usize = 8;
/// Default maximum number of distinct background images loaded per page.
pub const DEFAULT_MAX_BACKGROUNDS: usize = backgrounds::DEFAULT_MAX_BACKGROUNDS;
/// Default maximum size of one background image (8 MiB).
pub const DEFAULT_MAX_BACKGROUND_BYTES: usize = backgrounds::DEFAULT_MAX_BACKGROUND_BYTES;
/// Default total background-image byte budget per page (16 MiB).
pub const DEFAULT_MAX_TOTAL_BACKGROUND_BYTES: usize =
    backgrounds::DEFAULT_MAX_TOTAL_BACKGROUND_BYTES;

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

/// Whether the renderer executes the page's classic scripts.
///
/// The default is [`ScriptingMode::Enabled`] when the crate is compiled with its `js`
/// feature (the default features) and [`ScriptingMode::Disabled`] when built with
/// `--no-default-features`. Disabling scripting leaves the parsed DOM untouched (scripts
/// are ignored) and is exposed on the CLI as `--no-scripting`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptingMode {
    /// Execute classic `<script>` elements after parsing and re-render the mutated DOM.
    Enabled,
    /// Ignore scripts entirely (no fetch, no execution).
    Disabled,
}

impl Default for ScriptingMode {
    fn default() -> Self {
        if cfg!(feature = "js") {
            Self::Enabled
        } else {
            Self::Disabled
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
    /// Maximum concatenated author CSS size (inline + external + imports).
    pub max_total_css_bytes: usize,
    /// Maximum `@import` nesting depth.
    pub max_import_depth: usize,
    /// Maximum number of `<img>` elements fetched and decoded.
    pub max_images: usize,
    /// Maximum downloaded size of a single image (bytes).
    pub max_image_bytes: usize,
    /// Maximum total downloaded image bytes for the page.
    pub max_total_image_bytes: usize,
    /// Maximum number of distinct `background-image`/`list-style-image` URLs loaded.
    pub max_backgrounds: usize,
    /// Maximum downloaded size of a single background image (bytes).
    pub max_background_bytes: usize,
    /// Maximum total downloaded background-image bytes for the page.
    pub max_total_background_bytes: usize,
    /// Whether classic page scripts run and their DOM mutations are rendered.
    pub scripting: ScriptingMode,
    /// Script discovery/loading caps and execution budgets.
    pub scripting_limits: ScriptingLimits,
    /// HTML parser anti-DoS limits (depth/nodes/attributes/input size).
    pub parse_options: html::ParseOptions,
    /// Networking options (timeouts, caps, TLS configuration, user agent).
    pub fetch_options: FetchOptions,
    /// Enable a **per-render cookie jar** (`--cookies`): the document's `Set-Cookie`
    /// headers are stored and sent to its subresources/redirects for this render only.
    /// Ignored when [`Self::cookie_jar`] is set.
    pub cookies: bool,
    /// Optional caller-owned cookie jar shared across renders (desktop shell session).
    /// Implies cookies are enabled; the jar also switches CORS to credentialed mode.
    pub cookie_jar: Option<Arc<Mutex<net::CookieJar>>>,
    /// Optional explicit CORS `Origin` override for subresource fetches (`--origin`).
    ///
    /// When `None`, an `http(s)` document's own origin is used automatically; `file:`/
    /// `data:` documents send no `Origin`. Cross-origin subresource CORS failures are
    /// non-fatal warnings (the resource is skipped, the page still renders).
    pub origin: Option<String>,
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
            max_import_depth: DEFAULT_MAX_IMPORT_DEPTH,
            max_images: DEFAULT_MAX_IMAGES,
            max_image_bytes: DEFAULT_MAX_IMAGE_BYTES,
            max_total_image_bytes: DEFAULT_MAX_TOTAL_IMAGE_BYTES,
            max_backgrounds: DEFAULT_MAX_BACKGROUNDS,
            max_background_bytes: DEFAULT_MAX_BACKGROUND_BYTES,
            max_total_background_bytes: DEFAULT_MAX_TOTAL_BACKGROUND_BYTES,
            scripting: ScriptingMode::default(),
            scripting_limits: ScriptingLimits::default(),
            parse_options: html::ParseOptions::DEFAULT,
            fetch_options: FetchOptions::default(),
            cookies: false,
            cookie_jar: None,
            origin: None,
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
    /// Background-image/`list-style-image` discovery, fetching and decoding (WP-5.9).
    pub backgrounds: Duration,
    /// Script discovery, fetching, execution, microtasks and timer pumping (WP-6.2/6.3).
    pub scripting: Duration,
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
    /// Total author CSS bytes (inline + external + imported).
    pub css_bytes: usize,
    /// Number of `@import`ed stylesheets successfully loaded and applied.
    pub imports_loaded: usize,
    /// Number of `@import` directives skipped (media mismatch, cycle/dedup, depth or
    /// fetch/parse/limit problems).
    pub imports_skipped: usize,
    /// Deepest `@import` nesting level reached (`0` = no imports).
    pub max_import_depth: usize,
    /// Number of `<style>`/`<link>` sources skipped because their media did not match.
    pub media_skipped: usize,
    /// Number of `<img>` images successfully loaded and decoded.
    pub images_loaded: usize,
    /// Number of `<img>` images that failed (missing, too large, bad format, ...).
    pub images_failed: usize,
    /// Downloaded (encoded) image bytes accepted by the caps.
    pub image_bytes: usize,
    /// Distinct `background-image`/`list-style-image` URLs loaded and decoded.
    pub background_images_loaded: usize,
    /// Distinct background URLs that failed (missing, blocked, too large, non-PNG, ...).
    pub background_images_failed: usize,
    /// Downloaded (encoded) background-image bytes accepted by the caps.
    pub background_bytes: usize,
    /// Classic scripts evaluated without error.
    pub scripts_executed: usize,
    /// Scripts that raised a JavaScript exception or exhausted a budget.
    pub scripts_failed: usize,
    /// Accepted script source bytes (inline + external).
    pub script_bytes: usize,
    /// Captured `console.*` entries (see also warnings for `warn`/`error` levels).
    pub console: Vec<ConsoleMessage>,
    /// Number of display-list items painted.
    pub display_items: usize,
    /// Additive hit-testing regions recorded while painting (same display list: the PNG is
    /// unchanged; regions feed link hit-testing in the shell).
    pub hit_regions: usize,
    /// Cookies currently stored in the render's jar (`0` when cookies are disabled).
    pub cookies_stored: usize,
    /// `Content-Encoding` of the main document response after transparent decoding
    /// (`Some("gzip")`/`Some("deflate")` prove the body was compressed on the wire).
    pub content_encoding: Option<String>,
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
    /// Additive hit-testing regions (paint order) produced by the same paint pass as
    /// [`Self::display_list`]; the PNG is byte-identical with or without them.
    pub hit_regions: Vec<paint::HitRegion>,
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
            .field("hit_regions", &self.hit_regions.len())
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
    // One cookie jar per render (or a caller-owned shared one): the document, its
    // subresources and redirect hops all use it. A jar also enables CORS credentials.
    let cookie_jar = options.cookie_jar.clone().or_else(|| {
        options
            .cookies
            .then(|| Arc::new(Mutex::new(net::CookieJar::new())))
    });
    let mut document_fetch_options = options.fetch_options.clone();
    document_fetch_options.cookie_jar = cookie_jar.clone();
    // The top-level navigation request carries no `Origin` header.
    document_fetch_options.origin = None;
    let mut loader = ResourceLoader::with_options(document_fetch_options.clone());
    let loaded = load_document(&url, &mut loader, &document_fetch_options)?;
    // Subresource fetches send the document's origin (CORS) and its cookies.
    let subresource_origin = options
        .origin
        .clone()
        .or_else(|| match loaded.url.scheme() {
            Scheme::Http | Scheme::Https => net::Origin::from_url(&loaded.url).map(|o| o.as_str()),
            Scheme::File | Scheme::Data => None,
        });
    let mut subresource_fetch_options = document_fetch_options.clone();
    subresource_fetch_options.origin = subresource_origin;
    let mut loader = ResourceLoader::with_options(subresource_fetch_options);
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
    let mut document = parse.document;
    let decode_parse_time = decode_start.elapsed();

    // Base URL for every relative subresource (scripts, stylesheets, images, backgrounds).
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

    let scripting_start = Instant::now();
    let mut script_bytes = 0usize;
    let mut scripting_execution: Option<scripts::ScriptExecution> = None;
    if options.scripting == ScriptingMode::Enabled && !cfg!(feature = "js") {
        warnings.push(
            "scripting was requested but this build has no `js` feature; scripts are ignored"
                .to_owned(),
        );
    }
    if options.scripting == ScriptingMode::Enabled && cfg!(feature = "js") {
        let mut script_warnings = Vec::new();
        let candidates = scripts::discover_scripts(&document, &mut script_warnings);
        let load_report = if candidates.is_empty() {
            scripts::ScriptLoadReport::default()
        } else {
            scripts::load_scripts(
                &candidates,
                &base_url,
                &mut loader,
                options.scripting_limits,
                options.no_network,
                &mut script_warnings,
            )
        };
        script_bytes = load_report.script_bytes;
        let mut execution = scripts::execute_scripts(
            document,
            &document_url,
            load_report.scripts,
            options.scripting_limits,
            &mut script_warnings,
        );
        document = std::mem::take(&mut execution.document);
        warnings.extend(script_warnings);
        scripting_execution = Some(execution);
    }
    let scripting_time = scripting_start.elapsed();
    let (scripts_executed, scripts_failed, console) = match scripting_execution {
        Some(execution) => (execution.executed, execution.failed, execution.console),
        None => (0, 0, Vec::new()),
    };

    let styles_start = Instant::now();
    let media_context = css::MediaContext::with_viewport(
        options.viewport.width as f32,
        options.viewport.height as f32,
    );
    let collected = stylesheets::collect_stylesheets(&document, &base_url, &media_context);
    warnings.extend(collected.warnings.iter().cloned());

    let (stylesheet_collection, style_summary) = build_author_stylesheet_collection(
        &collected,
        &base_url,
        &mut loader,
        options,
        &media_context,
        &mut warnings,
    )?;
    let styles = resolve_styles_with_collection(&document, &stylesheet_collection);
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

    let backgrounds_start = Instant::now();
    let loaded_backgrounds = backgrounds::load_backgrounds(
        &document,
        &styles,
        &base_url,
        &mut loader,
        options.no_network,
        backgrounds::BackgroundLimits {
            max_backgrounds: options.max_backgrounds,
            max_background_bytes: options.max_background_bytes,
            max_total_bytes: options.max_total_background_bytes,
        },
        &mut warnings,
    );
    let backgrounds_time = backgrounds_start.elapsed();

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
    // Additive hit-testing: the painter records `HitRegion`s while emitting the same
    // display-list items, so the rasterized pixels/PNG are byte-identical.
    let (display_list, hit_regions) = paint::build_display_list_with_hit_regions(
        &layout_root,
        viewport,
        &loaded_images.store,
        &loaded_backgrounds.images,
    );
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

    let hit_region_count = hit_regions.len();
    let cookies_stored = cookie_jar
        .as_ref()
        .map(|jar| {
            jar.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .len()
        })
        .unwrap_or(0);
    let content_encoding = loaded.content_encoding.clone();
    let total = total_start.elapsed();
    Ok(RenderOutput {
        png,
        pixmap,
        document,
        styles,
        layout: layout_root,
        display_list,
        hit_regions,
        document_url,
        stats: RenderStats {
            document_bytes: loaded.bytes.len(),
            charset: decoded.charset,
            status: loaded.status,
            external_stylesheets: style_summary.external_stylesheets,
            inline_stylesheets: style_summary.inline_stylesheets,
            css_bytes: style_summary.css_bytes,
            imports_loaded: style_summary.imports_loaded,
            imports_skipped: style_summary.imports_skipped,
            max_import_depth: style_summary.max_import_depth,
            media_skipped: collected.media_skipped,
            images_loaded: loaded_images.loaded,
            images_failed: loaded_images.failed,
            image_bytes: loaded_images.downloaded_bytes,
            background_images_loaded: loaded_backgrounds.loaded,
            background_images_failed: loaded_backgrounds.failed,
            background_bytes: loaded_backgrounds.downloaded_bytes,
            scripts_executed,
            scripts_failed,
            script_bytes,
            console,
            display_items,
            hit_regions: hit_region_count,
            cookies_stored,
            content_encoding,
            limit_violations: parse.limit_violations,
            timings: RenderTimings {
                fetch: fetch_time,
                decode_parse: decode_parse_time,
                styles: styles_time,
                images: images_time,
                backgrounds: backgrounds_time,
                scripting: scripting_time,
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
    /// Main-document `Content-Encoding`, if any (body already decoded by `net`).
    content_encoding: Option<String>,
}

/// Loads the main document, using `net::fetch` for `http(s)` (to observe the HTTP status)
/// and the cached [`ResourceLoader`] for `file:`/`data:` URLs.
fn load_document(
    url: &Url,
    loader: &mut ResourceLoader,
    fetch_options: &FetchOptions,
) -> Result<LoadedDocument, RenderError> {
    match url.scheme() {
        Scheme::Http | Scheme::Https => {
            let response = net::fetch(url, fetch_options)?;
            if response.status >= 400 {
                return Err(RenderError::HttpStatus {
                    url: response.final_url.as_str(),
                    status: response.status,
                });
            }
            let content_type = net::resolve_content_type(&response.headers, &response.body);
            let content_encoding = response.header("content-encoding").map(str::to_owned);
            Ok(LoadedDocument {
                bytes: response.body,
                url: response.final_url,
                content_type,
                status: Some(response.status),
                content_encoding,
            })
        }
        Scheme::File | Scheme::Data => {
            let resource = loader.load(url)?;
            Ok(LoadedDocument {
                bytes: resource.bytes.clone(),
                url: resource.url.clone(),
                content_type: resource.content_type.clone(),
                status: None,
                content_encoding: None,
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

/// Author stylesheet summary for [`RenderStats`].
struct StyleSummary {
    external_stylesheets: usize,
    inline_stylesheets: usize,
    css_bytes: usize,
    imports_loaded: usize,
    imports_skipped: usize,
    max_import_depth: usize,
}

/// Mutable state shared by the recursive `@import` walk.
struct ImportWalk<'a> {
    loader: &'a mut ResourceLoader,
    options: &'a RenderOptions,
    media: &'a css::MediaContext,
    warnings: &'a mut Vec<String>,
    visited: std::collections::HashSet<String>,
    loaded: usize,
    skipped: usize,
    max_depth: usize,
    css_bytes: usize,
}

/// Builds the author [`css::StyleCollection`] in cascade order, resolving `@import`
/// directives recursively (depth-limited, deduplicated, media-gated, non-fatal errors).
fn build_author_stylesheet_collection(
    collected: &stylesheets::CollectedStyles,
    base_url: &Url,
    loader: &mut ResourceLoader,
    options: &RenderOptions,
    media: &css::MediaContext,
    warnings: &mut Vec<String>,
) -> Result<(css::StyleCollection, StyleSummary), RenderError> {
    let mut collection = css::create_user_agent_style_collection();
    collection.media = *media;
    let mut summary = StyleSummary {
        external_stylesheets: 0,
        inline_stylesheets: 0,
        css_bytes: 0,
        imports_loaded: 0,
        imports_skipped: 0,
        max_import_depth: 0,
    };
    let mut walk = ImportWalk {
        loader,
        options,
        media,
        warnings,
        visited: std::collections::HashSet::new(),
        loaded: 0,
        skipped: 0,
        max_depth: 0,
        css_bytes: 0,
    };

    for source in &collected.sources {
        match source {
            stylesheets::CssSource::Inline { text, .. } => {
                summary.inline_stylesheets += 1;
                account_css_bytes(&mut summary.css_bytes, text.len(), options)?;
                let sheet = css::parse_stylesheet(text);
                walk_imports(
                    &mut collection,
                    &sheet,
                    base_url,
                    0,
                    options.max_stylesheets,
                    &mut walk,
                )?;
                collection.add_stylesheet(&sheet, css::Origin::Author);
            }
            stylesheets::CssSource::External { url, .. } => {
                summary.external_stylesheets += 1;
                let total_sheets = summary.external_stylesheets + walk.loaded;
                if total_sheets > options.max_stylesheets {
                    return Err(RenderError::TooManyStylesheets {
                        count: total_sheets,
                        max: options.max_stylesheets,
                    });
                }
                let Some(text) =
                    load_stylesheet_text(url, &mut *walk.loader, options, &mut *walk.warnings)?
                else {
                    continue;
                };
                account_css_bytes(&mut summary.css_bytes, text.len(), options)?;
                // Register the top-level URL so an import cycle back to it is detected.
                walk.visited.insert(url.cache_key());
                let sheet = css::parse_stylesheet(&text);
                walk_imports(
                    &mut collection,
                    &sheet,
                    url,
                    0,
                    options.max_stylesheets,
                    &mut walk,
                )?;
                collection.add_stylesheet(&sheet, css::Origin::Author);
            }
        }
    }
    summary.css_bytes += walk.css_bytes;
    summary.imports_loaded = walk.loaded;
    summary.imports_skipped = walk.skipped;
    summary.max_import_depth = walk.max_depth;
    Ok((collection, summary))
}

/// Recursively loads and adds the `@import`ed sheets of `sheet` **before** its own rules.
///
/// Ordering per CSS: for each import in document order, first the imported sheet's own
/// imports, then the imported sheet's rules (via
/// [`css::StyleCollection::add_import_stylesheet`], which also re-checks the media gate).
fn walk_imports(
    collection: &mut css::StyleCollection,
    sheet: &css::Stylesheet,
    sheet_url: &Url,
    depth: usize,
    max_total_sheets: usize,
    walk: &mut ImportWalk<'_>,
) -> Result<(), RenderError> {
    for import in &sheet.imports {
        if depth >= walk.options.max_import_depth {
            walk.skipped += 1;
            walk.warnings.push(format!(
                "skipping @import \"{}\" at depth {depth}: max_import_depth={} exceeded",
                import.url, walk.options.max_import_depth
            ));
            continue;
        }
        let url = match sheet_url.join(&import.url) {
            Ok(url) => url,
            Err(error) => {
                walk.skipped += 1;
                walk.warnings.push(format!(
                    "skipping invalid @import \"{}\": {error}",
                    import.url
                ));
                continue;
            }
        };
        if stylesheets::ensure_supported_scheme(&url).is_err() {
            walk.skipped += 1;
            walk.warnings
                .push(format!("skipping @import {url}: unsupported URL scheme"));
            continue;
        }
        if !css::evaluate_media(&import.media, walk.media) {
            walk.skipped += 1;
            continue;
        }
        // Deduplicate by final URL (also breaks import cycles).
        if !walk.visited.insert(url.cache_key()) {
            walk.skipped += 1;
            continue;
        }
        if walk.loaded + 1 > max_total_sheets {
            return Err(RenderError::TooManyStylesheets {
                count: walk.loaded + 1,
                max: max_total_sheets,
            });
        }
        let Some(text) =
            load_stylesheet_text(&url, &mut *walk.loader, walk.options, &mut *walk.warnings)?
        else {
            continue;
        };
        account_css_bytes(&mut walk.css_bytes, text.len(), walk.options)?;
        let imported = css::parse_stylesheet(&text);
        walk.loaded += 1;
        walk.max_depth = walk.max_depth.max(depth + 1);
        // Imported sheet's own imports first, then its rules at this cascade position.
        walk_imports(
            collection,
            &imported,
            &url,
            depth + 1,
            max_total_sheets,
            walk,
        )?;
        collection.add_import_stylesheet(import, &imported, css::Origin::Author);
    }
    Ok(())
}

/// Loads one stylesheet body honouring `--no-network` and the per-sheet size cap.
///
/// Missing/unreachable sheets are non-fatal (browser behaviour) and reported as warnings.
fn load_stylesheet_text(
    url: &Url,
    loader: &mut ResourceLoader,
    options: &RenderOptions,
    warnings: &mut Vec<String>,
) -> Result<Option<String>, RenderError> {
    if options.no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
        warnings.push(format!(
            "skipping external stylesheet {url} (--no-network is set)"
        ));
        return Ok(None);
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
            Ok(Some(String::from_utf8_lossy(&resource.bytes).into_owned()))
        }
        Err(error) => {
            warnings.push(format!("skipping stylesheet {url}: {error}"));
            Ok(None)
        }
    }
}

/// Adds `additional` bytes to the author CSS budget, failing on overflow.
fn account_css_bytes(
    total: &mut usize,
    additional: usize,
    options: &RenderOptions,
) -> Result<(), RenderError> {
    *total += additional;
    if *total > options.max_total_css_bytes {
        return Err(RenderError::CssBudgetExceeded {
            bytes: *total,
            max: options.max_total_css_bytes,
        });
    }
    Ok(())
}

/// Resolves computed styles for every element with a prebuilt [`css::StyleCollection`].
///
/// Mirrors `css::resolve_document_styles_with_media` but accepts a collection that already
/// contains the author sheets (including recursively imported ones in cascade order).
fn resolve_styles_with_collection(
    doc: &Document,
    collection: &css::StyleCollection,
) -> HashMap<NodeId, ComputedStyle> {
    let mut styles: HashMap<NodeId, ComputedStyle> = HashMap::new();
    let root = doc.root();
    let mut stack = Vec::new();
    let mut child = doc.node(root).first_child;
    while let Some(child_id) = child {
        stack.push(child_id);
        child = doc.node(child_id).next_sibling;
    }
    while let Some(node_id) = stack.pop() {
        if let Some(element) = doc.node(node_id).as_element() {
            let inline_declarations = element
                .get_attribute("style")
                .map(|style| css::expand_declarations(&css::parse_list_of_declarations(style)));
            let cascaded = css::resolve_cascaded_properties(
                doc,
                node_id,
                collection,
                inline_declarations.as_deref(),
            );
            let parent_style = doc
                .node(node_id)
                .parent
                .and_then(|parent| styles.get(&parent));
            let root_style = styles.get(&root);
            let computed = css::ComputedStyle::compute(&cascaded, parent_style, root_style);
            styles.insert(node_id, computed);
        }
        let mut child = doc.node(node_id).last_child;
        while let Some(child_id) = child {
            stack.push(child_id);
            child = doc.node(child_id).prev_sibling;
        }
    }
    styles
}
