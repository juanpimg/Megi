//! # CSS Media Queries Level 3/4 (subset)
//!
//! Parser and evaluator for media query lists, shared by the cascade (`@media` rules),
//! `@import` gating and consumers such as the `render` crate for `<link media="...">`.
//!
//! Supported grammar: comma-separated query lists; `[not | only]? <media-type>` with
//! `and <condition>`; standalone conditions with nested parentheses, `and`, `or` and `not`;
//! media types `all`, `screen` and `print` (any other type never matches); and the features
//! `min-width`, `max-width`, `width`, `min-height`, `max-height`, `height` (px/em/rem/pt/pc/
//! in/cm/mm; `em`/`rem` are resolved against the 16px initial font size) and `orientation`
//! (`portrait`/`landscape`). Range syntax (`(width >= 600px)`, `(width < 40em)`) and
//! `calc()`/math functions in feature values are supported (Media Queries 4 §4.2, §8).
//! Features that are syntactically valid but not modelled
//! (`resolution`, `prefers-color-scheme`, `hover`, `aspect-ratio`, boolean features, ...)
//! evaluate to `false`; this is the conservative choice used by the cascade.
//!
//! Evaluation rules (CSS Media Queries 4):
//! - An empty media query list matches every context (this is what `@import "x.css";` means).
//! - A list matches when at least one of its queries matches.
//! - Width/height/orientation features need a known viewport in [`MediaContext`]; when the
//!   viewport is unknown (`None`) they evaluate to `false`.
//! - `not` negates the whole query (media type included); `only` has no effect on modern user
//!   agents (it only hides the query from legacy parsers).
//! - Invalid queries inside a list are forgivingly replaced by a never-matching query
//!   (Media Queries 4 §3.2); a malformed single query therefore matches nothing.

use crate::token::CssToken;

mod chunk1;
pub use chunk1::ColorScheme;
pub use chunk1::MAX_MEDIA_NESTING;
pub use chunk1::MediaCondition;
pub use chunk1::MediaContext;
pub use chunk1::MediaFeature;
#[allow(unused_imports)]
pub(crate) use chunk1::MediaParser;
pub use chunk1::MediaQualifier;
pub use chunk1::MediaQuery;
pub use chunk1::MediaQueryList;
pub use chunk1::MediaType;
pub use chunk1::Orientation;
pub use chunk1::evaluate;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_condition;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_media_query;
pub use chunk1::parse_media_query_list;
#[allow(unused_imports)]
pub(crate) use chunk1::split_top_level_commas;
mod chunk2;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_condition_group;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_feature;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_length_value;
#[allow(unused_imports)]
pub(crate) use chunk2::single_ident;
