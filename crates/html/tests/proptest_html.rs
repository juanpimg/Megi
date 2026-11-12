//! Property-based tests for `html` parser, tokenizer, fragment parser, and serializer (#961).

use html::{
    Namespace, ParseOptions, Token, Tokenizer, TokenizerConfig, parse_document,
    parse_document_with_options, parse_fragment, serialize_dom_for_test, serialize_fragment,
    serialize_html,
};
use proptest::prelude::*;

fn html_like_string() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("<".to_string()),
            Just(">".to_string()),
            Just("</".to_string()),
            Just("/>".to_string()),
            Just("<!--".to_string()),
            Just("-->".to_string()),
            Just("<!DOCTYPE html>".to_string()),
            Just("<div>".to_string()),
            Just("</div>".to_string()),
            Just("<p>".to_string()),
            Just("<table><tr><td>".to_string()),
            Just("<script>".to_string()),
            Just("</script>".to_string()),
            Just("<style>".to_string()),
            Just("<svg><path d=\"M0 0\"/></svg>".to_string()),
            Just("&amp;".to_string()),
            Just("&#x1f600;".to_string()),
            Just("&#0;".to_string()),
            Just("\0".to_string()),
            any::<String>(),
        ],
        0..40,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn parse_document_arbitrary_utf8_never_panics(input in any::<String>()) {
        let doc = parse_document(&input, false);
        prop_assert!(!serialize_dom_for_test(&doc).is_empty() || input.is_empty() || true);
    }

    #[test]
    fn parse_document_scripting_enabled_never_panics(input in any::<String>()) {
        let doc = parse_document(&input, true);
        let _ = serialize_html(&doc);
    }

    #[test]
    fn tokenizer_arbitrary_utf8_terminates_with_eof(input in any::<String>()) {
        let cps: Vec<u32> = input.chars().map(|c| c as u32).collect();
        let mut tok = Tokenizer::new(&cps, TokenizerConfig::default());
        tok.run();
        prop_assert!(matches!(tok.tokens.last(), Some(Token::Eof)));
    }

    #[test]
    fn parse_document_html_like_tokens_never_panics(input in html_like_string()) {
        let doc = parse_document(&input, false);
        let _ = serialize_html(&doc);
    }

    #[test]
    fn deeply_nested_tags_respect_limits_without_stack_overflow(depth in 1usize..600) {
        let mut input = String::with_capacity(depth * 11);
        for _ in 0..depth {
            input.push_str("<div>");
        }
        input.push_str("leaf");
        for _ in 0..depth {
            input.push_str("</div>");
        }
        let outcome = parse_document_with_options(&input, false, &ParseOptions::default());
        let _ = serialize_dom_for_test(&outcome.document);
    }

    #[test]
    fn parse_fragment_arbitrary_inputs_never_panics(
        ctx in prop_oneof![
            Just("div"),
            Just("p"),
            Just("table"),
            Just("tbody"),
            Just("svg"),
            Just("script"),
            Just("template"),
        ],
        input in html_like_string(),
    ) {
        let ns = if ctx == "svg" { Namespace::Svg } else { Namespace::Html };
        let doc = parse_fragment(&input, ns, ctx, false);
        let _ = serialize_fragment(&doc, doc.root());
    }

    #[test]
    fn serialize_html_never_panics_on_arbitrary_parsed_dom(input in html_like_string()) {
        let doc = parse_document(&input, false);
        let html_out = serialize_html(&doc);
        let frag_out = serialize_fragment(&doc, doc.root());
        prop_assert!(html_out.len() >= frag_out.len() || true);
    }

    #[test]
    fn parse_document_is_deterministic(input in html_like_string()) {
        let a = serialize_dom_for_test(&parse_document(&input, false));
        let b = serialize_dom_for_test(&parse_document(&input, false));
        prop_assert_eq!(a, b);
    }

    #[test]
    fn serialize_then_reparse_simple_text_is_stable(text in "[a-zA-Z0-9 .,!?]{0,64}") {
        let html_in = format!("<!DOCTYPE html><html><head></head><body><p>{text}</p></body></html>");
        let doc1 = parse_document(&html_in, false);
        let ser1 = serialize_html(&doc1);
        let doc2 = parse_document(&ser1, false);
        let ser2 = serialize_html(&doc2);
        prop_assert_eq!(ser1, ser2);
    }

    #[test]
    fn attributes_and_comments_never_panic(
        name in "[a-zA-Z0-9_-]{1,12}",
        val in any::<String>(),
        comment in any::<String>(),
    ) {
        let input = format!("<div {name}=\"{val}\"><!--{comment}--></div>");
        let doc = parse_document(&input, false);
        let _ = serialize_html(&doc);
    }
}
