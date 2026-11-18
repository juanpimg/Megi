//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// A basic compound selector (tag name, `#id`, `.class`) for matching DOM elements.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

impl SimpleSelector {
    /// Evaluates whether the element at `node_id` in `doc` matches this selector.
    pub fn matches(&self, doc: &megi_core::Document, node_id: megi_core::NodeId) -> bool {
        let compound = CompoundSelector {
            tag_name: self.tag_name.clone(),
            id: self.id.clone(),
            classes: self.classes.clone(),
            attributes: Vec::new(),
            pseudo_classes: Vec::new(),
            pseudo_element: None,
            never_matches: false,
            id_count: u32::from(self.id.is_some()),
        };
        compound.matches(doc, node_id)
    }
}

#[cfg(test)]
pub(crate) mod lib_tests_helpers {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use megi_core::{Attribute, Document, Namespace};
    /// Computes the style of `body > <tag style="...">` and returns it.
    pub(crate) fn style_with_inline(tag: &str, inline: &str) -> ComputedStyle {
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
    pub(crate) fn style_pair(
        parent_inline: &str,
        child_inline: &str,
    ) -> (ComputedStyle, ComputedStyle) {
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
}
