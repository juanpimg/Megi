//! # WHATWG HTML §15 Default User-Agent Stylesheet
//!
//! Provides the normative default UA styles for HTML elements per
//! <https://html.spec.whatwg.org/multipage/rendering.html>, plus an **approximate form
//! controls** section (R102) for the desktop shell. This engine paints no native widgets:
//! inputs/textarea/select/button are ordinary CSS boxes with borders/padding/backgrounds;
//! checkbox/radio are squares/circles whose `checked` state comes from the DOM `checked`
//! attribute (`:checked`/`:disabled`/`:focus`/`:enabled` are implemented as dynamic pseudo-
//! classes; attribute selectors are also used where stable author hooks are useful). The shell additionally
//! sets `data-megi-focused`/`data-megi-active` attributes while editing so the focus ring and
//! pressed state can be styled here. This is deliberately Chrome-approximate, not pixel-
//! exact.

use std::sync::OnceLock;

use crate::cascade::{Origin, StyleCollection};
use crate::rule::Stylesheet;

mod chunk1;
#[allow(unused_imports)] pub use chunk1::USER_AGENT_STYLESHEET;
#[allow(unused_imports)] pub use chunk1::parse_user_agent_stylesheet;
#[allow(unused_imports)] pub(crate) use chunk1::cached_user_agent_stylesheet;
#[allow(unused_imports)] pub use chunk1::create_user_agent_style_collection;
mod chunk2;
