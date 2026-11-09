//! # W3C Selectors Level 4 AST & DOM Matching Engine
//!
//! Provides compound and complex selector representation, token stream parsing,
//! and high-performance matching over [`dom::Document`] and [`dom::NodeId`].

use dom::{Document, Namespace, NodeId, NodeKind};

/// Operator for attribute selectors (`[attr=value]`, `[attr~=value]`, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrOperator {
    /// Exact equality `[attr=val]`
    Exact,
    /// Whitespace-separated word list `[attr~=val]`
    Includes,
    /// Dash-separated match `[attr|=val]` (`val` or `val-*`)
    DashMatch,
    /// Prefix match `[attr^=val]`
    Prefix,
    /// Suffix match `[attr$=val]`
    Suffix,
    /// Substring match `[attr*=val]`
    Substring,
}

/// An attribute selector requirement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributeSelector {
    pub name: String,
    pub operator: Option<AttrOperator>,
    pub value: Option<String>,
    pub case_insensitive: bool,
}

impl AttributeSelector {
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        let Some(el) = doc.node(node_id).as_element() else {
            return false;
        };
        let attr_val = el
            .attributes
            .iter()
            .find(|a| a.local_name.eq_ignore_ascii_case(&self.name))
            .map(|a| a.value.as_str());

        let Some(val) = attr_val else {
            return false;
        };

        let Some(op) = self.operator else {
            return true; // Simple existence check `[attr]`
        };

        let expected = self.value.as_deref().unwrap_or_default();
        let eq = |a: &str, b: &str| {
            if self.case_insensitive {
                a.eq_ignore_ascii_case(b)
            } else {
                a == b
            }
        };

        match op {
            AttrOperator::Exact => eq(val, expected),
            AttrOperator::Includes => val.split_ascii_whitespace().any(|word| eq(word, expected)),
            AttrOperator::DashMatch => {
                eq(val, expected)
                    || (val.len() > expected.len()
                        && val.as_bytes().get(expected.len()) == Some(&b'-')
                        && eq(&val[..expected.len()], expected))
            }
            AttrOperator::Prefix => {
                if self.case_insensitive {
                    val.to_ascii_lowercase()
                        .starts_with(&expected.to_ascii_lowercase())
                } else {
                    val.starts_with(expected)
                }
            }
            AttrOperator::Suffix => {
                if self.case_insensitive {
                    val.to_ascii_lowercase()
                        .ends_with(&expected.to_ascii_lowercase())
                } else {
                    val.ends_with(expected)
                }
            }
            AttrOperator::Substring => {
                if self.case_insensitive {
                    val.to_ascii_lowercase()
                        .contains(&expected.to_ascii_lowercase())
                } else {
                    val.contains(expected)
                }
            }
        }
    }
}

/// Formula $An+B$ for `:nth-child(An+B)` pseudo-classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnPlusB {
    pub a: i32,
    pub b: i32,
}

impl AnPlusB {
    pub fn matches(&self, index_1_based: i32) -> bool {
        if self.a == 0 {
            index_1_based == self.b
        } else {
            let diff = index_1_based - self.b;
            diff % self.a == 0 && diff / self.a >= 0
        }
    }
}

/// Parses an An+B formula according to W3C CSS Syntax Level 3 §3.3 & Selectors Level 4 §15.
pub fn parse_an_plus_b(s: &str) -> Option<AnPlusB> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.eq_ignore_ascii_case("odd") {
        return Some(AnPlusB { a: 2, b: 1 });
    }
    if s.eq_ignore_ascii_case("even") {
        return Some(AnPlusB { a: 2, b: 0 });
    }

    // Integer only `B`
    if let Ok(b) = s.parse::<i32>() {
        return Some(AnPlusB { a: 0, b });
    }

    // Must contain 'n' or 'N'
    let n_idx = s.find(['n', 'N'])?;
    let (prefix, suffix) = (&s[..n_idx], &s[n_idx + 1..]);

    let a = match prefix.trim() {
        "" | "+" => 1,
        "-" => -1,
        num_str => num_str.parse::<i32>().ok()?,
    };

    let suffix_trimmed = suffix.replace(' ', "");
    let b = if suffix_trimmed.is_empty() {
        0
    } else {
        suffix_trimmed.parse::<i32>().ok()?
    };

    Some(AnPlusB { a, b })
}

/// Supported pseudo-classes in Selectors Level 4.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PseudoClass {
    FirstChild,
    LastChild,
    OnlyChild,
    Empty,
    Root,
    NthChild(AnPlusB),
}

impl PseudoClass {
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        match self {
            Self::Root => {
                // In HTML, the document element (root <html>)
                let parent = doc.node(node_id).parent;
                parent == Some(doc.root())
            }
            Self::Empty => {
                // No element children and no non-empty text children
                let mut curr = doc.node(node_id).first_child;
                while let Some(child_id) = curr {
                    match &doc.node(child_id).kind {
                        NodeKind::Element(_) => return false,
                        NodeKind::Text(s) if !s.is_empty() => return false,
                        _ => {}
                    }
                    curr = doc.node(child_id).next_sibling;
                }
                true
            }
            Self::FirstChild => {
                let Some(parent_id) = doc.node(node_id).parent else {
                    return false;
                };
                let mut curr = doc.node(parent_id).first_child;
                while let Some(child_id) = curr {
                    if doc.node(child_id).as_element().is_some() {
                        return child_id == node_id;
                    }
                    curr = doc.node(child_id).next_sibling;
                }
                false
            }
            Self::LastChild => {
                let Some(parent_id) = doc.node(node_id).parent else {
                    return false;
                };
                let mut curr = doc.node(parent_id).last_child;
                while let Some(child_id) = curr {
                    if doc.node(child_id).as_element().is_some() {
                        return child_id == node_id;
                    }
                    curr = doc.node(child_id).prev_sibling;
                }
                false
            }
            Self::OnlyChild => {
                Self::FirstChild.matches(doc, node_id) && Self::LastChild.matches(doc, node_id)
            }
            Self::NthChild(anb) => {
                let Some(parent_id) = doc.node(node_id).parent else {
                    return false;
                };
                let mut idx = 0;
                let mut curr = doc.node(parent_id).first_child;
                while let Some(child_id) = curr {
                    if doc.node(child_id).as_element().is_some() {
                        idx += 1;
                        if child_id == node_id {
                            return anb.matches(idx);
                        }
                    }
                    curr = doc.node(child_id).next_sibling;
                }
                false
            }
        }
    }
}

/// Specificity of a selector per W3C Selectors Level 4 §16.
/// Represented as a 3-tuple `(a, b, c)`:
/// - `a`: ID selectors (`#id`)
/// - `b`: class selectors (`.class`), attribute selectors (`[attr]`), and pseudo-classes (`:first-child`)
/// - `c`: type selectors (`div`) and pseudo-elements (`::before`)
///
/// Universal selector (`*`) and combinators (` `, `>`, `+`, `~`) contribute zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity {
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

impl Specificity {
    pub const ZERO: Self = Self { a: 0, b: 0, c: 0 };

    pub const fn new(a: u32, b: u32, c: u32) -> Self {
        Self { a, b, c }
    }
}

impl std::ops::Add for Specificity {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self {
            a: self.a + rhs.a,
            b: self.b + rhs.b,
            c: self.c + rhs.c,
        }
    }
}

impl std::ops::AddAssign for Specificity {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.a += rhs.a;
        self.b += rhs.b;
        self.c += rhs.c;
    }
}

impl std::fmt::Display for Specificity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.a, self.b, self.c)
    }
}

/// A compound selector without combinators (e.g. `div#main.active[data-kind]:first-child`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompoundSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<AttributeSelector>,
    pub pseudo_classes: Vec<PseudoClass>,
}

impl CompoundSelector {
    /// Calculates the specificity of this compound selector according to Selectors Level 4 §16.
    pub fn specificity(&self) -> Specificity {
        let a = if self.id.is_some() { 1 } else { 0 };
        let b = (self.classes.len() + self.attributes.len() + self.pseudo_classes.len()) as u32;
        let c = if self.tag_name.as_deref().is_some_and(|t| t != "*") {
            1
        } else {
            0
        };
        Specificity::new(a, b, c)
    }

    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        let Some(el) = doc.node(node_id).as_element() else {
            return false;
        };

        if el.namespace != Namespace::Html {
            return false;
        }

        if let Some(ref tag) = self.tag_name {
            if tag != "*" && !el.local_name.eq_ignore_ascii_case(tag) {
                return false;
            }
        }

        if let Some(ref expected_id) = self.id {
            if el.get_attribute("id") != Some(expected_id.as_str()) {
                return false;
            }
        }

        if !self.classes.is_empty() {
            let Some(class_attr) = el.get_attribute("class") else {
                return false;
            };
            let elem_classes: Vec<&str> = class_attr.split_ascii_whitespace().collect();
            for c in &self.classes {
                if !elem_classes.iter().any(|ec| ec.eq_ignore_ascii_case(c)) {
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
    /// Matches the target element `node_id` against this complex selector.
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
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

    /// Calculates the total specificity of this complex selector chain (§16).
    pub fn specificity(&self) -> Specificity {
        let mut spec = Specificity::ZERO;
        for (compound, _) in &self.compounds {
            spec += compound.specificity();
        }
        spec
    }

    fn matches_from_right(&self, doc: &Document, current_node: NodeId, index: usize) -> bool {
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

/// A comma-separated list of complex selectors (`h1, h2, .title`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectorList {
    pub selectors: Vec<ComplexSelector>,
}

impl SelectorList {
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        self.selectors.iter().any(|sel| sel.matches(doc, node_id))
    }

    /// If this selector list matches the given element, returns the maximum specificity
    /// among all the complex selectors in the list that matched.
    pub fn matching_specificity(&self, doc: &Document, node_id: NodeId) -> Option<Specificity> {
        self.selectors
            .iter()
            .filter(|sel| sel.matches(doc, node_id))
            .map(|sel| sel.specificity())
            .max()
    }

    /// Calculates the maximum specificity among all selectors in this list.
    pub fn max_specificity(&self) -> Specificity {
        self.selectors
            .iter()
            .map(|s| s.specificity())
            .max()
            .unwrap_or(Specificity::ZERO)
    }
}

/// Parses a selector string into a [`SelectorList`].
pub fn parse_selectors(input: &str) -> SelectorList {
    let mut list = SelectorList::default();
    for part in input.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(complex) = parse_complex_selector(trimmed) {
            list.selectors.push(complex);
        }
    }
    list
}

/// Parses a single complex selector string.
fn parse_complex_selector(input: &str) -> Option<ComplexSelector> {
    let mut compounds = Vec::new();
    let mut tokens = tokenize_selector_parts(input);
    if tokens.is_empty() {
        return None;
    }

    while !tokens.is_empty() {
        let compound = parse_compound_selector(&mut tokens)?;
        let comb = if tokens.is_empty() {
            None
        } else {
            let next = tokens.remove(0);
            match next.as_str() {
                ">" => Some(Combinator::Child),
                "+" => Some(Combinator::NextSibling),
                "~" => Some(Combinator::SubsequentSibling),
                " " => Some(Combinator::Descendant),
                _ => Some(Combinator::Descendant),
            }
        };
        compounds.push((compound, comb));
    }

    Some(ComplexSelector { compounds })
}

fn tokenize_selector_parts(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '>' | '+' | '~' => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                    current.clear();
                }
                parts.push(ch.to_string());
            }
            ' ' | '\t' | '\n' => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                    current.clear();
                }
                while chars.peek().copied().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                if let Some(&next_c) = chars.peek() {
                    if !matches!(next_c, '>' | '+' | '~')
                        && !parts.is_empty()
                        && parts.last().map(|s| s.as_str()) != Some(">")
                        && parts.last().map(|s| s.as_str()) != Some("+")
                        && parts.last().map(|s| s.as_str()) != Some("~")
                    {
                        parts.push(" ".to_string());
                    }
                }
            }
            '[' => {
                current.push('[');
                for inner in chars.by_ref() {
                    current.push(inner);
                    if inner == ']' {
                        break;
                    }
                }
            }
            '(' => {
                current.push('(');
                for inner in chars.by_ref() {
                    current.push(inner);
                    if inner == ')' {
                        break;
                    }
                }
            }
            other => current.push(other),
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.to_string());
    }
    parts
}

fn parse_compound_selector(tokens: &mut Vec<String>) -> Option<CompoundSelector> {
    if tokens.is_empty() {
        return None;
    }
    let s = tokens.remove(0);
    if s == " " || s == ">" || s == "+" || s == "~" {
        return None;
    }

    let mut compound = CompoundSelector::default();
    let mut chars = s.chars().peekable();

    // Check tag name
    if chars
        .peek()
        .copied()
        .is_some_and(|c| c.is_alphabetic() || c == '*' || c == '_')
    {
        let mut tag = String::new();
        while chars
            .peek()
            .copied()
            .is_some_and(|c| c.is_alphanumeric() || c == '*' || c == '-' || c == '_')
        {
            tag.push(chars.next().unwrap());
        }
        compound.tag_name = Some(tag);
    }

    while let Some(ch) = chars.next() {
        match ch {
            '#' => {
                let mut id = String::new();
                while chars
                    .peek()
                    .copied()
                    .is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_')
                {
                    id.push(chars.next().unwrap());
                }
                compound.id = Some(id);
            }
            '.' => {
                let mut class = String::new();
                while chars
                    .peek()
                    .copied()
                    .is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_')
                {
                    class.push(chars.next().unwrap());
                }
                compound.classes.push(class);
            }
            ':' => {
                let mut pseudo = String::new();
                while chars
                    .peek()
                    .copied()
                    .is_some_and(|c| c.is_alphabetic() || c == '-')
                {
                    pseudo.push(chars.next().unwrap());
                }
                let mut arg = None;
                if chars.peek() == Some(&'(') {
                    chars.next();
                    let mut arg_str = String::new();
                    for inner in chars.by_ref() {
                        if inner == ')' {
                            break;
                        }
                        arg_str.push(inner);
                    }
                    arg = Some(arg_str);
                }
                match pseudo.to_ascii_lowercase().as_str() {
                    "first-child" => compound.pseudo_classes.push(PseudoClass::FirstChild),
                    "last-child" => compound.pseudo_classes.push(PseudoClass::LastChild),
                    "only-child" => compound.pseudo_classes.push(PseudoClass::OnlyChild),
                    "empty" => compound.pseudo_classes.push(PseudoClass::Empty),
                    "root" => compound.pseudo_classes.push(PseudoClass::Root),
                    "nth-child" => {
                        if let Some(arg_str) = arg {
                            if let Some(anb) = parse_an_plus_b(&arg_str) {
                                compound.pseudo_classes.push(PseudoClass::NthChild(anb));
                            }
                        }
                    }
                    _ => {}
                }
            }
            '[' => {
                let mut attr_spec = String::new();
                for inner in chars.by_ref() {
                    if inner == ']' {
                        break;
                    }
                    attr_spec.push(inner);
                }
                if let Some(attr_sel) = parse_attribute_selector(&attr_spec) {
                    compound.attributes.push(attr_sel);
                }
            }
            _ => {}
        }
    }

    Some(compound)
}

fn parse_attribute_selector(spec: &str) -> Option<AttributeSelector> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }

    let ops = [
        ("~=", AttrOperator::Includes),
        ("|=", AttrOperator::DashMatch),
        ("^=", AttrOperator::Prefix),
        ("$=", AttrOperator::Suffix),
        ("*=", AttrOperator::Substring),
        ("=", AttrOperator::Exact),
    ];

    for (op_str, op) in ops {
        if let Some(idx) = spec.find(op_str) {
            let name = spec[..idx].trim().to_string();
            let mut val_part = spec[idx + op_str.len()..].trim();
            let mut case_insensitive = false;
            if val_part.ends_with(" i") || val_part.ends_with(" I") {
                case_insensitive = true;
                val_part = val_part[..val_part.len() - 2].trim();
            }
            let val = val_part.trim_matches(|c| c == '"' || c == '\'').to_string();
            return Some(AttributeSelector {
                name,
                operator: Some(op),
                value: Some(val),
                case_insensitive,
            });
        }
    }

    Some(AttributeSelector {
        name: spec.to_string(),
        operator: None,
        value: None,
        case_insensitive: false,
    })
}
