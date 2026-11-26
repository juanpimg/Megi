use super::*;

pub const RENDER_DEBOUNCE: Duration = Duration::from_millis(120);

/// A full-page render request, tagged with a monotonically increasing generation.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderRequest {
    /// Monotonic generation; used to drop stale results.
    pub generation: u64,
    /// Normalized document URL.
    pub input: String,
    /// CSS viewport width (`physical / scale`, clamped to the render cap).
    pub css_width: u32,
    /// CSS viewport height (`physical / scale`, clamped to the render cap).
    pub css_height: u32,
    /// Physical device pixels per CSS pixel for the raster pass (default `1.0`).
    ///
    /// The CSS viewport above is unchanged; this only scales the display list and
    /// the resulting frame. The window sets it from the winit `scale_factor`.
    pub raster_scale: f32,
    /// When set, relative resources resolve against this URL (used when `input` is a
    /// patched-DOM `data:` snapshot rendered for form editing).
    pub base_override: Option<String>,
    /// Whether page scripts run for this request (disabled for patched-DOM snapshots).
    pub scripting: bool,
    /// Optional find-in-page query: the frame carries its hits/highlight rectangles.
    pub find_query: Option<String>,
    /// Host-dispatched window events (e.g. `popstate`/`hashchange` after a same-document
    /// history navigation, #1503). Applied by `view_options` to the render options.
    pub host_events: Vec<String>,
    /// Structured UI/DOM events (keyboard/input/focus/change/submit) forwarded from the
    /// shell into the page's live JS realm (#1504).
    pub host_dom_events: Vec<megi_webview::render::HostDomEvent>,
    /// Optional form POST `(Content-Type, body)` for the main document request (#1538).
    pub form_post: Option<(String, Vec<u8>)>,
}

impl RenderRequest {
    /// Sets the raster scale (the browser window derives it from winit's
    /// `scale_factor`). CSS viewport geometry and hit regions stay untouched.
    pub fn with_raster_scale(mut self, raster_scale: f32) -> Self {
        self.raster_scale = raster_scale;
        self
    }
}

/// A finished page frame in physical-device-pixel resolution (`0RGB`, row-major).
///
/// `width`/`height` are raster dimensions (`css_width/height * raster_scale`);
/// `content_height`, link/form/find rectangles and hit regions remain in CSS px.
#[derive(Clone, Debug)]
pub struct RenderFrame {
    /// Generation of the request that produced this frame.
    pub generation: u64,
    /// Frame width in physical pixels (CSS viewport width × raster scale).
    pub width: u32,
    /// Frame height in physical pixels (whole page × raster scale).
    pub height: u32,
    /// `width * height` pixels in `0RGB` format.
    pub pixels: Vec<u32>,
    /// PNG bytes produced by the render pipeline (same bytes as `--screenshot`). Kept in the
    /// frame so downloads (`Ctrl+S`, `--save-png`) write them directly without re-encoding;
    /// memory trade-off: one extra compressed copy per frame (documented).
    pub png_bytes: Vec<u8>,
    /// Scrollable page height in CSS pixels (clamped to the render cap).
    ///
    /// Never scaled: pointer, scroll and hit-region math stay in CSS coordinates.
    pub content_height: u32,
    /// Whether the page was taller than [`scroll::MAX_CONTENT_HEIGHT`] and got clipped.
    pub capped: bool,
    /// Clickable `<a href>` regions in page CSS coordinates, extracted from the same
    /// render pass as the pixels (so the hit map always matches the frame).
    pub links: Vec<LinkRegion>,
    /// Rendered `<img>` regions (page CSS coordinates) for the context menu's image actions.
    pub images: Vec<crate::images::ImageRegion>,
    /// Document `<title>` text, if the page declared one.
    pub title: Option<String>,
    /// Final document URL after HTTP redirects (the history entry is replaced with it).
    pub document_url: String,
    /// Scripts executed by the page's event loop (0 = no JS activity to refresh for).
    pub scripts_executed: usize,
    /// Page timers still pending when the render's event loop stopped (`setInterval`, or a
    /// timer chain cut by the virtual-time/task budget). Non-zero enables auto-refresh.
    pub script_timers_pending: usize,
    /// Non-fatal render warnings (CORS denials, missing resources, ...).
    pub warnings: usize,
    /// Script errors (exceptions/budget exhaustions) reported by the renderer.
    pub script_errors: usize,
    /// Rendered form controls (page CSS coordinates) for the shell editor.
    pub forms: Vec<crate::forms::FormControl>,
    /// DOM snapshot of this frame (patched and re-rendered by the form editor).
    pub document: Arc<megi_webview::core::Document>,
    /// Find-in-page hits for [`RenderRequest::find_query`] (empty when no query).
    pub find_matches: Vec<crate::find::FindMatch>,
    /// Highlight rectangles (page CSS coordinates) for those hits, from the layout tree.
    pub find_rects: Vec<crate::links::CssRect>,
    /// `location.*` navigation requests made by page scripts (#1828).
    pub location_actions: Vec<megi_webview::render::LocationAction>,
    /// History API navigation requests made by page scripts (#1828).
    pub history_actions: Vec<megi_webview::render::HistoryAction>,
}

/// Message sent from the worker back to the event loop.
///
/// `Frame` is intentionally much larger than `Failed`: it carries the page pixels, the DOM
/// snapshot, form/link/find maps and the render-produced PNG bytes, so downloads can write
/// them without re-encoding. Only one message per request is in flight (channel of two
/// variants), so boxing would add indirection without a real benefit.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum WorkerMessage {
    /// A frame was rendered successfully.
    Frame(RenderFrame),
    /// Rendering failed; the payload is user-facing text for stderr/title.
    Failed {
        /// Generation of the failed request.
        generation: u64,
        /// Error text.
        message: String,
    },
}

impl WorkerMessage {
    /// Generation of the request this message refers to.
    pub const fn generation(&self) -> u64 {
        match self {
            Self::Frame(frame) => frame.generation,
            Self::Failed { generation, .. } => *generation,
        }
    }
}

/// Latest-frame bookkeeping in the event-loop thread.
///
/// [`FrameStore::begin_request`] bumps the generation; [`FrameStore::accept`]
/// applies a worker message only when it belongs to the current generation.
#[derive(Clone, Debug, Default)]
pub struct FrameStore {
    pub(crate) generation: u64,
    pub(crate) pending: bool,
    pub(crate) frame: Option<RenderFrame>,
    pub(crate) error: Option<String>,
}
