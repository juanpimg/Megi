//! # Megi CSS (`css`)
//!
//! Foundational types, W3C CSS Syntax Module Level 3 tokenizer & parser, and Selectors Level 4
//! matching engine operating over [`dom::Document`] and [`dom::NodeId`].
//!
//! Round 2 additions: functional `:is()`/`:where()`/`:not()`/`:has()` and the remaining
//! structural pseudo-classes, pre-cascade shorthand expansion (see [`shorthand`]) so that
//! origin/specificity/source order decide per longhand, the `font` shorthand, and basic
//! `@media` support ([`MediaContext`], [`resolve_document_styles_with_viewport`]).

#![forbid(unsafe_code)]

pub mod calc;
pub mod cascade;
pub mod component_value;
pub mod custom;
mod flex;
mod grid;
pub mod media;
pub mod parser;
pub mod properties;
pub mod query;
pub mod rule;
pub mod selector;
pub mod shorthand;
pub mod state;
pub mod token;
pub mod tokenizer;
pub mod user_agent;

pub use calc::{
    CalcContext, CalcValue, evaluate as evaluate_calc, with_viewport as with_calc_viewport,
};
pub use cascade::{
    CascadeOriginImportance, CompiledRule, MatchedDeclaration, Origin, StyleCollection,
    resolve_cascaded_properties, resolve_document_styles, resolve_document_styles_with_media,
    resolve_document_styles_with_media_and_state, resolve_document_styles_with_viewport,
};
pub use component_value::{ComponentValue, FunctionCall, SimpleBlock};
pub use flex::{AlignItems, AlignSelf, FlexDirection, FlexWrap, JustifyContent};
pub use grid::{AutoRepeat, AutoRepeatKind, GridLine, GridTemplate, TrackMax, TrackMin, TrackSize};
pub use media::{
    MediaCondition, MediaContext, MediaFeature, MediaQualifier, MediaQuery, MediaQueryList,
    MediaType, Orientation, evaluate as evaluate_media, parse_media_query_list,
};
pub use parser::{TokenStream, consume_declaration, parse_list_of_declarations, parse_stylesheet};
pub use properties::CalcLengths;
pub use properties::{
    BackgroundAttachment, BackgroundClip, BackgroundLayer, BackgroundOrigin, BackgroundPosition,
    BackgroundRepeat, BackgroundRepeatAxis, BackgroundSize, BorderCollapse, BorderRadii,
    BorderStyle, BoxShadow, BoxSizing, CaptionSide, Clear, Color, ComputedStyle, ConicGradient,
    Display, EmptyCells, Float, FontStyle, Gradient, GradientKind, GradientStop,
    InterpolationSpace, Length, LinearDirection, LinearGradient, ListStyleType, MAX_GRADIENT_STOPS,
    MAX_GRADIENTS, Overflow, Position, RadialExtent, RadialGradient, RadialShape, Radius,
    RectEdges, TableLayout, TextAlign, TextDecoration, TextShadow, TextTransform, VerticalAlign,
    Visibility, WhiteSpace,
};
pub use query::{
    SelectorParseError, matches, matches_with_state, query_selector, query_selector_all,
    try_matches, try_matches_with_state, try_query_selector, try_query_selector_all,
};
pub use rule::{AtRule, Declaration, FontFaceRule, ImportRule, Rule, StyleRule, Stylesheet};
pub use selector::{
    AnPlusB, AttrOperator, AttributeSelector, Combinator, ComplexSelector, CompoundSelector,
    PseudoClass, RelativeSelector, SelectorList, Specificity, parse_selectors,
};
pub use shorthand::expand_declarations;
pub use state::{StateContext, StateGuard, current as current_state, with_state};
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
    #[test]
    fn test_line_height_number_factor_and_inheritance() {
        // Parent: font-size 16px, line-height 1.5 (unitless factor, computed px 24).
        let (parent, child) = style_pair("font-size: 16px; line-height: 1.5;", "font-size: 32px;");
        assert_eq!(parent.line_height, Some(24.0));
        assert_eq!(parent.line_height_factor, Some(1.5));

        // The child inherits the *factor*; layout re-resolves 1.5 * 32px = 48px. CSS keeps the
        // inherited computed px (24) for backwards compatibility.
        assert_eq!(child.line_height_factor, Some(1.5));
        assert_eq!(child.line_height, Some(24.0));

        // Absolute lengths and `normal` are not factors.
        let px = style_with_inline("div", "line-height: 24px;");
        assert_eq!(px.line_height, Some(24.0));
        assert_eq!(px.line_height_factor, None);

        let normal = style_with_inline("div", "line-height: normal;");
        assert_eq!(normal.line_height, Some(16.0 * 1.2));
        assert_eq!(normal.line_height_factor, None);

        // Percentages now compute against the element font-size (and are not a factor).
        let pct = style_with_inline("div", "font-size: 20px; line-height: 150%;");
        assert_eq!(pct.line_height, Some(30.0));
        assert_eq!(pct.line_height_factor, None);

        // `inherit_from` copies the factor too.
        let mut inherited = ComputedStyle::initial();
        inherited.inherit_from(&parent);
        assert_eq!(inherited.line_height_factor, Some(1.5));
        assert_eq!(inherited.line_height, Some(24.0));
    }

    #[test]
    fn test_cascade_details_inline_important_root_and_attribute_operators() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, body);

        // Inline `!important` beats an author `!important` declaration (inline specificity).
        let important = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", "color: red !important;")],
        );
        doc.append_child(body, important);
        let styles = resolve_document_styles(&doc, Some("div { color: blue !important; }"));
        assert_eq!(styles.get(&important).expect("style").color, Color::RED);

        // A normal inline declaration loses against an author `!important` declaration.
        let normal = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", "color: red;")],
        );
        doc.append_child(body, normal);
        let styles = resolve_document_styles(&doc, Some("div { color: blue !important; }"));
        assert_eq!(styles.get(&normal).expect("style").color, Color::BLUE);

        // `:root` matches the document element only (the non-inherited property shows it).
        let styles = resolve_document_styles(&doc, Some(":root { margin-top: 7px; }"));
        assert_eq!(
            styles.get(&html).expect("html style").margin.top,
            Length::Px(7.0)
        );
        assert_eq!(
            styles.get(&body).expect("body style").margin.top,
            Length::Px(8.0)
        );

        // Attribute selectors: `~=`, `|=`, `^=`, `$=` and `*=` beyond the `=` already covered.
        let element = doc.create_element(
            Namespace::Html,
            "p",
            vec![
                Attribute::new("data-words", "alpha beta"),
                Attribute::new("lang", "en-US"),
                Attribute::new("data-prefix", "megi-core"),
                Attribute::new("data-suffix", "engine.css"),
                Attribute::new("data-sub", "x-megi-y"),
            ],
        );
        doc.append_child(body, element);
        for selector in [
            "[data-words~=\"beta\"]",
            "[lang|=\"en\"]",
            "[data-prefix^=\"megi\"]",
            "[data-suffix$=\".css\"]",
            "[data-sub*=\"megi\"]",
        ] {
            assert!(
                parse_selectors(selector).matches(&doc, element),
                "{selector} should match"
            );
        }
        for selector in [
            "[data-words~=\"gamma\"]",
            "[lang|=\"fr\"]",
            "[data-prefix^=\"core\"]",
            "[data-suffix$=\".png\"]",
            "[data-sub*=\"nope\"]",
        ] {
            assert!(
                !parse_selectors(selector).matches(&doc, element),
                "{selector} should not match"
            );
        }
    }
    // ---------------------------------------------------------------------------------------
    // Round 5: additive table foundations (display table-*, border-spacing, border-collapse,
    // table-layout, caption-side, empty-cells and the UA table rules of WHATWG HTML §15).
    // ---------------------------------------------------------------------------------------

    #[test]
    fn test_display_table_variants_parse() {
        for (value, expected) in [
            ("table", Display::Table),
            ("inline-table", Display::InlineTable),
            ("table-row-group", Display::TableRowGroup),
            ("table-header-group", Display::TableHeaderGroup),
            ("table-footer-group", Display::TableFooterGroup),
            ("table-row", Display::TableRow),
            ("table-column-group", Display::TableColumnGroup),
            ("table-column", Display::TableColumn),
            ("table-cell", Display::TableCell),
            ("table-caption", Display::TableCaption),
        ] {
            assert_eq!(Display::parse(value), Some(expected), "display: {value}");
        }
        // Existing values keep working and unknown values stay invalid.
        assert_eq!(Display::parse("block"), Some(Display::Block));
        assert_eq!(Display::parse("list-item"), Some(Display::ListItem));
        assert_eq!(Display::parse("table-foo"), None);
    }

    #[test]
    fn test_table_properties_parse_and_initial() {
        let initial = ComputedStyle::initial();
        assert_eq!(initial.border_spacing, (0.0, 0.0));
        assert_eq!(initial.border_collapse, BorderCollapse::Separate);
        assert_eq!(initial.table_layout, TableLayout::Auto);
        assert_eq!(initial.caption_side, CaptionSide::Top);
        assert_eq!(initial.empty_cells, EmptyCells::Show);

        // One or two lengths; font-relative lengths resolve against the font-size.
        assert_eq!(
            style_with_inline("table", "border-spacing: 3px;").border_spacing,
            (3.0, 3.0)
        );
        assert_eq!(
            style_with_inline("table", "border-spacing: 1px 4px;").border_spacing,
            (1.0, 4.0)
        );
        assert_eq!(
            style_with_inline("table", "border-spacing: 0.5em 1em;").border_spacing,
            (8.0, 16.0)
        );

        // `div` has no UA `border-spacing`, so invalid values fall back to the initial value.
        for invalid in [
            "border-spacing: -1px;",
            "border-spacing: 10%;",
            "border-spacing: auto;",
        ] {
            assert_eq!(
                style_with_inline("div", invalid).border_spacing,
                (0.0, 0.0),
                "{invalid}"
            );
        }

        let table = style_with_inline(
            "table",
            "border-collapse: collapse; table-layout: fixed; \
             caption-side: bottom; empty-cells: hide;",
        );
        assert_eq!(table.border_collapse, BorderCollapse::Collapse);
        assert_eq!(table.table_layout, TableLayout::Fixed);
        assert_eq!(table.caption_side, CaptionSide::Bottom);
        assert_eq!(table.empty_cells, EmptyCells::Hide);

        // Enum parsers.
        assert_eq!(
            BorderCollapse::parse("separate"),
            Some(BorderCollapse::Separate)
        );
        assert_eq!(
            BorderCollapse::parse("collapse"),
            Some(BorderCollapse::Collapse)
        );
        assert_eq!(BorderCollapse::parse("nope"), None);
        assert_eq!(TableLayout::parse("fixed"), Some(TableLayout::Fixed));
        assert_eq!(CaptionSide::parse("bottom"), Some(CaptionSide::Bottom));
        assert_eq!(EmptyCells::parse("hide"), Some(EmptyCells::Hide));

        // None of the table properties are inherited.
        let (parent, child) = style_pair(
            "border-spacing: 5px; border-collapse: collapse; table-layout: fixed; \
             caption-side: bottom; empty-cells: hide;",
            "",
        );
        assert_eq!(parent.border_spacing, (5.0, 5.0));
        assert_eq!(child.border_spacing, (0.0, 0.0));
        assert_eq!(child.border_collapse, BorderCollapse::Separate);
        assert_eq!(child.table_layout, TableLayout::Auto);
        assert_eq!(child.caption_side, CaptionSide::Top);
        assert_eq!(child.empty_cells, EmptyCells::Show);
    }

    #[test]
    fn test_ua_table_stylesheet() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let table = doc.create_element(Namespace::Html, "table", vec![]);
        let caption = doc.create_element(Namespace::Html, "caption", vec![]);
        let colgroup = doc.create_element(Namespace::Html, "colgroup", vec![]);
        let col = doc.create_element(Namespace::Html, "col", vec![]);
        let thead = doc.create_element(Namespace::Html, "thead", vec![]);
        let head_row = doc.create_element(Namespace::Html, "tr", vec![]);
        let th = doc.create_element(Namespace::Html, "th", vec![]);
        let tbody = doc.create_element(Namespace::Html, "tbody", vec![]);
        let body_row = doc.create_element(Namespace::Html, "tr", vec![]);
        let td = doc.create_element(Namespace::Html, "td", vec![]);
        let tfoot = doc.create_element(Namespace::Html, "tfoot", vec![]);
        let foot_row = doc.create_element(Namespace::Html, "tr", vec![]);
        let foot_cell = doc.create_element(Namespace::Html, "td", vec![]);

        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        doc.append_child(body, table);
        doc.append_child(table, caption);
        doc.append_child(table, colgroup);
        doc.append_child(colgroup, col);
        doc.append_child(table, thead);
        doc.append_child(thead, head_row);
        doc.append_child(head_row, th);
        doc.append_child(table, tbody);
        doc.append_child(tbody, body_row);
        doc.append_child(body_row, td);
        doc.append_child(table, tfoot);
        doc.append_child(tfoot, foot_row);
        doc.append_child(foot_row, foot_cell);

        let styles = resolve_document_styles(&doc, None);
        let style = |node| styles.get(&node).expect("table style");

        assert_eq!(style(table).display, Display::Table);
        assert_eq!(style(table).border_spacing, (2.0, 2.0));
        assert_eq!(style(table).border_collapse, BorderCollapse::Separate);
        assert_eq!(style(table).box_sizing, BoxSizing::BorderBox);
        // WHATWG §15: table border-color is gray.
        assert_eq!(
            style(table).border_color,
            RectEdges::all(Color::new(128, 128, 128, 255))
        );

        assert_eq!(style(caption).display, Display::TableCaption);
        assert_eq!(style(caption).text_align, TextAlign::Center);
        assert_eq!(style(colgroup).display, Display::TableColumnGroup);
        assert_eq!(style(col).display, Display::TableColumn);
        assert_eq!(style(thead).display, Display::TableHeaderGroup);
        assert_eq!(style(tbody).display, Display::TableRowGroup);
        assert_eq!(style(tfoot).display, Display::TableFooterGroup);
        assert_eq!(style(head_row).display, Display::TableRow);
        assert_eq!(style(body_row).display, Display::TableRow);
        assert_eq!(style(th).display, Display::TableCell);
        assert_eq!(style(td).display, Display::TableCell);
        assert_eq!(style(th).padding.top, Length::Px(1.0));
        assert_eq!(style(th).font_weight, 700);
        assert_eq!(style(th).text_align, TextAlign::Center);
        assert_eq!(style(td).padding.left, Length::Px(1.0));
    }
    // ---------------------------------------------------------------------------------------
    // Round 6: `@import` parsing and full media query lists.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn test_parse_stylesheet_imports() {
        let sheet = parse_stylesheet(
            "@import url(\"base.css\");\n@import \"print.css\" print;\nbody { color: red; }",
        );
        assert_eq!(sheet.imports.len(), 2);
        assert_eq!(sheet.imports[0].url, "base.css");
        assert!(sheet.imports[0].media.is_empty());
        assert_eq!(sheet.imports[1].url, "print.css");
        assert_eq!(sheet.rules.len(), 1);

        // The import media list is parsed and can be evaluated by consumers.
        let screen_context = MediaContext::default();
        let print_context = MediaContext {
            viewport: None,
            screen: false,
        };
        assert!(!media::evaluate(&sheet.imports[1].media, &screen_context));
        assert!(media::evaluate(&sheet.imports[1].media, &print_context));

        // Unquoted url() form plus a media condition.
        let sheet = parse_stylesheet("@import url(theme.css) screen and (min-width: 600px);");
        assert_eq!(sheet.imports.len(), 1);
        assert_eq!(sheet.imports[0].url, "theme.css");
        assert!(media::evaluate(
            &sheet.imports[0].media,
            &MediaContext::with_viewport(800.0, 600.0)
        ));
        assert!(!media::evaluate(
            &sheet.imports[0].media,
            &MediaContext::with_viewport(400.0, 600.0)
        ));

        // Imports after other rules are ignored, and unsupported forms are not exposed.
        let sheet = parse_stylesheet("body { color: red; } @import \"late.css\";");
        assert!(sheet.imports.is_empty());
        assert_eq!(sheet.rules.len(), 1);
        let sheet = parse_stylesheet("@import layer(base.css); p { color: red; }");
        assert!(sheet.imports.is_empty());
    }

    #[test]
    fn test_import_stylesheet_media_gating() {
        let imported = parse_stylesheet("p { color: red; }");
        let mut collection = StyleCollection::new();
        collection.media = MediaContext::with_viewport(800.0, 600.0);

        let screen_import = ImportRule {
            url: "a.css".to_string(),
            media: parse_media_query_list("screen and (min-width: 700px)"),
        };
        assert!(collection.add_import_stylesheet(&screen_import, &imported, Origin::Author));

        let print_import = ImportRule {
            url: "b.css".to_string(),
            media: parse_media_query_list("print"),
        };
        assert!(!collection.add_import_stylesheet(&print_import, &imported, Origin::Author));
        assert_eq!(collection.rules.len(), 1);

        // The added imported sheet participates in the cascade.
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let p = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, p);
        assert!(
            collection.rules[0].selectors.matches(&doc, p),
            "imported rule should be indexed and match"
        );
    }

    #[test]
    fn test_cascade_media_queries_full() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let p = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, p);

        // `and` + orientation with a known viewport.
        let css = "@media (min-width: 600px) and (orientation: landscape) { p { color: red; } }";
        let wide = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
        assert_eq!(wide.get(&p).expect("style").color, Color::RED);
        let portrait = resolve_document_styles_with_viewport(&doc, Some(css), 600.0, 800.0);
        assert_eq!(portrait.get(&p).expect("style").color, Color::BLACK);

        // `not` condition.
        let css = "@media not (min-width: 900px) { p { color: red; } }";
        let styles = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
        assert_eq!(styles.get(&p).expect("style").color, Color::RED);

        // Query list with `em` and `print`: either query may apply.
        let css = "@media (max-width: 40em), print { p { color: red; } }";
        let narrow = resolve_document_styles_with_viewport(&doc, Some(css), 400.0, 800.0);
        assert_eq!(narrow.get(&p).expect("style").color, Color::RED);
        let print_context = MediaContext {
            viewport: None,
            screen: false,
        };
        let print = resolve_document_styles_with_media(&doc, Some(css), print_context);
        assert_eq!(print.get(&p).expect("style").color, Color::RED);
        let screen_wide = resolve_document_styles_with_viewport(&doc, Some(css), 1000.0, 600.0);
        assert_eq!(screen_wide.get(&p).expect("style").color, Color::BLACK);
    }
    // ---------------------------------------------------------------------------------------
    // Round 7: query API is covered in `query::tests`; image parsing below.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn test_background_image_and_list_style_image_parsing() {
        // Plain URLs: quoted, single-quoted and unquoted; outer quotes are removed.
        for value in [
            "background-image: url(bg.png);",
            "background-image: url(\"bg.png\");",
            "background-image: url('bg.png');",
        ] {
            let style = style_with_inline("div", value);
            assert_eq!(
                style.background_image,
                Some("bg.png".to_string()),
                "{value}"
            );
        }
        // URLs may contain spaces/slashes when quoted.
        assert_eq!(
            style_with_inline("div", "background-image: url(\"img/hero 1.png\");").background_image,
            Some("img/hero 1.png".to_string())
        );

        // `none` and unsupported image types (gradients) compute to None.
        assert_eq!(
            style_with_inline("div", "background-image: none;").background_image,
            None
        );
        for value in [
            "background-image: linear-gradient(red, blue);",
            "background-image: radial-gradient(circle, #fff, #000);",
            "background-image: repeating-linear-gradient(red 0, blue 10px);",
            "background-image: image-set(url(a.png) 1x);",
        ] {
            assert_eq!(
                style_with_inline("div", value).background_image,
                None,
                "{value}"
            );
        }
        // Invalid values fall back to the initial None.
        assert_eq!(
            style_with_inline("div", "background-image: 42px;").background_image,
            None
        );

        // Multi-layer values use the first plain URL layer.
        assert_eq!(
            style_with_inline(
                "div",
                "background-image: url(first.png), linear-gradient(red, blue);"
            )
            .background_image,
            Some("first.png".to_string())
        );

        // The `background` shorthand populates color and image at the same time.
        let shorthand = style_with_inline("div", "background: #eee url(bg.png) no-repeat center;");
        assert_eq!(shorthand.background_color, Color::new(238, 238, 238, 255));
        assert_eq!(shorthand.background_image, Some("bg.png".to_string()));

        let color_only = style_with_inline("div", "background: red;");
        assert_eq!(color_only.background_color, Color::RED);
        assert_eq!(color_only.background_image, None);

        let image_only = style_with_inline("div", "background: url(bg.png);");
        assert_eq!(image_only.background_color, Color::TRANSPARENT);
        assert_eq!(image_only.background_image, Some("bg.png".to_string()));

        // Gradients in the shorthand are preserved as images but compute to None.
        assert_eq!(
            style_with_inline("div", "background: url(a.png) linear-gradient(red, blue);")
                .background_image,
            Some("a.png".to_string())
        );
        assert_eq!(
            style_with_inline("div", "background: linear-gradient(red, blue);").background_image,
            None
        );

        // `list-style-image` longhand and `list-style` shorthand.
        assert_eq!(
            style_with_inline("ul", "list-style-image: url(bullet.png);").list_style_image,
            Some("bullet.png".to_string())
        );
        let list_shorthand = style_with_inline("ul", "list-style: url(bullet.png) square;");
        assert_eq!(list_shorthand.list_style_type, ListStyleType::Square);
        assert_eq!(
            list_shorthand.list_style_image,
            Some("bullet.png".to_string())
        );
        let list_none = style_with_inline("ul", "list-style: none;");
        assert_eq!(list_none.list_style_type, ListStyleType::None);
        assert_eq!(list_none.list_style_image, None);

        // Images are not inherited.
        let (parent, child) = style_pair(
            "background-image: url(parent.png); list-style-image: url(parent-bullet.png);",
            "",
        );
        assert_eq!(parent.background_image, Some("parent.png".to_string()));
        assert_eq!(
            parent.list_style_image,
            Some("parent-bullet.png".to_string())
        );
        assert_eq!(child.background_image, None);
        assert_eq!(child.list_style_image, None);

        // Initial values.
        let initial = ComputedStyle::initial();
        assert_eq!(initial.background_image, None);
        assert_eq!(initial.list_style_image, None);
    }
}

#[cfg(test)]
mod background_geometry_tests {
    //! `background-repeat`/`-size`/`-position` + `background` shorthand (R89).

    use super::*;
    use dom::{Attribute, Document, Namespace};

    fn style_with_inline(inline: &str) -> ComputedStyle {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let element = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", inline)],
        );
        doc.append_child(doc.root(), body);
        doc.append_child(body, element);
        let styles = resolve_document_styles(&doc, None);
        styles.get(&element).cloned().expect("computed style")
    }

    #[test]
    fn test_background_geometry_initial_values() {
        let style = ComputedStyle::initial();
        assert_eq!(style.background_repeat, BackgroundRepeat::REPEAT);
        assert_eq!(style.background_size, BackgroundSize::Auto);
        assert_eq!(style.background_position, BackgroundPosition::ZERO);
        assert!(!style.background_geometry_explicit);
    }

    #[test]
    fn test_background_image_only_keeps_legacy_compatibility_flag() {
        let style = style_with_inline("background-image: url(bg.png);");
        assert_eq!(style.background_image, Some("bg.png".to_string()));
        assert!(
            !style.background_geometry_explicit,
            "image-only declarations keep the legacy paint behaviour"
        );
    }

    #[test]
    fn test_background_repeat_parsing() {
        let repeat_x = style_with_inline("background-repeat: repeat-x;");
        assert_eq!(repeat_x.background_repeat.x, BackgroundRepeatAxis::Repeat);
        assert_eq!(repeat_x.background_repeat.y, BackgroundRepeatAxis::NoRepeat);
        let repeat_y = style_with_inline("background-repeat: repeat-y;");
        assert_eq!(repeat_y.background_repeat.x, BackgroundRepeatAxis::NoRepeat);
        assert_eq!(repeat_y.background_repeat.y, BackgroundRepeatAxis::Repeat);
        let no_repeat = style_with_inline("background-repeat: no-repeat;");
        assert_eq!(no_repeat.background_repeat, BackgroundRepeat::NO_REPEAT);
        let pair = style_with_inline("background-repeat: space round;");
        assert_eq!(pair.background_repeat.x, BackgroundRepeatAxis::Space);
        assert_eq!(pair.background_repeat.y, BackgroundRepeatAxis::Round);
        assert!(pair.background_geometry_explicit);
        let invalid = style_with_inline("background-repeat: banana;");
        assert_eq!(invalid.background_repeat, BackgroundRepeat::REPEAT);
    }

    #[test]
    fn test_background_size_parsing() {
        assert_eq!(
            style_with_inline("background-size: cover;").background_size,
            BackgroundSize::Cover
        );
        assert_eq!(
            style_with_inline("background-size: contain;").background_size,
            BackgroundSize::Contain
        );
        assert_eq!(
            style_with_inline("background-size: 100px 50%;").background_size,
            BackgroundSize::Explicit(Length::Px(100.0), Length::Percent(50.0))
        );
        assert_eq!(
            style_with_inline("background-size: 50% auto;").background_size,
            BackgroundSize::Explicit(Length::Percent(50.0), Length::Auto)
        );
        let shorthand_width = style_with_inline("background-size: 40px;");
        assert_eq!(
            shorthand_width.background_size,
            BackgroundSize::Explicit(Length::Px(40.0), Length::Px(40.0))
        );
    }

    #[test]
    fn test_background_position_parsing() {
        let center = style_with_inline("background-position: center;");
        assert_eq!(center.background_position.x, Length::Percent(50.0));
        assert_eq!(center.background_position.y, Length::Percent(50.0));
        let right_bottom = style_with_inline("background-position: right bottom;");
        assert_eq!(right_bottom.background_position.x, Length::Percent(100.0));
        assert_eq!(right_bottom.background_position.y, Length::Percent(100.0));
        let top_left = style_with_inline("background-position: top left;");
        assert_eq!(top_left.background_position.x, Length::Percent(0.0));
        assert_eq!(top_left.background_position.y, Length::Percent(0.0));
        let mixed = style_with_inline("background-position: 10px 75%;");
        assert_eq!(mixed.background_position.x, Length::Px(10.0));
        assert_eq!(mixed.background_position.y, Length::Percent(75.0));
        let one_value = style_with_inline("background-position: 25px;");
        assert_eq!(one_value.background_position.x, Length::Px(25.0));
        assert_eq!(one_value.background_position.y, Length::Percent(50.0));
    }

    #[test]
    fn test_background_shorthand_with_geometry() {
        let style = style_with_inline("background: red url(bg.png) no-repeat center / cover;");
        assert_eq!(style.background_color, Color::RED);
        assert_eq!(style.background_image, Some("bg.png".to_string()));
        assert_eq!(style.background_repeat, BackgroundRepeat::NO_REPEAT);
        assert_eq!(style.background_size, BackgroundSize::Cover);
        assert_eq!(style.background_position.x, Length::Percent(50.0));
        assert_eq!(style.background_position.y, Length::Percent(50.0));
        assert!(style.background_geometry_explicit);
    }

    #[test]
    fn test_background_shorthand_resets_omitted_components() {
        let style = style_with_inline("background: url(bg.png);");
        assert_eq!(style.background_image, Some("bg.png".to_string()));
        assert_eq!(style.background_repeat, BackgroundRepeat::REPEAT);
        assert_eq!(style.background_size, BackgroundSize::Auto);
        assert_eq!(style.background_position, BackgroundPosition::ZERO);
        assert!(style.background_geometry_explicit);
    }

    #[test]
    fn test_background_longhand_overrides_shorthand() {
        let style = style_with_inline(
            "background: url(bg.png) no-repeat center / cover; \
             background-repeat: repeat-y; background-position: 5px 6px;",
        );
        assert_eq!(style.background_repeat.x, BackgroundRepeatAxis::NoRepeat);
        assert_eq!(style.background_repeat.y, BackgroundRepeatAxis::Repeat);
        assert_eq!(style.background_position.x, Length::Px(5.0));
        assert_eq!(style.background_position.y, Length::Px(6.0));
        assert_eq!(style.background_size, BackgroundSize::Cover);
    }
}

#[cfg(test)]
mod background_layers_tests {
    //! `background-origin`/`clip`/`attachment`, multi-layer lists and layered
    //! `background` shorthand (R94).

    use super::*;
    use dom::{Attribute, Document, Namespace};

    fn style_with_inline(inline: &str) -> ComputedStyle {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let element = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", inline)],
        );
        doc.append_child(doc.root(), body);
        doc.append_child(body, element);
        let styles = resolve_document_styles(&doc, None);
        styles.get(&element).cloned().expect("computed style")
    }

    #[test]
    fn test_origin_clip_attachment_defaults() {
        let style = ComputedStyle::initial();
        assert_eq!(style.background_layers.len(), 0);
        let layer = style.background_layers.first().cloned().unwrap_or_default();
        assert_eq!(layer.origin, BackgroundOrigin::PaddingBox);
        assert_eq!(layer.clip, BackgroundClip::BorderBox);
        assert_eq!(layer.attachment, BackgroundAttachment::Scroll);
    }

    #[test]
    fn test_origin_clip_attachment_longhands() {
        let style = style_with_inline(
            "background-image: url(a.png); background-origin: content-box; \
             background-clip: content-box; background-attachment: fixed;",
        );
        assert_eq!(style.background_layers.len(), 1);
        assert_eq!(
            style.background_layers[0].origin,
            BackgroundOrigin::ContentBox
        );
        assert_eq!(style.background_layers[0].clip, BackgroundClip::ContentBox);
        assert_eq!(
            style.background_layers[0].attachment,
            BackgroundAttachment::Fixed
        );
        assert!(style.background_geometry_explicit);
    }

    #[test]
    fn test_background_clip_text_is_parsed() {
        let style = style_with_inline("background-clip: text;");
        let layer = style.background_layers.first().cloned().unwrap_or_default();
        assert_eq!(layer.clip, BackgroundClip::Text);
    }

    #[test]
    fn test_multiple_layers_parse_lists_cyclically() {
        let style = style_with_inline(
            "background-image: url(top.png), url(bottom.png); \
             background-repeat: no-repeat; \
             background-size: cover, contain; \
             background-position: center; \
             background-origin: content-box;",
        );
        assert_eq!(style.background_layers.len(), 2);
        assert_eq!(
            style.background_layers[0].image,
            Some("top.png".to_string())
        );
        assert_eq!(
            style.background_layers[1].image,
            Some("bottom.png".to_string())
        );
        // Single-valued lists repeat to every layer.
        assert_eq!(
            style.background_layers[0].repeat,
            BackgroundRepeat::NO_REPEAT
        );
        assert_eq!(
            style.background_layers[1].repeat,
            BackgroundRepeat::NO_REPEAT
        );
        assert_eq!(
            style.background_layers[0].origin,
            BackgroundOrigin::ContentBox
        );
        assert_eq!(
            style.background_layers[1].origin,
            BackgroundOrigin::ContentBox
        );
        // Multi-valued lists are consumed per layer.
        assert_eq!(style.background_layers[0].size, BackgroundSize::Cover);
        assert_eq!(style.background_layers[1].size, BackgroundSize::Contain);
        assert_eq!(style.background_layers[0].position.x, Length::Percent(50.0));
        assert!(style.background_geometry_explicit);
        // The legacy scalar view mirrors the first (top) layer.
        assert_eq!(style.background_image, Some("top.png".to_string()));
        assert_eq!(style.background_size, BackgroundSize::Cover);
    }

    #[test]
    fn test_layered_background_shorthand() {
        let style = style_with_inline(
            "background: url(top.png) no-repeat center / cover content-box, \
             url(bottom.png) repeat-x padding-box;",
        );
        assert_eq!(style.background_layers.len(), 2);
        let top = &style.background_layers[0];
        assert_eq!(top.image, Some("top.png".to_string()));
        assert_eq!(top.repeat, BackgroundRepeat::NO_REPEAT);
        assert_eq!(top.size, BackgroundSize::Cover);
        assert_eq!(top.origin, BackgroundOrigin::ContentBox);
        assert_eq!(top.clip, expected_clip_for_origin(top.origin));
        let bottom = &style.background_layers[1];
        assert_eq!(bottom.image, Some("bottom.png".to_string()));
        assert_eq!(bottom.repeat.x, BackgroundRepeatAxis::Repeat);
        assert_eq!(bottom.repeat.y, BackgroundRepeatAxis::NoRepeat);
        assert_eq!(bottom.origin, BackgroundOrigin::PaddingBox);
        assert!(style.background_geometry_explicit);
    }

    /// The shorthand maps a single box keyword to both origin and clip; both are
    /// represented by the same keyword string, so the clip enum mirrors it.
    fn expected_clip_for_origin(origin: BackgroundOrigin) -> BackgroundClip {
        match origin {
            BackgroundOrigin::ContentBox => BackgroundClip::ContentBox,
            BackgroundOrigin::PaddingBox => BackgroundClip::PaddingBox,
            BackgroundOrigin::BorderBox => BackgroundClip::BorderBox,
        }
    }

    #[test]
    fn test_background_attachment_in_shorthand() {
        let style = style_with_inline("background: url(a.png) fixed;");
        assert_eq!(
            style.background_layers[0].attachment,
            BackgroundAttachment::Fixed
        );
    }

    #[test]
    fn test_single_background_image_keeps_legacy_flag() {
        let style = style_with_inline("background-image: url(only.png);");
        assert_eq!(style.background_layers.len(), 1);
        assert!(!style.background_geometry_explicit);
    }
}

#[cfg(test)]
mod decoration_tests {
    //! `border-radius`, `box-shadow` and `text-shadow` (R99).

    use super::*;
    use crate::properties::MAX_BOX_SHADOWS;
    use dom::{Attribute, Document, Namespace};

    fn style_with_inline(inline: &str) -> ComputedStyle {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let element = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", inline)],
        );
        doc.append_child(doc.root(), body);
        doc.append_child(body, element);
        let styles = resolve_document_styles(&doc, None);
        styles.get(&element).cloned().expect("computed style")
    }

    fn style_pair(parent: &str, child: &str) -> (ComputedStyle, ComputedStyle) {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let parent_node = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", parent)],
        );
        let child_node =
            doc.create_element(Namespace::Html, "p", vec![Attribute::new("style", child)]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, parent_node);
        doc.append_child(parent_node, child_node);
        let styles = resolve_document_styles(&doc, None);
        (
            styles.get(&parent_node).cloned().expect("parent"),
            styles.get(&child_node).cloned().expect("child"),
        )
    }

    #[test]
    fn test_decoration_defaults() {
        let style = ComputedStyle::initial();
        assert!(style.border_radius.is_zero());
        assert!(style.box_shadows.is_empty());
        assert!(style.text_shadows.is_empty());
    }

    #[test]
    fn test_border_radius_shorthand_expansion() {
        let all = style_with_inline("border-radius: 4px;");
        assert_eq!(
            all.border_radius.top_left,
            Radius::circular(Length::Px(4.0))
        );
        assert_eq!(
            all.border_radius.bottom_right,
            Radius::circular(Length::Px(4.0))
        );

        let two = style_with_inline("border-radius: 4px 8px;");
        assert_eq!(two.border_radius.top_left.x, Length::Px(4.0));
        assert_eq!(two.border_radius.top_right.x, Length::Px(8.0));
        assert_eq!(two.border_radius.bottom_left.x, Length::Px(8.0));

        let three = style_with_inline("border-radius: 1px 2px 3px;");
        assert_eq!(three.border_radius.top_left.x, Length::Px(1.0));
        assert_eq!(three.border_radius.top_right.x, Length::Px(2.0));
        assert_eq!(three.border_radius.bottom_right.x, Length::Px(3.0));
        assert_eq!(three.border_radius.bottom_left.x, Length::Px(2.0));

        let four = style_with_inline("border-radius: 1px 2px 3px 4px;");
        assert_eq!(four.border_radius.bottom_left.x, Length::Px(4.0));
    }

    #[test]
    fn test_border_radius_elliptical_slash() {
        let style = style_with_inline("border-radius: 10px / 20px;");
        assert_eq!(style.border_radius.top_left.x, Length::Px(10.0));
        assert_eq!(style.border_radius.top_left.y, Length::Px(20.0));
        let mixed = style_with_inline("border-radius: 10px 5px / 20% 30%;");
        assert_eq!(mixed.border_radius.top_left.x, Length::Px(10.0));
        assert_eq!(mixed.border_radius.top_left.y, Length::Percent(20.0));
        assert_eq!(mixed.border_radius.top_right.y, Length::Percent(30.0));
    }

    #[test]
    fn test_border_radius_longhands_override_shorthand() {
        let style = style_with_inline("border-radius: 4px; border-top-left-radius: 12px 6px;");
        assert_eq!(style.border_radius.top_left.x, Length::Px(12.0));
        assert_eq!(style.border_radius.top_left.y, Length::Px(6.0));
        assert_eq!(style.border_radius.top_right.x, Length::Px(4.0));
    }

    #[test]
    fn test_invalid_border_radius_is_tolerated() {
        let style = style_with_inline("border-radius: -4px auto;");
        assert!(style.border_radius.is_zero());
    }

    #[test]
    fn test_box_shadow_parsing() {
        let none = style_with_inline("box-shadow: none;");
        assert!(none.box_shadows.is_empty());

        let style = style_with_inline("box-shadow: 2px 3px 4px 5px rgba(0, 0, 0, 0.5) inset;");
        assert_eq!(style.box_shadows.len(), 1);
        let shadow = style.box_shadows[0];
        assert_eq!(shadow.offset_x, 2.0);
        assert_eq!(shadow.offset_y, 3.0);
        assert_eq!(shadow.blur, 4.0);
        assert_eq!(shadow.spread, 5.0);
        assert_eq!(shadow.color.r, 0);
        assert_eq!(shadow.color.a, 128);
        assert!(shadow.inset);

        let defaults = style_with_inline("box-shadow: 1px 2px red;");
        assert_eq!(defaults.box_shadows[0].blur, 0.0);
        assert_eq!(defaults.box_shadows[0].spread, 0.0);
        assert_eq!(defaults.box_shadows[0].color, Color::RED);

        let current = style_with_inline("color: #336699; box-shadow: 1px 1px;");
        assert_eq!(
            current.box_shadows[0].color,
            Color::new(0x33, 0x66, 0x99, 255)
        );
    }

    #[test]
    fn test_multiple_box_shadows_are_capped() {
        let mut values = Vec::new();
        for index in 0..40 {
            values.push(format!("{index}px {}px red", index + 1));
        }
        let style = style_with_inline(&format!("box-shadow: {};", values.join(", ")));
        assert_eq!(style.box_shadows.len(), MAX_BOX_SHADOWS);
        assert!(style.box_shadows.len() <= 32);
    }

    #[test]
    fn test_text_shadow_and_inheritance() {
        let style = style_with_inline("text-shadow: 1px 2px 3px blue, -1px 0 red;");
        assert_eq!(style.text_shadows.len(), 2);
        assert_eq!(style.text_shadows[0].blur, 3.0);
        assert_eq!(style.text_shadows[0].color, Color::new(0, 0, 255, 255));
        assert_eq!(style.text_shadows[1].color, Color::RED);

        let (parent, child) = style_pair("text-shadow: 1px 1px 2px red;", "color: blue;");
        assert_eq!(
            child.text_shadows, parent.text_shadows,
            "text-shadow inherits"
        );
        let (shadow_parent, shadow_child) = style_pair("box-shadow: 1px 1px red;", "");
        assert!(
            shadow_child.box_shadows.is_empty(),
            "box-shadow does not inherit"
        );
        assert_eq!(shadow_parent.box_shadows.len(), 1);
    }
}
