//! # Megi CSS (`css`)
//!
//! Foundational types, W3C CSS Syntax Module Level 3 tokenizer & parser, and Selectors Level 4
//! matching engine operating over [`dom::Document`] and [`dom::NodeId`].

pub mod component_value;
pub mod parser;
pub mod rule;
pub mod selector;
pub mod token;
pub mod tokenizer;

pub use component_value::{ComponentValue, FunctionCall, SimpleBlock};
pub use parser::{TokenStream, consume_declaration, parse_list_of_declarations, parse_stylesheet};
pub use rule::{AtRule, Declaration, Rule, StyleRule, Stylesheet};
pub use selector::{
    AnPlusB, AttrOperator, AttributeSelector, Combinator, ComplexSelector, CompoundSelector,
    PseudoClass, SelectorList, parse_selectors,
};
pub use token::{CssToken, HashType, NumberType};
pub use tokenizer::{CssTokenizer, preprocess_css_input, tokenize};

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
}
