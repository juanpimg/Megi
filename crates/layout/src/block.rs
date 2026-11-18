//! # Block Formatting Context (BFC) Layout Algorithm
//!
//! Normative implementation of CSS 2.2 §8.3.1 (collapsing margins), §10.3.3
//! (block widths and over-constrained equations), §10.4/§10.7 (min/max sizes),
//! §10.6 (auto heights) and CSS Sizing Module Level 3 (`box-sizing`).
//!
//! ## Scope and known simplifications
//!
//! * Floats are placed in the current formatting context and in-flow block children
//!   are narrowed to the right/left float edge. Line boxes are shortened at block
//!   granularity (a full float-aware inline layout is out of scope).
//! * Absolutely positioned boxes are laid out after the normal flow of their parent;
//!   `bottom`/percentage offsets against an auto-height positioned ancestor are treated
//!   as `auto` (the ancestor height is not yet known at that point).
//! * Floats/positioned boxes nested inside an inline formatting context are ignored by
//!   the IFC (block-in-inline normalization is still out of scope).
//! * `display: flex/grid/table*` are treated as independent formatting contexts for
//!   margin collapsing purposes.
//! * Margins of self-collapsing boxes (no border, padding, content or height) are not
//!   collapsed through in this milestone.
//! * `direction`/`unicode-bidi` are not modelled: the over-constrained horizontal
//!   equation always uses the LTR rule (ignore `margin-right`).
//! * Percentage `height` resolves against a definite containing block height; against
//!   an auto-height containing block it behaves as `auto` (CSS 2.2 §10.5).
//! * `aspect-ratio` (CSS Box Sizing 4 §5): a definite width derives the automatic
//!   height (and a definite height the automatic width of in-flow blocks); both
//!   definite sizes keep their declared values. Replaced boxes prefer their
//!   intrinsic ratio with `auto` (or the declared ratio when it is explicit).

mod absolute;
mod core;
mod floats;
mod height;
mod intrinsic;
mod margin;
mod positioning;
mod sizing;
mod width;

// Preserve the existing API surface at `crate::block::*`.
pub use self::core::{layout_block, layout_block_with_measurer};
pub use self::floats::MAX_FLOATS_PER_CONTEXT;

pub(crate) use self::absolute::*;
pub(crate) use self::core::*;
pub(crate) use self::floats::*;
pub(crate) use self::height::*;
pub(crate) use self::intrinsic::*;
pub(crate) use self::margin::*;
pub(crate) use self::positioning::*;
pub(crate) use self::sizing::*;
pub(crate) use self::width::*;
