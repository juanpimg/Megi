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
    parse_document_with_options, parse_fragment_with_options,
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
            // Lift the byte ceiling: this test is about stack safety on a 16 MB dump, not
            // about the truncation behaviour (covered by the budget tests below).
            let dump = html::serializer::serialize_dom_for_test_with_limit(
                &outcome.document,
                64 * 1024 * 1024,
            );
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

#[test]
fn serialization_has_an_output_budget_and_reports_truncation() {
    // #647: `out: String` grew without bound, so a hostile DOM (one huge text node or a deep
    // chain) OOMed the caller with no error path.
    let mut doc = dom::Document::new();
    let root = doc.root();
    let body = doc.create_element(dom::Namespace::Html, "body", Vec::new());
    doc.append_child(root, body);
    let div = doc.create_element(dom::Namespace::Html, "div", Vec::new());
    doc.append_child(body, div);
    let text = doc.create_text_node(&"x".repeat(64 * 1024));
    doc.append_child(div, text);

    let options = html::serializer::HtmlSerializeOptions::scripting_disabled()
        .with_max_output_bytes(4 * 1024);
    let out = html::serialize_html_with_options(&doc, options);
    assert!(
        out.len() <= 4 * 1024 + 32,
        "the serializer must respect the ceiling (got {} bytes)",
        out.len()
    );
    assert!(
        out.ends_with("<!-- output truncated -->"),
        "truncation must be visible in the output: {:?}",
        &out[out.len().saturating_sub(48)..]
    );

    // The default ceiling is high enough that ordinary documents are untouched.
    let small = html::serialize_html(&doc);
    assert!(small.contains("<div>"));
    assert!(!small.contains("truncated"));
}

#[test]
fn deep_serialization_does_not_allocate_per_node_indent() {
    // The canonical tree dump used `"  ".repeat(depth)` per node (O(n·depth) bytes). A 4k-deep
    // chain must still serialize, quickly, and mark truncation at the byte cap.
    let mut doc = dom::Document::new();
    let root = doc.root();
    let body = doc.create_element(dom::Namespace::Html, "body", Vec::new());
    doc.append_child(root, body);
    let mut parent = body;
    for _ in 0..4_000 {
        let child = doc.create_element(dom::Namespace::Html, "div", Vec::new());
        doc.append_child(parent, child);
        parent = child;
    }
    let out = html::serializer::serialize_dom_for_test(&doc);
    assert!(!out.is_empty());
    assert!(
        out.len() <= html::serializer::MAX_SERIALIZED_BYTES + 64,
        "the tree dump must respect its byte ceiling"
    );
}

#[test]
fn text_byte_budget_bounds_a_single_huge_text_node() {
    // #642: the node budget counts nodes, so a pure-text document is one text node no matter how
    // long it is. The text budget caps the bytes actually stored.
    let input = "x".repeat(200_000);
    let options = ParseOptions {
        max_text_bytes: 4_096,
        ..ParseOptions::default()
    };
    let outcome = parse_document_with_options(&input, false, &options);
    let mut total = 0usize;
    collect_text_bytes(&outcome.document, outcome.document.root(), &mut total, 0);
    assert!(
        total <= 4_096,
        "stored text must respect max_text_bytes (got {total})"
    );
    assert!(
        outcome.limit_violations.text_bytes_dropped > 0,
        "dropping text must be reported"
    );
}

#[test]
fn table_text_buffer_is_bounded_by_the_text_budget() {
    // While in "in table text" the characters are buffered in a Vec<char> and re-dispatched
    // later; a long run used to hold the whole input at 4 B per char.
    let input = format!("<table>{}</table>", "y".repeat(50_000));
    let options = ParseOptions {
        max_text_bytes: 2_048,
        ..ParseOptions::default()
    };
    let outcome = parse_document_with_options(&input, false, &options);
    assert!(
        outcome.limit_violations.text_bytes_dropped > 0,
        "the buffered table text must be bounded"
    );
}

fn collect_text_bytes(doc: &dom::Document, node: dom::NodeId, total: &mut usize, depth: usize) {
    if depth > 64 {
        return;
    }
    if let dom::NodeKind::Text(text) = &doc.node(node).kind {
        *total += text.len();
    }
    let mut child = doc.node(node).first_child;
    while let Some(id) = child {
        collect_text_bytes(doc, id, total, depth + 1);
        child = doc.node(id).next_sibling;
    }
}

/// Text of the first `<p>` in a parsed document (`text_content` on the root does not descend).
fn paragraph_text(document: &dom::Document) -> String {
    document
        .get_elements_by_tag_name("p")
        .first()
        .map(|node| document.text_content(*node))
        .unwrap_or_default()
}

#[test]
fn huge_numeric_character_reference_runs_stay_linear_and_out_of_range() {
    // #638: every digit of a numeric character reference ran a saturating multiply/add, so
    // `&#` + millions of digits burned one arithmetic step per digit. The accumulator now stops
    // after eight significant digits (still consuming the run to find the terminator) and
    // reports the out-of-range error once.
    use html::{ParseOptions, parse_document_with_options};

    let run = "9".repeat(200_000);
    let input = format!("<p>&#{run};</p>");
    let started = std::time::Instant::now();
    let outcome = parse_document_with_options(&input, false, &ParseOptions::default());
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "a 200k-digit character reference took {elapsed:?}"
    );
    let text = paragraph_text(&outcome.document);
    assert_eq!(text, "\u{FFFD}", "an out-of-range reference becomes U+FFFD");

    // Leading zeros do not consume the significant-digit budget.
    let outcome = parse_document_with_options(
        &format!("<p>&#x{}41;</p>", "0".repeat(50)),
        false,
        &ParseOptions::default(),
    );
    let text = paragraph_text(&outcome.document);
    assert_eq!(
        text, "A",
        "leading zeros must not make `&#x0…041;` overflow"
    );
}

#[test]
fn token_values_are_capped_to_the_per_token_budget() {
    // #636: `max_input_bytes` bounds the document, not a single value. One unterminated
    // comment, doctype id, quoted attribute value or script/RCDATA run used to grow a single
    // buffer to input size, which the tree builder/DOM/serializer then copied.
    use html::tokenizer::{MAX_TOKEN_BYTES, Token, Tokenizer, TokenizerConfig};

    let big = "x".repeat(MAX_TOKEN_BYTES + 4096);

    let mut tok = Tokenizer::from_str_input(&format!("<!--{big}-->"), TokenizerConfig::default());
    tok.run();
    let comment = tok
        .tokens
        .iter()
        .find_map(|token| match token {
            Token::Comment(data) => Some(data.clone()),
            _ => None,
        })
        .expect("a comment token");
    assert_eq!(comment.len(), MAX_TOKEN_BYTES, "comment must be capped");
    assert!(
        tok.token_bytes_dropped > 0,
        "the truncation must be counted"
    );

    let mut tok =
        Tokenizer::from_str_input(&format!("<a href=\"{big}\">"), TokenizerConfig::default());
    tok.run();
    let value_len = tok
        .tokens
        .iter()
        .find_map(|token| match token {
            Token::StartTag { attributes, .. } => attributes
                .iter()
                .find(|(name, _)| name == "href")
                .map(|(_, value)| value.len()),
            _ => None,
        })
        .expect("an href attribute");
    assert_eq!(value_len, MAX_TOKEN_BYTES, "attribute value must be capped");

    // A processing instruction target accumulates while the tokenizer scans for the data,
    // so one `<?` + a long run grows `pending_pi_target` to input size.
    let mut tok = Tokenizer::from_str_input(
        &format!("<?{}", "x".repeat(MAX_TOKEN_BYTES + 4096)),
        TokenizerConfig::default(),
    );
    tok.run();
    let target_len = tok.tokens.iter().find_map(|token| match token {
        Token::ProcessingInstruction { target, .. } => Some(target.len()),
        _ => None,
    });
    if let Some(len) = target_len {
        assert_eq!(len, MAX_TOKEN_BYTES, "PI target must be capped");
        assert!(
            tok.token_bytes_dropped > 0,
            "the truncation must be counted"
        );
    }
}

#[test]
fn active_formatting_list_is_bounded() {
    // #643: the Noah's Ark clause only removes a fourth *identical* entry, so distinct
    // `(tag, attributes)` pairs grew the list without bound and every character run then
    // cloned the whole thing in `reconstruct_active_formatting_elements`.
    use html::{ParseOptions, parse_document_with_options};

    let mut input = String::new();
    for index in 0..20_000 {
        write!(input, "<b id={index}>").unwrap();
    }
    input.push_str("text");

    let started = std::time::Instant::now();
    let outcome = parse_document_with_options(&input, false, &ParseOptions::DEFAULT);
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "20k distinct formatting elements took {elapsed:?}"
    );
    assert!(
        outcome.limit_violations.formatting_dropped > 0,
        "the overflow must be counted, got {:?}",
        outcome.limit_violations
    );
}

#[test]
fn self_writing_script_loops_are_fuelled() {
    // #639: a parser-inserted script can `document.write` another `<script>`, which executes in
    // turn and may write again — without a fuel cap the parse never finishes. Each execution and
    // each written byte must be budgeted.
    use dom::Document;
    use html::scripting::{ScriptExecution, ScriptExecutor};
    use html::{ParseOptions, parse_document_with_options_and_executor};

    struct LoopExecutor {
        runs: usize,
    }

    impl ScriptExecutor for LoopExecutor {
        fn execute(
            &mut self,
            _document: &mut Document,
            _script_text: &str,
            _source_name: &str,
        ) -> ScriptExecution {
            self.runs += 1;
            ScriptExecution {
                written_html: "<script>x</script>".to_string(),
                error: None,
            }
        }
    }

    let started = std::time::Instant::now();
    let outcome = parse_document_with_options_and_executor(
        "<script>x</script>",
        true,
        &ParseOptions::DEFAULT,
        Some(Box::new(LoopExecutor { runs: 0 })),
    );
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "a self-writing script must be fuelled, took {elapsed:?}"
    );
    assert!(
        outcome.limit_violations.scripts_skipped > 0
            || outcome.limit_violations.written_bytes_dropped > 0,
        "the fuel cap must be reported, got {:?}",
        outcome.limit_violations
    );
}

#[test]
fn many_small_document_writes_stay_linear() {
    // #639: every `document.write` re-inserted through `Tokenizer::insert_input`, which shifted
    // every line start and rebuilt the UTF-16 prefix table from scratch — quadratic in the
    // number of writes, which is exactly the normal `document.write` pattern.
    use dom::Document;
    use html::scripting::{ScriptExecution, ScriptExecutor};
    use html::{ParseOptions, parse_document_with_options_and_executor};

    struct SmallWrites {
        remaining: usize,
    }

    impl ScriptExecutor for SmallWrites {
        fn execute(
            &mut self,
            _document: &mut Document,
            _script_text: &str,
            _source_name: &str,
        ) -> ScriptExecution {
            if self.remaining == 0 {
                return ScriptExecution {
                    written_html: String::new(),
                    error: None,
                };
            }
            self.remaining -= 1;
            // A short write with a line break, repeated: the pathological shape for the old
            // per-write line-table shift.
            ScriptExecution {
                written_html: "<script>x</script>\n<p>y</p>\n".to_string(),
                error: None,
            }
        }
    }

    let started = std::time::Instant::now();
    let outcome = parse_document_with_options_and_executor(
        "<script>x</script>",
        true,
        &ParseOptions::DEFAULT,
        Some(Box::new(SmallWrites { remaining: 2_000 })),
    );
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "2 000 small writes took {elapsed:?}"
    );
    assert!(
        outcome.limit_violations.scripts_skipped > 0
            || outcome.limit_violations.written_bytes_dropped > 0
    );
}
