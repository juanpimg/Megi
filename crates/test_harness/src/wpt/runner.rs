//! Execute vendored WPT component fixtures with Boa and the offline shim.

use megi_js::{Budget, JsRuntime};

use super::{manifest, shim};

/// One `test()` outcome from a vendored fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WptResult {
    /// Fixture path relative to `tests/wpt/`.
    pub fixture: String,
    /// Subtest name as declared by the fixture.
    pub name: String,
    /// `PASS`, `FAIL`, `TIMEOUT`, `NOTRUN` or `harness_error`.
    pub status: String,
    /// Assertion message (empty on PASS).
    pub message: String,
}

impl WptResult {
    /// Unambiguous name used by the harness output and the generated report.
    pub fn display_name(&self) -> String {
        format!("{}::{}", self.fixture, self.name)
    }
}

#[derive(serde::Deserialize)]
struct RawResult {
    name: String,
    status: String,
    #[serde(default)]
    message: String,
}

/// Run every adopted fixture of one vendored WPT directory.
///
/// Each fixture gets a fresh realm over its parsed document. Scripts with an
/// external `src` are ignored: `testharness.js` is replaced by the local shim.
pub fn run_wpt_dir(root: &std::path::Path, dir: &str) -> Result<Vec<WptResult>, String> {
    let fixtures = manifest::adopted_fixtures(root, dir)?;
    let base = manifest::wpt_root(root);
    let resources = manifest::support_resources(root)?;
    let mut results = Vec::new();
    for fixture in fixtures {
        let path = base.join(&fixture);
        let html = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        results.extend(run_fixture(root, &fixture, &html, &resources)?);
    }
    Ok(results)
}

fn run_fixture(
    root: &std::path::Path,
    fixture: &str,
    html: &str,
    resources: &[(String, String)],
) -> Result<Vec<WptResult>, String> {
    let document = megi_html::parse_document(html, false);
    let url = format!("http://web-platform.test/tests/wpt/{fixture}");
    let mut runtime = JsRuntime::with_document(document, url)
        .map_err(|error| format!("{fixture}: runtime init failed: {error}"))?;
    runtime.set_budget(Budget::SMALL);
    runtime
        .eval(shim::TESHARNESS_SHIM, "<testharness-shim>")
        .map_err(|error| format!("{fixture}: shim failed: {error}"))?;
    for (name, json) in resources {
        let code = format!("globalThis.__megi_wpt_resources__[{name:?}] = {json};");
        runtime
            .eval(&code, "<wpt-support-resource>")
            .map_err(|error| format!("{fixture}: support resource {name} failed: {error}"))?;
    }
    for (path, script) in manifest::support_scripts_for(root, html)? {
        // WPT support scripts start with `"use strict";`. In direct eval that turns
        // top-level `var` into a local binding, so the fixture cannot see the helpers.
        // The vendored bytes stay untouched; only the injected copy drops that directive.
        let script = script.strip_prefix("\"use strict\";").unwrap_or(&script);
        if let Err(error) = runtime.eval(script, &format!("<wpt-support-script {path}>")) {
            // Some upstream helper scripts call a function defined by the fixture that
            // loads them. That is not a runner failure: record it on this fixture only.
            return Ok(vec![WptResult {
                fixture: fixture.to_owned(),
                name: format!("<support-script {path}>"),
                status: "harness_error".to_owned(),
                message: error.to_string(),
            }]);
        }
    }
    // WHATWG HTML §7.3: root elements with an `id` are exposed as named properties
    // on the Window. WPT fixtures use that shorthand (`test1`, `host`, ...); the
    // offline runner has to install those bindings before the fixture's scripts run.
    for id in element_ids(html) {
        let code = format!(
            "if (!({id:?} in globalThis)) globalThis[{id:?}] = document.getElementById({id:?});"
        );
        let _ = runtime.eval(&code, "<wpt-named-element>");
    }

    let mut results = Vec::new();
    // `.any.js` fixtures are scripts with a `// META:` header, not HTML documents.
    let scripts = if fixture.ends_with(".js") {
        vec![html.to_owned()]
    } else {
        inline_scripts(html)
    };
    for (index, script) in scripts.into_iter().enumerate() {
        let source = format!("{fixture}#inline-{index}");
        if let Err(error) = runtime.eval(&script, &source) {
            results.push(WptResult {
                fixture: fixture.to_owned(),
                name: format!("<script-{index}>"),
                status: "harness_error".to_owned(),
                message: error.to_string(),
            });
            continue;
        }
        if let Err(error) = runtime.run_microtasks() {
            results.push(WptResult {
                fixture: fixture.to_owned(),
                name: format!("<microtasks-{index}>"),
                status: "harness_error".to_owned(),
                message: error.to_string(),
            });
        }
    }
    runtime
        .eval(shim::FLUSH, "<testharness-flush>")
        .map_err(|error| format!("{fixture}: flush failed: {error}"))?;
    let serialized = runtime
        .eval(
            "JSON.stringify(globalThis.__megi_wpt_results__)",
            "<testharness-results>",
        )
        .map_err(|error| format!("{fixture}: cannot read results: {error}"))?;
    let raw: Vec<RawResult> = serde_json::from_str(&serialized)
        .map_err(|error| format!("{fixture}: invalid shim results: {error}"))?;
    results.reserve(raw.len());
    // Upstream fixtures occasionally declare the same subtest name twice (e.g. two
    // `atob({})` IDL cases). The report key is sha256(test + NUL + subtest), so give
    // repeated names a deterministic suffix instead of collapsing two results into one.
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for item in raw {
        if !matches!(
            item.status.as_str(),
            "PASS" | "FAIL" | "TIMEOUT" | "NOTRUN" | "harness_error"
        ) {
            return Err(format!("{fixture}: unknown status {:?}", item.status));
        }
        let occurrence = seen.entry(item.name.clone()).or_insert(0);
        let name = if *occurrence == 0 {
            item.name
        } else {
            format!("{} #{}", item.name, *occurrence + 1)
        };
        *occurrence += 1;
        results.push(WptResult {
            fixture: fixture.to_owned(),
            name,
            status: item.status,
            message: item.message,
        });
    }
    Ok(results)
}

/// Extract inline classic scripts, skipping `src=` and non-JavaScript `type=`.
fn inline_scripts(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut scripts = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = lower[cursor..].find("<script") {
        let start = cursor + offset;
        let after_tag = start + "<script".len();
        let boundary = lower.as_bytes().get(after_tag).copied().unwrap_or(b'>');
        if boundary != b'>' && !boundary.is_ascii_whitespace() {
            cursor = after_tag;
            continue;
        }
        let Some(tag_end_offset) = lower[after_tag..].find('>') else {
            break;
        };
        let tag_end = after_tag + tag_end_offset;
        let attributes = lower[after_tag..tag_end]
            .split(|c: char| c.is_whitespace() || c == '=' || c == '/' || c == '"' || c == '\'')
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        let Some(close_offset) = lower[tag_end..].find("</script") else {
            break;
        };
        let body_end = tag_end + close_offset;
        cursor = body_end + "</script>".len();
        if attributes.contains(&"src") {
            continue;
        }
        if let Some(index) = attributes.iter().position(|token| *token == "type") {
            let value = attributes.get(index + 1).copied().unwrap_or("");
            if !matches!(
                value,
                "text/javascript" | "application/javascript" | "module"
            ) {
                continue;
            }
        }
        scripts.push(html[tag_end + 1..body_end].to_owned());
    }
    scripts
}

/// Collect literal `id="..."` / `id='...'` values from a fixture's markup.
fn element_ids(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut ids = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = lower[cursor..].find("id=") {
        let mut at = cursor + offset + 3;
        let bytes = lower.as_bytes();
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        let Some(quote) = bytes.get(at).copied() else {
            break;
        };
        if quote != b'"' && quote != b'\'' {
            cursor = at;
            continue;
        }
        let start = at + 1;
        let Some(rel_end) = lower[start..].find(quote as char) else {
            break;
        };
        let end = start + rel_end;
        ids.push(html[start..end].to_owned());
        cursor = end + 1;
    }
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_inline_classic_scripts() {
        let html = r#"
          <script src="/resources/testharness.js"></script>
          <script type="text/plain">ignored()</script>
          <script>test(function() { assert_true(true); }, "real")</script>
        "#;
        assert_eq!(
            inline_scripts(html),
            vec!["test(function() { assert_true(true); }, \"real\")".to_owned()]
        );
    }
}
