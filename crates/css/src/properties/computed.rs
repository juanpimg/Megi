//! `ComputedStyle` and CSS cascade computation (extracted from `properties`).

use std::collections::HashMap;

use crate::rule::Declaration;

use super::calc::*;
use super::parse::*;
use super::values::*;

mod chunk1;
#[allow(unused_imports)]
pub use chunk1::ComputedStyle;
#[allow(unused_imports)]
pub(crate) use chunk1::StyleFingerprint;
#[allow(unused_imports)]
pub(crate) use chunk1::find_declaration;
mod chunk2;
mod chunk3;
mod chunk4;
mod chunk5;
