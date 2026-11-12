//! Minimal CSS subset used by inline SVG `<style>` elements.
//!
//! Supported: type (`rect`), class (`.foo`), id (`#bar`) compound selectors,
//! descendant combinators (`g .foo rect`), selector lists (`a, b`) and
//! declaration blocks with presentation properties. Unsupported constructs
//! (at-rules, child/sibling combinators, pseudo-classes) are skipped without
//! panicking. The matching itself is implemented here against the XML tree; the
//! CSS text is parsed by a small self-contained parser (no dependency cycle).

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
pub(crate) struct MatchedDeclaration {
    pub specificity: u32,
    pub order: usize,
    pub name: String,
    pub value: String,
}

/// Parses a stylesheet into rules, skipping unsupported/invalid constructs.
pub(crate) fn parse_stylesheet(text: &str) -> Vec<StyleRule> {
    if text.len() > MAX_STYLESHEET_BYTES {
        return Vec::new();
    }
    let stripped = strip_comments(text);
    let bytes = stripped.as_bytes();
    let mut rules = Vec::new();
    let mut position = 0usize;
    let mut order = 0usize;
    while position < bytes.len() && rules.len() < MAX_RULES {
        while position < bytes.len() && bytes[position].is_ascii_whitespace() {
            position += 1;
        }
        if position >= bytes.len() {
            break;
        }
        // At-rules: skip the whole block (not supported in this subset).
        if bytes[position] == b'@' {
            position = skip_block(bytes, position);
            continue;
        }
        let selector_start = position;
        let mut block_start = None;
        while position < bytes.len() {
            if bytes[position] == b'{' {
                block_start = Some(position);
                break;
            }
            if bytes[position] == b'}' || bytes[position] == b';' {
                break;
            }
            position += 1;
        }
        let Some(block_start) = block_start else {
            position += 1;
            continue;
        };
        let selector_text = stripped[selector_start..block_start].trim();
        let block_end = find_block_end(bytes, block_start + 1);
        let declaration_text = &stripped[block_start + 1..block_end];
        position = (block_end + 1).min(bytes.len());
        if selector_text.is_empty() || selector_text.len() > 2048 {
            continue;
        }
        let mut selectors = Vec::new();
        for raw in selector_text.split(',') {
            if selectors.len() >= MAX_SELECTORS_PER_RULE {
                break;
            }
            if let Some(selector) = parse_selector(raw) {
                selectors.push(selector);
            }
        }
        if selectors.is_empty() {
            continue;
        }
        let declarations = parse_declarations(declaration_text);
        if declarations.is_empty() {
            continue;
        }
        rules.push(StyleRule {
            selectors,
            declarations,
            order,
        });
        order += 1;
    }
    rules
}

/// Returns every declaration that applies to `element`, in cascade order
/// (lowest specificity/order first; callers apply later entries last).
pub(crate) fn matching_declarations(
    rules: &[StyleRule],
    element: &XmlElement,
    ancestors: &[&XmlElement],
    budget: &mut SelectorBudget,
) -> Result<Vec<MatchedDeclaration>, SvgError> {
    let mut matched = Vec::new();
    for rule in rules {
        let mut best_specificity: Option<u32> = None;
        for selector in &rule.selectors {
            budget.charge()?;
            if selector.matches(element, ancestors) {
                let specificity = selector.specificity();
                best_specificity =
                    Some(best_specificity.map_or(specificity, |best| best.max(specificity)));
            }
        }
        let Some(specificity) = best_specificity else {
            continue;
        };
        for declaration in &rule.declarations {
            matched.push(MatchedDeclaration {
                specificity,
                order: rule.order,
                name: declaration.name.clone(),
                value: declaration.value.clone(),
            });
        }
    }
    matched.sort_by_key(|declaration| (declaration.specificity, declaration.order));
    Ok(matched)
}

/// Parses a `style="..."` attribute into declarations.
pub(crate) fn parse_declarations(text: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    for raw in text.split(';') {
        let Some((name, value)) = raw.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name.is_empty() || value.is_empty() {
            continue;
        }
        declarations.push(Declaration {
            name,
            value: value.to_owned(),
        });
    }
    declarations
}

fn parse_selector(raw: &str) -> Option<Selector> {
    let mut parts = Vec::new();
    // `>` is treated as a descendant combinator (documented approximation).
    let compounds: Vec<String> = raw
        .replace('>', " ")
        .split_ascii_whitespace()
        .map(str::to_owned)
        .collect();
    if compounds.len() > MAX_SELECTOR_PARTS {
        // Fail closed: reject over-long selectors instead of silently truncating.
        return None;
    }
    for compound in compounds {
        if compound.is_empty() {
            continue;
        }
        parts.push(parse_simple_selector(&compound)?);
    }
    if parts.is_empty() || parts.len() > MAX_SELECTOR_PARTS {
        return None;
    }
    Some(Selector { parts })
}

fn parse_simple_selector(raw: &str) -> Option<SimpleSelector> {
    let bytes = raw.as_bytes();
    let mut index = 0usize;
    let mut tag = None;
    let mut id = None;
    let mut classes = Vec::new();
    if bytes.first().is_some_and(|byte| *byte == b'*') {
        index += 1;
    } else if bytes
        .first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'-'))
    {
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'-'))
        {
            index += 1;
        }
        tag = Some(raw[start..index].to_ascii_lowercase());
    }
    while index < bytes.len() {
        let marker = bytes[index];
        if !matches!(marker, b'.' | b'#') {
            return None;
        }
        index += 1;
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'-'))
        {
            index += 1;
        }
        if start == index {
            return None;
        }
        let name = raw[start..index].to_owned();
        if marker == b'#' {
            if id.is_some() {
                return None;
            }
            id = Some(name);
        } else {
            classes.push(name);
        }
    }
    if tag.is_none() && id.is_none() && classes.is_empty() {
        return None;
    }
    Some(SimpleSelector { tag, id, classes })
}

fn strip_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] == b'/' && bytes.get(position + 1) == Some(&b'*') {
            position += 2;
            while position + 1 < bytes.len()
                && !(bytes[position] == b'*' && bytes[position + 1] == b'/')
            {
                position += 1;
            }
            position = (position + 2).min(bytes.len());
        } else {
            let ch = text[position..].chars().next().unwrap_or('\0');
            result.push(ch);
            position += ch.len_utf8();
        }
    }
    result
}

fn find_block_end(bytes: &[u8], start: usize) -> usize {
    let mut depth = 1usize;
    let mut position = start;
    while position < bytes.len() {
        match bytes[position] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return position;
                }
            }
            _ => {}
        }
        position += 1;
    }
    bytes.len()
}

fn skip_block(bytes: &[u8], start: usize) -> usize {
    let mut position = start;
    while position < bytes.len() && bytes[position] != b'{' && bytes[position] != b';' {
        position += 1;
    }
    if position < bytes.len() && bytes[position] == b'{' {
        find_block_end(bytes, position + 1) + 1
    } else {
        position + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse_document;

    fn element(source: &str) -> XmlElement {
        parse_document(source.as_bytes(), &crate::Limits::default()).expect("xml")
    }

    #[test]
    fn test_parse_rules_selectors_and_declarations() {
        let rules = parse_stylesheet(
            "rect { fill: red } .card, #hero text { stroke: blue; stroke-width: 2 }",
        );
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].declarations[0].name, "fill");
        assert_eq!(rules[1].selectors.len(), 2);
        assert_eq!(rules[1].declarations.len(), 2);
    }

    #[test]
    fn test_unsupported_and_invalid_rules_are_skipped() {
        let rules =
            parse_stylesheet("@media screen { rect { fill: red } } rect:hover { fill: blue }");
        assert!(rules.is_empty());
        let rules = parse_stylesheet("/* comment */ rect { fill: green }");
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn test_specificity_ordering() {
        let rules = parse_stylesheet("rect { fill: red } .a { fill: green } #x { fill: blue }");
        let root = element("<svg><rect id='x' class='a'/></svg>");
        let rect = match &root.children[0] {
            crate::xml::XmlNode::Element(element) => element,
            _ => unreachable!(),
        };
        let ancestors = vec![&root];
        let matched = matching_declarations(
            &rules,
            rect,
            &ancestors,
            &mut SelectorBudget {
                used: 0,
                max: usize::MAX,
            },
        )
        .expect("cascade");
        let colors: Vec<&str> = matched
            .iter()
            .map(|declaration| declaration.value.as_str())
            .collect();
        assert_eq!(colors, vec!["red", "green", "blue"]);
    }

    #[test]
    fn test_descendant_combinator_requires_ancestor() {
        let rules = parse_stylesheet("g rect { fill: red }");
        let root = element("<svg><g><rect class='a'/></g></svg>");
        let group = match &root.children[0] {
            crate::xml::XmlNode::Element(element) => element,
            _ => unreachable!(),
        };
        let rect = match &group.children[0] {
            crate::xml::XmlNode::Element(element) => element,
            _ => unreachable!(),
        };
        let ancestors = vec![&root, group];
        assert!(
            !matching_declarations(
                &rules,
                rect,
                &ancestors,
                &mut SelectorBudget {
                    used: 0,
                    max: usize::MAX,
                },
            )
            .expect("cascade")
            .is_empty()
        );
        // Without the `g` ancestor the selector must not match.
        let root2 = element("<svg><rect class='a'/></svg>");
        let rect2 = match &root2.children[0] {
            crate::xml::XmlNode::Element(element) => element,
            _ => unreachable!(),
        };
        assert!(
            matching_declarations(
                &rules,
                rect2,
                &[&root2],
                &mut SelectorBudget {
                    used: 0,
                    max: usize::MAX,
                },
            )
            .expect("cascade")
            .is_empty()
        );
    }
}
