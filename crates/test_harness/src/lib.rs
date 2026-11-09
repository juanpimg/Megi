//! # Megi Closed-Loop Test Harness (`test_harness`)
//!
//! Provides the closed-loop automated conformance runner for:
//! - `html5lib-tests/tokenizer/*.test` JSON fixtures ([`tokenizer_harness`]).
//! - `html5lib-tests/tree-construction/*.dat` and `.json` fixtures ([`tree_harness`]).

pub mod tokenizer_harness;
pub mod tree_harness;

use std::path::{Path, PathBuf};

/// Locates the workspace root directory containing `tests/html5lib-tests`.
pub fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("crates/test_harness must reside two levels below workspace root")
        .to_path_buf()
}

/// Returns the path to `tests/html5lib-tests/tokenizer`.
pub fn tokenizer_tests_dir() -> PathBuf {
    workspace_root().join("tests/html5lib-tests/tokenizer")
}

/// Returns the path to `tests/html5lib-tests/tree-construction`.
pub fn tree_construction_dat_dir() -> PathBuf {
    workspace_root().join("tests/html5lib-tests/tree-construction")
}

/// Returns the path to `tests/html5lib-tests/tree-construction-json`.
pub fn tree_construction_json_dir() -> PathBuf {
    workspace_root().join("tests/html5lib-tests/tree-construction-json")
}
