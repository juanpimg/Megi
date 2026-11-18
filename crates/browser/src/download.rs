//! Downloads/exports from the shell: the rendered page as PNG and the rendered DOM as HTML.
//!
//! `Ctrl+S` saves the current frame as `megi-<host>-<n>.png`; `Ctrl+Shift+S` saves the DOM
//! snapshot (including the doctype) as `megi-<host>-<n>.html`. Files go to `--download-dir`,
//! else `$HOME/Downloads` (falling back to the system temp directory). Names are sanitized
//! (`[a-z0-9-]`, capped) and never overwrite an existing download: `<n>` is the first free
//! integer starting at 1. Everything returns typed errors / `Result` instead of panicking;
//! the window reports the result in its title and stdout (no visual prompt).
//!
//! Limits: no resource downloads (only the page itself), no "Save as" dialog, no MIME
//! negotiation beyond the two exporters, and the HTML snapshot contains the post-script DOM
//! (scripts are not re-run when reopening it).

pub(crate) use crate::links::CssRect;
pub(crate) use std::fs;
pub(crate) use std::path::{Path, PathBuf};

mod paths;
pub use paths::*;
mod save;
pub use save::*;
mod progress;
pub use progress::*;
#[cfg(test)]
mod tests;
