use super::*;

#[test]
fn foreign_mismatched_end_tag_records_parse_error() {
    // #1739: WHATWG HTML §13.2.6.5 ("An end tag, if the current node is not an element in
    // the HTML namespace", step 1-2): mismatched foreign end tag must record a parse error.
    // Here </ellipse> leaves <circle> open (1st error), then </svg> also sees <circle> as
    // the current node before popping up to <svg> (2nd error).
    let outcome = parse_document_with_options(
        "<svg><circle></ellipse></svg>",
        false,
        &ParseOptions::default(),
    );
    assert_eq!(
        outcome.parse_errors.len(),
        2,
        "expected 2 parse errors (</ellipse> and </svg> while <circle> open), got {:?}",
        outcome.parse_errors
    );
    assert!(outcome.parse_errors[0].contains("</ellipse>"));
    assert!(outcome.parse_errors[1].contains("</svg>"));
    // Tree construction still recovers cleanly and closes <svg> on </svg>.
    let dump = megi_html::serialize_dom_for_test(&outcome.document);
    assert!(dump.contains("<svg svg>"), "{dump}");
    assert!(dump.contains("<svg circle>"), "{dump}");

    // When <circle> is properly closed first, only </ellipse> triggers a parse error.
    let single = parse_document_with_options(
        "<svg><circle></circle></ellipse></svg>",
        false,
        &ParseOptions::default(),
    );
    assert_eq!(
        single.parse_errors.len(),
        1,
        "expected 1 parse error for </ellipse> while <svg> is open, got {:?}",
        single.parse_errors
    );
    assert!(single.parse_errors[0].contains("</ellipse>"));
}

#[test]
fn foreign_matching_and_camelcase_end_tags_record_no_parse_error() {
    let matching = parse_document_with_options(
        "<svg><circle></circle></svg>",
        false,
        &ParseOptions::default(),
    );
    assert!(
        matching.parse_errors.is_empty(),
        "matching end tag must not emit parse error: {:?}",
        matching.parse_errors
    );

    let camel = parse_document_with_options(
        "<svg><foreignObject></foreignObject></svg>",
        false,
        &ParseOptions::default(),
    );
    assert!(
        camel.parse_errors.is_empty(),
        "ASCII case-insensitive match for foreignObject must not emit parse error: {:?}",
        camel.parse_errors
    );
}

#[test]
fn foreign_ancestor_end_tag_records_single_parse_error_and_respects_cap() {
    let nested =
        parse_document_with_options("<svg><g><circle></svg>", false, &ParseOptions::default());
    assert_eq!(
        nested.parse_errors.len(),
        1,
        "step 2 checks current node once before the loop, got {:?}",
        nested.parse_errors
    );

    let flood_count = megi_html::tokenizer::MAX_PARSE_ERRORS + 64;
    let mut html = String::from("<svg><circle>");
    for _ in 0..flood_count {
        html.push_str("</ellipse>");
    }
    html.push_str("</svg>");
    let flooded = parse_document_with_options(&html, false, &ParseOptions::default());
    assert_eq!(
        flooded.parse_errors.len(),
        megi_html::tokenizer::MAX_PARSE_ERRORS,
        "parse_errors must respect MAX_PARSE_ERRORS cap"
    );

    let frag = parse_fragment_with_options(
        "<g></path>X",
        megi_core::Namespace::Svg,
        "path",
        false,
        &ParseOptions::default(),
    );
    assert_eq!(
        frag.parse_errors.len(),
        1,
        "fragment mismatched foreign end tag must record 1 parse error, got {:?}",
        frag.parse_errors
    );
}
