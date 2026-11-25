//! # Inline Formatting Context (IFC) Layout Engine
//!
//! Normative implementation of W3C CSS 2.1 §9.4.2 (Inline formatting contexts),
//! §10.8 (Line height calculations, strut, and vertical-align), and
//! CSS Text Module Level 3 (§3–§5: white-space processing and line breaking).
//!
//! The engine is split into cohesive submodules that are declared here (the
//! crate root only re-exports the public API):
//! - `text`: font metrics, text measurement and `text-transform`.
//! - `token`: the shared inline token type (leaf module, no IFC dependencies).
//! - `tokens` / `tokenize_text`: the linearized token stream and text tokenization.
//! - `line_breaking`: greedy wrapping at soft wrap opportunities.
//! - `baseline`: strut, half-leading and `vertical-align` extent math.
//! - `inline_boxes` / `format_line`: line box and fragment assembly.
//! - `bidi`: UAX #9 reordering of a line's token run.
//! - `ifc`: the formatting-context entry points that drive the pipeline.

use std::borrow::Cow;
use std::sync::Arc;

use megi_core::NodeId;
use megi_css::{ComputedStyle, Length, TextAlign, TextTransform, VerticalAlign, WhiteSpace};

use crate::box_tree::{BoxType, LayoutBox};
use megi_core::geometry::Rect;

mod baseline;
mod bidi;
mod format_line;
mod ifc;
mod inline_boxes;
mod line_breaking;
mod text;
mod token;
mod tokenize_text;
mod tokens;

// Public API (also re-exported from the crate root).
pub use ifc::{layout_inline_formatting_context, layout_inline_formatting_context_with_measurer};
pub use text::{DefaultTextMeasurer, FontMetrics, TextMeasurer};

// Crate-internal API used by other layout modules and shared by the submodules.
pub(crate) use baseline::{align_extents, anchor_for_style, line_box_extents};
pub(crate) use bidi::{bidi_control_direction, reorder_line_tokens_bidi};
pub(crate) use format_line::format_line;
pub(crate) use ifc::layout_inline_formatting_context_with_measurer_and_pos;
pub(crate) use inline_boxes::{
    ActiveBoxFrame, ActiveBoxInfo, LineContext, close_remaining_boxes, push_text_fragment,
    try_format_simple_text_line,
};
pub(crate) use line_breaking::{break_into_lines, token_advance};
pub(crate) use text::{measure_text_width, metrics_styled, shaped_width, transform_text};
pub(crate) use token::InlineToken;
pub(crate) use tokens::{collect_inline_tokens, resolve_length};
