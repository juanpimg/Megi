//! # Megi WebView (`megi-webview`)
//!
//! Platform-agnostic, embeddable engine API. It renders pages into an owned
//! RGBA frame and exposes hit-testing and navigation state, but knows nothing
//! about windows, event loops, chrome or a graphics backend.
//!
//! ```no_run
//! use megi_webview::{WebView, WebViewOptions};
//! let mut view = WebView::new(WebViewOptions::new(800, 600)?).unwrap();
//! let frame = view.load("data:text/html,<h1>Hello</h1>").unwrap();
//! assert_eq!(frame.png[1..4], *b"PNG");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::fmt;

use megi_core::Document;
use megi_display::{HitLink, HitRegion};
use megi_raster::Pixmap;
use megi_render::{RenderError, RenderOptions, Viewport, render_url};

/// Configuration for an embedded WebView.
#[derive(Clone, Debug)]
pub struct WebViewOptions {
    /// CSS viewport used for layout and rasterization.
    pub viewport: Viewport,
    /// Refuse `http:`/`https:` documents and subresources.
    pub no_network: bool,
}

impl WebViewOptions {
    /// Creates options with validated dimensions.
    pub fn new(width: u32, height: u32) -> Result<Self, WebError> {
        let viewport = Viewport::new(width, height).map_err(WebError::Render)?;
        Ok(Self { viewport, no_network: false })
    }
}

/// Errors surfaced to an embedding host.
#[derive(Debug)]
pub enum WebError {
    /// The requested viewport was rejected by the renderer.
    Render(RenderError),
    /// The navigation target could not be rendered.
    Load(String),
}

impl fmt::Display for WebError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Render(e) => write!(f, "invalid viewport: {e}"),
            Self::Load(msg) => write!(f, "load failed: {msg}"),
        }
    }
}

impl std::error::Error for WebError {}

/// A rendered, owned frame ready for presentation by any backend.
pub struct WebFrame {
    /// Final document URL after redirects.
    pub url: String,
    /// RGBA8 pixels in CSS-pixel coordinates.
    pub pixmap: Pixmap,
    /// Deterministic PNG encoding of `pixmap`.
    pub png: Vec<u8>,
    /// Painting-order regions for hit-testing.
    pub hit_regions: Vec<HitRegion>,
    /// Parsed DOM of this frame (kept for link resolution).
    pub document: Document,
}

/// An input event translated by the host from its native event system.
#[derive(Clone, Debug, PartialEq)]
pub enum WebEvent {
    /// Pointer moved to `(x, y)` in CSS pixels.
    PointerMove { x: f32, y: f32 },
    /// Primary button pressed at `(x, y)`.
    PointerDown { x: f32, y: f32 },
    /// Primary button released at `(x, y)`.
    PointerUp { x: f32, y: f32 },
    /// Scroll wheel delta in CSS pixels.
    Wheel { dx: f32, dy: f32 },
    /// Text input (already decoded to UTF-8).
    TextInput(String),
    /// Viewport resized by the host.
    Resize { width: u32, height: u32 },
}

/// An embeddable engine instance. `Send` state is owned; the host drives it.
pub struct WebView {
    options: WebViewOptions,
    history: Vec<String>,
    index: usize,
    frame: Option<WebFrame>,
    events: VecDeque<WebEvent>,
}

impl WebView {
    /// Creates a WebView with no loaded document.
    pub fn new(options: WebViewOptions) -> Result<Self, WebError> {
        Ok(Self {
            options,
            history: Vec::new(),
            index: 0,
            frame: None,
            events: VecDeque::new(),
        })
    }

    /// Renders a URL into an owned frame without touching navigation history.
    fn render(&self, url: &str) -> Result<WebFrame, WebError> {
        let mut options = RenderOptions {
            viewport: self.options.viewport,
            ..RenderOptions::default()
        };
        options.no_network = self.options.no_network;
        let output = render_url(url, &options).map_err(|e| WebError::Load(e.to_string()))?;
        Ok(WebFrame {
            url: output.document_url.as_str().to_string(),
            pixmap: output.pixmap,
            png: output.png,
            hit_regions: output.hit_regions,
            document: output.document,
        })
    }

    /// Renders `url` and makes it the current history entry.
    pub fn load(&mut self, url: &str) -> Result<&WebFrame, WebError> {
        let frame = self.render(url)?;
        self.history.truncate(self.index + 1);
        self.history.push(url.to_owned());
        self.index = self.history.len() - 1;
        self.frame = Some(frame);
        Ok(self.frame.as_ref().expect("frame just stored"))
    }

    /// Returns the current frame, if any page has been loaded.
    pub fn frame(&self) -> Option<&WebFrame> {
        self.frame.as_ref()
    }

    /// Topmost DOM node under `(x, y)`, in CSS pixels.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<megi_core::NodeId> {
        let frame = self.frame.as_ref()?;
        megi_display::hit_test(&frame.hit_regions, x, y)
    }

    /// Nearest `<a href>` under `(x, y)`.
    pub fn link_at(&self, x: f32, y: f32) -> Option<HitLink> {
        let frame = self.frame.as_ref()?;
        megi_display::find_link_at(&frame.document, &frame.hit_regions, x, y)
    }

    /// Enqueues a host-translated event for the next `tick()`.
    pub fn enqueue_event(&mut self, event: WebEvent) {
        if let WebEvent::Resize { width, height } = event
            && let Ok(viewport) = Viewport::new(width, height)
        {
            self.options.viewport = viewport;
        }
        self.events.push_back(event);
    }

    /// Drains queued events. The host uses them to drive scrolling/focus state.
    pub fn drain_events(&mut self) -> Vec<WebEvent> {
        self.events.drain(..).collect()
    }

    /// Advances the engine (timers/event loop). Currently a no-op: the renderer
    /// runs its bounded virtual event loop as part of `load`.
    pub fn tick(&mut self) {}

    /// Current history URL.
    pub fn current_url(&self) -> Option<&str> {
        self.history.get(self.index).map(String::as_str)
    }

    /// Whether a previous history entry exists.
    pub fn can_back(&self) -> bool {
        self.index > 0
    }

    /// Whether a newer history entry exists.
    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.history.len()
    }

    /// Navigates to the previous entry.
    pub fn back(&mut self) -> Option<&WebFrame> {
        if !self.can_back() { return None; }
        self.index -= 1;
        let url = self.history[self.index].clone();
        let frame = self.render(&url).ok()?;
        self.frame = Some(frame);
        self.frame.as_ref()
    }

    /// Navigates to the next entry.
    pub fn forward(&mut self) -> Option<&WebFrame> {
        if !self.can_forward() { return None; }
        self.index += 1;
        let url = self.history[self.index].clone();
        let frame = self.render(&url).ok()?;
        self.frame = Some(frame);
        self.frame.as_ref()
    }

    /// Reloads the current entry.
    pub fn reload(&mut self) -> Option<&WebFrame> {
        let url = self.history.get(self.index)?.clone();
        let frame = self.render(&url).ok()?;
        self.frame = Some(frame);
        self.frame.as_ref()
    }
}

/// Engine crates exposed through the facade. Embedders use these re-exports
/// instead of depending on individual engine crates.
pub use megi_core as core;
pub use megi_css as css;
pub use megi_display as display;
pub use megi_fonts as fonts;
pub use megi_html as html;
pub use megi_image as image;
pub use megi_layout as layout;
pub use megi_net as net;
pub use megi_paint as paint;
pub use megi_raster as raster;
pub use megi_render as render;
