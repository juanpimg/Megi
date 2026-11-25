//! Read the adopted WPT fixture set from `tests/wpt/MANIFEST.json`.

use std::path::{Path, PathBuf};

/// Root of the pinned, vendored WPT fixtures.
pub fn wpt_root(root: &Path) -> PathBuf {
    root.join("tests/wpt")
}

/// Directory for a vendored WPT suite, rejecting traversal outside `tests/wpt`.
pub fn wpt_dir(root: &Path, dir: &str) -> Result<PathBuf, String> {
    let parts = Path::new(dir);
    if dir.is_empty() || parts.is_absolute() || parts.components().any(|c| c.as_os_str() == "..") {
        return Err(format!("rejected WPT directory {dir:?}"));
    }
    Ok(wpt_root(root).join(dir))
}

#[derive(serde::Deserialize)]
struct Entry {
    path: String,
    dir: String,
    kind: String,
    #[serde(default)]
    subtest_count_expected: usize,
}

fn entries(root: &Path) -> Result<Vec<Entry>, String> {
    let path = wpt_root(root).join("MANIFEST.json");
    let bytes =
        std::fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid {}: {error}", path.display()))
}

/// Fixture paths (relative to `tests/wpt/`) adopted for a suite, sorted for determinism.
pub fn adopted_fixtures(root: &Path, dir: &str) -> Result<Vec<String>, String> {
    wpt_dir(root, dir)?;
    let mut adopted: Vec<String> = entries(root)?
        .into_iter()
        .filter(|entry| entry.dir == dir && entry.kind == "testharness")
        .map(|entry| entry.path)
        .collect();
    adopted.sort();
    if adopted.is_empty() {
        return Err(format!("no adopted WPT fixtures for {dir:?}"));
    }
    Ok(adopted)
}

/// Support JSON data (basename → raw JSON), injected into the offline `fetch` shim.
pub fn support_resources(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut resources = Vec::new();
    for entry in entries(root)? {
        if entry.kind != "support" || !entry.path.ends_with(".json") {
            continue;
        }
        let path = wpt_root(root).join(&entry.path);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", entry.path))?;
        let name = entry
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&entry.path)
            .to_owned();
        resources.push((name, text));
    }
    resources.sort();
    Ok(resources)
}

/// Support JavaScript (WPT `resources/*.js`) injected before a fixture's own scripts.
pub fn support_scripts(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut scripts = Vec::new();
    for entry in entries(root)? {
        if entry.kind != "support" || !entry.path.ends_with(".js") {
            continue;
        }
        let path = wpt_root(root).join(&entry.path);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", entry.path))?;
        scripts.push((entry.path.clone(), text));
    }
    scripts.sort();
    Ok(scripts)
}

/// Support scripts actually referenced by one fixture's HTML source.
pub fn support_scripts_for(root: &Path, html: &str) -> Result<Vec<(String, String)>, String> {
    Ok(support_scripts(root)?
        .into_iter()
        .filter(|(path, _)| {
            let name = path.rsplit('/').next().unwrap_or(path);
            html.contains(name)
        })
        .collect())
}

/// All directories with adopted fixtures, sorted for deterministic report ordering.
pub fn adopted_dirs(root: &Path) -> Result<Vec<String>, String> {
    let mut dirs: Vec<String> = entries(root)?
        .into_iter()
        .filter(|entry| entry.kind == "testharness")
        .map(|entry| entry.dir)
        .collect();
    dirs.sort();
    dirs.dedup();
    Ok(dirs)
}

/// Sum of the manifest's adopted subtest counts for one suite.
pub fn expected_subtests(root: &Path, dir: &str) -> usize {
    entries(root)
        .ok()
        .into_iter()
        .flatten()
        .filter(|entry| entry.dir == dir && entry.kind == "testharness")
        .map(|entry| entry.subtest_count_expected)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wpt_dir_rejects_traversal() {
        assert!(wpt_dir(Path::new("/root"), "../outside").is_err());
        assert!(wpt_dir(Path::new("/root"), "dom/nodes").is_ok());
    }

    #[test]
    fn support_resources_are_readable_json() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("test_harness lives in <root>/crates/test_harness");
        let resources = support_resources(root).expect("support data must be vendored");
        let (name, json) = resources
            .iter()
            .find(|(name, _)| name == "base64.json")
            .expect("the atob fixture's support data must be declared");
        assert_eq!(name, "base64.json");
        assert!(serde_json::from_str::<serde_json::Value>(json).is_ok());
    }
}
