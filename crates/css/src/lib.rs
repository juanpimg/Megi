//! # Megi CSS (`css`)
//!
//! Foundational types and selector-matching primitives for W3C CSS Syntax Module Level 3
//! and Selectors Level 4, operating over [`dom::Document`] and [`dom::NodeId`].

use dom::{Document, Namespace, NodeId};

/// W3C CSS Syntax Module Level 3 §4 Token types.
#[derive(Clone, Debug, PartialEq)]
pub enum CssToken {
    Ident(String),
    Function(String),
    AtKeyword(String),
    Hash { value: String, is_id: bool },
    String(String),
    Number(f64),
    Percentage(f64),
    Dimension { value: f64, unit: String },
    Whitespace,
    Colon,
    Semicolon,
    Comma,
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Delim(char),
}

/// A basic compound selector (tag name, `#id`, `.class`) for matching DOM elements.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

impl SimpleSelector {
    /// Evaluates whether the element at `node_id` in `doc` matches this selector.
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        let Some(el) = doc.node(node_id).as_element() else {
            return false;
        };
        if el.namespace != Namespace::Html {
            return false;
        }
        if let Some(ref tag) = self.tag_name
            && !el.local_name.eq_ignore_ascii_case(tag)
        {
            return false;
        }
        if let Some(ref expected_id) = self.id
            && el.get_attribute("id") != Some(expected_id.as_str())
        {
            return false;
        }
        if !self.classes.is_empty() {
            let Some(class_attr) = el.get_attribute("class") else {
                return false;
            };
            let element_classes: Vec<&str> = class_attr.split_ascii_whitespace().collect();
            for c in &self.classes {
                if !element_classes.contains(&c.as_str()) {
                    return false;
                }
            }
        }
        true
    }
}

/// A CSS property declaration (`property: value`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub value: String,
    pub important: bool,
}

/// A CSS style rule pairing selectors with declarations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleRule {
    pub selectors: Vec<SimpleSelector>,
    pub declarations: Vec<Declaration>,
}

/// A parsed CSS stylesheet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stylesheet {
    pub rules: Vec<StyleRule>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use dom::Attribute;

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
}
