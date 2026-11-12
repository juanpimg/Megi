//! Robustness regression tests for the core-engine audit (`F1`, `F3`, `F10`-style API
//! hardening) and the anti-DoS parsing limits.
//!
//! The historical failure mode being guarded here is a **process abort** (SIGABRT from a
//! Rust stack overflow), not a recoverable panic, so the deep-input cases run inside a
//! worker thread with a deliberately small stack: if the implementation regresses to
//! recursion, the `join()` below fails the test instead of silently passing on the main
//! thread's 8 MiB stack.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use html::{
    ParseErrorCode, ParseOptions, Token, Tokenizer, TokenizerConfig, parse_document,
    parse_document_with_options, parse_fragment_with_options, serialize_dom_for_test,
};

const SMALL_STACK: usize = 512 * 1024;

fn in_thread_with_stack<T: Send + 'static>(
    stack_size: usize,
    task: impl FnOnce() -> T + Send + 'static,
) -> T {
    std::thread::Builder::new()
        .stack_size(stack_size)
        .spawn(task)
        .expect("worker thread must spawn")
        .join()
        .expect("worker thread must not panic/abort")
}

fn find_element_by_name(input: &str, local_name: &str) -> bool {
    let doc = parse_document(input, false);
    let mut stack = vec![doc.root()];
    while let Some(current) = stack.pop() {
        for child in doc.children(current) {
            if let Some(el) = doc.node(child).as_element()
                && el.local_name == local_name
            {
                return true;
            }
            stack.push(child);
        }
    }
    false
}

#[test]
fn deeply_nested_divs_do_not_abort_the_process_with_default_limits() {
    // Audit F1: ~100k nested `<div>` used to abort in `update_selectedcontent_elements`
    // because that post-processing pass recursed once per nesting level on every parse.
    for nesting in [100_000usize, 200_000] {
        let input = "<div>".repeat(nesting);
        let outcome = in_thread_with_stack(SMALL_STACK, move || {
            parse_document_with_options(&input, false, &ParseOptions::default())
        });
        assert!(
            outcome.limit_violations.depth_exceeded > 0,
            "the depth limit must stop the {nesting}-deep chain"
        );
        assert!(
            outcome.document.len() <= ParseOptions::DEFAULT.max_depth + 16,
            "arena grew past the depth limit: {} nodes",
            outcome.document.len()
        );
    }
}

#[test]
fn select_postprocessing_is_iterative_on_deep_trees() {
    // Audit F1 attribution: the overflow happened while walking the tree for
    // `<selectedcontent>`, even without a `<select>`. Keep a `<select>` so the pass runs,
    // then nest 12k elements below it (a recursive walk overflows a 512 KiB stack).
    let mut input = String::from("<select><option selected>One</option></select>");
    input.push_str(&"<span>".repeat(12_000));
    let options = ParseOptions {
        max_depth: 20_000,
        max_nodes: 100_000,
        ..ParseOptions::DEFAULT
    };
    let outcome = in_thread_with_stack(SMALL_STACK, move || {
        parse_document_with_options(&input, false, &options)
    });
    assert!(
        outcome.limit_violations.is_empty(),
        "generous limits should not fire: {:?}",
        outcome.limit_violations
    );
    assert!(outcome.document.len() > 12_000);
}

#[test]
fn selectedcontent_fixup_still_populates_selected_option_content() {
    let doc = parse_document(
        "<select><selectedcontent></selectedcontent><option selected>One</option><option>Two</option></select>",
        false,
    );
    // Find `<selectedcontent>` and read its text.
    let mut stack = vec![doc.root()];
    let mut selected_content_text = None;
    while let Some(current) = stack.pop() {
        for child in doc.children(current) {
            if let Some(el) = doc.node(child).as_element()
                && el.local_name == "selectedcontent"
            {
                selected_content_text = doc
                    .children(child)
                    .next()
                    .and_then(|text_id| doc.node(text_id).as_text())
                    .map(str::to_owned);
            }
            stack.push(child);
        }
    }
    assert_eq!(
        selected_content_text.as_deref(),
        Some("One"),
        "<selectedcontent> must mirror the selected option's children"
    );
}

#[test]
fn serializer_handles_deep_dom_iteratively() {
    // Audit F6 (`serializer.rs`): the recursive serializer aborted on deep DOMs. Parse a
    // 4000-level chain with relaxed depth, then serialize it on a 512 KiB stack. The
    // indentation is inherently O(depth^2) in bytes, hence 4000 (≈16 MB of output).
    let input = "<span>".repeat(4_000);
    let options = ParseOptions {
        max_depth: 10_000,
        max_nodes: 100_000,
        ..ParseOptions::DEFAULT
    };
    let (line_count, starts_with_html, ends_with_span) =
        in_thread_with_stack(SMALL_STACK, move || {
            let outcome = parse_document_with_options(&input, false, &options);
            let dump = serialize_dom_for_test(&outcome.document);
            (
                dump.lines().count(),
                dump.starts_with("| <html>"),
                dump.ends_with("<span>"),
            )
        });
    // `html > head > body > 4000 nested spans`: one line per element.
    assert_eq!(line_count, 3 + 4_000);
    assert!(starts_with_html);
    assert!(ends_with_span, "unexpected dump tail");
}

#[test]
fn tokenizer_handles_many_attributes_in_linear_time() {
    // Audit F3: `PendingTag::finish_attribute` used a linear `.any()` scan per attribute,
    // making a single tag with 50k attributes quadratic (~seconds of CPU).
    let mut input = String::from("<a");
    for i in 0..50_000 {
        write!(input, " b{i}").unwrap();
    }
    input.push('>');

    let start = Instant::now();
    let mut tok = Tokenizer::from_str_input(&input, TokenizerConfig::default());
    tok.run();
    let elapsed = start.elapsed();

    let attribute_count = tok
        .tokens
        .iter()
        .find_map(|token| match token {
            Token::StartTag { attributes, .. } => Some(attributes.len()),
            _ => None,
        })
        .expect("start tag must be emitted");
    assert_eq!(attribute_count, 50_000);
    assert!(tok.errors.is_empty());
    assert!(
        elapsed < Duration::from_secs(10),
        "tokenizing 50k attributes took {elapsed:?}; duplicate detection may be quadratic again"
    );
}

#[test]
fn tokenizer_still_reports_duplicate_attributes_and_keeps_the_first() {
    let mut tok = Tokenizer::from_str_input("<a x=1 x=2 y=3>", TokenizerConfig::default());
    tok.run();
    let attributes = tok
        .tokens
        .iter()
        .find_map(|token| match token {
            Token::StartTag { attributes, .. } => Some(attributes.clone()),
            _ => None,
        })
        .expect("start tag must be emitted");
    assert_eq!(
        attributes,
        vec![
            ("x".to_string(), "1".to_string()),
            ("y".to_string(), "3".to_string())
        ]
    );
    assert!(
        tok.errors
            .iter()
            .any(|e| e.code == ParseErrorCode::DuplicateAttribute)
    );
}

#[test]
fn default_parse_options_do_not_fire_on_normal_documents() {
    let outcome = parse_document_with_options(
        "<!DOCTYPE html><p class=x>Hello <b>world</b>!</p>",
        false,
        &ParseOptions::default(),
    );
    assert!(outcome.limit_violations.is_empty());
    assert!(find_element_by_name("<p>Hello</p>", "p"));
}

#[test]
fn depth_limit_drops_further_elements() {
    let input = "<div>".repeat(10_000);
    let options = ParseOptions {
        max_depth: 64,
        ..ParseOptions::DEFAULT
    };
    let outcome = parse_document_with_options(&input, false, &options);
    assert!(outcome.limit_violations.depth_exceeded > 0);
    assert_eq!(outcome.limit_violations.nodes_exceeded, 0);
    assert!(
        outcome.document.len() < 128,
        "document must stay shallow: {} nodes",
        outcome.document.len()
    );
}

#[test]
fn node_limit_stops_arena_growth() {
    let input = "<br>".repeat(2_000);
    let options = ParseOptions {
        max_nodes: 50,
        ..ParseOptions::DEFAULT
    };
    let outcome = parse_document_with_options(&input, false, &options);
    assert!(outcome.limit_violations.nodes_exceeded > 0);
    assert!(
        outcome.document.len() <= 56,
        "arena grew past the node limit: {} nodes",
        outcome.document.len()
    );
}

#[test]
fn node_limit_also_stops_comment_allocations() {
    let input = "<!--x-->".repeat(2_000);
    let options = ParseOptions {
        max_nodes: 50,
        ..ParseOptions::DEFAULT
    };
    let outcome = parse_document_with_options(&input, false, &options);
    assert!(outcome.limit_violations.nodes_exceeded > 0);
    assert!(
        outcome.document.len() <= 56,
        "comment nodes grew past the node limit: {} nodes",
        outcome.document.len()
    );
}

#[test]
fn attribute_limit_truncates_element_attributes() {
    let mut input = String::from("<a");
    for i in 0..100 {
        write!(input, " b{i}").unwrap();
    }
    input.push('>');
    let options = ParseOptions {
        max_attributes_per_element: 8,
        ..ParseOptions::DEFAULT
    };
    let outcome = parse_document_with_options(&input, false, &options);
    assert_eq!(outcome.limit_violations.attributes_dropped, 92);

    let doc = &outcome.document;
    let mut stack = vec![doc.root()];
    let mut anchor_attributes = None;
    while let Some(current) = stack.pop() {
        for child in doc.children(current) {
            if let Some(el) = doc.node(child).as_element()
                && el.local_name == "a"
            {
                anchor_attributes = Some(el.attributes.clone());
            }
            stack.push(child);
        }
    }
    let anchor_attributes = anchor_attributes.expect("anchor element must exist");
    assert_eq!(anchor_attributes.len(), 8);
    assert_eq!(anchor_attributes[0].local_name, "b0");
}

#[test]
fn input_bytes_limit_truncates_trailing_input() {
    let input = "<p>hello</p>";
    let options = ParseOptions {
        max_input_bytes: 4,
        ..ParseOptions::DEFAULT
    };
    let outcome = parse_document_with_options(input, false, &options);
    assert!(outcome.limit_violations.input_truncated);
    assert!(outcome.limit_violations.total() >= 1);
    // The truncated prefix still parses without panicking.
    assert!(!outcome.document.is_empty());
}

#[test]
fn fragment_parser_honours_parse_options() {
    let options = ParseOptions {
        max_depth: 8,
        ..ParseOptions::DEFAULT
    };
    let input = "<div>".repeat(1_000);
    let outcome =
        parse_fragment_with_options(&input, html::Namespace::Html, "div", false, &options);
    assert!(outcome.limit_violations.depth_exceeded > 0);
    assert!(outcome.document.len() < 32);
}

#[test]
fn selectedcontent_cloning_respects_the_node_limit() {
    // The post-parse `<selectedcontent>` fixup clones the selected option's children with
    // no node check: 200 selectedcontents × 500 spans used to add ~100,000 nodes past a
    // 5,000-node limit and leave `nodes_exceeded` at zero.
    let option = format!(
        "<option selected><div>{}</div></option>",
        "<span></span>".repeat(500)
    );
    let selectedcontents = "<selectedcontent></selectedcontent>".repeat(200);
    let input = format!("<select>{option}{selectedcontents}</select>");
    let options = ParseOptions {
        max_nodes: 5_000,
        ..ParseOptions::DEFAULT
    };
    let outcome = parse_document_with_options(&input, false, &options);
    assert!(
        outcome.limit_violations.nodes_exceeded > 0,
        "the cloning fixup must report the node limit"
    );
    assert!(
        outcome.document.len() <= 5_000 + 501,
        "clones may overshoot one subtree at most: {} nodes",
        outcome.document.len()
    );
}

#[test]
fn selectedcontent_budget_is_counted_once_across_selects() {
    // The clone budget is document-wide: after the first select exhausts it the fixup must
    // stop (not clear every later select's content and count the violation again).
    let option = format!(
        "<option selected><div>{}</div></option>",
        "<span></span>".repeat(200)
    );
    let sc = "<selectedcontent></selectedcontent>".repeat(30);
    let input = format!("<select>{option}{sc}</select><select>{option}{sc}</select>");
    let options = ParseOptions {
        max_nodes: 3_000,
        ..ParseOptions::DEFAULT
    };
    let outcome = parse_document_with_options(&input, false, &options);
    assert_eq!(
        outcome.limit_violations.nodes_exceeded, 1,
        "the node violation is recorded once, not per select"
    );
}
