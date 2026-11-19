//! Observability and result types returned by the render pipeline.
//!
//! Moved verbatim from `lib.rs`; re-exported at the crate root.

use std::collections::HashMap;
use std::time::Duration;

use megi_core::{Document, NodeId};
use megi_css::ComputedStyle;
use megi_net::Url;

use crate::{ConsoleMessage, HistoryAction, LocationAction};
// Intra-doc links moved out of scope when the pipeline was split into modules.
#[allow(unused_imports)]
use crate::{RenderOptions, render_url_cached};

/// Wall-clock breakdown of one render, per pipeline stage.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderTimings {
    /// Main-document fetch (network, file or data URL).
    pub fetch: Duration,
    /// Charset detection/decoding + HTML parsing.
    pub decode_parse: Duration,
    /// Stylesheet collection, external loading and cascade/UA resolution for the
    /// post-script pass (the pass that feeds layout/paint).
    pub styles: Duration,
    /// Pre-script style pass (R96): stylesheet collection/loading plus the author+UA
    /// cascade snapshot exposed to `getComputedStyle`. Zero when the page ran no scripts.
    pub styles_pre_script: Duration,
    /// Web-font (`@font-face`) fetching, WOFF2 decoding and registration.
    pub fonts: Duration,
    /// Layout (box tree + BFC/IFC with real metrics).
    pub layout: Duration,
    /// Image discovery, fetching and decoding (`<img>`).
    pub images: Duration,
    /// Background-image/`list-style-image` discovery, fetching and decoding (WP-5.9).
    pub backgrounds: Duration,
    /// Script discovery, fetching, evaluation and microtasks (WP-6.2/6.3), excluding the
    /// event-loop stage below.
    pub scripting: Duration,
    /// Minimal event loop: microtasks, `DOMContentLoaded`/`load` dispatch and virtual-time
    /// timer callbacks (subset of the scripting stage).
    pub event_loop: Duration,
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
    /// SVG images (from `<img>` or CSS backgrounds) rasterized through the `svg` crate.
    pub svgs_rendered: usize,
    /// Inline `<svg>` elements replaced by synthetic `<img>` data URLs before layout (they
    /// are also counted by [`Self::images_loaded`]/[`Self::svgs_rendered`]).
    pub inline_svgs: usize,
    /// Web fonts successfully fetched, decoded and registered.
    pub fonts_loaded: usize,
    /// Web fonts skipped or failed (network, caps, decode, registration).
    pub fonts_failed: usize,
    /// Encoded bytes of the loaded web fonts.
    pub font_bytes: usize,
    /// Classic scripts evaluated without error.
    pub scripts_executed: usize,
    /// Scripts that raised a JavaScript exception or exhausted a budget.
    pub scripts_failed: usize,
    /// Accepted script source bytes (inline + external).
    pub script_bytes: usize,
    /// Captured `console.*` entries (see also warnings for `warn`/`error` levels).
    pub console: Vec<ConsoleMessage>,
    /// Number of captured `console.error` entries (also mirrored as warnings).
    pub console_errors: usize,
    /// Timer callbacks executed by the event loop (`setTimeout`/`setInterval`).
    pub script_tasks: usize,
    /// Event-loop iterations (timer batches + 1 ms virtual-time steps).
    pub script_loop_iterations: usize,
    /// Virtual time advanced by the event loop, in milliseconds.
    pub script_virtual_time_ms: u64,
    /// Lifecycle events dispatched (`DOMContentLoaded` on `document`, `load` on `window`).
    pub script_events: usize,
    /// Timers still pending when the event loop stopped (budget exhausted or interval).
    pub script_timers_pending: usize,
    /// `location.*` navigation requests captured during scripting (see
    /// [`RenderOutput::location_actions`]).
    pub location_actions: usize,
    /// Whether scripting hit a budget limit (virtual time, iterations, timer count).
    pub script_limited: bool,
    /// History API requests captured during scripting (see
    /// [`RenderOutput::history_actions`]).
    pub history_actions: usize,
    /// HTTP/2 server-pushed resources actually consumed by the render (images,
    /// stylesheets, scripts) from the per-load preload cache (R177).
    pub pushed_used: usize,
    /// Server-pushed resources dropped because a push cap was exceeded.
    pub pushed_discarded: usize,
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
    /// Referrer policy applied to subresource fetches (header > `<meta name="referrer">` >
    /// default `strict-origin-when-cross-origin`), e.g. `Some("origin")`.
    pub referrer_policy: Option<String>,
    /// HTML parser anti-DoS limits that fired (should be empty for normal pages).
    pub limit_violations: megi_html::LimitViolations,
    /// `true` when [`render_url_cached`] reused a cached [`megi_layout::LayoutBox`].
    pub cache_layout_hit: bool,
    /// Per-stage timings.
    pub timings: RenderTimings,
    /// Non-fatal warnings (missing stylesheet, invalid base, skipped media, ...).
    pub warnings: Vec<String>,
}

/// Result of a successful render.
pub struct RenderOutput {
    /// Deterministic PNG bytes, or **empty** when [`RenderOptions::encode_png`] was `false`.
    ///
    /// Call [`RenderOutput::ensure_png`] (or [`RenderOutput::png_bytes`]) after mutating the
    /// pixmap to encode lazily exactly once.
    pub png: Vec<u8>,
    /// Rasterized pixel buffer (opaque white canvas).
    pub pixmap: megi_raster::Pixmap,
    /// Parsed DOM document.
    pub document: Document,
    /// Computed styles per element.
    pub styles: HashMap<NodeId, ComputedStyle>,
    /// Laid-out root box.
    pub layout: megi_layout::LayoutBox,
    /// Flat display list consumed by the rasterizer.
    pub display_list: megi_display::DisplayList,
    /// Additive hit-testing regions (paint order) produced by the same paint pass as
    /// [`Self::display_list`]; the PNG is byte-identical with or without them.
    pub hit_regions: Vec<megi_display::HitRegion>,
    /// Final document URL (after HTTP redirects).
    pub document_url: Url,
    /// `location.assign/replace/reload` requests made by page scripts, in execution
    /// order. The renderer does not navigate; the shell is expected to act on these after a
    /// render (see [`LocationAction`]).
    pub location_actions: Vec<LocationAction>,
    /// History API requests made by page scripts, in execution order (see
    /// [`HistoryAction`]; the in-page `history.length`/`history.state` already reflect
    /// push/replace).
    pub history_actions: Vec<HistoryAction>,
    /// Pipeline observability data.
    pub stats: RenderStats,
}

impl RenderOutput {
    /// Encodes the PNG lazily when [`RenderOptions::encode_png`] skipped it (idempotent).
    ///
    /// This is the single re-encode entry point for callers that painted overlays on
    /// [`Self::pixmap`] (the shell chrome/find highlights): call it once at the end.
    pub fn ensure_png(&mut self) {
        if self.png.is_empty() {
            self.png = self.pixmap.to_png();
        }
    }

    /// Returns the PNG bytes, encoding them lazily if the eager pass was skipped.
    pub fn png_bytes(&mut self) -> &[u8] {
        self.ensure_png();
        &self.png
    }

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
