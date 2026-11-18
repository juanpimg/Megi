//! Find-in-page: case-insensitive text search over the rendered DOM and highlights.
//!
//! The search runs on the **DOM text nodes** of the frame (`megi_webview::core::NodeKind::Text`), in
//! document order, and returns `(NodeId, byte range)` pairs. Highlight rectangles are
//! derived from the **layout tree** (`BoxType::TextNode` inline fragments): the shell paints
//! one translucent yellow rectangle per layout text fragment that contains a hit, which is
//! the documented approximation ("line/fragment rectangles", no glyph-level runs). All hits
//! use `rgba(255, 235, 59, 0.45)` and the current hit uses a stronger blend, both in the
//! window (overlay drawn on the presented framebuffer, no re-render needed for the
//! highlight itself) and in headless screenshots (`--find QUERY`, painted into the pixmap
//! before PNG encoding).
//!
//! Deliberate limits: no regex, no whole-word/Unicode collation modes, no search inside
//! images or canvas, no incremental matching while the page scrolls (the frame carries the
//! hits computed at render time), and case-insensitive matching uses Unicode lowercase with
//! a best-effort byte-offset map (ASCII/UTF-8 text is exact).
//!
//! The pure state is testable without a display ([`FindState`], [`search`],
//! [`highlight_rects`], [`overlay_highlights_pixmap`]).

pub(crate) use std::ops::Range;

pub(crate) use megi_webview::core::{Document, NodeId};
pub(crate) use megi_webview::layout::{BoxType, LayoutBox};

pub(crate) use crate::links::CssRect;

mod search;
pub use search::*;
mod overlay;
pub use overlay::*;
mod state;
pub use state::*;
#[cfg(test)]
mod tests;
