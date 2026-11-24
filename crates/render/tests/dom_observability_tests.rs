//! #2269: `--json` must expose post-script DOM state so a blank SPA render can be
//! distinguished from a successfully hydrated one.
//!
//! The JSON fields `element_count` and `body_text` come from `dom_summary()` in
//! `crates/render/src/main.rs`. When this suite was added they were still
//! uncommitted there; the two CLI tests below intentionally require them and
//! panic with an explicit message until that change lands on `main`.

#![cfg(feature = "js")]

use std::path::PathBuf;
use std::process::Command;

use megi_render::{RenderOptions, ScriptingMode, Viewport, render_url};

const HYDRATED_TEXT: &str = "hydrated card";

/// Panic text shared by the JSON helpers so a missing field names its cause.
const MISSING_FIELD_HINT: &str = "requires the element_count/body_text fields from dom_summary() in crates/render/src/main.rs (#2269)";

fn fixture_path() -> PathBuf {
    // Cargo runs test binaries from the package root. Deriving the path at
    // runtime (instead of baking in CARGO_MANIFEST_DIR) keeps the fixture
    // resolvable when several worktrees share one CARGO_TARGET_DIR.
    std::env::current_dir()
        .expect("test binary has a working directory")
        .join("../../tests/fixtures/render/spa_dom_mutation.html")
}

fn options(scripting: ScriptingMode) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(400, 240).expect("viewport"),
        scripting,
        ..RenderOptions::default()
    }
}

fn render_fixture(scripting: ScriptingMode) -> String {
    let url = format!("file://{}", fixture_path().display());
    let output = render_url(&url, &options(scripting)).expect("fixture renders");
    let body = output.document.body().expect("document has a body");
    output.document.text_content(body)
}

/// Runs the CLI and returns its `--json` stdout. The output PNG goes to a unique
/// temporary path so parallel tests cannot collide.
fn run_cli(extra: &[&str]) -> String {
    let unique = format!(
        "megi-spa-2269-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let output_path = std::env::temp_dir().join(unique);
    let result = Command::new(env!("CARGO_BIN_EXE_render"))
        .arg(fixture_path())
        .arg(&output_path)
        .args(["--width", "400", "--height", "240", "--json"])
        .args(extra)
        .output()
        .expect("render binary runs");
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    assert!(
        result.status.success(),
        "render CLI failed: status={:?} stderr={stderr}",
        result.status
    );
    let json = String::from_utf8(result.stdout).expect("--json output is UTF-8");
    let _ = std::fs::remove_file(&output_path);
    json
}

fn field_start<'a>(document: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":");
    document
        .split_once(&marker)
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| {
            panic!("missing JSON key {key:?}: {MISSING_FIELD_HINT}\nfull output: {document}")
        })
}

fn json_number(document: &str, key: &str) -> u64 {
    let digits: String = field_start(document, key)
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("JSON key {key:?} is not a number in {document}"))
}

fn json_string(document: &str, key: &str) -> String {
    let rest = field_start(document, key);
    let rest = rest.strip_prefix('"').unwrap_or_else(|| {
        panic!("JSON key {key:?} is not a string: {MISSING_FIELD_HINT}\nfull output: {document}")
    });
    let mut decoded = String::new();
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return decoded,
            '\\' => match chars.next() {
                Some('n') => decoded.push('\n'),
                Some('r') => decoded.push('\r'),
                Some('t') => decoded.push('\t'),
                Some('"') => decoded.push('"'),
                Some('\\') => decoded.push('\\'),
                Some(other) => decoded.push(other),
                None => break,
            },
            other => decoded.push(other),
        }
    }
    panic!("unterminated JSON string for key {key:?} in {document}")
}

/// Fixture sanity check: it hydrates only when scripting is enabled. This one
/// passes on `origin/main` because it inspects the DOM through the library API,
/// independently of the pending `--json` fields.
#[test]
fn spa_fixture_hydrates_body_only_with_scripting_2269() {
    let with_js = render_fixture(ScriptingMode::Enabled);
    assert!(
        with_js.contains(HYDRATED_TEXT),
        "script must insert {HYDRATED_TEXT:?} into the body, got {with_js:?}"
    );
    let without_js = render_fixture(ScriptingMode::Disabled);
    assert!(
        !without_js.contains(HYDRATED_TEXT),
        "scripting-disabled render must not contain {HYDRATED_TEXT:?}, got {without_js:?}"
    );
}

/// CLI JSON must describe the post-script DOM, not just pixels and timings.
#[test]
fn render_json_exposes_spa_dom_summary_2269() {
    let json = run_cli(&[]);
    let element_count = json_number(&json, "element_count");
    assert!(
        element_count > 0,
        "hydrated SPA must report element_count > 0, got {element_count}"
    );
    let body_text = json_string(&json, "body_text");
    assert!(
        body_text.contains(HYDRATED_TEXT),
        "body_text must expose script-inserted text {HYDRATED_TEXT:?}, got {body_text:?}"
    );
}

/// The signal must detect hydration: with scripting disabled the observable
/// body text differs and no longer carries the script-inserted card.
#[test]
fn no_scripting_json_body_text_differs_2269() {
    let hydrated = json_string(&run_cli(&[]), "body_text");
    let no_scripting = json_string(&run_cli(&["--no-scripting"]), "body_text");
    assert!(
        hydrated.contains(HYDRATED_TEXT),
        "scripted render must contain {HYDRATED_TEXT:?}, got {hydrated:?}"
    );
    assert!(
        !no_scripting.contains(HYDRATED_TEXT),
        "--no-scripting must not contain {HYDRATED_TEXT:?}, got {no_scripting:?}"
    );
    assert_ne!(
        hydrated, no_scripting,
        "DOM observability signal must distinguish hydration from a blank render"
    );
}
