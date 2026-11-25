//! Minimal WPT component runner.
//!
//! Vendored `testharness`-style fixtures are executed as JavaScript through
//! `megi_js::JsRuntime` with the small shim in [`shim`]. Results come from the
//! shim's own `test()` calls; nothing is synthesized from fixture source text.
//!
//! `tests/wpt/MANIFEST.json` is the single source of truth for which fixtures
//! are adopted and how many subtests each one declares.

pub mod manifest;
#[cfg(feature = "js")]
pub mod runner;
pub mod shim;
#[cfg(feature = "js")]
pub mod summary;
