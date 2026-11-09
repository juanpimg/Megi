//! # WPT CSS Syntax & Selectors Closed-Loop Test Runner
//!
//! Validates Megi's CSS tokenizer, parser, and selectors against official WPT test files
//! located in `tests/wpt/css/css-syntax`.

use std::path::Path;

/// Result of executing a single CSS test expectation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CssTestResult {
    pub name: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
}

/// Runs the WPT `declarations-trim-whitespace.html` test.
pub fn run_wpt_declarations_trim_whitespace(wpt_dir: &Path) -> Vec<CssTestResult> {
    let test_file = wpt_dir.join("declarations-trim-whitespace.html");
    let content = std::fs::read_to_string(&test_file).unwrap_or_else(|_| {
        panic!("Failed to read {}", test_file.display());
    });

    // Parse the test HTML file with Megi's HTML engine (scripting disabled)
    let doc = html::parse_document(&content, false);

    // Find the style element text
    let mut style_text = String::new();
    let mut stack = vec![doc.root()];
    while let Some(node_id) = stack.pop() {
        if let Some(el) = doc.node(node_id).as_element() {
            if el.local_name == "style" {
                let mut child = doc.node(node_id).first_child;
                while let Some(c) = child {
                    if let Some(txt) = doc.node(c).as_text() {
                        style_text.push_str(txt);
                    }
                    child = doc.node(c).next_sibling;
                }
                break;
            }
        }
        let mut child = doc.node(node_id).last_child;
        while let Some(c) = child {
            stack.push(c);
            child = doc.node(c).prev_sibling;
        }
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

    results
}

/// Runs the WPT `input-preprocessing.html` test suite.
pub fn run_wpt_input_preprocessing() -> Vec<CssTestResult> {
    // 1. String inputs with NULL, CR, LF, FF
    let str_cases = [
        ("foo\0", "foo\u{FFFD}"),
        ("f\0oo", "f\u{FFFD}oo"),
        ("\0foo", "\u{FFFD}foo"),
        ("\0", "\u{FFFD}"),
        ("\0\0\0", "\u{FFFD}\u{FFFD}\u{FFFD}"),
        ("foo\r\nbar", "foo\nbar"),
        ("foo\rbar", "foo\nbar"),
        ("foo\x0Cbar", "foo\nbar"),
    ];

    let mut results = Vec::new();
    for (input, expected) in str_cases {
        let actual: String = css::preprocess_css_input(input).into_iter().collect();
        let passed = actual == expected;
        results.push(CssTestResult {
            name: format!("input-preprocessing.html: {input:?}"),
            passed,
            expected: expected.to_string(),
            actual,
        });
    }

    // 2. Surrogate code points tested via code point preprocessing (§3.3)
    let surrogate_cases: &[(&[u32], &str)] = &[
        (
            &[b'f' as u32, b'o' as u32, b'o' as u32, 0xD800],
            "foo\u{FFFD}",
        ),
        (
            &[b'f' as u32, 0xD800, b'o' as u32, b'o' as u32],
            "f\u{FFFD}oo",
        ),
        (
            &[0xD800, b'f' as u32, b'o' as u32, b'o' as u32],
            "\u{FFFD}foo",
        ),
        (&[0xD800], "\u{FFFD}"),
        (&[0xD800, 0xD800, 0xD800], "\u{FFFD}\u{FFFD}\u{FFFD}"),
    ];

    for (input_cps, expected) in surrogate_cases {
        let actual: String = css::preprocess_css_codepoints(input_cps)
            .into_iter()
            .collect();
        let passed = actual == *expected;
        results.push(CssTestResult {
            name: format!("input-preprocessing.html (surrogate cps): {input_cps:?}"),
            passed,
            expected: expected.to_string(),
            actual,
        });
    }

    results
}

/// Runs An+B formula validation from `anb-parsing.html`.
pub fn run_wpt_anb_parsing() -> Vec<CssTestResult> {
    let cases = [
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
    for (formula, expected_a, expected_b) in cases {
        let selector_str = format!(":nth-child({formula})");
        let list = css::parse_selectors(&selector_str);
        let mut matched_anb = None;
        if let Some(first_sel) = list.selectors.first() {
            if let Some((compound, _)) = first_sel.compounds.first() {
                for pc in &compound.pseudo_classes {
                    if let css::PseudoClass::NthChild(anb) = pc {
                        matched_anb = Some(*anb);
                    }
                }
            }
        }

        let passed = matched_anb
            == Some(css::AnPlusB {
                a: expected_a,
                b: expected_b,
            });
        results.push(CssTestResult {
            name: format!("anb-parsing.html: {formula}"),
            passed,
            expected: format!("AnPlusB {{ a: {expected_a}, b: {expected_b} }}"),
            actual: format!("{matched_anb:?}"),
        });
    }

    results
}
