//! # Shorthand-to-longhand expansion (pre-cascade)
//!
//! CSS shorthands (`border`, `border-*`, `margin`, `padding`, `background`, `font`,
//! `list-style`) are expanded into their longhand properties *before* the cascade, so that
//! origin, importance, specificity and source order are decided per longhand, as required by
//! CSS Cascading and Inheritance Level 4 §4. Previously the cascade resolved declarations by
//! property name, so e.g. a UA `h1 { margin-block: 1em }` beat an author `h1 { margin: 0 }`.
//!
//! Modelled longhands are reset by their shorthand as CSS requires; longhands this engine does
//! not model yet (`background-image`, `font-variant`, `font-stretch`, `list-style-position`,
//! ...) are silently ignored. A syntactically invalid shorthand drops the whole declaration
//! (all of its longhands), per CSS Syntax Level 3 §5.4.2. CSS-wide keywords (`inherit`,
//! `initial`, ...) on a shorthand are not modelled yet, so those declarations are dropped
//! rather than partially applied.
//!
//! `font` supports `[style || variant || weight || stretch]? <size>[/<line-height>]? <family>`
//! but only `font-style`, `font-weight`, `font-size`, `line-height` and `font-family` are
//! modelled; system fonts (`font: menu`, ...) are rejected.

use crate::properties::{
    BorderStyle, Color, Length, ListStyleType, parse_background_shorthand_layers,
    parse_border_width, split_border_radius_value, split_components,
};
use crate::rule::Declaration;

mod chunk1;
#[allow(unused_imports)]
pub(crate) use chunk1::BorderParts;
#[allow(unused_imports)]
pub(crate) use chunk1::FONT_SIZE_KEYWORDS;
#[allow(unused_imports)]
pub(crate) use chunk1::FONT_STRETCH_KEYWORDS;
#[allow(unused_imports)]
pub(crate) use chunk1::MAX_TIMELINE_ENTRIES;
#[allow(unused_imports)]
pub(crate) use chunk1::SIDES;
#[allow(unused_imports)]
pub(crate) use chunk1::SYSTEM_FONT_KEYWORDS;
#[allow(unused_imports)]
pub(crate) use chunk1::border_longhands;
#[allow(unused_imports)]
pub(crate) use chunk1::expand_border;
#[allow(unused_imports)]
pub(crate) use chunk1::expand_border_edges;
#[allow(unused_imports)]
pub(crate) use chunk1::expand_border_radius;
#[allow(unused_imports)]
pub(crate) use chunk1::expand_box_edges;
#[allow(unused_imports)]
pub use chunk1::expand_declarations;
#[allow(unused_imports)]
pub(crate) use chunk1::expand_properties;
#[allow(unused_imports)]
pub(crate) use chunk1::four_values;
#[allow(unused_imports)]
pub(crate) use chunk1::is_css_wide_keyword;
#[allow(unused_imports)]
pub(crate) use chunk1::is_image_component;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_border_parts;
mod chunk2;
#[allow(unused_imports)]
pub(crate) use chunk2::expand_animation;
#[allow(unused_imports)]
pub(crate) use chunk2::expand_background;
#[allow(unused_imports)]
pub(crate) use chunk2::expand_font;
#[allow(unused_imports)]
pub(crate) use chunk2::expand_transition;
#[allow(unused_imports)]
pub(crate) use chunk2::is_font_line_height;
#[allow(unused_imports)]
pub(crate) use chunk2::is_font_size;
mod chunk3;
#[allow(unused_imports)]
pub(crate) use chunk3::expand_list_style;
