//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;


#[cfg(test)]
pub(crate) mod tests_part3 {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use megi_core::{Attribute, Document, Namespace};
    #[allow(unused_imports)]
    use crate::lib_tests_helpers::*;
    #[test]
    fn test_is_where_not_matching_and_specificity() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("class", "card")],
        );
        let p = doc.create_element(Namespace::Html, "p", vec![Attribute::new("class", "note")]);
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, div);
        doc.append_child(div, p);
        doc.append_child(div, span);

        assert!(parse_selectors("div:is(.card, .panel)").matches(&doc, div));
        assert!(!parse_selectors("div:is(.panel)").matches(&doc, div));
        assert!(parse_selectors("p:is(.note, .warning)").matches(&doc, p));
        assert!(parse_selectors("div:where(.card)").matches(&doc, div));
        assert!(parse_selectors("p:not(.warning)").matches(&doc, p));
        assert!(!parse_selectors("p:not(.note)").matches(&doc, p));
        // Nested functional pseudo-classes.
        assert!(parse_selectors("div:is(:not(.panel), .card)").matches(&doc, div));

        // Specificity: :is()/:not()/:has() take the maximum argument, :where() contributes zero.
        assert_eq!(
            parse_selectors(":is(.a, #b)").max_specificity(),
            Specificity::new(1, 0, 0)
        );
        assert_eq!(
            parse_selectors(":not(#b)").max_specificity(),
            Specificity::new(1, 0, 0)
        );
        assert_eq!(
            parse_selectors(":where(#b)").max_specificity(),
            Specificity::ZERO
        );
        assert_eq!(
            parse_selectors(":has(> #b)").max_specificity(),
            Specificity::new(1, 0, 0)
        );
        assert_eq!(
            parse_selectors("p:is(.a, #b)").max_specificity(),
            Specificity::new(1, 0, 1)
        );
    }

    #[test]
    fn test_has_relative_selectors() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![Attribute::new("id", "anchor")]);
        let p = doc.create_element(Namespace::Html, "p", vec![Attribute::new("class", "note")]);
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        let em = doc.create_element(Namespace::Html, "em", vec![]);
        let aside = doc.create_element(Namespace::Html, "aside", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, div);
        doc.append_child(div, p);
        doc.append_child(div, span);
        doc.append_child(p, em);
        doc.append_child(body, aside);

        // Descendant (default), child, next-sibling and subsequent-sibling combinators.
        assert!(parse_selectors("div:has(p)").matches(&doc, div));
        assert!(parse_selectors("div:has(> p.note)").matches(&doc, div));
        assert!(!parse_selectors("div:has(> span.note)").matches(&doc, div));
        assert!(parse_selectors("div:has(> p > em)").matches(&doc, div));
        assert!(parse_selectors("div:has(+ aside)").matches(&doc, div));
        assert!(!parse_selectors("div:has(+ span)").matches(&doc, div));
        assert!(parse_selectors("body:has(> div ~ aside)").matches(&doc, body));
        assert!(parse_selectors("div:has(~ aside)").matches(&doc, div));
        // A selector list inside :has().
        assert!(parse_selectors("div:has(> p, > aside)").matches(&doc, div));
        // :has() participates in the cascade like any other pseudo-class.
        let styles = resolve_document_styles(&doc, Some("div:has(> p.note) { color: red; }"));
        assert_eq!(styles.get(&div).expect("div style").color, Color::RED);
    }

    #[test]
    fn has_matches_beyond_the_old_10k_descendant_cap() {
        // Regression test for #1133: :has() used to stop after 10,000 descendants and
        // silently report a false negative for matches deeper in the subtree.
        let mut doc = Document::new();
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), div);
        let mut parent = div;
        for i in 0..10_005 {
            let attrs = if i == 10_004 {
                vec![Attribute::new("class", "target")]
            } else {
                vec![]
            };
            let span = doc.create_element(Namespace::Html, "span", attrs);
            doc.append_child(parent, span);
            parent = span;
        }
        assert!(
            parse_selectors("div:has(span.target)").matches(&doc, div),
            ":has() must find a match past the old 10k descendant cap"
        );
    }

    #[test]
    fn test_structural_pseudo_classes() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let d1 = doc.create_element(Namespace::Html, "div", vec![]);
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        let d2 = doc.create_element(Namespace::Html, "div", vec![]);
        let p = doc.create_element(Namespace::Html, "p", vec![]);
        let d3 = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), body);
        for child in [d1, span, d2, p, d3] {
            doc.append_child(body, child);
        }

        assert!(parse_selectors("div:first-of-type").matches(&doc, d1));
        assert!(!parse_selectors("div:first-of-type").matches(&doc, d2));
        assert!(parse_selectors("span:first-of-type").matches(&doc, span));
        assert!(parse_selectors("div:last-of-type").matches(&doc, d3));
        assert!(!parse_selectors("div:last-of-type").matches(&doc, d1));
        assert!(parse_selectors("p:only-of-type").matches(&doc, p));
        assert!(!parse_selectors("div:only-of-type").matches(&doc, d2));

        assert!(parse_selectors("div:nth-of-type(1)").matches(&doc, d1));
        assert!(parse_selectors("div:nth-of-type(2)").matches(&doc, d2));
        assert!(parse_selectors("div:nth-of-type(odd)").matches(&doc, d3));
        assert!(!parse_selectors("div:nth-of-type(2)").matches(&doc, d3));
        assert!(parse_selectors("div:nth-last-of-type(1)").matches(&doc, d3));
        assert!(parse_selectors("div:nth-last-of-type(3)").matches(&doc, d1));

        // :nth-last-child counts all element siblings from the end.
        assert!(parse_selectors("div:nth-last-child(1)").matches(&doc, d3));
        assert!(parse_selectors("p:nth-last-child(2)").matches(&doc, p));
        assert!(parse_selectors("div:nth-last-child(odd)").matches(&doc, d2));

        // Single-child case keeps existing semantics.
        let lone_child = doc.create_element(Namespace::Html, "em", vec![]);
        let lone_parent = doc.create_element(Namespace::Html, "section", vec![]);
        doc.append_child(lone_parent, lone_child);
        doc.append_child(body, lone_parent);
        assert!(parse_selectors("em:only-child").matches(&doc, lone_child));
    }

    #[test]
    fn test_cascade_shorthand_expansion_ordering() {
        // Author `margin: 0` must beat the UA `h1 { margin-block: 0.67em }` longhands.
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let h1 = doc.create_element(Namespace::Html, "h1", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, h1);
        let styles = resolve_document_styles(&doc, Some("h1 { margin: 0; }"));
        let h1_style = styles.get(&h1).expect("h1 style");
        assert_eq!(h1_style.margin, RectEdges::all(Length::Px(0.0)));

        // The UA `hr` computed style keeps its own border longhands.
        let hr = doc.create_element(Namespace::Html, "hr", vec![]);
        doc.append_child(body, hr);
        let styles = resolve_document_styles(&doc, None);
        let hr_style = styles.get(&hr).expect("hr style");
        assert_eq!(hr_style.border_width, RectEdges::all(1.0));
        assert_eq!(hr_style.border_style, RectEdges::all(BorderStyle::Solid));

        // Same-rule source order now decides between shorthand and longhand.
        let later_shorthand =
            style_with_inline("div", "border-top: 2px solid red; border: 1px solid black;");
        assert_eq!(later_shorthand.border_width, RectEdges::all(1.0));
        assert_eq!(later_shorthand.border_color, RectEdges::all(Color::BLACK));

        let later_longhand = style_with_inline(
            "div",
            "border: 1px solid black; border-top: 4px dashed red;",
        );
        assert_eq!(later_longhand.border_width.top, 4.0);
        assert_eq!(later_longhand.border_style.top, BorderStyle::Dashed);
        assert_eq!(later_longhand.border_width.right, 1.0);

        // `border: none` after the longhand resets all sides.
        let reset = style_with_inline("div", "border-top: 2px solid red; border: none;");
        assert_eq!(reset.border_width, RectEdges::all(0.0));

        // !important applies to every expanded longhand.
        let important =
            style_with_inline("div", "border: 2px solid red !important; border-top: none;");
        assert_eq!(important.border_width.top, 2.0);
        assert_eq!(important.border_width.right, 2.0);

        // margin-block vs margin follow source order in both directions.
        let logical_then_physical = style_with_inline("div", "margin-block: 10px; margin: 0;");
        assert_eq!(
            logical_then_physical.margin,
            RectEdges::all(Length::Px(0.0))
        );
        let physical_then_logical = style_with_inline("div", "margin: 0; margin-block: 10px;");
        assert_eq!(physical_then_logical.margin.top, Length::Px(10.0));
        assert_eq!(physical_then_logical.margin.bottom, Length::Px(10.0));
        assert_eq!(physical_then_logical.margin.left, Length::Px(0.0));

        // background shorthand vs background-color longhand: source order decides.
        assert_eq!(
            style_with_inline("div", "background-color: blue; background: red;").background_color,
            Color::RED
        );
        assert_eq!(
            style_with_inline("div", "background: red; background-color: blue;").background_color,
            Color::BLUE
        );
    }

    #[test]
    fn test_font_shorthand_computes_all_longhands() {
        let style = style_with_inline(
            "div",
            r#"font: italic bold 20px/1.5 "Helvetica Neue", Arial, sans-serif;"#,
        );
        assert_eq!(style.font_style, FontStyle::Italic);
        assert_eq!(style.font_weight, 700);
        assert_eq!(style.font_size, 20.0);
        assert_eq!(style.line_height, Some(30.0));
        assert!(style.font_family.contains("Helvetica Neue"));
        assert!(style.font_family.contains("sans-serif"));

        // Omitted longhands are reset to their initial values by the shorthand.
        let reset = style_with_inline("span", "font-weight: bold; font: 12px serif;");
        assert_eq!(reset.font_weight, 400);
        assert_eq!(reset.font_size, 12.0);
        assert_eq!(reset.font_family, "serif");
        assert!((reset.line_height.expect("line height") - 12.0 * 1.2).abs() < 0.01);

        // A `font` shorthand with a length line-height.
        let sized = style_with_inline("span", "font: 16px/24px monospace;");
        assert_eq!(sized.line_height, Some(24.0));

        // Invalid shorthand declarations are dropped instead of partially applied.
        let invalid = style_with_inline("span", "font: 12px;");
        assert_eq!(invalid.font_size, 16.0);
    }

    #[test]
    fn test_font_shorthand_in_cascade_beats_inherited() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let parent = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new(
                "style",
                "font-weight: bold; font-size: 20px;",
            )],
        );
        let child = doc.create_element(
            Namespace::Html,
            "p",
            vec![Attribute::new("style", "font: 12px serif;")],
        );
        doc.append_child(doc.root(), body);
        doc.append_child(body, parent);
        doc.append_child(parent, child);
        let styles = resolve_document_styles(&doc, None);
        let child_style = styles.get(&child).expect("child style");
        assert_eq!(child_style.font_size, 12.0);
        assert_eq!(child_style.font_weight, 400);
        assert_eq!(child_style.font_family, "serif");
    }

    #[test]
    fn test_media_queries_basic() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let p = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, p);

        // `screen` applies on the default (screen) medium; `print` does not.
        let styles = resolve_document_styles(&doc, Some("@media screen { p { color: red; } }"));
        assert_eq!(styles.get(&p).unwrap().color, Color::RED);
        let styles = resolve_document_styles(&doc, Some("@media print { p { color: red; } }"));
        assert_eq!(styles.get(&p).unwrap().color, Color::BLACK);
        let styles = resolve_document_styles(&doc, Some("@media all { p { color: red; } }"));
        assert_eq!(styles.get(&p).unwrap().color, Color::RED);

        // The `print` medium can be requested explicitly.
        let print = MediaContext {
            viewport: None,
            screen: false,
            color_scheme: ColorScheme::Light,
            reduced_motion: false,
        };
        let styles = resolve_document_styles_with_media(
            &doc,
            Some("@media print { p { color: red; } }"),
            print,
        );
        assert_eq!(styles.get(&p).unwrap().color, Color::RED);

        // Width features need a known viewport.
        let css = "@media (min-width: 600px) { p { color: red; } }";
        let unknown = resolve_document_styles(&doc, Some(css));
        assert_eq!(unknown.get(&p).unwrap().color, Color::BLACK);
        let wide = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
        assert_eq!(wide.get(&p).unwrap().color, Color::RED);
        let narrow = resolve_document_styles_with_viewport(&doc, Some(css), 400.0, 800.0);
        assert_eq!(narrow.get(&p).unwrap().color, Color::BLACK);

        // `and` combinations up to the stated basic support.
        let css = "@media screen and (min-width: 900px) { p { color: red; } }";
        let styles = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
        assert_eq!(styles.get(&p).unwrap().color, Color::BLACK);
        let styles = resolve_document_styles_with_viewport(&doc, Some(css), 1000.0, 600.0);
        assert_eq!(styles.get(&p).unwrap().color, Color::RED);

        // `max-width` with an em value (1em = 16px here).
        let css = "@media (max-width: 40em) { p { color: red; } }";
        let styles = resolve_document_styles_with_viewport(&doc, Some(css), 600.0, 800.0);
        assert_eq!(styles.get(&p).unwrap().color, Color::RED);
        let styles = resolve_document_styles_with_viewport(&doc, Some(css), 700.0, 800.0);
        assert_eq!(styles.get(&p).unwrap().color, Color::BLACK);

        // Comma-separated query lists match when any query matches.
        let css = "@media print, (min-width: 600px) { p { color: red; } }";
        let styles = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
        assert_eq!(styles.get(&p).unwrap().color, Color::RED);

        // Source order is preserved across @media blocks and normal rules.
        let css = "@media screen { p { color: red; } } p { color: blue; }";
        let styles = resolve_document_styles(&doc, Some(css));
        assert_eq!(styles.get(&p).unwrap().color, Color::BLUE);
        let css = "p { color: blue; } @media screen { p { color: red; } }";
        let styles = resolve_document_styles(&doc, Some(css));
        assert_eq!(styles.get(&p).unwrap().color, Color::RED);

        // Unsupported queries never apply (conservative).
        let css = "@media speech { p { color: red; } }";
        let styles = resolve_document_styles(&doc, Some(css));
        assert_eq!(styles.get(&p).unwrap().color, Color::BLACK);
    }
}
