//! Generated browser pages: `file://` directory listings and load-error pages.
//!
//! Both pages are plain deterministic HTML rendered through the normal pipeline from a
//! `data:text/html;charset=utf-8,` URL (no temporary files, no scripting). Directory
//! listing links are **absolute** `file://` URLs so navigation works from the generated
//! data document; names are HTML-escaped and percent-encoded in hrefs.
//!
//! Limits: directory listings are `file://` only (no `http(s)` indexes), entries are
//! read once with `std::fs` (no live refresh), and error pages do not run scripts.

pub(crate) use std::fmt::Write as _;
pub(crate) use std::io;
pub(crate) use std::path::{Path, PathBuf};

mod about;
pub use about::*;
mod html;
pub use html::*;
mod listing;
pub use listing::*;
mod input;
pub use input::*;
#[cfg(test)]
mod about_tests;
#[cfg(test)]
mod i18n_tests;
#[cfg(test)]
mod tests;
