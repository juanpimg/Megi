//! # WPT CSS Syntax & Selectors Closed-Loop Test Runner
//!
//! Validates Megi's CSS tokenizer, parser, and selectors against the official WPT test files
//! vendored in `tests/wpt/css/css-syntax` (see the README there for provenance and hashes).
//!
//! The fixtures are **read from disk on every run**: if one is missing the runner returns an
//! error and the gate fails instead of silently skipping. Expectations for preprocessing and
//! An+B are extracted from the upstream HTML/JS sources rather than duplicated from memory, so
//! fixture drift shows up as a loud error.
//!
//! The suite intentionally reports exactly 30 results: 9 declaration-trimming checks, 13 input
//! preprocessing checks (10 extracted from WPT + 3 newline-normalization cases that upstream
//! cannot express in JS), and 8 curated An+B checks whose expectations are cross-checked against
//! `anb-parsing.html`. An+B coverage of the remaining upstream assertions is tracked as follow-up.

use std::path::Path;

/// Result of executing a single CSS test expectation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CssTestResult {
    pub name: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
}

fn read_wpt_fixture(wpt_dir: &Path, file_name: &str) -> Result<String, String> {
    let path = wpt_dir.join(file_name);
    std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "missing WPT CSS fixture {}: {e}\n\
             (vendor it under tests/wpt/css/css-syntax/; see the README there)",
            path.display()
        )
    })
}

fn skip_js_whitespace(source: &str, mut idx: usize) -> usize {
    let bytes = source.as_bytes();
    while idx < bytes.len() && matches!(bytes[idx], b' ' | b'\t' | b'\n' | b'\r') {
        idx += 1;
    }
    idx
}

fn hex_value(bytes: &[u8], start: usize, len: usize) -> Result<u32, String> {
    let end = start
        .checked_add(len)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| "truncated hexadecimal escape in WPT fixture".to_string())?;
    let hex = std::str::from_utf8(&bytes[start..end])
        .map_err(|_| "invalid UTF-8 in hexadecimal escape of WPT fixture".to_string())?;
    u32::from_str_radix(hex, 16)
        .map_err(|_| format!("invalid hexadecimal escape `{hex}` in WPT fixture"))
}

/// Decodes a JS double-quoted string literal starting at `start` (the opening `"`).
///
/// Returns the raw code points (lone UTF-16 surrogates preserved, adjacent pairs combined) and
/// the byte index just past the closing quote. Only the escape forms used by the vendored WPT
/// fixtures are accepted; anything else is an error rather than silently mis-read.
fn decode_js_string(source: &str, start: usize) -> Result<(Vec<u32>, usize), String> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&b'"') {
        return Err(format!(
            "expected a double-quoted JS string at byte {start} of the WPT fixture"
        ));
    }
    let mut code_points: Vec<u32> = Vec::new();
    let mut idx = start + 1;
    while idx < bytes.len() {
        match bytes[idx] {
            b'"' => {
                return Ok((combine_surrogate_pairs(code_points), idx + 1));
            }
            b'\\' => {
                idx += 1;
                let escape = *bytes
                    .get(idx)
                    .ok_or_else(|| "unterminated escape in WPT fixture".to_string())?;
                match escape {
                    b'"' => code_points.push(b'"' as u32),
                    b'\'' => code_points.push(b'\'' as u32),
                    b'\\' => code_points.push(b'\\' as u32),
                    b'/' => code_points.push(b'/' as u32),
                    b'n' => code_points.push(0x0A),
                    b'r' => code_points.push(0x0D),
                    b't' => code_points.push(0x09),
                    b'0' => code_points.push(0x00),
                    b'x' => {
                        let value = hex_value(bytes, idx + 1, 2)?;
                        code_points.push(value);
                        idx += 2;
                    }
                    b'u' => {
                        if bytes.get(idx + 1) == Some(&b'{') {
                            let close = source[idx + 2..]
                                .find('}')
                                .ok_or_else(|| "unterminated \\u{...} escape".to_string())?
                                + idx
                                + 2;
                            let hex = &source[idx + 2..close];
                            let value = u32::from_str_radix(hex, 16)
                                .map_err(|_| format!("invalid \\u{{{hex}}} escape"))?;
                            code_points.push(value);
                            idx = close;
                        } else {
                            code_points.push(hex_value(bytes, idx + 1, 4)?);
                            idx += 4;
                        }
                    }
                    other => {
                        return Err(format!(
                            "unsupported JS escape `\\{}` in WPT fixture",
                            other as char
                        ));
                    }
                }
                idx += 1;
            }
            _ => {
                let ch = source[idx..]
                    .chars()
                    .next()
                    .ok_or_else(|| "truncated UTF-8 in WPT fixture".to_string())?;
                code_points.push(ch as u32);
                idx += ch.len_utf8();
            }
        }
    }
    Err("unterminated JS string literal in WPT fixture".to_string())
}

/// Combines adjacent UTF-16 surrogate escapes (e.g. `\uD83D\uDE00`) into scalar code points.
/// Lone surrogates are preserved so the CSS preprocessor can be asked to replace them.
fn combine_surrogate_pairs(code_points: Vec<u32>) -> Vec<u32> {
    let mut out = Vec::with_capacity(code_points.len());
    let mut idx = 0;
    while idx < code_points.len() {
        let cp = code_points[idx];
        let next = code_points.get(idx + 1).copied();
        if (0xD800..=0xDBFF).contains(&cp)
            && let Some(low) = next
            && (0xDC00..=0xDFFF).contains(&low)
        {
            out.push(0x1_0000 + ((cp - 0xD800) << 10) + (low - 0xDC00));
            idx += 2;
        } else {
            out.push(cp);
            idx += 1;
        }
    }
    out
}

/// Decoded `(input, expected)` code-point pair from a WPT JS test call.
type JsCallArgs = (Vec<u32>, Vec<u32>);

/// Collects every `func("arg1", "arg2")` call found in a fixture source (HTML/JS included).
///
/// Used to derive expectations directly from the vendored upstream files. A malformed call is an
/// error: the fixture format is known, so silently skipping it would hide test coverage.
fn extract_two_arg_js_calls(source: &str, func: &str) -> Result<Vec<JsCallArgs>, String> {
    let bytes = source.as_bytes();
    let mut calls = Vec::new();
    let mut search_from = 0;
    while let Some(relative) = source[search_from..].find(func) {
        let func_start = search_from + relative;
        let before = &source[..func_start];
        let is_identifier_char = func_start > 0 && {
            let previous = before.chars().next_back().unwrap();
            previous.is_alphanumeric() || previous == '_' || previous == '$'
        };
        // Skip the `function testParsing(...)` / `function testANB(...)` declarations.
        let is_declaration = before.trim_end().ends_with("function");
        if is_identifier_char || is_declaration {
            search_from = func_start + func.len();
            continue;
        }
        let mut idx = skip_js_whitespace(source, func_start + func.len());
        if bytes.get(idx) != Some(&b'(') {
            return Err(format!(
                "malformed `{func}` call in WPT fixture: expected `(` at byte {idx}"
            ));
        }
        idx = skip_js_whitespace(source, idx + 1);
        let (first, next) = decode_js_string(source, idx)?;
        idx = skip_js_whitespace(source, next);
        if bytes.get(idx) != Some(&b',') {
            return Err(format!(
                "malformed `{func}` call in WPT fixture: expected `,` at byte {idx}"
            ));
        }
        idx = skip_js_whitespace(source, idx + 1);
        let (second, next) = decode_js_string(source, idx)?;
        idx = skip_js_whitespace(source, next);
        if bytes.get(idx) != Some(&b')') {
            return Err(format!(
                "malformed `{func}` call in WPT fixture: expected `)` at byte {idx}"
            ));
        }
        calls.push((first, second));
        search_from = idx + 1;
    }
    Ok(calls)
}

fn code_points_to_string(code_points: &[u32]) -> String {
    code_points
        .iter()
        .map(|cp| char::from_u32(*cp).unwrap_or('\u{FFFD}'))
        .collect()
}

/// Renders a code-point sequence for test names, escaping non-printable/lone-surrogate values.
fn display_code_points(code_points: &[u32]) -> String {
    let mut out = String::from("\"");
    for cp in code_points {
        match char::from_u32(*cp) {
            Some(ch) => out.extend(ch.escape_default()),
            None => out.push_str(&format!("\\u{{{cp:04X}}}")),
        }
    }
    out.push('"');
    out
}

/// Runs the WPT `declarations-trim-whitespace.html` test.
pub fn run_wpt_declarations_trim_whitespace(wpt_dir: &Path) -> Result<Vec<CssTestResult>, String> {
    let content = read_wpt_fixture(wpt_dir, "declarations-trim-whitespace.html")?;

    // Parse the test HTML file with Megi's HTML engine (scripting disabled)
    let doc = html::parse_document(&content, false);

    // Find the style element text
    let mut style_text = String::new();
    let mut stack = vec![doc.root()];
    while let Some(node_id) = stack.pop() {
        if let Some(el) = doc.node(node_id).as_element()
            && el.local_name == "style"
        {
            let mut child = doc.node(node_id).first_child;
            while let Some(c) = child {
                if let Some(txt) = doc.node(c).as_text() {
                    style_text.push_str(txt);
                }
                child = doc.node(c).next_sibling;
            }
            break;
        }
        let mut child = doc.node(node_id).last_child;
        while let Some(c) = child {
            stack.push(c);
            child = doc.node(c).prev_sibling;
        }
    }
    if style_text.trim().is_empty() {
        return Err(format!(
            "WPT fixture {} does not contain the expected <style> block",
            wpt_dir.join("declarations-trim-whitespace.html").display()
        ));
    }

    let stylesheet = css::parse_stylesheet(&style_text);
    let mut decl_map = std::collections::HashMap::new();
    for rule in &stylesheet.rules {
        if let css::Rule::Style(style_rule) = rule {
            for decl in &style_rule.declarations {
                decl_map.insert(decl.name.clone(), decl.raw_value.clone());
            }
        }
    }

    let mut results = Vec::new();
    let expected = "bar";
    for i in 1..=9 {
        let prop = format!("--foo-{i}");
        let actual = decl_map.get(&prop).cloned().unwrap_or_default();
        let passed = actual == expected;
        results.push(CssTestResult {
            name: format!("declarations-trim-whitespace.html: {prop}"),
            passed,
            expected: expected.to_string(),
            actual,
        });
    }

    Ok(results)
}

/// Runs the WPT `input-preprocessing.html` test suite.
///
/// The 10 upstream `testParsing(input, expected)` cases are extracted from the fixture and replayed
/// through [`css::preprocess_css_codepoints`] (which supports lone surrogates, unlike `&str`). Three
/// CR/LF/FF newline-normalization cases are appended because the upstream fixture cannot express
/// them in JS (it says so in a comment); those come from CSS Syntax §3.3 directly.
pub fn run_wpt_input_preprocessing(wpt_dir: &Path) -> Result<Vec<CssTestResult>, String> {
    let content = read_wpt_fixture(wpt_dir, "input-preprocessing.html")?;
    let calls = extract_two_arg_js_calls(&content, "testParsing")?;
    if calls.len() != 10 {
        return Err(format!(
            "WPT fixture input-preprocessing.html drifted: expected 10 `testParsing` cases, found {}",
            calls.len()
        ));
    }

    let mut results = Vec::new();
    for (input_cps, expected_cps) in &calls {
        let actual_cps: Vec<u32> = css::preprocess_css_codepoints(input_cps)
            .into_iter()
            .map(|ch| ch as u32)
            .collect();
        let passed = actual_cps == *expected_cps;
        results.push(CssTestResult {
            name: format!(
                "input-preprocessing.html: {}",
                display_code_points(input_cps)
            ),
            passed,
            expected: display_code_points(expected_cps),
            actual: display_code_points(&actual_cps),
        });
    }

    // §3.3 newline normalization (CRLF / CR / FF => LF); not expressible in the upstream JS test.
    for (input, expected) in [
        ("foo\r\nbar", "foo\nbar"),
        ("foo\rbar", "foo\nbar"),
        ("foo\x0Cbar", "foo\nbar"),
    ] {
        let actual: String = css::preprocess_css_input(input).into_iter().collect();
        let passed = actual == expected;
        results.push(CssTestResult {
            name: format!("input-preprocessing.html (newline normalization): {input:?}"),
            passed,
            expected: expected.to_string(),
            actual,
        });
    }

    Ok(results)
}

/// Canonicalizes an `An+B` struct into the selector text WPT uses for `:nth-child(<An+B>)`.
fn format_an_plus_b(anb: css::AnPlusB) -> String {
    if anb.a == 0 {
        return anb.b.to_string();
    }
    let mut out = match anb.a {
        1 => "n".to_string(),
        -1 => "-n".to_string(),
        a => format!("{a}n"),
    };
    if anb.b > 0 {
        out.push('+');
        out.push_str(&anb.b.to_string());
    } else if anb.b < 0 {
        out.push_str(&anb.b.to_string());
    }
    out
}

/// Runs a curated subset of `anb-parsing.html` validated against the upstream expectations.
///
/// Each curated input is located in the vendored fixture, its expected `selectorText` is decoded
/// from upstream, and the engine's [`css::AnPlusB`] is canonicalized to the same textual form before
/// comparison. Missing inputs are an error (fixture drift), never a silent skip.
pub fn run_wpt_anb_parsing(wpt_dir: &Path) -> Result<Vec<CssTestResult>, String> {
    let content = read_wpt_fixture(wpt_dir, "anb-parsing.html")?;
    let calls = extract_two_arg_js_calls(&content, "testANB")?;
    if calls.len() != 67 {
        return Err(format!(
            "WPT fixture anb-parsing.html drifted: expected 67 `testANB` cases, found {}",
            calls.len()
        ));
    }

    let curated = [
        ("odd", 2, 1),
        ("even", 2, 0),
        ("1", 0, 1),
        ("5n", 5, 0),
        ("+n", 1, 0),
        ("-n", -1, 0),
        ("5n-5", 5, -5),
        ("n-5", 1, -5),
    ];

    let mut results = Vec::new();
    for (formula, expected_a, expected_b) in curated {
        let formula_cps: Vec<u32> = formula.chars().map(|ch| ch as u32).collect();
        let expected = calls
            .iter()
            .find(|(input_cps, _)| *input_cps == formula_cps)
            .map(|(_, expected_cps)| code_points_to_string(expected_cps))
            .ok_or_else(|| {
                format!(
                    "curated An+B case {formula:?} not found in vendored anb-parsing.html \
                     (fixture drift)"
                )
            })?;
        if expected == "parse error" {
            return Err(format!(
                "curated An+B case {formula:?} is expected to be a parse error upstream, \
                 but this harness treats it as valid"
            ));
        }

        // Extract the engine's parsed An+B from a :nth-child() selector.
        let selector_str = format!(":nth-child({formula})");
        let list = css::parse_selectors(&selector_str);
        let mut matched_anb = None;
        if let Some(first_sel) = list.selectors.first()
            && let Some((compound, _)) = first_sel.compounds.first()
        {
            for pseudo in &compound.pseudo_classes {
                if let css::PseudoClass::NthChild(anb) = pseudo {
                    matched_anb = Some(*anb);
                }
            }
        }

        let actual = matched_anb
            .map(format_an_plus_b)
            .unwrap_or_else(|| "parse error".to_string());
        let expected_anb = css::AnPlusB {
            a: expected_a,
            b: expected_b,
        };
        let canonical_expected = format_an_plus_b(expected_anb);
        if expected != canonical_expected {
            return Err(format!(
                "curated An+B case {formula:?} expectation mismatch: WPT expects {expected:?} \
                 but the curated {expected_a}n+{expected_b} canonicalizes to {canonical_expected:?}"
            ));
        }
        let passed = actual == expected;
        results.push(CssTestResult {
            name: format!("anb-parsing.html: {formula}"),
            passed,
            expected,
            actual,
        });
    }

    Ok(results)
}
