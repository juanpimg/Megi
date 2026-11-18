//! CLI argument parsing and URL/path normalization (pure, testable, no display needed).
//!
//! ```text
//! megi <url|file> [--width W] [--height H] [--screenshot PATH] [--title T]
//! ```
//!
//! Without `--screenshot` the CLI opens a winit window; with it, Megi runs headless and
//! writes a PNG (no `EventLoop` is created). Local filesystem paths are converted to
//! `file://` URLs; anything that parses as an absolute URL is used as-is.

pub(crate) use std::path::PathBuf;

pub(crate) use crate::error::ShellError;

mod args;
pub use args::*;
mod parse;
pub use parse::*;
mod finish;
pub use finish::*;
#[cfg(test)]
mod tests;
