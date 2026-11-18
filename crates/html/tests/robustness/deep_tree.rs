use super::*;

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
            let dump = megi_html::serializer::serialize_dom_for_test_with_limit(
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
fn serialization_has_an_output_budget_and_reports_truncation() {
    // #647: `out: String` grew without bound, so a hostile DOM (one huge text node or a deep
    // chain) OOMed the caller with no error path.
    let mut doc = megi_core::Document::new();
    let root = doc.root();
    let body = doc.create_element(megi_core::Namespace::Html, "body", Vec::new());
    doc.append_child(root, body);
    let div = doc.create_element(megi_core::Namespace::Html, "div", Vec::new());
    doc.append_child(body, div);
    let text = doc.create_text_node(&"x".repeat(64 * 1024));
    doc.append_child(div, text);

    let options = megi_html::serializer::HtmlSerializeOptions::scripting_disabled()
        .with_max_output_bytes(4 * 1024);
    let out = megi_html::serialize_html_with_options(&doc, options);
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
    let small = megi_html::serialize_html(&doc);
    assert!(small.contains("<div>"));
    assert!(!small.contains("truncated"));
}

#[test]
fn deep_serialization_does_not_allocate_per_node_indent() {
    // The canonical tree dump used `"  ".repeat(depth)` per node (O(n·depth) bytes). A 4k-deep
    // chain must still serialize, quickly, and mark truncation at the byte cap.
    let mut doc = megi_core::Document::new();
    let root = doc.root();
    let body = doc.create_element(megi_core::Namespace::Html, "body", Vec::new());
    doc.append_child(root, body);
    let mut parent = body;
    for _ in 0..4_000 {
        let child = doc.create_element(megi_core::Namespace::Html, "div", Vec::new());
        doc.append_child(parent, child);
        parent = child;
    }
    let out = megi_html::serializer::serialize_dom_for_test(&doc);
    assert!(!out.is_empty());
    assert!(
        out.len() <= megi_html::serializer::MAX_SERIALIZED_BYTES + 64,
        "the tree dump must respect its byte ceiling"
    );
}
