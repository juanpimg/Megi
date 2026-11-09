//! # Megi CSS (`css`)
//!
//! Foundational types, W3C CSS Syntax Module Level 3 tokenizer & parser, and Selectors Level 4
//! matching engine operating over [`dom::Document`] and [`dom::NodeId`].

pub mod cascade;
pub mod component_value;
pub mod parser;
pub mod properties;
pub mod rule;
pub mod selector;
pub mod token;
pub mod tokenizer;
pub mod user_agent;

pub use cascade::{
    CascadeOriginImportance, CompiledRule, MatchedDeclaration, Origin, StyleCollection,
    resolve_cascaded_properties, resolve_document_styles,
};
pub use component_value::{ComponentValue, FunctionCall, SimpleBlock};
pub use parser::{TokenStream, consume_declaration, parse_list_of_declarations, parse_stylesheet};
pub use properties::{
    BorderStyle, BoxSizing, Clear, Color, ComputedStyle, Display, Float, FontStyle, Length,
    ListStyleType, Overflow, Position, RectEdges, TextAlign, TextDecoration, VerticalAlign,
    Visibility, WhiteSpace,
};
pub use rule::{AtRule, Declaration, Rule, StyleRule, Stylesheet};
pub use selector::{
    AnPlusB, AttrOperator, AttributeSelector, Combinator, ComplexSelector, CompoundSelector,
    PseudoClass, SelectorList, Specificity, parse_selectors,
};
pub use token::{CssToken, HashType, NumberType};
pub use tokenizer::{CssTokenizer, preprocess_css_codepoints, preprocess_css_input, tokenize};
pub use user_agent::{
    USER_AGENT_STYLESHEET, create_user_agent_style_collection, parse_user_agent_stylesheet,
};

/// A basic compound selector (tag name, `#id`, `.class`) for matching DOM elements.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

impl SimpleSelector {
    /// Evaluates whether the element at `node_id` in `doc` matches this selector.
    pub fn matches(&self, doc: &dom::Document, node_id: dom::NodeId) -> bool {
        let compound = CompoundSelector {
            tag_name: self.tag_name.clone(),
            id: self.id.clone(),
            classes: self.classes.clone(),
            attributes: Vec::new(),
            pseudo_classes: Vec::new(),
        };
        compound.matches(doc, node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dom::{Attribute, Document, Namespace};

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

    /// Computes the style of `body > <tag style="...">` and returns it.
    fn style_with_inline(tag: &str, inline: &str) -> ComputedStyle {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let element =
            doc.create_element(Namespace::Html, tag, vec![Attribute::new("style", inline)]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, element);
        let styles = resolve_document_styles(&doc, None);
        styles.get(&element).cloned().expect("computed style")
    }

    /// Computes the styles of `body > div[style=parent] > p[style=child]`.
    fn style_pair(parent_inline: &str, child_inline: &str) -> (ComputedStyle, ComputedStyle) {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let parent = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", parent_inline)],
        );
        let child = doc.create_element(
            Namespace::Html,
            "p",
            vec![Attribute::new("style", child_inline)],
        );
        doc.append_child(doc.root(), body);
        doc.append_child(body, parent);
        doc.append_child(parent, child);
        let styles = resolve_document_styles(&doc, None);
        (
            styles.get(&parent).cloned().expect("parent style"),
            styles.get(&child).cloned().expect("child style"),
        )
    }

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
        // Unimplemented functional pseudo-classes are rejected, not treated as matching all.
        assert_eq!(parse_selectors("p:not(.x)").selectors.len(), 0);
        assert_eq!(parse_selectors("p:is(.x)").selectors.len(), 0);
        assert_eq!(parse_selectors("p:where(.x)").selectors.len(), 0);

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
}
