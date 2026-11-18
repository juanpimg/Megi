//! # Megi CSS (`css`)
//!
//! Foundational types, W3C CSS Syntax Module Level 3 tokenizer & parser, and Selectors Level 4
//! matching engine operating over [`megi_core::Document`] and [`megi_core::NodeId`].
//!
//! Round 2 additions: functional `:is()`/`:where()`/`:not()`/`:has()` and the remaining
//! structural pseudo-classes, pre-cascade shorthand expansion (see [`shorthand`]) so that
//! origin/specificity/source order decide per longhand, the `font` shorthand, and basic
//! `@media` support ([`MediaContext`], [`resolve_document_styles_with_viewport`]), **Cascade
//! Layers** (`@layer` declarations/blocks, nesting, compound/anonymous names, layer-order
//! cascade, `!important` inversion and `revert-layer`; fast path when no layer is declared)
//! and a
//! **Container Queries MVP** ([`container`]): `container-type: normal|inline-size|size`,
//! `container-name` and `@container [<name>] (min/max-width|inline-size|height|block-size)
//! ...)` with `and`. Sizes are predicted from the viewport down the block chain before
//! layout (single deterministic pass, no oscillation); style/scroll-state queries, nested
//! `@container` at-rules and `cqi`/`cqw` units are documented as unsupported. No `@container`
//! rule leaves existing cascades/geometry byte-identical.

#![forbid(unsafe_code)]

pub mod animation;
pub mod animation_runtime;
pub mod calc;
pub mod cascade;
mod color;
pub mod component_value;
pub mod container;
pub mod custom;
mod flex;
mod grid;
pub mod media;
pub mod parser;
pub mod properties;
pub mod query;
pub mod rule;
pub mod selector;
pub mod shorthand;
pub mod state;
pub mod token;
pub mod tokenizer;
pub mod transform;
pub mod user_agent;

pub use calc::{
    CalcContext, CalcValue, evaluate as evaluate_calc, with_viewport as with_calc_viewport,
};
pub use cascade::{
    CascadeOriginImportance, CompiledRule, MAX_INLINE_STYLE_BYTES, MatchedDeclaration, Origin,
    PseudoStyles, StyleCollection, apply_animations_to_pseudo_styles, parse_inline_declarations,
    parse_inline_style, resolve_backdrop_color, resolve_cascaded_properties,
    resolve_document_styles, resolve_document_styles_from_collection,
    resolve_document_styles_with_media, resolve_document_styles_with_media_and_state,
    resolve_document_styles_with_pseudos, resolve_document_styles_with_viewport,
    resolve_pseudo_cascaded_properties,
};
pub use component_value::{ComponentValue, FunctionCall, SimpleBlock};
pub use flex::{AlignItems, AlignSelf, FlexDirection, FlexWrap, JustifyContent};
pub use grid::{
    AutoRepeat, AutoRepeatKind, GridAutoFlow, GridContentDistribution, GridLine, GridTemplate,
    GridTemplateAreas, TrackMax, TrackMin, TrackSize,
};
pub use media::{
    ColorScheme, MediaCondition, MediaContext, MediaFeature, MediaQualifier, MediaQuery,
    MediaQueryList, MediaType, Orientation, evaluate as evaluate_media, parse_media_query_list,
};
pub use parser::{TokenStream, consume_declaration, parse_list_of_declarations, parse_stylesheet};
pub use properties::CalcLengths;
pub use properties::{
    AspectRatio, BackgroundAttachment, BackgroundClip, BackgroundLayer, BackgroundOrigin,
    BackgroundPosition, BackgroundRepeat, BackgroundRepeatAxis, BackgroundSize, BorderCollapse,
    BorderRadii, BorderStyle, BoxShadow, BoxSizing, CaptionSide, Clear, Color, ComputedStyle,
    ConicGradient, ContainerType, Content, ContentItem, Direction, Display, EmptyCells,
    FilterFunction, Float, FontStyle, Gradient, GradientKind, GradientStop, InterpolationSpace,
    Length, LinearDirection, LinearGradient, ListStyleType, MAX_FILTER_FUNCTIONS,
    MAX_GRADIENT_STOPS, MAX_GRADIENTS, MixBlendMode, ObjectFit, Overflow, Position, RadialExtent,
    RadialGradient, RadialShape, Radius, RectEdges, TableLayout, TextAlign, TextDecoration,
    TextShadow, TextTransform, UnicodeBidi, VerticalAlign, Visibility, WhiteSpace,
};
pub use query::{
    SelectorParseError, matches, matches_with_state, query_selector, query_selector_all,
    try_matches, try_matches_with_state, try_query_selector, try_query_selector_all,
};
pub use rule::{AtRule, Declaration, FontFaceRule, ImportRule, Rule, StyleRule, Stylesheet};
pub use selector::{
    AnPlusB, AttrOperator, AttributeSelector, Combinator, ComplexSelector, CompoundSelector,
    MAX_MATCH_STEPS, MAX_PASS_MATCH_STEPS, MAX_SELECTOR_NESTING, PseudoClass, PseudoElement,
    RelativeSelector, SelectorList, Specificity, parse_selectors, style_pass_scope,
};
pub use shorthand::expand_declarations;
pub use state::{StateContext, StateGuard, current as current_state, with_state};
pub use token::{CssToken, HashType, NumberType};
pub use tokenizer::{CssTokenizer, preprocess_css_codepoints, preprocess_css_input, tokenize};
pub use user_agent::{
    USER_AGENT_STYLESHEET, create_user_agent_style_collection, parse_user_agent_stylesheet,
};

#[path = "lib/chunk1.rs"]
mod chunk1;
#[allow(unused_imports)] pub use chunk1::SimpleSelector;
#[cfg(test)] #[allow(unused_imports)] pub(crate) use chunk1::lib_tests_helpers;
#[path = "lib/chunk2.rs"]
mod chunk2;
#[cfg(test)] #[allow(unused_imports)] pub(crate) use chunk2::tests_part1;
#[path = "lib/chunk3.rs"]
mod chunk3;
#[cfg(test)] #[allow(unused_imports)] pub(crate) use chunk3::tests_part2;
#[path = "lib/chunk4.rs"]
mod chunk4;
#[cfg(test)] #[allow(unused_imports)] pub(crate) use chunk4::tests_part3;
#[path = "lib/chunk5.rs"]
mod chunk5;
#[cfg(test)] #[allow(unused_imports)] pub(crate) use chunk5::tests_part4;
#[path = "lib/chunk6.rs"]
mod chunk6;
#[cfg(test)] #[allow(unused_imports)] pub(crate) use chunk6::tests_part5;
#[cfg(test)] #[allow(unused_imports)] pub(crate) use chunk6::background_geometry_tests;
#[path = "lib/chunk7.rs"]
mod chunk7;
#[cfg(test)] #[allow(unused_imports)] pub(crate) use chunk7::background_layers_tests;
#[cfg(test)] #[allow(unused_imports)] pub(crate) use chunk7::decoration_tests;
