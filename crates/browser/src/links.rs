//! Link hit-testing and URL resolution (pure, display-free).
//!
//! The render worker extracts, from a finished [`megi_webview::render::RenderOutput`], one
//! [`LinkRegion`] per painted primitive that resolves to an `<a href>`: its page
//! rectangle in CSS pixels (origin = top-left of the full document, no scroll) and the raw
//! `href` attribute. The primary source is the additive hit-testing layer of `paint`
//! (`RenderOutput::hit_regions` produced by the same paint pass as the pixels): for every
//! [`megi_webview::paint::HitRegion`] the nearest HTML `<a href>` ancestor is resolved with
//! [`megi_webview::paint::find_link_at`]. When the render output carries no hit regions (e.g. a
//! hand-built `RenderOutput` in a test, or a page with no painted content) a documented
//! fallback walks the layout tree (`LayoutBox::node_id()` → DOM `<a href>`) instead.
//!
//! The window converts a physical cursor position into page CSS coordinates with
//! [`physical_to_page`] (divide by the HiDPI scale factor, add the scroll offset) and asks
//! [`link_url_at`] which URL a left click activates. Regions are stored next to the frame,
//! so after every re-render the hit map matches the pixels on screen.
//!
//! ## Behaviour and limits
//!
//! - Topmost wins: regions are searched in reverse paint order.
//! - Empty/`href`-less anchors are not regions; anchors with zero-area boxes are skipped.
//! - The raw `href` is resolved against the page URL with [`megi_webview::net::Url::join`]; `javascript:`
//!   and unknown schemes are not navigable (documented no-op), while `http(s)`, `file:`
//!   and `data:` are.
//! - Fragment-only links (`#section`) resolve to the same document URL with a fragment and
//!   therefore count as a new navigation in this round (no in-page smooth scrolling yet).
//! - No forms, no `<area>`, no `javascript:` execution, no downloads.

pub(crate) use megi_webview::core::Namespace;
pub(crate) use megi_webview::layout::LayoutBox;

mod geometry;
pub use geometry::*;
mod collect;
pub use collect::*;
#[cfg(test)]
mod tests;
