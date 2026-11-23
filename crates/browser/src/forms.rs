//! Form controls in the desktop shell: hit-testing, focus, a simple text editor and DOM
//! patching (no backend, no JS).
//!
//! `render` executes page scripts and produces a `RenderOutput` with the DOM, the layout
//! tree and the painted pixels. The shell worker extracts one [`FormControl`] per rendered
//! `<input>`/`<textarea>`/`<select>`/`<button>` from the layout tree (element boxes carry
//! their DOM `NodeId`, like the link extractor) and stores it next to the frame. Each
//! control gets a **stable `data-megi-form-id`** attribute assigned in the worker; because
//! an edited page is re-rendered from a serialized DOM snapshot, `NodeId`s change across
//! renders but `form_id` does not, so focus and edits survive re-rendering.
//!
//! Editing path: the window patches the snapshot (`value`/`checked`/`data-megi-focused`/
//! `data-megi-active`), serializes it with `megi_webview::html::serialize_html`, renders it as an inline
//! `data:` document with `base_override` = the original page URL (so relative stylesheets/
//! images still resolve) and scripts disabled (the DOM snapshot already contains their
//! effects; page timers pause while editing — documented limit).
//!
//! Deliberate limits: no real widgets (paint renders CSS boxes only), no caret blink (the
//! focus ring is the caret), `select` shows its first option and has no dropdown,
//! checkbox/radio toggle their `checked` attribute, `Enter` never submits (no backend),
//! no validation/IME/selection ranges, and `:checked`/`:disabled`/`:focus` are attribute
//! selectors in the UA stylesheet ([`crate::links`] shows the same layout-based extraction
//! pattern).

pub(crate) use megi_webview::core::{Attribute, Document, NodeId};
pub(crate) use megi_webview::layout::LayoutBox;

pub(crate) use crate::links::CssRect;

mod model;
pub use model::*;
mod controls;
pub use controls::*;
mod edit;
pub use edit::*;
mod live_dom;
pub use live_dom::*;
mod live;
pub use live::*;
mod widget;
pub use widget::*;
mod submit;
pub use submit::*;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod tests;
