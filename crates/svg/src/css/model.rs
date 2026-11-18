use crate::SvgError;
use crate::xml::{XmlElement, local_name};

/// Maximum number of rules kept from one stylesheet.
pub(crate) const MAX_RULES: usize = 4096;
/// Maximum selector list length per rule.
pub(crate) const MAX_SELECTORS_PER_RULE: usize = 64;
/// Maximum descendant compounds per selector.
pub(crate) const MAX_SELECTOR_PARTS: usize = 16;
/// Maximum stylesheet size (bytes) accepted from `<style>` elements.
pub(crate) const MAX_STYLESHEET_BYTES: usize = 1024 * 1024;
/// Maximum number of declarations in one rule.
pub(crate) const MAX_DECLARATIONS_PER_RULE: usize = 256;

/// Document-wide budget for selector evaluations in the in-SVG stylesheet cascade.
///
/// The individual caps bound rules and selectors per rule, but their product times the
/// number of elements is not bounded, so a small document could force billions of
/// `matches` calls.
pub(crate) struct SelectorBudget {
    /// Evaluations used so far in the document.
    pub used: usize,
    /// Maximum allowed ([`crate::Limits::max_selector_checks`]).
    pub max: usize,
}

impl SelectorBudget {
    /// Charges one selector evaluation, failing once the budget is exhausted.
    pub fn charge(&mut self) -> Result<(), SvgError> {
        self.used = self.used.saturating_add(1);
        if self.used > self.max {
            return Err(SvgError::Limit(format!(
                "stylesheet selector budget exceeded ({} checks)",
                self.max
            )));
        }
        Ok(())
    }
}

/// One `name: value` declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Declaration {
    pub name: String,
    pub value: String,
}

/// A compound selector: `tag.class#id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SimpleSelector {
    /// Element local name (`None` = universal `*` or class/id-only compound).
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

impl SimpleSelector {
    /// True when the compound matches `element` (no ancestor constraints).
    pub fn matches(&self, element: &XmlElement) -> bool {
        if let Some(tag) = &self.tag
            && !local_name(&element.name).eq_ignore_ascii_case(tag)
        {
            return false;
        }
        if let Some(id) = &self.id
            && element.attribute("id") != Some(id)
        {
            return false;
        }
        if !self.classes.is_empty() {
            let class_attribute = element.attribute("class").unwrap_or("");
            for class in &self.classes {
                // Iterator instead of a `Vec` per selector evaluation: the cascade runs this
                // once per element per selector, so an allocation here is a remote DoS.
                if !class_attribute
                    .split_ascii_whitespace()
                    .any(|candidate| candidate == class)
                {
                    return false;
                }
            }
        }
        true
    }

    /// Specificity contribution: `(ids, classes, tags)`.
    fn specificity(&self) -> (u32, u32, u32) {
        (
            u32::from(self.id.is_some()),
            self.classes.len() as u32,
            u32::from(self.tag.is_some()),
        )
    }
}

/// A selector: compounds separated by descendant combinators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Selector {
    /// Compounds from left (outer) to right (subject).
    pub parts: Vec<SimpleSelector>,
}

impl Selector {
    /// Packed specificity key: ids ≫ classes ≫ tags.
    pub fn specificity(&self) -> u32 {
        let (mut ids, mut classes, mut tags) = (0, 0, 0);
        for part in &self.parts {
            let (i, c, t) = part.specificity();
            ids += i;
            classes += c;
            tags += t;
        }
        (ids.min(0x3ff) << 20) | (classes.min(0x3ff) << 10) | tags.min(0x3ff)
    }

    /// True when the selector matches `element`, walking its ancestor chain.
    pub fn matches(&self, element: &XmlElement, ancestors: &[&XmlElement]) -> bool {
        let Some(subject) = self.parts.last() else {
            return false;
        };
        if !subject.matches(element) {
            return false;
        }
        let mut ancestor_index = ancestors.len();
        for part in self.parts.iter().rev().skip(1) {
            loop {
                if ancestor_index == 0 {
                    return false;
                }
                ancestor_index -= 1;
                if part.matches(ancestors[ancestor_index]) {
                    break;
                }
            }
        }
        true
    }
}

/// One parsed style rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
    /// Source order (also used for cascade tie-breaking).
    pub order: usize,
}

/// A declaration that matched an element during the cascade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MatchedDeclaration<'a> {
    pub specificity: u32,
    pub order: usize,
    pub name: &'a str,
    pub value: &'a str,
}
