//! # `html5lib-tests` & WPT Tree Construction Conformance Harness (`tree_harness`)
//!
//! Supports both:
//! 1. Native `.dat` fixtures (`tests/html5lib-tests/tree-construction/*.dat` and
//!    `tests/wpt/html/syntax/parsing/resources/*.dat`) using the exact line-by-line
//!    state machine from WPT `test.js` (`parseDat()`).
//! 2. Exported `.json` fixtures (`tests/html5lib-tests/tree-construction-json/*.json`).
//!
//! Feeds each test case into [`html::parse_document`] or [`html::parse_fragment`],
//! serializes the resulting [`dom::Document`] via [`html::serialize_dom_for_test`],
//! and compares actual vs expected `#document` tree output.

use std::collections::HashMap;
use std::path::Path;

use dom::Namespace;
use html::{parse_document, parse_fragment, serialize_dom_for_test};
use serde::Deserialize;

/// Script execution mode directive for a tree-construction test case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScriptMode {
    Both,
    ScriptOff,
    ScriptOn,
}

/// A parsed tree-construction test case (from either `.dat` or `.json`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct TreeConstructionCase {
    pub index: usize,
    pub description: Option<String>,
    pub data: String,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub new_errors: Vec<String>,
    pub document_fragment: Option<String>,
    pub script_mode: ScriptMode,
    pub document: String,
}

#[derive(Clone, Debug, Deserialize)]
struct JsonTreeSuiteFile {
    #[allow(dead_code)]
    pub source_file: String,
    pub tests: Vec<TreeConstructionCase>,
}

/// Result of running a single tree-construction test case in a given `scripting_enabled` mode.
#[derive(Clone, Debug)]
pub struct TreeRunResult {
    pub file_name: String,
    pub index: usize,
    pub scripting_enabled: bool,
    pub passed: bool,
    pub input: String,
    pub expected_document: String,
    pub actual_document: String,
}

const KNOWN_DAT_HEADINGS: &[&str] = &[
    "description",
    "data",
    "errors",
    "new-errors",
    "errors-new",
    "document-fragment",
    "script-off",
    "script-on",
    "document",
];

/// Parses a `.dat` tree-construction file using the exact line-by-line state machine
/// from WPT `html/syntax/parsing/resources/test.js` (`parseDat`).
pub fn parse_dat_str(raw: &str) -> Vec<TreeConstructionCase> {
    let mut lines: Vec<&str> = raw.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }

    let mut test_maps: Vec<HashMap<String, String>> = Vec::new();
    let mut current: HashMap<String, String> = HashMap::new();
    let mut key = String::new();

    for line in lines {
        let maybe_heading = line
            .strip_prefix('#')
            .map(str::trim)
            .filter(|h| KNOWN_DAT_HEADINGS.contains(h));

        if let Some(heading) = maybe_heading {
            if ((heading == "data" && key != "description") || heading == "description")
                && !key.is_empty()
            {
                if let Some(val) = current.get_mut(&key)
                    && val.ends_with('\n')
                {
                    val.pop();
                }
                test_maps.push(std::mem::take(&mut current));
            }
            key = heading.to_owned();
            current.insert(key.clone(), String::new());
        } else if !key.is_empty()
            && let Some(val) = current.get_mut(&key)
        {
            val.push_str(line);
            val.push('\n');
        }
    }

    if !key.is_empty() {
        if let Some(val) = current.get_mut(&key)
            && val.ends_with('\n')
        {
            val.pop();
        }
        test_maps.push(current);
    }

    let mut out = Vec::with_capacity(test_maps.len());
    for (idx, mut map) in test_maps.into_iter().enumerate() {
        for v in map.values_mut() {
            if v.ends_with('\n') {
                v.pop();
            }
        }
        let errors = map
            .get("errors")
            .map(|s| {
                s.split('\n')
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let new_errors = map
            .get("new-errors")
            .or_else(|| map.get("errors-new"))
            .map(|s| {
                s.split('\n')
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        let script_mode = if map.contains_key("script-off") {
            ScriptMode::ScriptOff
        } else if map.contains_key("script-on") {
            ScriptMode::ScriptOn
        } else {
            ScriptMode::Both
        };

        out.push(TreeConstructionCase {
            index: idx,
            description: map.remove("description"),
            data: map.remove("data").unwrap_or_default(),
            errors,
            new_errors,
            document_fragment: map.remove("document-fragment"),
            script_mode,
            document: map.remove("document").unwrap_or_default(),
        });
    }
    out
}

/// Loads tree-construction test cases from a `.dat` file.
pub fn load_tree_dat_file(path: &Path) -> Result<Vec<TreeConstructionCase>, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let raw = String::from_utf8_lossy(&bytes);
    Ok(parse_dat_str(&raw))
}

/// Loads tree-construction test cases from an exported `.json` file.
pub fn load_tree_json_file(path: &Path) -> Result<Vec<TreeConstructionCase>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let parsed: JsonTreeSuiteFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON in {}: {e}", path.display()))?;
    Ok(parsed.tests)
}

fn parse_fragment_context(spec: &str) -> (Namespace, &str) {
    if let Some(rest) = spec.strip_prefix("svg ") {
        (Namespace::Svg, rest.trim())
    } else if let Some(rest) = spec.strip_prefix("math ") {
        (Namespace::MathMl, rest.trim())
    } else {
        (Namespace::Html, spec.trim())
    }
}

/// Executes a single [`TreeConstructionCase`] in the specified `scripting_enabled` mode.
pub fn execute_tree_case(
    file_name: &str,
    case: &TreeConstructionCase,
    scripting_enabled: bool,
) -> TreeRunResult {
    let doc = if let Some(ref frag_spec) = case.document_fragment {
        let (ns, local_name) = parse_fragment_context(frag_spec);
        parse_fragment(&case.data, ns, local_name, scripting_enabled)
    } else {
        parse_document(&case.data, scripting_enabled)
    };
    let actual_document = serialize_dom_for_test(&doc);
    let passed = actual_document == case.document;
    TreeRunResult {
        file_name: file_name.to_owned(),
        index: case.index,
        scripting_enabled,
        passed,
        input: case.data.clone(),
        expected_document: case.document.clone(),
        actual_document,
    }
}

/// Runs all test cases in a `.dat` or `.json` tree-construction file.
pub fn run_tree_suite_file(path: &Path) -> Result<Vec<TreeRunResult>, String> {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.dat")
        .to_owned();
    let cases = if path.extension().and_then(|s| s.to_str()) == Some("json") {
        load_tree_json_file(path)?
    } else {
        load_tree_dat_file(path)?
    };

    let mut results = Vec::new();
    for case in &cases {
        let modes: &[bool] = match case.script_mode {
            ScriptMode::ScriptOff => &[false],
            ScriptMode::ScriptOn => &[true],
            ScriptMode::Both => &[false, true],
        };
        for &scripting in modes {
            results.push(execute_tree_case(&file_name, case, scripting));
        }
    }
    Ok(results)
}
