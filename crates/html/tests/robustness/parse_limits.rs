use super::*;

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
        parse_fragment_with_options(&input, megi_html::Namespace::Html, "div", false, &options);
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

#[test]
fn table_text_budget_counts_encoded_bytes_not_chars() {
    // #1737: a 4-byte character must consume 4 bytes of max_text_bytes.
    let input = format!("<table>{}</table>", "😀".repeat(5_000));
    let options = ParseOptions {
        max_text_bytes: 2_048,
        ..ParseOptions::default()
    };
    let outcome = parse_document_with_options(&input, false, &options);
    let mut total = 0usize;
    collect_text_bytes(&outcome.document, outcome.document.root(), &mut total, 0);
    assert!(
        total <= 2_048,
        "stored table text exceeded the byte budget: {total}"
    );
}
