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
//! * **Errors**: a failed render becomes [`WorkerMessage::Failed`]; the window
//!   shows a white frame and logs to stderr.
//!
//! No new dependencies: `std::sync::mpsc`, `std::thread` and `std::time` only.
//! The production renderer ([`render_document`]) is separated from the worker
//! plumbing so tests can inject a fake renderer.

use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::convert::pixmap_to_0rgb;
use crate::links::{LinkRegion, collect_link_regions};
use crate::scroll;
use crate::title::document_title;

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
}

/// A finished page frame in CSS resolution (`0RGB`, row-major).
#[derive(Clone, Debug, PartialEq)]
pub struct RenderFrame {
    /// Generation of the request that produced this frame.
    pub generation: u64,
    /// Frame width in CSS pixels (the CSS viewport width).
    pub width: u32,
    /// Frame height in CSS pixels (whole page, capped at [`scroll::MAX_CONTENT_HEIGHT`]).
    pub height: u32,
    /// `width * height` pixels in `0RGB` format.
    pub pixels: Vec<u32>,
    /// Scrollable page height in CSS pixels (clamped to the render cap).
    pub content_height: u32,
    /// Whether the page was taller than [`scroll::MAX_CONTENT_HEIGHT`] and got clipped.
    pub capped: bool,
    /// Clickable `<a href>` regions in page CSS coordinates, extracted from the same
    /// render pass as the pixels (so the hit map always matches the frame).
    pub links: Vec<LinkRegion>,
    /// Document `<title>` text, if the page declared one.
    pub title: Option<String>,
    /// Final document URL after HTTP redirects (the history entry is replaced with it).
    pub document_url: String,
}

/// Message sent from the worker back to the event loop.
#[derive(Clone, Debug, PartialEq)]
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
            let message = match render(&latest) {
                Ok(frame) => WorkerMessage::Frame(frame),
                Err(message) => WorkerMessage::Failed {
                    generation: latest.generation,
                    message,
                },
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
    let probe = render_view(request, request.css_height, base_options)?;
    let (content_height, capped) =
        scroll::content_height(probe.layout.dimensions.margin_box().height);
    let target_height = content_height.clamp(1, scroll::MAX_CONTENT_HEIGHT);
    let output = if target_height > probe.pixmap.height {
        render_view(request, target_height, base_options)?
    } else {
        probe
    };
    let pixels = pixmap_to_0rgb(&output.pixmap);
    let links = collect_link_regions(&output);
    let title = document_title(&output.document);
    let document_url = output.document_url.as_str();
    Ok(RenderFrame {
        generation: request.generation,
        width: output.pixmap.width,
        height: output.pixmap.height,
        pixels,
        content_height: target_height,
        capped,
        links,
        title,
        document_url,
    })
}

/// Runs one `render` pass at `css_width x height`, applying `base_options`.
fn render_view(
    request: &RenderRequest,
    height: u32,
    base_options: &render::RenderOptions,
) -> Result<render::RenderOutput, String> {
    let viewport = render::Viewport::new(request.css_width, height).map_err(|error| {
        format!(
            "invalid render viewport {}x{height}: {error}",
            request.css_width
        )
    })?;
    let mut options = base_options.clone();
    options.viewport = viewport;
    render::render_url(&request.input, &options).map_err(|error| error.to_string())
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
            content_height: 2,
            capped: false,
            links: Vec::new(),
            title: None,
            document_url: "https://example.com/".to_string(),
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
        assert_eq!(
            message,
            WorkerMessage::Failed {
                generation: 1,
                message: "fail 1".to_string()
            }
        );
        assert!(store.accept(message));
        assert_eq!(store.error(), Some("fail 1"));
        drop(sender);
        handle.join().expect("join");
    }
}
