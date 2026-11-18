//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;


pub(crate) fn tokenize_selector_parts(input: &str) -> Vec<String> {
    if input.len() > MAX_SELECTOR_BYTES {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // A CSS escape is part of the current compound; never a combinator or a
            // separator. Copy it verbatim so `read_identifier` decodes it later.
            current.push(ch);
            match chars.peek().copied() {
                Some(h) if h.is_ascii_hexdigit() => {
                    let mut digits = 0;
                    while digits < 6 {
                        let Some(h) = chars.peek().copied().filter(|h| h.is_ascii_hexdigit())
                        else {
                            break;
                        };
                        current.push(h);
                        chars.next();
                        digits += 1;
                    }
                    if chars.peek().is_some_and(|c| c.is_whitespace())
                        && let Some(ws) = chars.next()
                    {
                        current.push(ws);
                    }
                }
                Some(next) => {
                    current.push(next);
                    chars.next();
                }
                None => {}
            }
            continue;
        }
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
        if parts.len() > MAX_SELECTOR_PARTS {
            return Vec::new();
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
pub(crate) const DYNAMIC_PSEUDO_CLASSES: &[&str] = &[
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


pub(crate) fn parse_compound_selector(tokens: &mut SelectorTokens) -> Option<CompoundSelector> {
    if tokens.is_empty() {
        return None;
    }
    let s = tokens.next_part()?;
    if matches!(s.as_str(), " " | ">" | "+" | "~") {
        return None;
    }

    let mut compound = CompoundSelector::default();
    let mut chars = s.chars().peekable();

    // Optional type selector or universal selector prefix.
    if chars
        .peek()
        .copied()
        .is_some_and(|c| c.is_alphabetic() || c == '*' || c == '_' || c == '\\')
    {
        if chars.peek() == Some(&'*') {
            chars.next();
            compound.tag_name = Some("*".to_string());
        } else {
            compound.tag_name = Some(read_identifier(&mut chars)?);
        }
    }

    while let Some(ch) = chars.next() {
        match ch {
            '#' => {
                let id = read_identifier(&mut chars)?;
                compound.id_count = compound.id_count.saturating_add(1);
                if let Some(existing) = &compound.id {
                    // `#a#b` requires two different ids on one element, which is impossible.
                    if existing != &id {
                        compound.never_matches = true;
                    }
                } else {
                    compound.id = Some(id);
                }
            }
            '.' => {
                let class = read_identifier(&mut chars)?;
                compound.classes.push(class);
            }
            ':' => {
                let is_pseudo_element = chars.peek() == Some(&':');
                if is_pseudo_element {
                    chars.next();
                }
                let pseudo = read_identifier(&mut chars)?;

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

                let name = pseudo.to_ascii_lowercase();
                if is_pseudo_element {
                    if pseudo.eq_ignore_ascii_case("backdrop") && argument.is_none() {
                        compound.pseudo_classes.push(PseudoClass::Backdrop);
                    } else if argument.is_none() {
                        let parsed = match name.as_str() {
                            "before" => Some(PseudoElement::Before),
                            "after" => Some(PseudoElement::After),
                            _ => None,
                        };
                        if let Some(pe) = parsed {
                            if compound.pseudo_element.is_some() {
                                compound.never_matches = true;
                            } else {
                                compound.pseudo_element = Some(pe);
                            }
                            continue;
                        }
                        // Other pseudo-elements (`::first-line`, `::placeholder`, ...) keep
                        // parsing cleanly while matching nothing directly.
                        compound.pseudo_classes.push(PseudoClass::Never);
                    } else {
                        compound.pseudo_classes.push(PseudoClass::Never);
                    }
                    continue;
                }

                let pseudo_class = match name.as_str() {
                    "first-child" if argument.is_none() => PseudoClass::FirstChild,
                    "last-child" if argument.is_none() => PseudoClass::LastChild,
                    "only-child" if argument.is_none() => PseudoClass::OnlyChild,
                    "empty" if argument.is_none() => PseudoClass::Empty,
                    "root" if argument.is_none() => PseudoClass::Root,
                    "first-of-type" if argument.is_none() => PseudoClass::FirstOfType,
                    "last-of-type" if argument.is_none() => PseudoClass::LastOfType,
                    "only-of-type" if argument.is_none() => PseudoClass::OnlyOfType,
                    "nth-child" => PseudoClass::NthChild(parse_an_plus_b(argument.as_deref()?)?),
                    "nth-last-child" => {
                        PseudoClass::NthLastChild(parse_an_plus_b(argument.as_deref()?)?)
                    }
                    "nth-of-type" => PseudoClass::NthOfType(parse_an_plus_b(argument.as_deref()?)?),
                    "nth-last-of-type" => {
                        PseudoClass::NthLastOfType(parse_an_plus_b(argument.as_deref()?)?)
                    }
                    "is" => PseudoClass::Is(parse_selector_argument(&argument)?),
                    "where" => PseudoClass::Where(parse_selector_argument(&argument)?),
                    "not" => PseudoClass::Not(parse_selector_argument(&argument)?),
                    "has" => PseudoClass::Has(parse_relative_selector_list(argument.as_deref()?)?),
                    "focus" if argument.is_none() => PseudoClass::Focus,
                    "focus-visible" if argument.is_none() => PseudoClass::FocusVisible,
                    "hover" if argument.is_none() => PseudoClass::Hover,
                    "active" if argument.is_none() => PseudoClass::Active,
                    "checked" if argument.is_none() => PseudoClass::Checked,
                    "disabled" if argument.is_none() => PseudoClass::Disabled,
                    "enabled" if argument.is_none() => PseudoClass::Enabled,
                    "read-only" if argument.is_none() => PseudoClass::ReadOnly,
                    "read-write" if argument.is_none() => PseudoClass::ReadWrite,
                    "required" if argument.is_none() => PseudoClass::Required,
                    "placeholder-shown" if argument.is_none() => PseudoClass::PlaceholderShown,
                    "modal" if argument.is_none() => PseudoClass::Modal,
                    "lang" => PseudoClass::Lang(parse_language_range(argument.as_deref()?)?),
                    // Legacy CSS2 single-colon syntax for `:before` and `:after`.
                    "before" if argument.is_none() => {
                        if compound.pseudo_element.is_some() {
                            compound.never_matches = true;
                        } else {
                            compound.pseudo_element = Some(PseudoElement::Before);
                        }
                        continue;
                    }
                    "after" if argument.is_none() => {
                        if compound.pseudo_element.is_some() {
                            compound.never_matches = true;
                        } else {
                            compound.pseudo_element = Some(PseudoElement::After);
                        }
                        continue;
                    }
                    // Legacy single-colon pseudo-element syntax (`:first-line`, `:first-letter`)
                    // matches nothing directly.
                    "first-line" | "first-letter" if argument.is_none() => PseudoClass::Never,
                    _ if DYNAMIC_PSEUDO_CLASSES.contains(&name.as_str()) && argument.is_none() => {
                        PseudoClass::Never
                    }
                    // Unknown pseudo-classes invalidate the selector list instead of
                    // over-matching every element (F13).
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


/// Parses the argument of `:is()`, `:where()` or `:not()` into a selector list.
/// Parses the first language range of `:lang(en, fr)`, trimming quotes and normalising case.
pub(crate) fn parse_language_range(input: &str) -> Option<String> {
    let first = input.split(',').next()?.trim();
    let unquoted = first
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            first
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(first);
    let range = unquoted.trim().to_ascii_lowercase();
    if range.is_empty()
        || !range
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return None;
    }
    Some(range)
}
