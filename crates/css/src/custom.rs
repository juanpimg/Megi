//! CSS custom properties (`--x`) and `var()` substitution.
//!
//! Custom properties are cascaded like normal declarations (they appear in
//! [`crate::cascade::resolve_cascaded_properties`] with their `--` name preserved) and are
//! **inherited**. `resolve_declarations` merges the parent's custom properties with the
//! element's own, then substitutes `var(--name[, fallback])` in every normal declaration
//! before the longhands are computed. A shorthand whose value contains `var()` is kept
//! unexpanded through the cascade and expanded here after substitution.
//!
//! Limits: at most [`MAX_SUBSTITUTION_DEPTH`] nested substitutions; cycles are detected and
//! fall back to the `var()` fallback (or make the property invalid → inherited/initial), never
//! panic.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::rule::Declaration;

mod chunk1;
pub use chunk1::MAX_CUSTOM_PROPERTIES;
pub use chunk1::MAX_SUBSTITUTION_BYTES;
pub use chunk1::MAX_SUBSTITUTION_DEPTH;
#[allow(unused_imports)]
pub(crate) use chunk1::ResolvedDeclarations;
#[allow(unused_imports)]
pub(crate) use chunk1::contains_var;
#[allow(unused_imports)]
pub(crate) use chunk1::expand_inner;
pub use chunk1::expand_variables;
#[allow(unused_imports)]
pub(crate) use chunk1::is_ident_byte;
#[allow(unused_imports)]
pub(crate) use chunk1::needs_resolution;
#[allow(unused_imports)]
pub(crate) use chunk1::next_var_function;
#[allow(unused_imports)]
pub(crate) use chunk1::normalize_custom_value;
#[allow(unused_imports)]
pub(crate) use chunk1::skip_function;
#[allow(unused_imports)]
pub(crate) use chunk1::spend;
#[allow(unused_imports)]
pub(crate) use chunk1::split_first_comma;
mod chunk2;
#[allow(unused_imports)]
pub(crate) use chunk2::resolve_declarations;
