//! # W3C CSS Cascading and Inheritance Level 4 (§6)
//!
//! Implements declaration origin and importance ranking, specificity calculation,
//! source-order tie breaking, and cascaded value resolution for DOM elements.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::rule::{Declaration, Rule, Stylesheet, serialize_component_values};
use crate::selector::{SelectorList, Specificity, parse_selectors};
use dom::{Document, NodeId};

/// Origin of a CSS declaration per W3C Cascading and Inheritance Level 4 §6.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Browser default User-Agent stylesheet.
    UserAgent,
    /// User personal stylesheet or accessibility overrides.
    User,
    /// Author stylesheet (`<style>`, `<link>`, or `style="..."` attribute).
    Author,
}

/// Cascade precedence rank per W3C Cascading and Inheritance Level 4 §6.2:
///
/// 1. User-Agent normal
/// 2. User normal
/// 3. Author normal
/// 4. Author `!important`
/// 5. User `!important`
/// 6. User-Agent `!important`
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CascadeOriginImportance {
    UserAgentNormal = 0,
    UserNormal = 1,
    AuthorNormal = 2,
    AuthorImportant = 3,
    UserImportant = 4,
    UserAgentImportant = 5,
}

impl CascadeOriginImportance {
    #[inline]
    pub fn new(origin: Origin, important: bool) -> Self {
        match (origin, important) {
            (Origin::UserAgent, false) => Self::UserAgentNormal,
            (Origin::User, false) => Self::UserNormal,
            (Origin::Author, false) => Self::AuthorNormal,
            (Origin::Author, true) => Self::AuthorImportant,
            (Origin::User, true) => Self::UserImportant,
            (Origin::UserAgent, true) => Self::UserAgentImportant,
        }
    }
}

/// A matched declaration along with its cascade priority metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchedDeclaration<'a> {
    pub declaration: &'a Declaration,
    pub origin_importance: CascadeOriginImportance,
    pub specificity: Specificity,
    pub source_order: u32,
}

impl<'a> Eq for MatchedDeclaration<'a> {}

impl<'a> PartialOrd for MatchedDeclaration<'a> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for MatchedDeclaration<'a> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.origin_importance
            .cmp(&other.origin_importance)
            .then_with(|| self.specificity.cmp(&other.specificity))
            .then_with(|| self.source_order.cmp(&other.source_order))
    }
}

/// A compiled CSS rule with pre-parsed selectors and declarations for fast matching.
#[derive(Clone, Debug)]
pub struct CompiledRule {
    pub selectors: SelectorList,
    pub declarations: Vec<Declaration>,
    pub origin: Origin,
    pub source_order: u32,
}

/// A collection of compiled stylesheets ready for cascade resolution.
#[derive(Clone, Debug, Default)]
pub struct StyleCollection {
    pub rules: Vec<CompiledRule>,
    pub next_source_order: u32,
}

impl StyleCollection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a stylesheet to the collection with the given origin.
    pub fn add_stylesheet(&mut self, stylesheet: &Stylesheet, origin: Origin) {
        for rule in &stylesheet.rules {
            if let Rule::Style(style_rule) = rule {
                let selector_str = serialize_component_values(&style_rule.prelude);
                let selectors = parse_selectors(&selector_str);
                if !selectors.selectors.is_empty() {
                    self.rules.push(CompiledRule {
                        selectors,
                        declarations: style_rule.declarations.clone(),
                        origin,
                        source_order: self.next_source_order,
                    });
                    self.next_source_order += 1;
                }
            }
        }
    }

    /// Parses and adds CSS text to the collection with the given origin.
    pub fn add_css(&mut self, css_text: &str, origin: Origin) {
        let stylesheet = crate::parser::parse_stylesheet(css_text);
        self.add_stylesheet(&stylesheet, origin);
    }
}

/// Resolves the cascaded property declarations for a DOM element node.
///
/// Evaluates:
/// 1. Matched rules in [`StyleCollection`] (User-Agent, User, Author).
/// 2. Inline declarations from the element's `style="..."` attribute (if present).
/// 3. Normalizes priority via `(Origin & Importance, Specificity, Source Order)`.
///
/// Returns a map of property name (lowercase) to the winning [`Declaration`].
pub fn resolve_cascaded_properties<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
) -> HashMap<String, &'a Declaration> {
    let mut best_candidates: HashMap<String, MatchedDeclaration<'a>> = HashMap::new();

    // 1. Match stylesheet rules
    for rule in &collection.rules {
        if let Some(spec) = rule.selectors.matching_specificity(doc, node_id) {
            for decl in &rule.declarations {
                let origin_importance = CascadeOriginImportance::new(rule.origin, decl.important);
                let candidate = MatchedDeclaration {
                    declaration: decl,
                    origin_importance,
                    specificity: spec,
                    source_order: rule.source_order,
                };

                match best_candidates.get_mut(&decl.name) {
                    Some(existing) => {
                        if candidate > *existing {
                            *existing = candidate;
                        }
                    }
                    None => {
                        best_candidates.insert(decl.name.clone(), candidate);
                    }
                }
            }
        }
    }

    // 2. Process inline styles from style attribute (Author origin with high specificity)
    if let Some(inline_decls) = inline_declarations {
        let inline_specificity = Specificity::new(100_000, 0, 0);
        let inline_source_order = collection.next_source_order + 10_000;
        for decl in inline_decls {
            let origin_importance = CascadeOriginImportance::new(Origin::Author, decl.important);
            let candidate = MatchedDeclaration {
                declaration: decl,
                origin_importance,
                specificity: inline_specificity,
                source_order: inline_source_order,
            };

            match best_candidates.get_mut(&decl.name) {
                Some(existing) => {
                    if candidate > *existing {
                        *existing = candidate;
                    }
                }
                None => {
                    best_candidates.insert(decl.name.clone(), candidate);
                }
            }
        }
    }

    best_candidates
        .into_iter()
        .map(|(k, v)| (k, v.declaration))
        .collect()
}

/// Resolves computed styles for all elements in the DOM document.
///
/// Pre-loads the default WHATWG HTML User-Agent stylesheet, then compiles and cascades
/// any author CSS supplied (e.g. from `<style>` tags or external stylesheets).
pub fn resolve_document_styles(
    doc: &Document,
    author_css: Option<&str>,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    let mut collection = crate::user_agent::create_user_agent_style_collection();
    if let Some(css) = author_css {
        collection.add_css(css, Origin::Author);
    }

    let mut styles: HashMap<NodeId, crate::properties::ComputedStyle> = HashMap::new();
    let root = doc.root();

    // Collect elements in top-down pre-order traversal
    let mut stack = Vec::new();
    let mut curr = doc.node(root).first_child;
    while let Some(child_id) = curr {
        stack.push(child_id);
        curr = doc.node(child_id).next_sibling;
    }

    while let Some(node_id) = stack.pop() {
        if let Some(el) = doc.node(node_id).as_element() {
            let inline_decls = el
                .get_attribute("style")
                .map(crate::parser::parse_list_of_declarations);

            let cascaded =
                resolve_cascaded_properties(doc, node_id, &collection, inline_decls.as_deref());

            let parent_style = doc.node(node_id).parent.and_then(|p| styles.get(&p));
            let root_style = styles.get(&root);

            let computed =
                crate::properties::ComputedStyle::compute(&cascaded, parent_style, root_style);
            styles.insert(node_id, computed);
        }

        // Push children in reverse order so first child is popped first
        let mut children = Vec::new();
        let mut child = doc.node(node_id).first_child;
        while let Some(c) = child {
            children.push(c);
            child = doc.node(c).next_sibling;
        }
        for c in children.into_iter().rev() {
            stack.push(c);
        }
    }

    styles
}
