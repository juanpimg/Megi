//! # W3C Selectors Level 4 AST & DOM Matching Engine
//!
//! Provides compound and complex selector representation, token stream parsing,
//! and high-performance matching over [`dom::Document`] and [`dom::NodeId`].
//!
//! Supported: type/universal selectors (case-insensitive for HTML elements, case-sensitive
//! for foreign SVG/MathML elements), `#id`, `.class`, attribute selectors (`=`, `~=`, `|=`,
//! `^=`, `$=`, `*=` plus the `i` flag), descendant/child/next-sibling/subsequent-sibling
//! combinators, and the structural pseudo-classes `:first-child`, `:last-child`,
//! `:only-child`, `:empty`, `:root` and `:nth-child(An+B)`.
//!
//! Still pending (documented, out of scope for this milestone): `@media` and other at-rules
//! in the cascade, `:is()`, `:where()`, `:not()`, `:has()`, `:nth-of-type()`/`:nth-last-*`,
//! `:lang()`, namespace prefixes and pseudo-element boxes. Dynamic pseudo-classes without
//! tracked interaction state match nothing, and unknown/unsupported pseudo-classes invalidate
//! the whole selector list instead of silently over-matching (audit findings F13/F14/F15/F16).

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
///
/// Pseudo-classes that depend on interaction or document state that this engine does not
/// track yet (`:hover`, `:active`, `:focus`, `:visited`, `:link`, ...) and pseudo-elements
/// (`::before`, `::after`, ...) are parsed as [`PseudoClass::Never`]: the selector stays
/// valid but can never match, instead of over-matching every element like the previous
/// fallback did.
///
/// Not implemented yet (selectors using them are currently rejected as invalid):
/// `:is()`, `:where()`, `:not()`, `:has()`, `:nth-of-type()`/`:nth-last-*`, `:lang()`,
/// attribute namespaces and the `s`/`i` case-sensitivity flags of `@namespace`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PseudoClass {
    FirstChild,
    LastChild,
    OnlyChild,
    Empty,
    Root,
    NthChild(AnPlusB),
    /// Known pseudo-class/pseudo-element whose state this engine cannot observe.
    Never,
}

impl PseudoClass {
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        match self {
            Self::Root => {
                // In HTML, the document element (root <html>)
                let parent = doc.node(node_id).parent;
                parent == Some(doc.root())
            }
            Self::Never => false,
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

    /// Matches this compound selector against a DOM element.
    ///
    /// Type selectors match HTML elements case-insensitively and foreign (SVG/MathML)
    /// elements case-sensitively. Unlike the previous implementation, type/class/id/attribute
    /// and universal selectors are no longer restricted to the HTML namespace, so styles now
    /// apply to SVG/MathML elements (F14).
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
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
///
/// Commas only split the list at the top level: commas inside attribute values, functional
/// pseudo-class arguments or quoted strings do not start a new selector (F15). Per CSS Syntax
/// Level 3 §5.4.4, a selector list containing one invalid or unsupported selector is itself
/// invalid, so the whole list is dropped instead of silently over-matching with its valid
/// parts (F13).
///
/// Pending and not implemented in this milestone: `@media`/at-rules in the cascade, and
/// `:is()`, `:where()`, `:not()`, `:has()`, which currently invalidate the selector list.
pub fn parse_selectors(input: &str) -> SelectorList {
    if input.trim().is_empty() {
        return SelectorList::default();
    }
    let parts = split_top_level_commas(input);
    let mut selectors = Vec::with_capacity(parts.len());
    for part in parts {
        let Some(complex) = parse_complex_selector(part.trim()) else {
            return SelectorList::default();
        };
        selectors.push(complex);
    }
    SelectorList { selectors }
}

/// Splits a selector list on top-level commas, ignoring commas nested in blocks or strings.
fn split_top_level_commas(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            current.push(ch);
            if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut current)),
            c => current.push(c),
        }
    }
    if !current.is_empty() || !parts.is_empty() {
        parts.push(current);
    }
    parts
}

/// Parses a single complex selector string.
fn parse_complex_selector(input: &str) -> Option<ComplexSelector> {
    let mut tokens = tokenize_selector_parts(input);
    let mut compounds = Vec::new();

    loop {
        let compound = parse_compound_selector(&mut tokens)?;
        if tokens.is_empty() {
            compounds.push((compound, None));
            break;
        }
        let combinator = match tokens.remove(0).as_str() {
            ">" => Combinator::Child,
            "+" => Combinator::NextSibling,
            "~" => Combinator::SubsequentSibling,
            " " => Combinator::Descendant,
            _ => return None,
        };
        compounds.push((compound, Some(combinator)));
    }

    Some(ComplexSelector { compounds })
}

/// Appends the currently accumulated selector chunk to `parts`, if it is not blank.
fn push_selector_chunk(parts: &mut Vec<String>, current: &mut String) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.to_string());
        current.clear();
    } else {
        current.clear();
    }
}

/// Splits a complex selector into compound-selector chunks and combinator tokens.
///
/// Whitespace becomes an explicit `" "` descendant-combinator token; `>`, `+` and `~` become
/// combinator tokens. Whitespace and combinators nested in `[...]`, `(...)` or strings stay
/// inside their chunk, so attribute values such as `[title="Hello, world"]` survive intact
/// (F15).
fn tokenize_selector_parts(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '>' | '+' | '~' => {
                push_selector_chunk(&mut parts, &mut current);
                parts.push(ch.to_string());
            }
            c if c.is_whitespace() => {
                push_selector_chunk(&mut parts, &mut current);
                while chars.peek().copied().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                let next = chars.peek().copied();
                let prev_is_combinator =
                    matches!(parts.last().map(String::as_str), Some(">" | "+" | "~"));
                if next.is_some()
                    && !matches!(next, Some('>' | '+' | '~'))
                    && !parts.is_empty()
                    && !prev_is_combinator
                {
                    parts.push(" ".to_string());
                }
            }
            '[' | '(' => {
                let (open, close) = if ch == '[' { ('[', ']') } else { ('(', ')') };
                current.push(open);
                let mut depth = 1usize;
                let mut quote: Option<char> = None;
                let mut escaped = false;
                for inner in chars.by_ref() {
                    if escaped {
                        current.push(inner);
                        escaped = false;
                        continue;
                    }
                    if let Some(q) = quote {
                        current.push(inner);
                        if inner == '\\' {
                            escaped = true;
                        } else if inner == q {
                            quote = None;
                        }
                        continue;
                    }
                    match inner {
                        '"' | '\'' => {
                            quote = Some(inner);
                            current.push(inner);
                        }
                        c if c == open => {
                            depth += 1;
                            current.push(c);
                        }
                        c if c == close => {
                            depth -= 1;
                            current.push(c);
                            if depth == 0 {
                                break;
                            }
                        }
                        c => current.push(c),
                    }
                }
            }
            other => current.push(other),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

/// Dynamic pseudo-classes whose state this engine does not track yet.
///
/// They are accepted by the parser but mapped to [`PseudoClass::Never`]: a rule such as
/// `a:hover { ... }` must not match every `a` element (CSS Selectors Level 4 §3).
const DYNAMIC_PSEUDO_CLASSES: &[&str] = &[
    "hover",
    "active",
    "focus",
    "focus-visible",
    "focus-within",
    "target",
    "target-within",
    "visited",
    "link",
    "any-link",
    "local-link",
    "checked",
    "indeterminate",
    "default",
    "disabled",
    "enabled",
    "required",
    "optional",
    "valid",
    "invalid",
    "in-range",
    "out-of-range",
    "read-only",
    "read-write",
    "placeholder-shown",
    "autofill",
    "fullscreen",
    "modal",
    "picture-in-picture",
    "playing",
    "paused",
];

fn parse_compound_selector(tokens: &mut Vec<String>) -> Option<CompoundSelector> {
    if tokens.is_empty() {
        return None;
    }
    let s = tokens.remove(0);
    if matches!(s.as_str(), " " | ">" | "+" | "~") {
        return None;
    }

    let mut compound = CompoundSelector::default();
    let mut chars = s.chars().peekable();

    // Optional type selector or universal selector prefix.
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
        if tag.is_empty() {
            return None;
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
                if id.is_empty() {
                    return None;
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
                if class.is_empty() {
                    return None;
                }
                compound.classes.push(class);
            }
            ':' => {
                let is_pseudo_element = chars.peek() == Some(&':');
                if is_pseudo_element {
                    chars.next();
                }
                let mut pseudo = String::new();
                while chars
                    .peek()
                    .copied()
                    .is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_')
                {
                    pseudo.push(chars.next().unwrap());
                }
                if pseudo.is_empty() {
                    return None;
                }

                let mut argument = None;
                if chars.peek() == Some(&'(') {
                    chars.next();
                    let mut depth = 1usize;
                    let mut arg = String::new();
                    for inner in chars.by_ref() {
                        match inner {
                            '(' => {
                                depth += 1;
                                arg.push(inner);
                            }
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                arg.push(inner);
                            }
                            c => arg.push(c),
                        }
                    }
                    if depth != 0 {
                        return None; // unbalanced functional pseudo-class
                    }
                    argument = Some(arg);
                }

                if is_pseudo_element {
                    // Pseudo-element boxes are not generated yet: the selector stays valid
                    // but can never match an element (F13).
                    compound.pseudo_classes.push(PseudoClass::Never);
                    continue;
                }

                let name = pseudo.to_ascii_lowercase();
                let pseudo_class = match name.as_str() {
                    "first-child" if argument.is_none() => PseudoClass::FirstChild,
                    "last-child" if argument.is_none() => PseudoClass::LastChild,
                    "only-child" if argument.is_none() => PseudoClass::OnlyChild,
                    "empty" if argument.is_none() => PseudoClass::Empty,
                    "root" if argument.is_none() => PseudoClass::Root,
                    "nth-child" => PseudoClass::NthChild(parse_an_plus_b(argument.as_deref()?)?),
                    // Legacy single-colon pseudo-element syntax (`:before`, `:first-line`, ...).
                    "before" | "after" | "first-line" | "first-letter" if argument.is_none() => {
                        PseudoClass::Never
                    }
                    _ if DYNAMIC_PSEUDO_CLASSES.contains(&name.as_str()) && argument.is_none() => {
                        PseudoClass::Never
                    }
                    // `:is()`/`:where()`/`:not()`/`:has()` and unknown pseudo-classes
                    // invalidate the selector list instead of over-matching (F13).
                    _ => return None,
                };
                compound.pseudo_classes.push(pseudo_class);
            }
            '[' => {
                let mut attr_spec = String::new();
                let mut quote: Option<char> = None;
                let mut escaped = false;
                let mut closed = false;
                for inner in chars.by_ref() {
                    if escaped {
                        attr_spec.push(inner);
                        escaped = false;
                        continue;
                    }
                    if let Some(q) = quote {
                        attr_spec.push(inner);
                        if inner == '\\' {
                            escaped = true;
                        } else if inner == q {
                            quote = None;
                        }
                        continue;
                    }
                    match inner {
                        '"' | '\'' => {
                            quote = Some(inner);
                            attr_spec.push(inner);
                        }
                        ']' => {
                            closed = true;
                            break;
                        }
                        c => attr_spec.push(c),
                    }
                }
                if !closed {
                    return None;
                }
                compound
                    .attributes
                    .push(parse_attribute_selector(&attr_spec)?);
            }
            _ => return None,
        }
    }

    Some(compound)
}

fn parse_attribute_selector(spec: &str) -> Option<AttributeSelector> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }

    // Find the operator outside quoted strings.
    let mut operator = None;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (byte_index, ch) in spec.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        let rest = &spec[byte_index + ch.len_utf8()..];
        match ch {
            '"' | '\'' => quote = Some(ch),
            '~' | '^' | '$' | '*' if rest.starts_with('=') => {
                let op = match ch {
                    '~' => AttrOperator::Includes,
                    '^' => AttrOperator::Prefix,
                    '$' => AttrOperator::Suffix,
                    _ => AttrOperator::Substring,
                };
                operator = Some((byte_index, op, 2usize));
                break;
            }
            '|' if rest.starts_with('=') => {
                operator = Some((byte_index, AttrOperator::DashMatch, 2usize));
                break;
            }
            '=' => {
                operator = Some((byte_index, AttrOperator::Exact, 1usize));
                break;
            }
            // Attribute namespaces (`[ns|attr]`) are not supported yet.
            '|' => return None,
            _ => {}
        }
    }

    let Some((op_index, op, op_len)) = operator else {
        // Presence selector `[attr]`.
        if !is_valid_attribute_name(spec) {
            return None;
        }
        return Some(AttributeSelector {
            name: spec.to_string(),
            operator: None,
            value: None,
            case_insensitive: false,
        });
    };

    let name = spec[..op_index].trim();
    if !is_valid_attribute_name(name) {
        return None;
    }

    let mut value_part = spec[op_index + op_len..].trim();
    let mut case_insensitive = false;
    if let Some((rest, flag)) = split_case_flag(value_part) {
        value_part = rest;
        case_insensitive = flag;
    }
    let value = strip_attribute_value_quotes(value_part)?;

    Some(AttributeSelector {
        name: name.to_string(),
        operator: Some(op),
        value: Some(value),
        case_insensitive,
    })
}

/// Returns whether `name` looks like a valid attribute local name for a selector.
fn is_valid_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && !name.chars().any(|c| {
            c.is_ascii_whitespace()
                || matches!(
                    c,
                    '"' | '\'' | '[' | ']' | '=' | '~' | '^' | '$' | '*' | '|'
                )
        })
}

/// Splits a trailing `i`/`s` case-sensitivity flag from an attribute selector value.
fn split_case_flag(value: &str) -> Option<(&str, bool)> {
    let (rest, flag) = value.trim_end().rsplit_once(char::is_whitespace)?;
    let case_insensitive = match flag {
        "i" | "I" => true,
        "s" | "S" => false,
        _ => return None,
    };
    Some((rest.trim_end(), case_insensitive))
}

/// Removes the surrounding quotes of an attribute selector value, validating unquoted values.
fn strip_attribute_value_quotes(value: &str) -> Option<String> {
    let value = value.trim();
    if let Some(first) = value.chars().next()
        && matches!(first, '"' | '\'')
        && value.len() >= 2
        && value.ends_with(first)
    {
        return Some(value[1..value.len() - 1].to_string());
    }
    if value.is_empty() || value.contains(char::is_whitespace) {
        return None;
    }
    Some(value.to_string())
}
