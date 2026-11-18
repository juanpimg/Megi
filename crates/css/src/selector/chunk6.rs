//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;


/// Selectors 4 §12.1: the element's language is the `lang` of the nearest ancestor-or-self.
/// A range matches when it equals the language or is a hyphen-prefix of it, case-insensitively.
pub(crate) fn matches_language(doc: &Document, node_id: NodeId, range: &str) -> bool {
    let mut current = Some(node_id);
    while let Some(id) = current {
        if let Some(element) = doc.node(id).as_element()
            && let Some(lang) = element.get_attribute("lang")
        {
            let language = lang.trim().to_ascii_lowercase();
            return language == range || language.starts_with(&format!("{range}-"));
        }
        current = doc.node(id).parent;
    }
    false
}


pub(crate) fn parse_selector_argument(argument: &Option<String>) -> Option<SelectorList> {
    let selectors = parse_selectors(argument.as_deref()?);
    if selectors.selectors.is_empty() {
        None
    } else {
        Some(selectors)
    }
}


/// Parses the argument of `:has()`: a comma-separated list of relative selectors.
///
/// Each relative selector may start with `>`, `+` or `~` (descendant when omitted), e.g.
/// `:has(> img)`, `:has(p)`, `:has(+ .note)`. Invalid or unsupported selectors make the whole
/// `:has()` pseudo-class invalid.
pub(crate) fn parse_relative_selector_list(input: &str) -> Option<Vec<RelativeSelector>> {
    let parts = split_top_level_commas(input);
    if parts.is_empty() {
        return None;
    }
    let mut relatives = Vec::with_capacity(parts.len());
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        let (leading, rest) = match part.chars().next()? {
            '>' => (Combinator::Child, part[1..].trim()),
            '+' => (Combinator::NextSibling, part[1..].trim()),
            '~' => (Combinator::SubsequentSibling, part[1..].trim()),
            _ => (Combinator::Descendant, part),
        };
        if rest.is_empty() {
            return None;
        }
        relatives.push(RelativeSelector {
            leading,
            selector: parse_complex_selector(rest)?,
        });
    }
    Some(relatives)
}


pub(crate) fn parse_attribute_selector(spec: &str) -> Option<AttributeSelector> {
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
pub(crate) fn is_valid_attribute_name(name: &str) -> bool {
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
pub(crate) fn split_case_flag(value: &str) -> Option<(&str, bool)> {
    let (rest, flag) = value.trim_end().rsplit_once(char::is_whitespace)?;
    let case_insensitive = match flag {
        "i" | "I" => true,
        "s" | "S" => false,
        _ => return None,
    };
    Some((rest.trim_end(), case_insensitive))
}


/// Removes the surrounding quotes of an attribute selector value, validating unquoted values.
pub(crate) fn strip_attribute_value_quotes(value: &str) -> Option<String> {
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


#[cfg(test)]
pub(crate) mod id_list_tests {
    use super::*;
    use megi_core::Attribute;

    fn doc_with_id(id: &str) -> (Document, NodeId) {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", Vec::new());
        doc.append_child(doc.root(), html);
        let div = doc.create_element(Namespace::Html, "div", vec![Attribute::new("id", id)]);
        doc.append_child(html, div);
        (doc, div)
    }

    #[test]
    fn conflicting_ids_never_match() {
        let (doc, node) = doc_with_id("a");
        assert!(
            !parse_selectors("#a#b").matches(&doc, node),
            "an element has a single id attribute"
        );
        assert!(
            parse_selectors("#a#a").matches(&doc, node),
            "repeating the same id is redundant but matchable"
        );
        assert!(!parse_selectors("div#b#c").matches(&doc, node));
    }

    #[test]
    fn every_id_counts_for_specificity() {
        let list = parse_selectors("#a#b");
        assert_eq!(list.max_specificity(), Specificity::new(2, 0, 0));
        assert_eq!(
            parse_selectors("#a#a").max_specificity(),
            Specificity::new(2, 0, 0)
        );
        assert_eq!(
            parse_selectors("div#a").max_specificity(),
            Specificity::new(1, 0, 1)
        );
    }
}


#[cfg(test)]
pub(crate) mod sibling_cache_tests {
    use super::*;

    #[test]
    fn nth_of_type_reuses_one_sibling_table() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", Vec::new());
        doc.append_child(doc.root(), body);
        for _ in 0..500 {
            let item = doc.create_element(Namespace::Html, "li", Vec::new());
            doc.append_child(body, item);
        }
        let selector = parse_selectors("li:nth-of-type(odd)");
        let _scope = document_scope();
        let matched = doc
            .children(body)
            .filter(|node| selector.matches(&doc, *node))
            .count();
        assert_eq!(matched, 250);
        assert_eq!(
            SIBLING_TABLES_BUILT.with(|count| count.get()),
            1,
            "one table for the parent, reused by every child"
        );
    }
}


#[cfg(test)]
pub(crate) mod specificity_overflow_tests {
    use super::Specificity;

    #[test]
    fn addition_saturates_instead_of_overflowing() {
        let huge = Specificity::new(u32::MAX, u32::MAX, u32::MAX);
        let one = Specificity::new(1, 1, 1);
        assert_eq!(huge + one, huge);
        let mut sum = Specificity::new(u32::MAX - 1, 0, 0);
        sum += Specificity::new(2, 0, 0);
        assert_eq!(sum, Specificity::new(u32::MAX, 0, 0));
    }
}


#[cfg(test)]
pub(crate) mod anplusb_tests {
    use super::*;

    #[test]
    fn anplusb_matches_does_not_overflow() {
        let formula = AnPlusB { a: -1, b: i32::MIN };
        // Must not panic; treat as no match rather than aborting.
        let _ = formula.matches(1);
        let _ = formula.matches(i32::MAX);
        let formula = AnPlusB { a: 1, b: i32::MIN };
        let _ = formula.matches(i32::MIN);
    }
}


#[cfg(test)]
pub(crate) mod style_pass_budget_tests {
    use super::*;

    #[test]
    fn a_style_pass_shares_the_step_budget_across_elements() {
        let _pass = style_pass_scope();
        {
            let _element = match_scope();
            // Pretend this element burned the whole shared budget.
            MATCH_STEPS.with(|steps| steps.set(0));
            assert!(!spend_match_step());
        }
        let _next_element = match_scope();
        // The next element of the same pass must not get a fresh 200k budget: that reset is
        // what turned a pathological stylesheet into quadratic work (#1413).
        assert!(!spend_match_step());
    }

    #[test]
    fn a_standalone_match_scope_still_gets_its_own_budget() {
        {
            let _element = match_scope();
            MATCH_STEPS.with(|steps| steps.set(0));
            assert!(!spend_match_step());
        }
        // Outside a style pass each top-level match/resolution keeps the documented
        // per-element budget.
        let _next = match_scope();
        assert!(spend_match_step());
    }

    #[test]
    fn nested_style_passes_install_the_budget_only_once() {
        let _outer = style_pass_scope();
        MATCH_STEPS.with(|steps| steps.set(1));
        let _inner = style_pass_scope();
        assert_eq!(MATCH_STEPS.with(|steps| steps.get()), 1);
    }
}


#[cfg(test)]
pub(crate) mod selector_budget_tests {
    use super::*;

    #[test]
    fn oversized_selectors_are_rejected_before_tokenizing() {
        let huge = "a".repeat(MAX_SELECTOR_BYTES + 1);
        assert!(tokenize_selector_parts(&huge).is_empty());

        let many = "a ".repeat(MAX_SELECTOR_PARTS + 2);
        assert!(many.len() < MAX_SELECTOR_BYTES);
        assert!(tokenize_selector_parts(&many).is_empty());

        assert!(!tokenize_selector_parts("a>b").is_empty());
    }
}
