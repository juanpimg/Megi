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
//! use megi_render::{RenderOptions, Viewport, render_url};
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
//! `<img>`/background-image load counts/bytes, event-loop counters (tasks, iterations,
//! virtual time, lifecycle events, pending timers), cookies currently stored in the render jar,
//! the main document's `Content-Encoding` (evidence that `gzip`/`deflate` was decoded),
//! HTML parser anti-DoS limit violations and non-fatal warnings (e.g. a 404 stylesheet, a
//! missing background image or a denied cross-origin resource).
//!
//! ## Hit-testing (WP-7.5b)
//!
//! [`RenderOutput::hit_regions`] carries `paint`'s additive [`megi_display::HitRegion`] list
//! produced by the same paint pass as [`RenderOutput::display_list`]: element
//! backgrounds/borders, text runs (inheriting their owner element's DOM node), decorations,
//! replaced images and list markers in painting order, clipped at emission time. The PNG
//! bytes are **identical** with or without regions (verified by the golden tests); hosts
//! resolve links with `megi_paint::find_link_at(&output.document, &output.hit_regions, x, y)`.
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
//! shared `net` loader (honoring `--no-network`) and executed through `megi_js::JsRuntime` with
//! per-script and total budgets ([`ScriptingLimits`], defaults aligned with
//! `megi_js::Budget::SMALL`). After the scripts, promise microtasks and a bounded number of
//! timer callbacks are pumped, `console.*` output is captured in
//! [`RenderStats::console`] (warn/error also become warnings) and the **mutated DOM** is
//! re-processed by the styles → layout → paint stages, so DOM/CSS changes reach the PNG.
//! Script exceptions and budget exhaustion are non-fatal warnings; the remaining scripts
//! still run until the total budget is spent. With `--no-default-features` the stage is a
//! no-op and the default mode is [`ScriptingMode::Disabled`].
//!
//! After the scripts, a **minimal event loop** runs on a deterministic virtual clock (no
//! wall-clock sleeps): promise microtasks are drained, `DOMContentLoaded` is dispatched on
//! `document`, timers (`setTimeout`/`setInterval`, delays included) run in `(due, id)`
//! order while the clock advances in 1 ms steps up to
//! [`ScriptingLimits::max_virtual_time_ms`] (default 1000 ms) with at most
//! [`ScriptingLimits::max_tasks`] callbacks (default 10.000), then `load` is dispatched on
//! `window` and its timers are drained. The DOM is re-rendered **once** at the end. Stats:
//! [`RenderStats::script_tasks`]/`script_loop_iterations`/`script_virtual_time_ms`/
//! `script_events`/`script_timers_pending`, plus [`RenderTimings::event_loop`].
//!
//! ### Browser APIs exposed to scripts (R96)
//!
//! - Before scripts run, the author+UA cascade is resolved **once** over the parsed DOM into
//!   a snapshot (`ScriptingHostContext`); `window.getComputedStyle(element)` returns the
//!   agreed property list (color/background-color, font size/family/weight/style, box
//!   width/height, margin-*/padding-*, display, position, text-align, line-height,
//!   border-*-width/color/style and opacity) serialized as CSS values. The snapshot
//!   describes the **pre-script** DOM: later `getComputedStyle` calls keep returning it
//!   even after script mutations (layout/paint *are* recomputed from the mutated DOM).
//! - `window.matchMedia(query).matches` is evaluated with the same [`megi_css::MediaContext`]
//!   (render viewport, screen medium) as the stylesheet cascade, and
//!   `innerWidth`/`innerHeight`/`devicePixelRatio` (fixed 1.0) come from the render
//!   viewport, so scripts and layout agree.
//! - `requestAnimationFrame` callbacks are queued 16 virtual milliseconds ahead and pumped
//!   by the deterministic event loop (timestamp = absolute virtual ms), so script-driven
//!   animations mutate the DOM and reach the PNG; `cancelAnimationFrame` is supported.
//! - `document.readyState` transitions `loading` (during scripts) → `interactive`
//!   (`DOMContentLoaded`) → `complete` (`load`). Uncaught exceptions from `eval`, timers
//!   and microtasks invoke `document.onerror`/`window.onerror`; they and `console.error`
//!   become non-fatal warnings ([`RenderStats::console_errors`] counts the latter). The
//!   pre-script pass is measured by [`RenderTimings::styles_pre_script`].
//! - The stylesheet collection loaded by the pre-script pass is reused by the post-script
//!   pass unless scripts added/removed `<style>`/`<link rel=stylesheet>` elements, so
//!   script-inserted stylesheets still apply.
//!
//! ### Referrer-Policy, SameSite and CSS interaction state (R112)
//!
//! - After the main document is loaded (and decoded), render derives its **referrer policy**
//!   from the response `Referrer-Policy` header, else `<meta name="referrer">`, else the
//!   browser default `strict-origin-when-cross-origin`, and installs it on the shared
//!   [`megi_net::ResourceLoader`] before any subresource fetch. Every CSS/script/image/font
//!   request therefore carries a policy-computed `Referer` (same-origin full URL,
//!   cross-origin origin, downgrades blocked where the policy requires it), and the same
//!   source URL lets `net` auto-classify cookie `SameSite` (Strict/Lax are withheld from
//!   cross-site subresources; None requires `Secure`). Recorded in
//!   [`RenderStats::referrer_policy`] and the CLI `--json` output. Render loads subresources
//!   only, so the approximate top-level model never sends Lax cookies on "navigation".
//! - Styles are resolved with an explicit **empty** [`megi_css::StateContext`]: render owns no
//!   focus/pointer state. `:checked`/`:disabled`/`:enabled`/`:read-only`/`:required`/
//!   `:placeholder-shown` are derived from DOM attributes by `megi_css::state` (so `<input
//!   checked>`/`disabled`/`placeholder` work in static documents), and dynamic states can be
//!   modelled with the documented `data-megi-*` attributes: the shell injects
//!   `data-megi-focused` (and optionally hover/active) after an interaction and re-renders.
//!   Limits: hover/active have no headless meaning, `:focus` is not tracked by render
//!   itself, and the state is a per-render snapshot (no live restyle/change events).
//!
//! HTTP/2 server push is opt-in via [`RenderOptions::accept_server_push`]: pushes received
//! while fetching the document are transferred to the subresource loader's per-render
//! preload cache and consumed by images/stylesheets/scripts (no second request); the
//! response completes before the cache is used. [`RenderStats::pushed_used`] reports how
//! many pushed resources the render actually consumed.
//!
//! `<script type="module">` runs through the js crate's static ES-module subset (relative
//! imports resolved against the importing module/baseURI, `file://`/`http(s)` loader with
//! `--no-network` and script caps); each module script is an independent graph root.
//!
//! Not yet supported: executing scripts *during* parsing (needed for the html5lib
//! `scripted_*` cases and script-driven `document.write` re-tokenization — WP-6.4),
//! `async` ordering, dynamic `import()`/top-level await/import maps, real-wall-clock/
//! event-driven loops (`change` events on
//! `MediaQueryList` are not fired), inline `onclick`-style handler attributes and used-px
//! `width`/`height` in `getComputedStyle` (specified values are returned).
//!
//! ## Deliberate limitations (this WP)
//!
//! - **JavaScript**: enabled by default through the `js` feature (see above); builds with
//!   `--no-default-features` parse scripts but never execute them.
//! - **Images**: `<img src>` is fetched (HTTP cache/redirects/TLS from `net`,
//!   honoring `--no-network` and the [`RenderOptions`] image caps) and decoded
//!   with the in-tree PNG decoder or the dependency-free `svg` crate. SVGs are
//!   detected by `Content-Type: image/svg+xml` or sniffed (`<svg`/`<?xml…<svg`),
//!   laid out with their intrinsic size (`width`/`height`/`viewBox`, default
//!   300×150 CSS px) and rasterized at `ceil(intrinsic)` pixels (documented <1 px
//!   rounding, capped at 16 Mpx); CSS `width`/`height` still stretch the bitmap
//!   into the content box (`object-fit: fill`). Missing / too-large / corrupt /
//!   unsupported images become gray placeholders plus warnings. SVG `text`, CSS,
//!   gradients, filters and animation are not rendered (svg crate limitations).
//! - **CSS backgrounds (WP-5.9/R97)**: `background-image`, every layer of
//!   `background_layers` (multi-layer backgrounds) and `list-style-image` URLs are
//!   collected from the computed styles, resolved against the **document** base URL,
//!   deduplicated, fetched with the shared loader (honoring `--no-network`) and decoded
//!   with the in-tree PNG decoder. They are handed to `paint` as
//!   [`megi_paint::BackgroundImages`] (keyed by the raw CSS URL string): a background bitmap is
//!   stretched into the element's padding box (no `background-repeat`/`size`/`position`
//!   yet), a `list-style-image` replaces the default marker, and the background color
//!   paints behind the bitmap. Caps: [`DEFAULT_MAX_BACKGROUNDS`] distinct images,
//!   [`DEFAULT_MAX_BACKGROUND_BYTES`] each and
//!   [`DEFAULT_MAX_TOTAL_BACKGROUND_BYTES`] per page. External stylesheets resolve their
//!   relative URLs against the document base URL (not the sheet's own URL); `@import` does
//!   not propagate a base. SVG images, `srcset` and `object-fit` modes other than `fill`
//!   remain out of scope.
//! - **Web fonts (`@font-face`, Phase 6)**: `@font-face` rules from every author sheet
//!   (including `@import`ed ones) are collected by [`webfonts`], deduplicated by URL,
//!   fetched with the shared loader (honoring `--no-network`) and decoded from WOFF2 into
//!   sfnt with pure-Rust `woff2` (Apache-2.0). Faces are registered under their CSS family
//!   alias in a per-render [`megi_fonts::FontDatabase`] (bundled faces + web faces) that is used
//!   by **both** layout ([`megi_layout::FontTextMeasurer`]) and paint
//!   ([`megi_raster::rasterize_display_list_with_font_database`]). Caps: 8 fonts, 8 MiB each,
//!   32 MiB total; failures are non-fatal warnings and `stats.fonts_loaded/failed/bytes`
//!   plus `timings.fonts` are reported (`--json` includes them).
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
//!   persistence, `SameSite` ignored and no Public Suffix List (`megi_net::cookies` subset).
//!   CORS is the `net` subset (no preflight cache/`Expose-Headers`); subresource denials are
//!   warnings, and the top-level navigation never sends `Origin` (no navigation CORS).
//! - Compression is transparent: the decoded body is used, the `Content-Encoding` header is
//!   not exposed on `RenderOutput` beyond [`RenderStats::content_encoding`] (main document),
//!   and subresource decoding counts are not observable through `megi_net::Resource` yet.
//! - Encodings: UTF-8 (with BOM) and Windows-1252/Latin-1 only; other declared encodings
//!   fail with a typed error (see [`charset`]).
//! - HTTP cache is the shared `net` loader cache (freshness/revalidation apply per
//!   resource); redirects are followed by `net`.
//! - Canvas is opaque white, the PNG encoder is the deterministic paint writer.

#![forbid(unsafe_code)]

pub mod backgrounds;
#[cfg(feature = "js")]
mod browser_host;
pub mod cache;
pub mod charset;
pub mod error;
pub mod frames;
pub mod images;
mod inline_svg;
pub mod scripts;
pub mod stylesheets;
mod svg_assets;
pub mod webfonts;
#[cfg(feature = "js")]
pub mod ws_bridge;

// Pipeline stages: sources live in `src/pipeline/`.
/// Layout/paint/raster and stage orchestration.
#[path = "pipeline/final_stage.rs"]
mod final_stage;
/// Recursive @import loading and cascade ordering.
#[path = "pipeline/imports.rs"]
mod imports;
/// Public configuration types and resource caps.
#[path = "pipeline/options.rs"]
mod options;
/// Observability and result types.
#[path = "pipeline/output.rs"]
mod output;
/// Styles/fonts/images/backgrounds stage.
#[path = "pipeline/resources.rs"]
mod resources;
/// Scripting stage.
#[path = "pipeline/script.rs"]
mod script;
/// Internal state threaded through pipeline stages.
#[path = "pipeline/state.rs"]
mod state;
/// Fetching one stylesheet body.
#[path = "pipeline/state_stylesheet_text.rs"]
mod state_stylesheet_text;
/// Cascade collection, stylesheet resolution and CSP.
#[path = "pipeline/styles.rs"]
mod styles;
/// Direct SVG document navigation.
#[path = "pipeline/svg.rs"]
mod svg;
/// URL entry points and main-document loading.
#[path = "pipeline/url.rs"]
mod url;

pub use cache::{RenderCache, RenderCacheStats};
pub use error::RenderError;
pub use scripts::{ConsoleMessage, RenderStorage, ScriptBudget, ScriptingLimits};

pub use options::{
    DEFAULT_MAX_BACKGROUND_BYTES, DEFAULT_MAX_BACKGROUNDS, DEFAULT_MAX_CSS_BYTES_PER_SHEET,
    DEFAULT_MAX_DOCUMENT_BYTES, DEFAULT_MAX_IMPORT_DEPTH, DEFAULT_MAX_STYLESHEETS,
    DEFAULT_MAX_TOTAL_BACKGROUND_BYTES, DEFAULT_MAX_TOTAL_CSS_BYTES,
    DEFAULT_MAX_TOTAL_WEB_FONT_BYTES, DEFAULT_MAX_WEB_FONT_BYTES, DEFAULT_MAX_WEB_FONTS,
    HistoryAction, HostDomEvent, HostEventTarget, HostUiEventKind, LocationAction,
    MAX_VIEWPORT_AREA, MAX_VIEWPORT_DIMENSION, RenderOptions, ScriptingMode, Viewport,
};
pub use output::{RenderOutput, RenderStats, RenderTimings};
#[cfg(all(feature = "pac", feature = "js"))]
pub use url::install_pac_evaluator;
pub use url::{render_url, render_url_cached};

#[cfg(feature = "js")]
pub(crate) use final_stage::render_pre_scripted_document;
pub(crate) use url::{FrameRenderContext, render_url_with_context};
