//! Public configuration types and resource caps for the render pipeline.
//!
//! Moved verbatim from `lib.rs`; re-exported at the crate root.

use std::sync::{Arc, Mutex};

use megi_net::FetchOptions;

use crate::images::{
    DEFAULT_MAX_IMAGE_BYTES, DEFAULT_MAX_IMAGES, DEFAULT_MAX_TOTAL_DECODED_PIXELS,
    DEFAULT_MAX_TOTAL_IMAGE_BYTES,
};
use crate::{RenderError, RenderStorage, ScriptingLimits, backgrounds, webfonts};
// Intra-doc links moved out of scope when the pipeline was split into modules.
#[allow(unused_imports)]
use crate::{RenderOutput, render_url};

/// Largest accepted viewport dimension for a single render.
pub const MAX_VIEWPORT_DIMENSION: u32 = 8192;
/// Maximum viewport area in CSS pixels (16 Mi pixels ≈ 4096x4096, 64 MiB of RGBA).
///
/// The per-side cap alone allows 8192x8192, whose pixmap is 256 MiB before any image
/// budget applies — one render request could exhaust a modest host (#671). 16 Mi pixels is
/// still past any real display (a 5K screen is ~15 Mi device pixels at DPR 1) while keeping
/// the canvas allocation bounded.
pub const MAX_VIEWPORT_AREA: u64 = 16 * 1024 * 1024;
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
/// Default maximum number of `@font-face` web fonts attempted per render (#531).
pub const DEFAULT_MAX_WEB_FONTS: usize = webfonts::MAX_WEB_FONTS;
/// Default maximum encoded bytes accepted for a single web font (#531).
pub const DEFAULT_MAX_WEB_FONT_BYTES: usize = webfonts::MAX_WEB_FONT_BYTES;
/// Default maximum encoded bytes accepted for all web fonts per render (#531).
pub const DEFAULT_MAX_TOTAL_WEB_FONT_BYTES: usize = webfonts::MAX_WEB_FONT_TOTAL_BYTES;

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
        if u64::from(width) * u64::from(height) > MAX_VIEWPORT_AREA {
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

/// A `location.assign/replace/reload` request made by a page script.
///
/// The renderer **does not navigate**: it records the requests in
/// [`RenderOutput::location_actions`] so the shell can decide to navigate or reload after the
/// render completes (the documented host channel for `location.*`). URLs are already
/// resolved against the document URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocationAction {
    /// `location.assign(url)` or an assignment to `location.href`.
    Assign(String),
    /// `location.replace(url)`.
    Replace(String),
    /// `location.reload()`.
    Reload,
}

/// A History API request made by a page script (queued in
/// [`RenderOutput::history_actions`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryAction {
    /// `history.pushState(state, title, url)` (state is JSON).
    Push { state: String, url: Option<String> },
    /// `history.replaceState(...)`.
    Replace { state: String, url: Option<String> },
    /// `history.back()`.
    Back,
    /// `history.forward()`.
    Forward,
    /// `history.go(delta)`.
    Go(i64),
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
    /// Decode images/backgrounds downscaled so neither side exceeds this many pixels
    /// (`--max-image-dim`).
    ///
    /// Additive performance option for large photos displayed much smaller than their
    /// intrinsic size: the `image` crate box-averages during/after decode (PNG while
    /// unfiltering; JPEG/GIF/WebP post-downsample, documented). The decoded dimensions
    /// reported to layout are the scaled ones, so use explicit CSS `width`/`height`
    /// (as the E2E fixture does) when relying on this option. `None` (default) decodes
    /// at full resolution.
    pub max_image_dim: Option<u32>,
    /// Maximum number of distinct `background-image`/`list-style-image` URLs loaded.
    pub max_backgrounds: usize,
    /// Maximum downloaded size of a single background image (bytes).
    pub max_background_bytes: usize,
    /// Maximum total downloaded background-image bytes for the page.
    pub max_total_background_bytes: usize,
    /// Maximum total decoded pixels across `<img>` and background bitmaps (#517).
    pub max_total_decoded_pixels: u64,
    /// Maximum number of `@font-face` web fonts attempted per render (#531).
    pub max_web_fonts: usize,
    /// Maximum encoded bytes accepted for a single web font (#531).
    pub max_web_font_bytes: usize,
    /// Maximum encoded bytes accepted for all web fonts per render (#531).
    pub max_total_web_font_bytes: usize,
    /// Time used to evaluate CSS animations/transitions, in milliseconds (`--time`).
    ///
    /// `0` (the initial state) is the default; a caller that wants a later animation frame
    /// sets it and re-renders. The pipeline is deterministic for a given value.
    pub time_ms: f64,
    /// Whether classic page scripts run and their DOM mutations are rendered.
    pub scripting: ScriptingMode,
    /// Script discovery/loading caps and execution budgets.
    pub scripting_limits: ScriptingLimits,
    /// HTML parser anti-DoS limits (depth/nodes/attributes/input size).
    pub parse_options: megi_html::ParseOptions,
    /// Networking options (timeouts, caps, TLS configuration, user agent).
    pub fetch_options: FetchOptions,
    /// Enable a **per-render cookie jar** (`--cookies`): the document's `Set-Cookie`
    /// headers are stored and sent to its subresources/redirects for this render only.
    /// Ignored when [`Self::cookie_jar`] is set.
    pub cookies: bool,
    /// Optional caller-owned cookie jar shared across renders (desktop shell session).
    /// Implies cookies are enabled; the jar also switches CORS to credentialed mode.
    pub cookie_jar: Option<Arc<Mutex<megi_net::CookieJar>>>,
    /// Optional explicit CORS `Origin` override for subresource fetches (`--origin`).
    ///
    /// When `None`, an `http(s)` document's own origin is used automatically; `file:`/
    /// `data:` documents send no `Origin`. Cross-origin subresource CORS failures are
    /// non-fatal warnings (the resource is skipped, the page still renders).
    pub origin: Option<String>,
    /// Opt-in streaming response path (R185): when enabled, the main `http(s)` document is
    /// fetched through [`megi_net::fetch_streaming`] (the body is delivered in chunks and
    /// accumulated), while subresources keep using the buffered path. Both paths produce
    /// byte-identical renders; future phases can stream large images/fonts here too.
    pub stream_responses: bool,
    /// Optional form POST body for the main document request (#1538): `(Content-Type, body)`.
    ///
    /// When set, the `http(s)` document is fetched with `megi_net::fetch_with_method("POST", …)` instead
    /// of `GET`. `None` (default) keeps the ordinary GET navigation unchanged.
    pub form_post: Option<(String, Vec<u8>)>,
    /// Accept HTTP/2 server pushes and preload images/stylesheets/scripts from the
    /// same-origin push cache of the document load (R177). Default `false`: the client
    /// advertises `SETTINGS_ENABLE_PUSH=0`. Pushes completing after the main response are
    /// cancelled; the cache is per render (see [`crate::RenderStats::pushed_used`]).
    pub accept_server_push: bool,
    /// Optional shared Web Storage for `localStorage` across renders (R155).
    ///
    /// Pass the same `Arc<Mutex<RenderStorage>>` to two renders to emulate a browser
    /// profile: values written by one page are visible to the next. `sessionStorage` is
    /// always a fresh per-render area. `None` (default) keeps storage in memory for this
    /// render only.
    pub storage: Option<Arc<Mutex<RenderStorage>>>,
    /// Events the host wants dispatched on `window` after `load` (R155), e.g.
    /// `"hashchange"` or `"popstate"`. Listeners can mutate the DOM and run timers; the
    /// final styles/layout/paint pass includes their effects. Empty by default.
    pub host_events: Vec<String>,
    /// Structured DOM/UI events (keyboard, input, focus, change, submit) forwarded from the
    /// host shell into the page's live JS realm after `load` (#1504).
    pub host_dom_events: Vec<HostDomEvent>,
    /// Enable deterministic cooperative Web Workers (R194). Default `true`; worker
    /// scripts are loaded from `file://` URLs only (the renderer owns all I/O).
    pub enable_workers: bool,
    /// Encode the deterministic PNG at the end of the pipeline (default `true`).
    ///
    /// Set to `false` when the caller will modify [`RenderOutput::pixmap`] before writing an
    /// image (the shell's `--chrome`/`--find` overlays do this): the pipeline then leaves
    /// [`RenderOutput::png`] empty and the caller encodes **once** after the overlay with
    /// [`RenderOutput::ensure_png`]. The pixmap, DOM, layout, stats and warnings are
    /// identical either way; only the eager PNG bytes are skipped (R201).
    pub encode_png: bool,
    /// Device pixel ratio used for responsive image selection (`srcset` / `<picture>`), default `1.0`.
    pub device_pixel_ratio: f32,
}

/// Target selector for a structured host-dispatched DOM/UI event (#1504).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostEventTarget {
    /// Dispatch on `window`.
    Window,
    /// Dispatch on `document`.
    Document,
    /// Dispatch on the element with matching `data-megi-form-id`.
    FormControl(u32),
    /// Dispatch on the `<form>` owning the control with `data-megi-form-id`.
    OwningForm(u32),
    /// Dispatch on the focused element (`[data-megi-focused]`), falling back to `document`.
    Focused,
}

/// Typed payload for a structured host-dispatched UI event (#1504).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostUiEventKind {
    /// Standard DOM `Event` (`change`, `submit`, etc.).
    Simple,
    /// `KeyboardEvent` (`keydown`, `keyup`).
    Keyboard {
        /// Key value (`KeyboardEvent.key`).
        key: String,
        /// Physical/logical key code (`KeyboardEvent.code`).
        code: String,
        /// Control modifier state.
        ctrl_key: bool,
        /// Shift modifier state.
        shift_key: bool,
        /// Alt modifier state.
        alt_key: bool,
        /// Meta/Super modifier state.
        meta_key: bool,
        /// Auto-repeat flag.
        repeat: bool,
    },
    /// `InputEvent` (`input`).
    Input {
        /// Inserted text data (`None` for deletions).
        data: Option<String>,
        /// Input operation type (`"insertText"`, `"deleteContentBackward"`, ...).
        input_type: String,
    },
    /// `FocusEvent` (`focus`, `blur`).
    Focus,
}

/// Structured host UI event queued in [`RenderOptions::host_dom_events`] (#1504).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostDomEvent {
    /// DOM event type name (e.g. `"keydown"`, `"input"`, `"focus"`, `"blur"`, `"change"`).
    pub event_type: String,
    /// Target element or global object inside the rendered document.
    pub target: HostEventTarget,
    /// Whether the event bubbles (`focus`/`blur` are `false`; `keydown`/`input`/`change`/`submit` are `true`).
    pub bubbles: bool,
    /// Whether the event is cancelable.
    pub cancelable: bool,
    /// Typed event constructor/payload kind.
    pub kind: HostUiEventKind,
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
            max_image_dim: None,
            max_backgrounds: DEFAULT_MAX_BACKGROUNDS,
            max_background_bytes: DEFAULT_MAX_BACKGROUND_BYTES,
            max_total_background_bytes: DEFAULT_MAX_TOTAL_BACKGROUND_BYTES,
            max_total_decoded_pixels: DEFAULT_MAX_TOTAL_DECODED_PIXELS,
            max_web_fonts: DEFAULT_MAX_WEB_FONTS,
            max_web_font_bytes: DEFAULT_MAX_WEB_FONT_BYTES,
            max_total_web_font_bytes: DEFAULT_MAX_TOTAL_WEB_FONT_BYTES,
            time_ms: 0.0,
            scripting: ScriptingMode::default(),
            scripting_limits: ScriptingLimits::default(),
            enable_workers: true,
            encode_png: true,
            parse_options: megi_html::ParseOptions::DEFAULT,
            fetch_options: FetchOptions::default(),
            cookies: false,
            cookie_jar: None,
            origin: None,
            stream_responses: false,
            form_post: None,
            accept_server_push: false,
            storage: None,
            host_events: Vec::new(),
            device_pixel_ratio: 1.0,
            host_dom_events: Vec::new(),
        }
    }
}
