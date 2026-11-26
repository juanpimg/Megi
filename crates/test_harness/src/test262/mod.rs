//! Minimal Test262-style smoke runner for ECMAScript language expressions.
//!
//! This is **not** the full Test262 suite. `tests/test262/MANIFEST.json` pins a curated set of
//! locally authored, Test262-style cases (frontmatter + `assert.*`) that exercise semantics Boa
//! 0.22 supports. The declared scope is reproduced verbatim in every published report so a green
//! local total can never be mistaken for full Test262 conformance.
//!
//! SHA-256 pins are verified by `tools/test262_report.py` (stdlib `hashlib`); this module reads
//! only the manifest's case list and evaluates each case in a fresh realm.

pub mod manifest;
#[cfg(feature = "js")]
pub mod runner;
#[cfg(feature = "js")]
pub mod summary;
