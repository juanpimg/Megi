//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

#[cfg(test)]
pub(crate) mod tests_part1 {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use crate::lib_tests_helpers::*;
    #[allow(unused_imports)]
    use megi_core::{Attribute, Document, Namespace};
    #[test]
    fn test_simple_selector_matches_dom_element() {
        let mut doc = Document::new();
        let div = doc.create_element(
            Namespace::Html,
            "div",
            vec![
                Attribute::new("id", "main"),
                Attribute::new("class", "container active"),
            ],
        );
        doc.append_child(doc.root(), div);

        let sel = SimpleSelector {
            tag_name: Some("div".into()),
            id: Some("main".into()),
            classes: vec!["container".into(), "active".into()],
        };
        assert!(sel.matches(&doc, div));

        let non_matching = SimpleSelector {
            tag_name: Some("span".into()),
            id: None,
            classes: vec![],
        };
        assert!(!non_matching.matches(&doc, div));
    }

    #[test]
    fn test_css_tokenizer_basic_tokens() {
        let tokens = tokenize("h1.title { color: #f00; font-size: 16px; width: 100%; }");
        assert!(
            tokens
                .iter()
                .any(|t| matches!(t, CssToken::Ident(s) if s == "h1"))
        );
        assert!(tokens.iter().any(|t| matches!(t, CssToken::Delim('.'))));
        assert!(
            tokens
                .iter()
                .any(|t| matches!(t, CssToken::Ident(s) if s == "title"))
        );
        assert!(tokens.iter().any(|t| matches!(t, CssToken::LeftBrace)));
        assert!(
            tokens
                .iter()
                .any(|t| matches!(t, CssToken::Hash { value, .. } if value == "f00"))
        );
        assert!(tokens.iter().any(|t| matches!(t, CssToken::Dimension { value, unit, .. } if *value == 16.0 && unit == "px")));
        assert!(
            tokens
                .iter()
                .any(|t| matches!(t, CssToken::Percentage { value, .. } if *value == 100.0))
        );
        assert!(tokens.iter().any(|t| matches!(t, CssToken::RightBrace)));
    }

    #[test]
    fn test_css_tokenizer_escapes_and_comments() {
        let tokens = tokenize("/* comment */ \\61\\62 c { content: \"hello\\A world\"; }");
        assert!(
            tokens
                .iter()
                .any(|t| matches!(t, CssToken::Ident(s) if s == "abc"))
        );
        assert!(
            tokens
                .iter()
                .any(|t| matches!(t, CssToken::String(s) if s == "hello\nworld"))
        );
    }

    #[test]
    fn test_parse_stylesheet_and_declarations() {
        let css = r#"
            body > div.main {
                color: red !important;
                background-color: blue;
            }
            @media screen {
                p { font-size: 14px; }
            }
        "#;
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 2);

        if let Rule::Style(ref rule) = stylesheet.rules[0] {
            assert_eq!(rule.declarations.len(), 2);
            assert_eq!(rule.declarations[0].name, "color");
            assert!(rule.declarations[0].important);
            assert_eq!(rule.declarations[1].name, "background-color");
            assert!(!rule.declarations[1].important);
        } else {
            panic!("Expected style rule");
        }

        if let Rule::At(ref at) = stylesheet.rules[1] {
            assert_eq!(at.name, "media");
            assert!(at.block.is_some());
        } else {
            panic!("Expected at-rule");
        }
    }

    #[test]
    fn test_complex_selector_and_attribute_matching() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(
            Namespace::Html,
            "div",
            vec![
                Attribute::new("id", "container"),
                Attribute::new("class", "box active"),
                Attribute::new("data-mode", "test-case"),
            ],
        );
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, div);
        doc.append_child(div, span);

        let sel_list = parse_selectors("body > div#container.box[data-mode^=\"test\"] span");
        assert!(sel_list.matches(&doc, span));

        let non_match = parse_selectors("div > p");
        assert!(!non_match.matches(&doc, span));
    }

    #[test]
    fn test_pseudo_class_first_child_and_empty() {
        let mut doc = Document::new();
        let parent = doc.create_element(Namespace::Html, "ul", vec![]);
        let first = doc.create_element(Namespace::Html, "li", vec![]);
        let second = doc.create_element(Namespace::Html, "li", vec![]);
        doc.append_child(doc.root(), parent);
        doc.append_child(parent, first);
        doc.append_child(parent, second);

        let first_child_sel = parse_selectors("li:first-child");
        assert!(first_child_sel.matches(&doc, first));
        assert!(!first_child_sel.matches(&doc, second));

        let empty_sel = parse_selectors("li:empty");
        assert!(empty_sel.matches(&doc, first));
    }

    #[test]
    fn test_selector_specificity_calculation() {
        let sel_id = parse_selectors("#main");
        assert_eq!(sel_id.max_specificity(), Specificity::new(1, 0, 0));

        let sel_class = parse_selectors(".container.active");
        assert_eq!(sel_class.max_specificity(), Specificity::new(0, 2, 0));

        let sel_complex = parse_selectors("div#main > ul.nav li:first-child[data-role]");
        // div#main: 1 id, 1 tag = (1, 0, 1)
        // ul.nav: 1 class, 1 tag = (0, 1, 1)
        // li:first-child[data-role]: 1 pseudo, 1 attr, 1 tag = (0, 2, 1)
        // Total: a = 1, b = 3, c = 3
        assert_eq!(sel_complex.max_specificity(), Specificity::new(1, 3, 3));

        // Order check
        assert!(Specificity::new(1, 0, 0) > Specificity::new(0, 10, 10));
        assert!(Specificity::new(0, 2, 0) > Specificity::new(0, 1, 5));
    }

    #[test]
    fn test_cascade_origin_and_importance() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let h1 = doc.create_element(
            Namespace::Html,
            "h1",
            vec![
                Attribute::new("id", "heading"),
                Attribute::new("class", "title main"),
                Attribute::new("style", "color: red;"),
            ],
        );
        doc.append_child(doc.root(), body);
        doc.append_child(body, h1);

        let author_css = r#"
            h1 { color: black; font-size: 20px; }
            .title { color: green; font-size: 24px; }
            #heading { color: orange; font-size: 28px; }
            p { color: purple !important; }
        "#;

        let styles = resolve_document_styles(&doc, Some(author_css));
        let h1_style = styles.get(&h1).expect("h1 computed style");

        // Specificity: #heading (1,0,0) beats .title (0,1,0) and h1 (0,0,1) for font-size
        assert_eq!(h1_style.font_size, 28.0);
        // Inline style `color: red` beats author stylesheet `color: orange`
        assert_eq!(h1_style.color, Color::RED);
        // From User-Agent stylesheet: h1 has display: block and font-weight: 700
        assert_eq!(h1_style.display, Display::Block);
        assert_eq!(h1_style.font_weight, 700);
    }

    #[test]
    fn test_document_style_resolution_and_inheritance() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let p = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, p);

        let author_css = "body { color: rgb(0, 128, 0); font-family: Arial; }";
        let styles = resolve_document_styles(&doc, Some(author_css));

        let body_style = styles.get(&body).expect("body computed style");
        let p_style = styles.get(&p).expect("p computed style");

        // Body defaults from User-Agent: margin 8px
        assert_eq!(body_style.margin.top, Length::Px(8.0));
        assert_eq!(body_style.margin.left, Length::Px(8.0));

        // Paragraph inherits color and font-family from body
        assert_eq!(p_style.color, Color::new(0, 128, 0, 255));
        assert_eq!(p_style.font_family, "Arial");
        // Paragraph from UA has display: block
        assert_eq!(p_style.display, Display::Block);
    }

    #[test]
    fn test_white_space_and_vertical_align() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let pre = doc.create_element(Namespace::Html, "pre", vec![]);
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        let sup = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, pre);
        doc.append_child(pre, span);
        doc.append_child(body, sup);

        let author_css = "
            body { white-space: nowrap; }
            span.custom { vertical-align: 5px; }
            span.super { vertical-align: super; }
        ";
        let styles = resolve_document_styles(&doc, Some(author_css));

        let body_style = styles.get(&body).expect("body style");
        assert_eq!(body_style.white_space, WhiteSpace::Nowrap);

        // Pre has UA rule: white-space: pre ! (overrides inherited nowrap because UA/author cascade)
        // Wait, pre has UA rule white-space: pre; author rule is body { white-space: nowrap },
        // Author rule on body inherits to pre, BUT UA rule on pre is directly targeted at pre!
        let pre_style = styles.get(&pre).expect("pre style");
        assert_eq!(pre_style.white_space, WhiteSpace::Pre);
        assert_eq!(pre_style.font_family, "monospace");

        // Span inside pre inherits white-space: pre from pre
        let span_style = styles.get(&span).expect("span style");
        assert_eq!(span_style.white_space, WhiteSpace::Pre);

        // Test VerticalAlign parsing
        assert_eq!(
            VerticalAlign::parse("baseline"),
            Some(VerticalAlign::Baseline)
        );
        assert_eq!(VerticalAlign::parse("middle"), Some(VerticalAlign::Middle));
        assert_eq!(VerticalAlign::parse("top"), Some(VerticalAlign::Top));
        assert_eq!(VerticalAlign::parse("bottom"), Some(VerticalAlign::Bottom));
        assert_eq!(VerticalAlign::parse("sub"), Some(VerticalAlign::Sub));
        assert_eq!(VerticalAlign::parse("super"), Some(VerticalAlign::Super));
        assert_eq!(
            VerticalAlign::parse("text-top"),
            Some(VerticalAlign::TextTop)
        );
        assert_eq!(
            VerticalAlign::parse("text-bottom"),
            Some(VerticalAlign::TextBottom)
        );
        assert_eq!(
            VerticalAlign::parse("12px"),
            Some(VerticalAlign::Length(12.0))
        );
        assert_eq!(
            VerticalAlign::parse("-4px"),
            Some(VerticalAlign::Length(-4.0))
        );
    }

    // ---------------------------------------------------------------------------------------

    // Phase 4 foundations (WP-4.0 partial): border shorthands/`border-style: none`, background

    // shorthand color extraction, new computed properties and selector fidelity F13/F14/F15.

    // ---------------------------------------------------------------------------------------

    #[test]
    fn test_border_style_none_zeroes_computed_width() {
        // CSS 2.2 §8.5.3: `border-style: none` (the initial value) => computed width 0.
        let style = style_with_inline("div", "border-width: 5px; border-color: red;");
        assert_eq!(style.border_width, RectEdges::all(0.0));

        let hidden = style_with_inline("div", "border: 5px hidden red;");
        assert_eq!(hidden.border_width, RectEdges::all(0.0));
        assert_eq!(hidden.border_style, RectEdges::all(BorderStyle::Hidden));

        let solid = style_with_inline("div", "border: 5px solid red;");
        assert_eq!(solid.border_width, RectEdges::all(5.0));
        assert_eq!(solid.border_color, RectEdges::all(Color::RED));
        assert_eq!(solid.border_style, RectEdges::all(BorderStyle::Solid));

        // A non-none style without an explicit width uses the initial `medium` (3px).
        let medium_initial = style_with_inline("div", "border-style: solid;");
        assert_eq!(medium_initial.border_width, RectEdges::all(3.0));
    }

    #[test]
    fn test_per_side_border_longhands_and_shorthands() {
        // `<width> || <style> || <color>` in any order, per side.
        let style = style_with_inline(
            "div",
            "border-bottom: 4px solid red; border-left: blue dashed 2px;",
        );
        assert_eq!(style.border_width.bottom, 4.0);
        assert_eq!(style.border_style.bottom, BorderStyle::Solid);
        assert_eq!(style.border_color.bottom, Color::RED);
        assert_eq!(style.border_width.left, 2.0);
        assert_eq!(style.border_style.left, BorderStyle::Dashed);
        assert_eq!(style.border_color.left, Color::BLUE);
        assert_eq!(style.border_width.top, 0.0);
        assert_eq!(style.border_width.right, 0.0);

        // Per-side longhands override the per-side shorthand.
        let longhands = style_with_inline(
            "div",
            "border-top: 1px solid red; border-top-width: 6px; border-top-color: blue;",
        );
        assert_eq!(longhands.border_width.top, 6.0);
        assert_eq!(longhands.border_color.top, Color::BLUE);
        assert_eq!(longhands.border_style.top, BorderStyle::Solid);
        assert_eq!(longhands.border_width.bottom, 0.0);
    }
}
