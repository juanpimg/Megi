//! Read the curated, SHA-pinned case list. Pin verification lives in `tools/test262_report.py`.

use std::path::{Path, PathBuf};

/// One curated Test262-style case as declared in `tests/test262/MANIFEST.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// Path relative to `tests/test262/` (also the report `subtest`).
    pub path: String,
    /// Exact SHA-256 of the fixture bytes, verified by the Python report generator.
    pub sha256: String,
    /// Expected byte length, verified by the Python report generator.
    pub bytes: usize,
    /// Group used for the `--- Test262 <dir> Suites ---` sections.
    pub dir: String,
}

/// Root of the curated fixtures.
pub fn test262_root(root: &Path) -> PathBuf {
    root.join("tests").join("test262")
}

/// Load cases in manifest order. A malformed or empty manifest is a harness error.
pub fn load_cases(root: &Path) -> Result<Vec<Case>, String> {
    let path = test262_root(root).join("MANIFEST.json");
    let bytes =
        std::fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid manifest {}: {error}", path.display()))?;
    let schema = value.get("schema").and_then(serde_json::Value::as_str);
    if schema != Some("megi-test262-manifest/1") {
        return Err(format!("unsupported manifest schema in {}", path.display()));
    }
    let entries = value
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("manifest {} has no cases array", path.display()))?;
    if entries.is_empty() {
        return Err(format!("manifest {} declares no cases", path.display()));
    }
    let mut cases = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let field = |name: &str| {
            entry
                .get(name)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("cases[{index}] needs a string {name}"))
        };
        let bytes = entry
            .get("bytes")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("cases[{index}] needs an integer bytes"))?;
        cases.push(Case {
            path: field("path")?.to_owned(),
            sha256: field("sha256")?.to_owned(),
            bytes: usize::try_from(bytes)
                .map_err(|_| format!("cases[{index}].bytes is too large"))?,
            dir: field("dir")?.to_owned(),
        });
    }
    Ok(cases)
}
