//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;


/// Matches `compounds[index]` relative to `current` through `combinator`, recursing forward.
pub(crate) fn relative_chain_matches(
    doc: &Document,
    current: NodeId,
    compounds: &[(CompoundSelector, Option<Combinator>)],
    index: usize,
    combinator: Combinator,
) -> bool {
    if !spend_match_step() {
        return false;
    }
    let (compound, next) = &compounds[index];
    let is_last = index + 1 == compounds.len();

    // Test each candidate as it is visited (lazy DFS): this spends the match budget once
    // per candidate instead of once per call, and removes the old fixed 10,000-node cut
    // that silently produced false negatives on large documents (#1133).
    let check = |candidate: NodeId| -> bool {
        if !spend_match_step() {
            return false;
        }
        if !compound.matches(doc, candidate) {
            return false;
        }
        if is_last {
            return true;
        }
        if let Some(next_combinator) = next {
            return relative_chain_matches(doc, candidate, compounds, index + 1, *next_combinator);
        }
        false
    };
    let exhausted = || MATCH_EXHAUSTED.with(|flag| flag.get());

    match combinator {
        Combinator::Child => {
            let mut child = doc.node(current).first_child;
            while let Some(child_id) = child {
                if check(child_id) {
                    return true;
                }
                if exhausted() {
                    return false;
                }
                child = doc.node(child_id).next_sibling;
            }
        }
        Combinator::Descendant => {
            let mut stack = Vec::new();
            let mut child = doc.node(current).first_child;
            while let Some(child_id) = child {
                stack.push(child_id);
                child = doc.node(child_id).next_sibling;
            }
            while let Some(candidate) = stack.pop() {
                if check(candidate) {
                    return true;
                }
                if exhausted() {
                    return false;
                }
                let mut child = doc.node(candidate).first_child;
                while let Some(child_id) = child {
                    stack.push(child_id);
                    child = doc.node(child_id).next_sibling;
                }
            }
        }
        Combinator::NextSibling => {
            let mut sibling = doc.node(current).next_sibling;
            while let Some(sibling_id) = sibling {
                if exhausted() {
                    return false;
                }
                if doc.node(sibling_id).as_element().is_some() && check(sibling_id) {
                    return true;
                }
                sibling = doc.node(sibling_id).next_sibling;
            }
        }
        Combinator::SubsequentSibling => {
            let mut sibling = doc.node(current).next_sibling;
            while let Some(sibling_id) = sibling {
                if exhausted() {
                    return false;
                }
                if doc.node(sibling_id).as_element().is_some() && check(sibling_id) {
                    return true;
                }
                sibling = doc.node(sibling_id).next_sibling;
            }
        }
    }
    false
}


/// A comma-separated list of complex selectors (`h1, h2, .title`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectorList {
    pub selectors: Vec<ComplexSelector>,
}


impl SelectorList {
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        with_match_budget(|| self.matches_inner(doc, node_id))
    }

    pub(crate) fn matches_inner(&self, doc: &Document, node_id: NodeId) -> bool {
        self.selectors.iter().any(|sel| sel.matches(doc, node_id))
    }

    /// If this selector list matches the given element, returns the maximum specificity
    /// among all the complex selectors in the list that matched.
    pub fn matching_specificity(&self, doc: &Document, node_id: NodeId) -> Option<Specificity> {
        // Share one budget across the alternative selectors of the list.
        let _scope = match_scope();
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
    // Functional pseudo-class arguments recurse through this same entry point; past the cap
    // the list is dropped as invalid, which also invalidates the declaration that uses it.
    let Some(_nesting) = ParserNestingGuard::enter() else {
        return SelectorList::default();
    };
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


/// Decodes one CSS escape whose introducing `\` has already been consumed
/// (CSS Syntax §4.3.7). Returns `U+FFFD` for an invalid or out-of-range escape.
pub(crate) fn consume_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<char> {
    match chars.peek().copied() {
        None => Some('\u{FFFD}'),
        // A newline after the backslash is not a valid escape: do not consume it.
        Some(c) if c == '\n' || c == '\r' || c == '\u{c}' => Some('\u{FFFD}'),
        Some(c) if c.is_ascii_hexdigit() => {
            let mut value: u32 = 0;
            let mut digits = 0;
            while digits < 6 {
                let Some(h) = chars.peek().copied().filter(|h| h.is_ascii_hexdigit()) else {
                    break;
                };
                chars.next();
                value = value * 16 + h.to_digit(16).unwrap_or(0);
                digits += 1;
            }
            // One whitespace character terminates the hex escape and is consumed.
            if chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
            if value == 0 || value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
                Some('\u{FFFD}')
            } else {
                char::from_u32(value)
            }
        }
        Some(c) => {
            chars.next();
            Some(c)
        }
    }
}


/// Reads a CSS identifier from the raw selector text, decoding escapes, so that an
/// author's `md\:flex` becomes the class name `md:flex` instead of `md` followed by a
/// `:flex` pseudo-class. Used for type names, ids, classes and pseudo-class names.
pub(crate) fn read_identifier(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let mut out = String::new();
    loop {
        match chars.peek().copied() {
            Some('\\') => {
                chars.next();
                if let Some(decoded) = consume_escape(chars) {
                    out.push(decoded);
                }
            }
            Some(c) if c.is_alphanumeric() || c == '-' || c == '_' => {
                chars.next();
                out.push(c);
            }
            _ => break,
        }
    }
    if out.is_empty() { None } else { Some(out) }
}


/// Splits a selector list on top-level commas, ignoring commas nested in blocks or strings.
pub(crate) fn split_top_level_commas(input: &str) -> Vec<String> {
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


/// Forward-only cursor over selector parts. `Vec::remove(0)` shifts every
/// remaining `String` header, making an n-part selector O(n²) (#1414).
pub(crate) struct SelectorTokens {
    pub(crate) parts: Vec<String>,
    pub(crate) pos: usize,
}


impl SelectorTokens {
    pub(crate) fn new(parts: Vec<String>) -> Self {
        Self { parts, pos: 0 }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pos >= self.parts.len()
    }

    pub(crate) fn next_part(&mut self) -> Option<String> {
        let part = self.parts.get(self.pos)?.clone();
        self.pos += 1;
        Some(part)
    }
}


/// Parses a single complex selector string.
pub(crate) fn parse_complex_selector(input: &str) -> Option<ComplexSelector> {
    let mut tokens = SelectorTokens::new(tokenize_selector_parts(input));
    let mut compounds = Vec::new();

    loop {
        let compound = parse_compound_selector(&mut tokens)?;
        if tokens.is_empty() {
            compounds.push((compound, None));
            break;
        }
        // Pseudo-elements are only valid on the originating (rightmost) compound.
        if compound.pseudo_element.is_some() {
            return None;
        }
        let combinator = match tokens.next_part()?.as_str() {
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
pub(crate) fn push_selector_chunk(parts: &mut Vec<String>, current: &mut String) {
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
/// Anti-DoS caps for a single selector: beyond these the selector is rejected
/// instead of spending O(n²) time in the old `Vec::remove(0)` parser (#1414).
pub(crate) const MAX_SELECTOR_BYTES: usize = 8 * 1024;

pub(crate) const MAX_SELECTOR_PARTS: usize = 64;
