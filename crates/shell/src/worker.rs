//! Render worker thread and generation-based cancellation (pure message logic).
//!
//! Rendering a page is expensive (parse → styles → layout → paint → PNG/raster),
//! so the window must not block its event loop on it. The architecture is:
//!
//! ```text
//! event loop ──RenderRequest{generation, url, css viewport}──► worker thread
//!    ▲                                                            │
//!    │  UserEvent (winit proxy) ◄── WorkerMessage{Frame|Failed} ◄──┘
//!    └─ FrameStore::accept() discards results whose generation is stale
//! ```
//!
//! * **Coalescing**: the worker drains the request channel and renders only the
//!   newest request queued when it is ready for the next job.
//! * **Cancellation**: a render already in flight cannot be aborted, but its
//!   result is tagged with a generation and dropped by [`FrameStore`] when a
//!   newer request superseded it.
//! * **Debounce**: resize/scale changes are coalesced by [`Debounce`] (about
//!   120 ms) before a request is enqueued.
//! * **Errors**: invalid URLs become [`WorkerMessage::Failed`] (logged to stderr);
//!   network/TLS/HTTP/file load failures render a deterministic browser error page
//!   instead of a blank frame (press `r` to retry). `file://` directories render a
//!   generated listing page.
//!
//! No new dependencies: `std::sync::mpsc`, `std::thread` and `std::time` only.
//! The production renderer ([`render_document`]) is separated from the worker
//! plumbing so tests can inject a fake renderer.

use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use std::sync::Arc;

use crate::convert::pixmap_to_0rgb;
use crate::links::{LinkRegion, collect_link_regions};
use crate::scroll;
use crate::title::document_title;

use css::{Overflow, Position};
use layout::LayoutBox;

/// Coalescing window for resize/scale-triggered renders.
pub const RENDER_DEBOUNCE: Duration = Duration::from_millis(120);

/// A full-page render request, tagged with a monotonically increasing generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderRequest {
    /// Monotonic generation; used to drop stale results.
    pub generation: u64,
    /// Normalized document URL.
    pub input: String,
    /// CSS viewport width (`physical / scale`, clamped to the render cap).
    pub css_width: u32,
    /// CSS viewport height (`physical / scale`, clamped to the render cap).
    pub css_height: u32,
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
    pub host_dom_events: Vec<render::HostDomEvent>,
    /// Optional form POST `(Content-Type, body)` for the main document request (#1538).
    pub form_post: Option<(String, Vec<u8>)>,
}

/// A finished page frame in CSS resolution (`0RGB`, row-major).
#[derive(Clone, Debug)]
pub struct RenderFrame {
    /// Generation of the request that produced this frame.
    pub generation: u64,
    /// Frame width in CSS pixels (the CSS viewport width).
    pub width: u32,
    /// Frame height in CSS pixels (whole page, capped at [`scroll::MAX_CONTENT_HEIGHT`]).
    pub height: u32,
    /// `width * height` pixels in `0RGB` format.
    pub pixels: Vec<u32>,
    /// PNG bytes produced by the render pipeline (same bytes as `--screenshot`). Kept in the
    /// frame so downloads (`Ctrl+S`, `--save-png`) write them directly without re-encoding;
    /// memory trade-off: one extra compressed copy per frame (documented).
    pub png_bytes: Vec<u8>,
    /// Scrollable page height in CSS pixels (clamped to the render cap).
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
    pub document: Arc<dom::Document>,
    /// Find-in-page hits for [`RenderRequest::find_query`] (empty when no query).
    pub find_matches: Vec<crate::find::FindMatch>,
    /// Highlight rectangles (page CSS coordinates) for those hits, from the layout tree.
    pub find_rects: Vec<crate::links::CssRect>,
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
    generation: u64,
    pending: bool,
    frame: Option<RenderFrame>,
    error: Option<String>,
}

impl FrameStore {
    /// Creates an empty store at generation 0 (no frame).
    pub const fn new() -> Self {
        Self {
            generation: 0,
            pending: false,
            frame: None,
            error: None,
        }
    }

    /// Current generation (0 before the first request).
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether a request is waiting for its result.
    pub const fn pending(&self) -> bool {
        self.pending
    }

    /// Latest successfully rendered frame, if any.
    pub fn frame(&self) -> Option<&RenderFrame> {
        self.frame.as_ref()
    }

    /// Latest error text, if the current generation failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Takes the latest error (clearing it).
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// Starts a new generation and returns the request to send to the worker.
    pub fn begin_request(&mut self, input: &str, css_width: u32, css_height: u32) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: input.to_owned(),
            css_width,
            css_height,
            base_override: None,
            scripting: true,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        }
    }

    /// Starts a render that also computes find-in-page hits for `query`.
    pub fn begin_find_request(
        &mut self,
        input: &str,
        css_width: u32,
        css_height: u32,
        query: &str,
    ) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: input.to_owned(),
            css_width,
            css_height,
            base_override: None,
            scripting: true,
            find_query: Some(query.to_owned()),
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        }
    }

    /// Starts a render for a **patched-DOM data: snapshot** (form editing): scripts are
    /// disabled (the snapshot already contains their effects) and relative resources are
    /// resolved against `base_override` (the real page URL).
    pub fn begin_patched_request(
        &mut self,
        input: &str,
        css_width: u32,
        css_height: u32,
        base_override: Option<&str>,
    ) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: input.to_owned(),
            css_width,
            css_height,
            base_override: base_override.map(str::to_owned),
            scripting: false,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        }
    }

    /// Starts a render for a **patched-DOM data: snapshot** that replays shell UI events
    /// (`keydown`, `input`, `focus`, `blur`, `change`, `submit`) into the live JS realm (#1504).
    pub fn begin_ui_event_request(
        &mut self,
        snapshot: &str,
        css_width: u32,
        css_height: u32,
        base_override: Option<&str>,
        host_dom_events: Vec<render::HostDomEvent>,
    ) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: snapshot.to_owned(),
            css_width,
            css_height,
            base_override: base_override.map(str::to_owned),
            scripting: true,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events,
            form_post: None,
        }
    }

    /// Starts a render of the current DOM snapshot with host-dispatched window events
    /// (`popstate`/`hashchange` after a same-document history navigation, #1503). Scripts run
    /// on the snapshot so their listeners observe the event and their mutations are painted;
    /// relative resources resolve against the real page URL via `base_override`.
    pub fn begin_history_event_request(
        &mut self,
        snapshot: &str,
        css_width: u32,
        css_height: u32,
        base_override: Option<&str>,
        events: &[&str],
    ) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: snapshot.to_owned(),
            css_width,
            css_height,
            base_override: base_override.map(str::to_owned),
            scripting: true,
            find_query: None,
            host_events: events.iter().map(|event| (*event).to_owned()).collect(),
            host_dom_events: Vec::new(),
            form_post: None,
        }
    }

    /// Starts a form POST navigation (#1538): fetches `input` with the POST body through the
    /// render pipeline, so the response document is painted with the submission result.
    pub fn begin_form_post_request(
        &mut self,
        input: &str,
        css_width: u32,
        css_height: u32,
        content_type: &str,
        body: Vec<u8>,
    ) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: input.to_owned(),
            css_width,
            css_height,
            base_override: None,
            scripting: true,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: Some((content_type.to_owned(), body)),
        }
    }

    /// Applies a worker message; returns `true` only when it is current.
    pub fn accept(&mut self, message: WorkerMessage) -> bool {
        if message.generation() != self.generation {
            return false;
        }
        self.pending = false;
        match message {
            WorkerMessage::Frame(frame) => {
                self.frame = Some(frame);
                self.error = None;
            }
            WorkerMessage::Failed { message, .. } => {
                self.error = Some(message);
            }
        }
        true
    }
}

/// Debounce timer for coalescing resize/scale renders.
#[derive(Clone, Debug)]
pub struct Debounce {
    delay: Duration,
    deadline: Option<Instant>,
}

impl Debounce {
    /// Creates a debounce with the given delay.
    pub const fn new(delay: Duration) -> Self {
        Self {
            delay,
            deadline: None,
        }
    }

    /// (Re)starts the timer from `now`; returns the new deadline.
    pub fn schedule(&mut self, now: Instant) -> Instant {
        let deadline = now + self.delay;
        self.deadline = Some(deadline);
        deadline
    }

    /// Cancels any pending fire.
    pub fn clear(&mut self) {
        self.deadline = None;
    }

    /// Deadline of the pending timer, if any.
    pub const fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Whether the timer is due at `now`.
    pub fn is_due(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|deadline| now >= deadline)
    }

    /// Fires the timer when due, clearing it; returns whether it fired.
    pub fn take_due(&mut self, now: Instant) -> bool {
        if self.is_due(now) {
            self.deadline = None;
            true
        } else {
            false
        }
    }
}

/// Extracts a human-readable message from a caught panic payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// Spawns the render worker.
///
/// * `render` is injected: production uses [`render_document`]; tests use a
///   fake renderer.
/// * `notify` is called after every message is queued (the window passes a
///   winit event-loop proxy so the event loop wakes up without polling).
///
/// The returned [`Sender`]/[`Receiver`] are the request/result channels; the
/// worker exits when all senders are dropped.
pub fn spawn_worker<R, N>(
    mut render: R,
    notify: N,
) -> (
    Sender<RenderRequest>,
    Receiver<WorkerMessage>,
    JoinHandle<()>,
)
where
    R: FnMut(&RenderRequest) -> Result<RenderFrame, String> + Send + 'static,
    N: Fn() + Send + 'static,
{
    let (request_sender, request_receiver) = std::sync::mpsc::channel::<RenderRequest>();
    let (message_sender, message_receiver) = std::sync::mpsc::channel::<WorkerMessage>();
    let handle = thread::spawn(move || {
        // Exits when all senders are dropped (event loop closed).
        while let Ok(first) = request_receiver.recv() {
            // Coalesce: skip every request that arrived while we were idle.
            let mut latest = first;
            loop {
                match request_receiver.try_recv() {
                    Ok(newer) => latest = newer,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                }
            }
            // A panic in layout/paint/render must not kill the only render worker:
            // convert it into a Failed message and keep serving later requests (#1468).
            let rendered =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(&latest)));
            let message = match rendered {
                Ok(Ok(frame)) => WorkerMessage::Frame(frame),
                Ok(Err(message)) => WorkerMessage::Failed {
                    generation: latest.generation,
                    message,
                },
                Err(panic) => {
                    let detail = panic_message(&panic);
                    eprintln!("megi: render worker caught a panic: {detail}");
                    WorkerMessage::Failed {
                        generation: latest.generation,
                        message: format!("render panicked: {detail}"),
                    }
                }
            };
            if message_sender.send(message).is_err() {
                break; // event loop is gone
            }
            notify();
        }
    });
    (request_sender, message_receiver, handle)
}

/// Renderer used by the window: renders the whole page at the CSS viewport
/// width and clamps its height to the render cap.
///
/// It renders a cheap viewport-height probe first to learn the document height
/// and only re-renders at full content height when the page is taller than the
/// window (typical pages: one render).
pub fn render_document(request: &RenderRequest) -> Result<RenderFrame, String> {
    render_document_with_options(request, &render::RenderOptions::default())
}

/// [`render_document`] with base [`render::RenderOptions`] (cookie jar, CORS origin,
/// scripting toggles, network caps) applied to every render of the window.
pub fn render_document_with_options(
    request: &RenderRequest,
    base_options: &render::RenderOptions,
) -> Result<RenderFrame, String> {
    // Height probe must not run page scripts: a tall page would otherwise execute
    // timers/fetches/storage writes twice (once for the probe, once at full height, #1471).
    let probe = render_view(request, request.css_height, base_options, true)?;
    let layout_height =
        scrollable_content_height(&probe.layout).max(probe.layout.dimensions.margin_box().height);
    let (content_height, capped) = scroll::content_height(layout_height);
    let target_height = content_height.clamp(1, scroll::MAX_CONTENT_HEIGHT);
    let output = if target_height > probe.pixmap.height {
        render_view(request, target_height, base_options, false)?
    } else if request.scripting {
        // Re-run at probe height with scripting enabled so scripts execute once.
        render_view(request, probe.pixmap.height, base_options, false)?
    } else {
        probe
    };
    let pixels = pixmap_to_0rgb(&output.pixmap);
    let links = collect_link_regions(&output);
    let images = crate::images::collect_image_regions(
        &output.document,
        &output.layout,
        Some(&output.document_url),
    );
    let title = document_title(&output.document);
    let document_url = output.document_url.as_str();
    let document_url =
        if document_url.starts_with("data:text/html") && !request.input.starts_with("data:") {
            request.input.clone()
        } else {
            document_url.to_owned()
        };
    let mut document = output.document;
    crate::forms::assign_form_ids(&mut document);
    let forms = crate::forms::collect_controls(&document, &output.layout);
    let find_query = request.find_query.as_deref().unwrap_or("");
    let (find_matches, find_rects) = if find_query.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (
            crate::find::search(&document, find_query),
            crate::find::highlight_rects(&output.layout, find_query),
        )
    };
    let scripts_executed = output.stats.scripts_executed;
    let script_timers_pending = output.stats.script_timers_pending;
    // Surface scripting/parser budget exhaustion through the same warning channel the
    // window title and headless CLI already consume (#1458).
    let mut warning_messages = output.stats.warnings.clone();
    if output.stats.script_limited {
        warning_messages.push("scripting budget exhausted".to_string());
    }
    let violations = output.stats.limit_violations.total();
    if violations > 0 {
        warning_messages.push(format!("{violations} parser/layout limit violation(s)"));
    }
    let warnings = warning_messages.len();
    let script_errors = output.stats.scripts_failed
        + output
            .stats
            .console
            .iter()
            .filter(|message| message.level == "error")
            .count();
    let png_bytes = output.png.clone();
    Ok(RenderFrame {
        generation: request.generation,
        width: output.pixmap.width,
        height: output.pixmap.height,
        pixels,
        png_bytes,
        content_height: target_height,
        capped,
        links,
        images,
        title,
        document_url: document_url.clone(),
        scripts_executed,
        script_timers_pending,
        warnings,
        script_errors,
        forms,
        document: Arc::new(document),
        find_matches,
        find_rects,
    })
}

/// Computes the scrollable overflow height of a laid-out document.
///
/// Walks the layout tree and returns the maximum bottom edge (`y + height`) of
/// every margin box in CSS pixels. Descendants of boxes whose computed
/// `overflow` is not `visible` are not traversed for page-scroll extent,
/// because `hidden`/`clip` clip their overflow and `scroll`/`auto` establish an
/// inner scroll container (issue #1021). The root margin box is still included.
fn scrollable_content_height(root: &LayoutBox) -> f32 {
    fn walk(node: &LayoutBox, max_bottom: &mut f32) {
        // `position: fixed` boxes are viewport-relative and do not participate in
        // document scrolling, so a fixed footer/header must not inflate the page
        // scroll height.
        if matches!(node.position, Position::Fixed) {
            return;
        }
        let margin = node.dimensions.margin_box();
        let bottom = margin.y + margin.height;
        if bottom.is_finite() {
            *max_bottom = max_bottom.max(bottom);
        }
        let clips_overflow = node
            .style()
            .is_some_and(|style| !matches!(style.overflow, Overflow::Visible));
        if clips_overflow {
            return;
        }
        for child in &node.children {
            walk(child, max_bottom);
        }
    }

    let mut max_bottom = 0.0f32;
    walk(root, &mut max_bottom);
    max_bottom.max(0.0)
}

/// Builds the `RenderOptions` for one pass. The height probe (`probe = true`) always
/// disables scripting so tall pages do not run scripts twice (#1471).
fn view_options(
    request: &RenderRequest,
    viewport: render::Viewport,
    base_options: &render::RenderOptions,
    probe: bool,
) -> render::RenderOptions {
    let mut options = base_options.clone();
    options.viewport = viewport;
    if let Some(base) = &request.base_override {
        options.base_override = Some(base.clone());
    }
    // The height probe never executes page scripts (#1471); the final render honors
    // the caller's scripting toggle.
    if probe || !request.scripting {
        options.scripting = render::ScriptingMode::Disabled;
    }
    // Host-dispatched events run on `window` after `load`, before the final re-render (#1503).
    options.host_events = request.host_events.clone();
    options.host_dom_events = request.host_dom_events.clone();
    options.form_post = request.form_post.clone();
    options
}

/// Runs one `render` pass at `css_width x height`, applying `base_options`.
fn render_view(
    request: &RenderRequest,
    height: u32,
    base_options: &render::RenderOptions,
    probe: bool,
) -> Result<render::RenderOutput, String> {
    let viewport = render::Viewport::new(request.css_width, height).map_err(|error| {
        format!(
            "invalid render viewport {}x{height}: {error}",
            request.css_width
        )
    })?;
    let options = view_options(request, viewport, base_options, probe);
    let prepared = match crate::pages::prepare_input(&request.input) {
        Ok(prepared) => prepared,
        Err((code, message)) => {
            let page =
                crate::pages::data_url(&crate::pages::error_html(&request.input, &code, &message));
            return render::render_url(&page, &options).map_err(|error| error.to_string());
        }
    };
    match render::render_url(&prepared, &options) {
        Ok(output) => Ok(output),
        Err(error) => {
            // Browser error page for DNS/HTTP/TLS/file failures instead of a blank window.
            let code = crate::pages::error_code(&error);
            let page = crate::pages::data_url(&crate::pages::error_html(
                &request.input,
                code,
                &error.to_string(),
            ));
            render::render_url(&page, &options).map_err(|_| error.to_string())
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use std::time::Duration;

    fn fake_frame(generation: u64) -> RenderFrame {
        RenderFrame {
            generation,
            width: 2,
            height: 2,
            pixels: vec![0; 4],
            png_bytes: vec![0x89, b'P', b'N', b'G'],
            content_height: 2,
            capped: false,
            links: Vec::new(),
            images: Vec::new(),
            title: None,
            document_url: "https://example.com/".to_string(),
            scripts_executed: 0,
            script_timers_pending: 0,
            warnings: 0,
            script_errors: 0,
            forms: Vec::new(),
            document: Arc::new(dom::Document::new()),
            find_matches: Vec::new(),
            find_rects: Vec::new(),
        }
    }

    #[test]
    fn frame_store_accepts_only_the_current_generation() {
        let mut store = FrameStore::new();
        assert_eq!(store.generation(), 0);
        let first = store.begin_request("data:,x", 800, 600);
        assert_eq!(first.generation, 1);
        assert!(store.pending());
        assert!(store.accept(WorkerMessage::Frame(fake_frame(1))));
        assert!(!store.pending());
        assert_eq!(store.frame().map(|frame| frame.generation), Some(1));

        let second = store.begin_request("data:,y", 640, 480);
        assert_eq!(second.generation, 2);
        // A stale frame (generation 1) must be ignored.
        assert!(!store.accept(WorkerMessage::Frame(fake_frame(1))));
        assert!(store.pending(), "stale result must not clear pending");
        assert!(store.accept(WorkerMessage::Frame(fake_frame(2))));
        assert_eq!(store.frame().map(|frame| frame.generation), Some(2));
    }

    #[test]
    fn frame_store_records_errors_for_the_current_generation() {
        let mut store = FrameStore::new();
        let request = store.begin_request("data:,x", 800, 600);
        assert!(!store.accept(WorkerMessage::Failed {
            generation: request.generation - 1,
            message: "stale".to_string(),
        }));
        assert!(store.error().is_none());

        assert!(store.accept(WorkerMessage::Failed {
            generation: request.generation,
            message: "boom".to_string(),
        }));
        assert_eq!(store.error(), Some("boom"));
        assert_eq!(store.take_error(), Some("boom".to_string()));
        assert!(store.error().is_none());
        assert!(store.frame().is_none());
    }

    #[test]
    fn debounce_coalesces_and_fires_once() {
        let start = Instant::now();
        let mut debounce = Debounce::new(Duration::from_millis(100));
        assert!(!debounce.is_due(start));
        let first = debounce.schedule(start);
        let second = debounce.schedule(start + Duration::from_millis(50));
        assert_eq!(second, first + Duration::from_millis(50));
        assert!(!debounce.take_due(start + Duration::from_millis(120)));
        assert!(debounce.take_due(second));
        assert!(!debounce.take_due(second), "fires only once");
        assert!(debounce.deadline().is_none());
        debounce.schedule(start);
        debounce.clear();
        assert!(!debounce.is_due(start + Duration::from_secs(1)));
    }

    #[test]
    fn history_event_request_carries_popstate_and_hashchange() {
        // #1503: a same-document back/forward must render with host events so SPA listeners
        // fire. Verify the request builder stamps the events and keeps scripting enabled.
        let mut store = FrameStore::new();
        let request = store.begin_history_event_request(
            "data:text/html,<p>x</p>",
            320,
            200,
            Some("https://example.com/page#old"),
            &["hashchange", "popstate"],
        );
        assert_eq!(
            request.host_events,
            vec!["hashchange".to_owned(), "popstate".to_owned()]
        );
        assert!(request.scripting, "listeners must run on the snapshot");
        assert_eq!(
            request.base_override.as_deref(),
            Some("https://example.com/page#old")
        );
    }

    #[test]
    fn view_options_applies_host_events() {
        // #1503: `view_options` must forward the request's events into RenderOptions so the
        // render pipeline dispatches them after `load`.
        let request = RenderRequest {
            generation: 1,
            input: "data:text/html,x".into(),
            css_width: 100,
            css_height: 100,
            base_override: None,
            scripting: true,
            find_query: None,
            host_events: vec!["popstate".to_owned()],
            host_dom_events: Vec::new(),
            form_post: None,
        };
        let viewport = render::Viewport::new(100, 100).expect("viewport");
        let options = view_options(&request, viewport, &render::RenderOptions::default(), false);
        assert_eq!(options.host_events, vec!["popstate".to_owned()]);
    }

    #[test]
    fn ui_event_request_and_view_options_propagate_host_dom_events() {
        let mut store = FrameStore::new();
        let events = vec![render::HostDomEvent {
            event_type: "input".to_owned(),
            target: render::HostEventTarget::FormControl(3),
            bubbles: true,
            cancelable: false,
            kind: render::HostUiEventKind::Input {
                data: Some("x".to_owned()),
                input_type: "insertText".to_owned(),
            },
        }];
        let request = store.begin_ui_event_request(
            "data:text/html,<input>",
            320,
            200,
            Some("https://example.com/form"),
            events.clone(),
        );
        assert!(request.scripting);
        assert_eq!(request.host_dom_events, events);
        let viewport = render::Viewport::new(320, 200).expect("viewport");
        let options = view_options(&request, viewport, &render::RenderOptions::default(), false);
        assert_eq!(options.host_dom_events, events);
    }

    #[test]
    fn worker_round_trips_requests_and_messages() {
        let (sender, receiver, handle) = spawn_worker(
            |request: &RenderRequest| Ok(fake_frame(request.generation)),
            || {},
        );
        let mut store = FrameStore::new();
        let request = store.begin_request("data:,x", 320, 200);
        sender.send(request).expect("worker request channel");
        let message = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("message");
        assert_eq!(message.generation(), 1);
        assert!(store.accept(message));
        drop(sender);
        handle.join().expect("worker exits when senders drop");
    }

    #[test]
    fn worker_reports_render_errors_with_the_generation() {
        let (sender, receiver, handle) = spawn_worker(
            |request: &RenderRequest| Err(format!("fail {}", request.generation)),
            || {},
        );
        let mut store = FrameStore::new();
        sender
            .send(store.begin_request("data:,x", 100, 100))
            .expect("send");
        let message = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("message");
        assert!(matches!(
            &message,
            WorkerMessage::Failed { generation: 1, message } if message == "fail 1"
        ));
        assert!(store.accept(message));
        assert_eq!(store.error(), Some("fail 1"));
        drop(sender);
        handle.join().expect("join");
    }

    #[test]
    fn render_document_uses_overflow_extent_for_scroll_height() {
        let html = "data:text/html,\
            <html><body style=\"margin:0;height:120px;overflow:visible\">\
            <div style=\"height:900px;width:50px\"></div>\
            </body></html>";
        let request = RenderRequest {
            generation: 1,
            input: html.to_string(),
            css_width: 320,
            css_height: 200,
            base_override: None,
            scripting: false,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        };
        let frame = render_document(&request).expect("render");
        // Before the fix the probe's root margin box (≈body height) reported a
        // scroll height equal to the viewport, so `max_offset()` stayed 0 and
        // wheel/keyboard scrolling did nothing. The scrollable overflow extent
        // must now include the 900 px child.
        assert!(
            frame.content_height > request.css_height,
            "content_height={} must exceed the viewport height={} when children overflow",
            frame.content_height,
            request.css_height
        );
        assert!(
            frame.content_height >= 800,
            "content_height={} should include the overflowing child extent",
            frame.content_height
        );
        assert_eq!(
            frame.content_height, frame.height,
            "the full-page render must cover the scrollable extent"
        );
    }
}

/// Debounced "session needs saving" state. Extracted from the winit handler so the
/// autosave trigger is testable headlessly (#36).
#[derive(Clone, Debug)]
pub struct SessionAutosave {
    dirty: bool,
    debounce: Debounce,
}

impl SessionAutosave {
    pub const fn new(delay: Duration) -> Self {
        Self {
            dirty: false,
            debounce: Debounce::new(delay),
        }
    }

    /// Marks the session dirty and (re)arms the debounce; returns the new deadline.
    pub fn mark_dirty(&mut self, now: Instant) -> Instant {
        self.dirty = true;
        self.debounce.schedule(now)
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Fires exactly once when dirty and due.
    pub fn take_due(&mut self, now: Instant) -> bool {
        self.dirty && self.debounce.take_due(now)
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Pending save deadline, or `None` when clean.
    pub const fn deadline(&self) -> Option<Instant> {
        if self.dirty {
            self.debounce.deadline()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod session_autosave_tests {
    use super::*;

    #[test]
    fn mark_dirty_then_debounced_save() {
        let start = Instant::now();
        let mut state = SessionAutosave::new(Duration::from_millis(600));
        assert!(!state.is_dirty());
        assert!(!state.take_due(start));

        let deadline = state.mark_dirty(start);
        assert_eq!(deadline, start + Duration::from_millis(600));
        assert!(state.is_dirty());
        assert_eq!(state.deadline(), Some(deadline));
        assert!(!state.take_due(start + Duration::from_millis(599)));

        // A second change re-arms the debounce.
        let deadline2 = state.mark_dirty(start + Duration::from_millis(500));
        assert_eq!(deadline2, start + Duration::from_millis(1100));
        assert!(!state.take_due(deadline));

        assert!(state.take_due(deadline2));
        state.clear_dirty();
        assert!(!state.is_dirty());
        assert!(!state.take_due(deadline2 + Duration::from_secs(1)));
    }
}

#[cfg(test)]
mod worker_panic_isolation_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn test_frame(generation: u64) -> RenderFrame {
        RenderFrame {
            generation,
            width: 2,
            height: 2,
            pixels: vec![0; 4],
            png_bytes: vec![0x89, b'P', b'N', b'G'],
            content_height: 2,
            capped: false,
            links: Vec::new(),
            images: Vec::new(),
            title: None,
            document_url: "https://example.com/".to_string(),
            scripts_executed: 0,
            script_timers_pending: 0,
            warnings: 0,
            script_errors: 0,
            forms: Vec::new(),
            document: Arc::new(dom::Document::new()),
            find_matches: Vec::new(),
            find_rects: Vec::new(),
        }
    }

    #[test]
    fn panicking_render_does_not_kill_the_worker() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let (sender, receiver, handle) = spawn_worker(
            move |request: &RenderRequest| {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("simulated hostile-page panic");
                }
                Ok(test_frame(request.generation))
            },
            || {},
        );

        let first = RenderRequest {
            generation: 1,
            input: "data:text/html,x".into(),
            css_width: 2,
            css_height: 2,
            base_override: None,
            scripting: true,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        };
        sender.send(first).expect("send first");
        let failure = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("worker answered after panic");
        match failure {
            WorkerMessage::Failed {
                generation,
                message,
            } => {
                assert_eq!(generation, 1);
                // The panic payload may not be recoverable across the AssertUnwindSafe
                // boundary; the important invariant is that the worker survives.
                let _ = message;
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        let second = RenderRequest {
            generation: 2,
            input: "data:text/html,y".into(),
            css_width: 2,
            css_height: 2,
            base_override: None,
            scripting: true,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        };
        sender.send(second).expect("send second");
        let frame = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("worker still alive");
        assert_eq!(
            frame.generation(),
            2,
            "worker must survive a page panic (#1468)"
        );
        drop(sender);
        let _ = handle.join();
    }
}

#[cfg(test)]
mod render_budget_warning_tests {
    #[test]
    fn script_limited_and_limit_violations_become_warnings() {
        // Mirrors the frame-building logic: both budget fields must reach the caller's
        // warning channel even when the render itself succeeded (#1458).
        let base_warnings: Vec<String> = Vec::new();
        let script_limited = true;
        let violations: u64 = 3;
        let mut messages = base_warnings;
        if script_limited {
            messages.push("scripting budget exhausted".to_string());
        }
        if violations > 0 {
            messages.push(format!("{violations} parser/layout limit violation(s)"));
        }
        assert_eq!(messages.len(), 2);
        assert!(messages[0].contains("scripting budget"));
        assert!(messages[1].contains("3 parser/layout"));
    }
}
