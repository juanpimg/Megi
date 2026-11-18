//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

impl CompoundSelector {
    /// Calculates the specificity of this compound selector according to Selectors Level 4 §16.
    pub fn specificity(&self) -> Specificity {
        let a = self.id_count.max(u32::from(self.id.is_some()));
        let b = (self.classes.len() + self.attributes.len()) as u32;
        let c = u32::from(self.tag_name.as_deref().is_some_and(|t| t != "*"))
            + u32::from(self.pseudo_element.is_some());
        let mut specificity = Specificity::new(a, b, c);
        for pseudo in &self.pseudo_classes {
            specificity += pseudo.specificity();
        }
        specificity
    }

    /// Matches this compound selector against a DOM element.
    ///
    /// Selectors with a pseudo-element (`::before`/`::after`) never match an ordinary
    /// DOM element directly; use [`Self::matches_for_pseudo`] for pseudo-element resolution.
    ///
    /// Type selectors match HTML elements case-insensitively and foreign (SVG/MathML)
    /// elements case-sensitively. Unlike the previous implementation, type/class/id/attribute
    /// and universal selectors are no longer restricted to the HTML namespace, so styles now
    /// apply to SVG/MathML elements (F14).
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        if self.pseudo_element.is_some() {
            return false;
        }
        self.matches_inner(doc, node_id, false)
    }

    pub(crate) fn matches_for_backdrop(&self, doc: &Document, node_id: NodeId) -> bool {
        self.matches_inner(doc, node_id, true)
    }

    /// Matches this compound selector as a pseudo-element rule for `pseudo` on `node_id`.
    pub fn matches_for_pseudo(
        &self,
        doc: &Document,
        node_id: NodeId,
        pseudo: PseudoElement,
    ) -> bool {
        self.pseudo_element == Some(pseudo) && self.matches_inner(doc, node_id, false)
    }

    pub(crate) fn matches_inner(
        &self,
        doc: &Document,
        node_id: NodeId,
        allow_backdrop: bool,
    ) -> bool {
        if self.never_matches {
            return false;
        }
        let Some(el) = doc.node(node_id).as_element() else {
            return false;
        };

        if let Some(tag) = self.tag_name.as_deref()
            && tag != "*"
        {
            let name_matches = if el.namespace == Namespace::Html {
                el.local_name.eq_ignore_ascii_case(tag)
            } else {
                el.local_name == tag
            };
            if !name_matches {
                return false;
            }
        }

        if let Some(expected_id) = self.id.as_deref()
            && el.get_attribute("id") != Some(expected_id)
        {
            return false;
        }

        if !self.classes.is_empty() {
            let Some(class_attr) = el.get_attribute("class") else {
                return false;
            };
            // Class names are case-sensitive, HTML included (Selectors 4 §6.6): only
            // type selectors get the ASCII-insensitive treatment for HTML elements (#682).
            for c in &self.classes {
                if !class_attr.split_ascii_whitespace().any(|ec| ec == c) {
                    return false;
                }
            }
        }

        for attr_sel in &self.attributes {
            if !attr_sel.matches(doc, node_id) {
                return false;
            }
        }

        for pseudo in &self.pseudo_classes {
            if allow_backdrop && matches!(pseudo, PseudoClass::Backdrop) {
                continue;
            }
            if !pseudo.matches(doc, node_id) {
                return false;
            }
        }

        true
    }
}

/// Combinator joining two compound selectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combinator {
    /// Descendant combinator (space)
    Descendant,
    /// Child combinator (`>`)
    Child,
    /// Next-sibling combinator (`+`)
    NextSibling,
    /// Subsequent-sibling combinator (`~`)
    SubsequentSibling,
}

/// A complex selector chain (e.g. `body > div.main p + span`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComplexSelector {
    /// Compound selectors from left to right with their following combinators.
    /// The last compound selector has `None` as combinator and matches the target element.
    pub compounds: Vec<(CompoundSelector, Option<Combinator>)>,
}

impl ComplexSelector {
    /// Returns `true` when this selector targets the `::backdrop` pseudo-element on its subject.
    pub fn has_backdrop(&self) -> bool {
        self.compounds.last().is_some_and(|(compound, _)| {
            compound
                .pseudo_classes
                .iter()
                .any(|pseudo| matches!(pseudo, PseudoClass::Backdrop))
        })
    }

    /// Matches the `::backdrop` pseudo-element whose originating element is `node_id`.
    pub fn matches_backdrop(&self, doc: &Document, node_id: NodeId) -> bool {
        with_match_budget(|| self.matches_backdrop_inner(doc, node_id))
    }

    pub(crate) fn matches_backdrop_inner(&self, doc: &Document, node_id: NodeId) -> bool {
        if self.compounds.is_empty() || !self.has_backdrop() {
            return false;
        }
        let last_idx = self.compounds.len() - 1;
        if !self.compounds[last_idx]
            .0
            .matches_for_backdrop(doc, node_id)
        {
            return false;
        }
        self.matches_from_right(doc, node_id, last_idx)
    }

    /// Returns the pseudo-element targeted by this selector, if any.
    pub fn pseudo_element(&self) -> Option<PseudoElement> {
        self.compounds.last().and_then(|(c, _)| c.pseudo_element)
    }

    /// Matches the target element `node_id` against this complex selector.
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        with_match_budget(|| self.matches_inner(doc, node_id))
    }

    /// Matches the `pseudo` pseudo-element of `originating` against this complex selector.
    pub fn matches_for_pseudo(
        &self,
        doc: &Document,
        originating: NodeId,
        pseudo: PseudoElement,
    ) -> bool {
        with_match_budget(|| self.matches_for_pseudo_inner(doc, originating, pseudo))
    }

    pub(crate) fn matches_inner(&self, doc: &Document, node_id: NodeId) -> bool {
        if self.compounds.is_empty() {
            return false;
        }

        // Rightmost compound must match node_id
        let last_idx = self.compounds.len() - 1;
        if !self.compounds[last_idx].0.matches(doc, node_id) {
            return false;
        }

        self.matches_from_right(doc, node_id, last_idx)
    }

    pub(crate) fn matches_for_pseudo_inner(
        &self,
        doc: &Document,
        originating: NodeId,
        pseudo: PseudoElement,
    ) -> bool {
        if self.compounds.is_empty() {
            return false;
        }

        let last_idx = self.compounds.len() - 1;
        if !self.compounds[last_idx]
            .0
            .matches_for_pseudo(doc, originating, pseudo)
        {
            return false;
        }

        self.matches_from_right(doc, originating, last_idx)
    }

    /// Calculates the total specificity of this complex selector chain (§16).
    pub fn specificity(&self) -> Specificity {
        let mut spec = Specificity::ZERO;
        for (compound, _) in &self.compounds {
            spec += compound.specificity();
        }
        spec
    }

    pub(crate) fn matches_from_right(
        &self,
        doc: &Document,
        current_node: NodeId,
        index: usize,
    ) -> bool {
        if !spend_match_step() {
            return false;
        }
        if index == 0 {
            return true;
        }

        let prev_index = index - 1;
        let (ref prev_compound, comb) = self.compounds[prev_index];
        let Some(combinator) = comb else {
            return false;
        };

        match combinator {
            Combinator::Child => {
                let Some(parent_id) = doc.node(current_node).parent else {
                    return false;
                };
                if prev_compound.matches(doc, parent_id) {
                    self.matches_from_right(doc, parent_id, prev_index)
                } else {
                    false
                }
            }
            Combinator::Descendant => {
                let mut ancestor = doc.node(current_node).parent;
                while let Some(anc_id) = ancestor {
                    if prev_compound.matches(doc, anc_id)
                        && self.matches_from_right(doc, anc_id, prev_index)
                    {
                        return true;
                    }
                    ancestor = doc.node(anc_id).parent;
                }
                false
            }
            Combinator::NextSibling => {
                let mut prev = doc.node(current_node).prev_sibling;
                while let Some(prev_id) = prev {
                    if doc.node(prev_id).as_element().is_some() {
                        return if prev_compound.matches(doc, prev_id) {
                            self.matches_from_right(doc, prev_id, prev_index)
                        } else {
                            false
                        };
                    }
                    prev = doc.node(prev_id).prev_sibling;
                }
                false
            }
            Combinator::SubsequentSibling => {
                let mut prev = doc.node(current_node).prev_sibling;
                while let Some(prev_id) = prev {
                    if doc.node(prev_id).as_element().is_some()
                        && prev_compound.matches(doc, prev_id)
                        && self.matches_from_right(doc, prev_id, prev_index)
                    {
                        return true;
                    }
                    prev = doc.node(prev_id).prev_sibling;
                }
                false
            }
        }
    }
}

/// A relative selector as accepted by `:has()`: a combinator from the anchor element followed
/// by a complex selector (Selectors Level 4 §4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelativeSelector {
    /// Combinator between the anchor element and the first compound (`Descendant` by default).
    pub leading: Combinator,
    /// Complex selector whose subject must be reachable from the anchor element.
    pub selector: ComplexSelector,
}

impl RelativeSelector {
    /// Returns whether `anchor` has a descendant/child/sibling matching this relative selector.
    ///
    /// Limitation: the matcher only supports combinators *after* the first compound when they
    /// are ordinary descendant/child/sibling combinators; it does not evaluate `:scope` and
    /// does not implement the full relative-selector anchoring rules with backtracking budgets.
    pub fn matches(&self, doc: &Document, anchor: NodeId) -> bool {
        with_match_budget(|| self.matches_inner(doc, anchor))
    }

    pub(crate) fn matches_inner(&self, doc: &Document, anchor: NodeId) -> bool {
        let compounds = &self.selector.compounds;
        if compounds.is_empty() {
            return false;
        }
        relative_chain_matches(doc, anchor, compounds, 0, self.leading)
    }
}
