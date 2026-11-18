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

pub(crate) use std::sync::mpsc::{Receiver, Sender, TryRecvError};
pub(crate) use std::thread::{self, JoinHandle};
pub(crate) use std::time::{Duration, Instant};

pub(crate) use std::sync::Arc;

pub(crate) use crate::convert::pixmap_to_0rgb;
pub(crate) use crate::links::{LinkRegion, collect_link_regions};
pub(crate) use crate::scroll;
pub(crate) use crate::title::document_title;

pub(crate) use megi_webview::css::{Overflow, Position};
pub(crate) use megi_webview::layout::LayoutBox;

mod types;
pub use types::*;
mod render;
mod store;
pub use render::*;
mod tests_extra;
#[cfg(test)]
mod unit_tests;
pub use tests_extra::*;
