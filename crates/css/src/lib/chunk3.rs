//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;


#[cfg(test)]
pub(crate) mod tests_part2 {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use megi_core::{Attribute, Document, Namespace};
    #[allow(unused_imports)]
    use crate::lib_tests_helpers::*;
    #[test]
    fn test_border_shorthand_four_value_expansion() {
        let style = style_with_inline(
            "div",
            "border-width: 1px 2px 3px 4px; border-style: solid dashed dotted double; \
             border-color: red green blue black;",
        );
        assert_eq!(
            style.border_width,
            RectEdges {
                top: 1.0,
                right: 2.0,
                bottom: 3.0,
                left: 4.0
            }
        );
        assert_eq!(
            style.border_style,
            RectEdges {
                top: BorderStyle::Solid,
                right: BorderStyle::Dashed,
                bottom: BorderStyle::Dotted,
                left: BorderStyle::Double
            }
        );
        assert_eq!(
            style.border_color,
            RectEdges {
                top: Color::RED,
                right: Color::GREEN,
                bottom: Color::BLUE,
                left: Color::BLACK
            }
        );

        // Two-value expansion: top/bottom, then right/left (CSS 2.2 §8.5).
        let two = style_with_inline("div", "border-style: solid dashed;");
        assert_eq!(
            two.border_style,
            RectEdges {
                top: BorderStyle::Solid,
                right: BorderStyle::Dashed,
                bottom: BorderStyle::Solid,
                left: BorderStyle::Dashed
            }
        );

        // A duplicated slot invalidates the whole shorthand instead of partially applying it.
        let invalid = style_with_inline("div", "border: 1px solid red green;");
        assert_eq!(invalid.border_width, RectEdges::all(0.0));
    }

    #[test]
    fn test_border_width_keywords_and_current_color() {
        let style = style_with_inline(
            "div",
            "color: rgb(1, 2, 3); border: thick solid currentColor;",
        );
        assert_eq!(style.border_width, RectEdges::all(5.0));
        assert_eq!(style.border_color, RectEdges::all(Color::new(1, 2, 3, 255)));
    }

    #[test]
    fn test_background_shorthand_extracts_color_anywhere() {
        assert_eq!(
            style_with_inline("div", "background: #eee;").background_color,
            Color::new(238, 238, 238, 255)
        );
        assert_eq!(
            style_with_inline("div", "background: red none;").background_color,
            Color::RED
        );
        assert_eq!(
            style_with_inline("div", "background: url(bg.png) no-repeat #0f0;").background_color,
            Color::new(0, 255, 0, 255)
        );
        assert_eq!(
            style_with_inline("div", "background: none;").background_color,
            Color::TRANSPARENT
        );
        // The `background-color` longhand still wins over the shorthand.
        assert_eq!(
            style_with_inline("div", "background: red; background-color: blue;").background_color,
            Color::BLUE
        );
    }

    #[test]
    fn test_float_clear_overflow_visibility_parsed() {
        let style = style_with_inline(
            "div",
            "float: right; clear: both; overflow: scroll; visibility: hidden;",
        );
        assert_eq!(style.float, Float::Right);
        assert_eq!(style.clear, Clear::Both);
        assert_eq!(style.overflow, Overflow::Scroll);
        assert_eq!(style.visibility, Visibility::Hidden);

        let initial = ComputedStyle::initial();
        assert_eq!(initial.float, Float::None);
        assert_eq!(initial.clear, Clear::None);
        assert_eq!(initial.overflow, Overflow::Visible);
        assert_eq!(initial.visibility, Visibility::Visible);

        // `overflow: <x> <y>` is stored as its first (block-start) value for now.
        assert_eq!(
            style_with_inline("div", "overflow: hidden auto;").overflow,
            Overflow::Hidden
        );
        assert_eq!(Overflow::parse("clip"), Some(Overflow::Clip));
        assert_eq!(Visibility::parse("collapse"), Some(Visibility::Collapse));
        assert_eq!(Float::parse("left"), Some(Float::Left));
        assert_eq!(Clear::parse("right"), Some(Clear::Right));
    }

    #[test]
    fn test_visibility_and_list_style_inherited_text_decoration_not() {
        let (parent, child) = style_pair(
            "visibility: hidden; list-style-type: square; text-decoration: underline;",
            "",
        );
        assert_eq!(parent.visibility, Visibility::Hidden);
        assert_eq!(parent.list_style_type, ListStyleType::Square);
        assert_eq!(parent.text_decoration, TextDecoration::UNDERLINE);

        // `visibility` and `list-style-type` are inherited...
        assert_eq!(child.visibility, Visibility::Hidden);
        assert_eq!(child.list_style_type, ListStyleType::Square);
        // ...but `text-decoration` is not (CSS Text Decoration 3 §2).
        assert!(child.text_decoration.is_empty());
    }

    #[test]
    fn test_text_decoration_bitset() {
        let style = style_with_inline("div", "text-decoration: underline overline;");
        assert!(style.text_decoration.contains(TextDecoration::UNDERLINE));
        assert!(style.text_decoration.contains(TextDecoration::OVERLINE));
        assert!(!style.text_decoration.contains(TextDecoration::LINE_THROUGH));

        let strike = style_with_inline("div", "text-decoration: line-through;");
        assert_eq!(strike.text_decoration, TextDecoration::LINE_THROUGH);

        assert!(
            style_with_inline("div", "text-decoration: none;")
                .text_decoration
                .is_empty()
        );

        assert_eq!(TextDecoration::parse("none underline"), None);
        assert_eq!(TextDecoration::parse("blink"), None);
        // Unmodelled decoration components are ignored so the line keyword is not lost.
        assert_eq!(
            TextDecoration::parse("underline red").map(|d| d.bits()),
            Some(TextDecoration::UNDERLINE.bits())
        );
        let combined = TextDecoration::UNDERLINE | TextDecoration::LINE_THROUGH;
        assert_eq!(combined.bits(), 0b101);
    }

    #[test]
    fn test_list_style_type_values_and_ua_defaults() {
        // UA stylesheet: `ul` => disc (also the initial value), `ol` => decimal.
        assert_eq!(
            style_with_inline("ul", "").list_style_type,
            ListStyleType::Disc
        );
        assert_eq!(
            style_with_inline("ol", "").list_style_type,
            ListStyleType::Decimal
        );
        assert_eq!(
            style_with_inline("ul", "list-style-type: square;").list_style_type,
            ListStyleType::Square
        );
        assert_eq!(
            style_with_inline("ul", "list-style-type: none;").list_style_type,
            ListStyleType::None
        );
        // `list-style` shorthand fallback for elements without a UA longhand.
        assert_eq!(
            style_with_inline("div", "list-style: circle;").list_style_type,
            ListStyleType::Circle
        );
        assert_eq!(
            style_with_inline("div", "list-style: none inside;").list_style_type,
            ListStyleType::None
        );
    }

    #[test]
    fn test_dynamic_pseudo_classes_never_match() {
        let mut doc = Document::new();
        let a = doc.create_element(
            Namespace::Html,
            "a",
            vec![Attribute::new("href", "https://example.test/")],
        );
        doc.append_child(doc.root(), a);

        for selector in [
            "a:hover",
            "a:active",
            "a:focus",
            "a:visited",
            "a:link",
            "a:focus-visible",
        ] {
            let list = parse_selectors(selector);
            assert_eq!(
                list.selectors.len(),
                1,
                "{selector} should stay a valid selector"
            );
            assert!(
                !list.matches(&doc, a),
                "{selector} must not match a plain <a> element"
            );
        }

        // Pseudo-elements never match the originating element, but keep the rest of the
        // selector list valid.
        assert!(!parse_selectors("a::before").matches(&doc, a));
        assert!(parse_selectors("a::before, a").matches(&doc, a));
        // Legacy single-colon pseudo-element syntax is handled the same way.
        assert!(!parse_selectors("a:before").matches(&doc, a));
        assert!(parse_selectors("a:first-line, a").matches(&doc, a));
    }

    #[test]
    fn test_unknown_pseudo_class_invalidates_selector_list() {
        let mut doc = Document::new();
        let p = doc.create_element(Namespace::Html, "p", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, p);

        assert_eq!(parse_selectors("p:unknown-pseudo").selectors.len(), 0);
        // One invalid selector invalidates the whole list (CSS Syntax 3 §5.4.4).
        assert_eq!(parse_selectors("p, p:unknown-pseudo").selectors.len(), 0);
        // Functional pseudo-classes are supported now: they parse and match correctly.
        assert_eq!(parse_selectors("p:not(.x)").selectors.len(), 1);
        assert!(parse_selectors("p:not(.x)").matches(&doc, p));
        assert!(!parse_selectors("p:is(.x)").matches(&doc, p));
        assert!(!parse_selectors("p:where(.x)").matches(&doc, p));
        // An empty or invalid argument still invalidates the selector.
        assert_eq!(parse_selectors("p:is()").selectors.len(), 0);
        assert_eq!(parse_selectors("p:not(:unknown-pseudo)").selectors.len(), 0);

        // The cascade drops rules whose selector list is invalid instead of over-matching.
        let styles = resolve_document_styles(&doc, Some("p:unknown-pseudo { color: red; }"));
        assert_eq!(styles.get(&p).expect("p style").color, Color::BLACK);
    }

    #[test]
    fn test_attribute_selector_with_comma_inside_value() {
        let mut doc = Document::new();
        let p = doc.create_element(
            Namespace::Html,
            "p",
            vec![Attribute::new("title", "Hello, world")],
        );
        doc.append_child(doc.root(), p);

        // F15: the comma inside the quoted value must not split the selector list.
        let list = parse_selectors("[title=\"Hello, world\"]");
        assert_eq!(list.selectors.len(), 1);
        assert!(list.matches(&doc, p));

        // Top-level commas still split, one quoted selector at a time.
        let list = parse_selectors("span, [title=\"Hello, world\"]");
        assert_eq!(list.selectors.len(), 2);
        assert!(list.matches(&doc, p));

        // Case-sensitivity flag and unquoted values keep working.
        assert!(parse_selectors("[title=\"hello, world\" i]").matches(&doc, p));
        assert!(!parse_selectors("[title=\"hello, world\"]").matches(&doc, p));
        assert!(parse_selectors("[title^=\"Hello\"]").matches(&doc, p));
        assert!(parse_selectors("[title]").matches(&doc, p));

        // The full cascade path (stylesheet parsing -> component-value serialization ->
        // selector list parsing) also keeps the comma inside the attribute value.
        let styles =
            resolve_document_styles(&doc, Some("[title=\"Hello, world\"] { color: red; }"));
        assert_eq!(styles.get(&p).expect("p style").color, Color::RED);
    }

    #[test]
    fn test_selector_matches_svg_and_mathml() {
        let mut doc = Document::new();
        let svg = doc.create_element(Namespace::Svg, "svg", vec![]);
        let circle = doc.create_element(
            Namespace::Svg,
            "circle",
            vec![Attribute::new("class", "dot")],
        );
        let path = doc.create_element(Namespace::Svg, "path", vec![]);
        let math = doc.create_element(Namespace::MathMl, "math", vec![]);
        let mi = doc.create_element(Namespace::MathMl, "mi", vec![]);
        doc.append_child(doc.root(), svg);
        doc.append_child(svg, circle);
        doc.append_child(svg, path);
        doc.append_child(doc.root(), math);
        doc.append_child(math, mi);

        // F14: selectors now match foreign (SVG/MathML) elements.
        assert!(parse_selectors("circle").matches(&doc, circle));
        assert!(parse_selectors("svg > circle").matches(&doc, circle));
        assert!(parse_selectors("*").matches(&doc, svg));
        assert!(parse_selectors(".dot").matches(&doc, circle));
        assert!(parse_selectors("math > mi").matches(&doc, mi));

        // HTML element names stay case-insensitive; foreign element names are case-sensitive.
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), div);
        assert!(parse_selectors("DIV").matches(&doc, div));
        assert!(!parse_selectors("CIRCLE").matches(&doc, circle));

        // The cascade now produces computed styles for foreign elements too.
        let styles = resolve_document_styles(&doc, Some("circle { color: red; }"));
        assert_eq!(styles.get(&circle).expect("circle style").color, Color::RED);
    }

    // ---------------------------------------------------------------------------------------

    // Round 2: Selectors Level 4 functional/structural pseudo-classes, shorthand expansion in

    // the cascade and the `font` shorthand.

    // ---------------------------------------------------------------------------------------
}
