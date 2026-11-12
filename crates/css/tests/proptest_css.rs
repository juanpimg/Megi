//! Property-based tests for `css` tokenizer, stylesheet/declaration parsers,
//! selector matching, `@media`, `calc()`, color parsing, and style cascade (#961).

use css::{
    MediaContext, parse_list_of_declarations, parse_media_query_list, parse_stylesheet,
    resolve_document_styles_with_viewport, tokenizer::tokenize, try_query_selector,
    try_query_selector_all,
};
use dom::{Attribute, Document, Namespace};
use proptest::prelude::*;

fn sample_document() -> Document {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let div = doc.create_element(
        Namespace::Html,
        "div",
        vec![
            Attribute::new("id", "main"),
            Attribute::new("class", "box active"),
        ],
    );
    let span = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, div);
    doc.append_child(div, span);
    doc
}

fn css_like_string() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("{".to_string()),
            Just("}".to_string()),
            Just("(".to_string()),
            Just(")".to_string()),
            Just("[".to_string()),
            Just("]".to_string()),
            Just(":".to_string()),
            Just(";".to_string()),
            Just(",".to_string()),
            Just("!".to_string()),
            Just("important".to_string()),
            Just("@media".to_string()),
            Just("@layer".to_string()),
            Just("@keyframes".to_string()),
            Just("calc(".to_string()),
            Just("rgb(".to_string()),
            Just("hsl(".to_string()),
            Just("var(--x,".to_string()),
            Just(":is(".to_string()),
            Just(":where(".to_string()),
            Just(":has(".to_string()),
            Just("div#main.box".to_string()),
            Just("10px".to_string()),
            Just("50%".to_string()),
            Just("/* comment */".to_string()),
            any::<String>(),
        ],
        0..40,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn tokenizer_arbitrary_utf8_never_panics(input in any::<String>()) {
        let tokens = tokenize(&input);
        prop_assert!(tokens.len() <= input.len().saturating_add(16));
    }

    #[test]
    fn parse_stylesheet_arbitrary_utf8_never_panics(input in any::<String>()) {
        let sheet = parse_stylesheet(&input);
        prop_assert_eq!(sheet.rules.len(), parse_stylesheet(&input).rules.len());
    }

    #[test]
    fn parse_list_of_declarations_arbitrary_utf8_never_panics(input in any::<String>()) {
        let decls = parse_list_of_declarations(&input);
        prop_assert_eq!(decls.len(), parse_list_of_declarations(&input).len());
    }

    #[test]
    fn parse_media_query_list_arbitrary_utf8_never_panics(input in any::<String>()) {
        let mql = parse_media_query_list(&input);
        let ctx = MediaContext::with_viewport(1280.0, 720.0);
        let _ = css::evaluate_media(&mql, &ctx);
    }

    #[test]
    fn try_query_selector_arbitrary_utf8_never_panics(sel in any::<String>()) {
        let doc = sample_document();
        let _ = try_query_selector(&doc, &sel);
        let _ = try_query_selector_all(&doc, &sel);
    }

    #[test]
    fn color_declarations_never_panic(
        r in any::<i32>(),
        g in any::<i32>(),
        b in any::<i32>(),
        a in any::<f32>(),
        tail in "[a-zA-Z0-9 %,./()-]{0,20}",
    ) {
        let doc = sample_document();
        let css = format!(
            "#main {{ color: rgb({r} {g} {b} / {a}); background-color: hsl({r}deg 50% 50% / {a}); border-color: rgb({r},{g},{b},{tail}); }}"
        );
        let styles = resolve_document_styles_with_viewport(&doc, Some(&css), 800.0, 600.0);
        prop_assert!(!styles.is_empty());
    }

    #[test]
    fn calc_expressions_never_panic(expr in css_like_string()) {
        let doc = sample_document();
        let css = format!("#main {{ width: calc({expr}); height: calc(10px + ({expr})); }}");
        let styles = resolve_document_styles_with_viewport(&doc, Some(&css), 800.0, 600.0);
        for style in styles.values() {
            prop_assert!(style.opacity.is_finite());
            prop_assert!(style.font_size.is_finite());
        }
    }

    #[test]
    fn deeply_nested_blocks_and_parens_never_overflow_stack(depth in 1usize..400) {
        let mut css = String::with_capacity(depth * 4 + 16);
        for _ in 0..depth {
            css.push_str("{(");
        }
        css.push_str("a:b");
        for _ in 0..depth {
            css.push_str(")}");
        }
        let _ = parse_stylesheet(&css);
        let _ = parse_list_of_declarations(&css);
    }

    #[test]
    fn cascade_on_css_like_input_is_deterministic_and_finite(
        css in css_like_string(),
        w in 0.0f32..5000.0,
        h in 0.0f32..5000.0,
    ) {
        let doc = sample_document();
        let s1 = resolve_document_styles_with_viewport(&doc, Some(&css), w, h);
        let s2 = resolve_document_styles_with_viewport(&doc, Some(&css), w, h);
        prop_assert_eq!(s1.len(), s2.len());
        for (id, a) in &s1 {
            let b = &s2[id];
            prop_assert_eq!(a.color, b.color);
            prop_assert_eq!(a.display, b.display);
            prop_assert!(a.font_size.is_finite());
            prop_assert!(a.opacity.is_finite());
        }
    }

    #[test]
    fn selector_matching_with_css_like_strings_never_panics(sel in css_like_string()) {
        let doc = sample_document();
        let _ = try_query_selector_all(&doc, &sel);
    }
}
