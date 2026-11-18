//! CSS Grid value parsing (`display: grid | inline-grid`, templates, gaps and
//! item alignment) for the MVP grid layout in `crates/layout/src/grid.rs`.
//!
//! Supported subset:
//! * `display: grid | inline-grid`.
//! * `grid-template-columns` / `grid-template-rows`: lists of `<length>`,
//!   `<percentage>`, `<number>fr`, `auto`, `minmax(min, max)` and
//!   `repeat(<integer>, <track-list>)` / `repeat(auto-fill|auto-fit, …)`.
//! * `grid-auto-rows`: same track syntax for implicit rows.
//! * `grid-column` / `grid-row` (+ `-start`/`-end` longhands): numeric lines
//!   (negative counts from the end), named lines idents, `span <integer>` and
//!   `auto`.
//! * `grid-auto-flow`: `row`/`column` with optional `dense`.
//! * `grid-template-areas` (rectangular quoted rows of names/`.`) and
//!   `grid-area: <name>` (or the four-line shorthand).
//! * `justify-content`/`align-content` track distribution keywords.
//! * `order` for grid/flex items (stable reordering).
//! * `justify-items` / `align-items` (`stretch`/`start`/`end`/`center`,
//!   `flex-start`/`flex-end` tolerated) and `justify-self`/`align-self`.
//! * `row-gap`/`column-gap`/`gap` (shared with flexbox).
//!
//! Out of scope (documented in `layout/grid.rs`): `subgrid`, `grid-auto-columns`,
//! masonry, `repeat()` with named lines inside, and full CSS Grid
//! placement-conflict resolution; `auto-fit` empty-track collapse is applied by
//! layout after placement (repeated range only).

use crate::flex::{AlignItems, AlignSelf};
use crate::properties::{Length, find_declaration};
use crate::rule::Declaration;

mod chunk1;
#[allow(unused_imports)]
pub use chunk1::AutoRepeat;
#[allow(unused_imports)]
pub use chunk1::AutoRepeatKind;
#[allow(unused_imports)]
pub use chunk1::ComputedGrid;
#[allow(unused_imports)]
pub use chunk1::GridAutoFlow;
#[allow(unused_imports)]
pub use chunk1::GridContentDistribution;
#[allow(unused_imports)]
pub use chunk1::GridLine;
#[allow(unused_imports)]
pub use chunk1::GridTemplate;
#[allow(unused_imports)]
pub use chunk1::GridTemplateAreas;
#[allow(unused_imports)]
pub(crate) use chunk1::MAX_REPEAT;
#[allow(unused_imports)]
pub(crate) use chunk1::MAX_TRACKS;
#[allow(unused_imports)]
pub use chunk1::TrackMax;
#[allow(unused_imports)]
pub use chunk1::TrackMin;
#[allow(unused_imports)]
pub use chunk1::TrackSize;
#[allow(unused_imports)]
pub(crate) use chunk1::compute_grid;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_auto_flow;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_content_distribution;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_grid_area;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_placement;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_template_areas;
mod chunk2;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_align_items;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_align_self;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_grid_line;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_minmax;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_template;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_track;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_track_list;
#[allow(unused_imports)]
pub(crate) use chunk2::split_token;
mod chunk3;
