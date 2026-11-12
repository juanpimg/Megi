//! CSS custom properties (`--x`) and `var()` substitution.
//!
//! Custom properties are cascaded like normal declarations (they appear in
//! [`crate::cascade::resolve_cascaded_properties`] with their `--` name preserved) and are
//! **inherited**. `resolve_declarations` merges the parent's custom properties with the
//! element's own, then substitutes `var(--name[, fallback])` in every normal declaration
//! before the longhands are computed. A shorthand whose value contains `var()` is kept
//! unexpanded through the cascade and expanded here after substitution.
//!
//! Limits: at most [`MAX_SUBSTITUTION_DEPTH`] nested substitutions; cycles are detected and
//! fall back to the `var()` fallback (or make the property invalid → inherited/initial), never
//! panic.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::rule::Declaration;

/// Maximum nested `var()` substitutions.
pub const MAX_SUBSTITUTION_DEPTH: usize = 32;

/// Maximum bytes a single `var()` expansion may produce (about 64× the input, clamped).
///
/// Every reference re-expands its variable, so a chain like `--aN: var(--aN-1) var(--aN-1)`
/// is exponential: 31 doublings asked for ~4 GiB. The budget bounds the work and memory.
pub const MAX_SUBSTITUTION_BYTES: usize = 1024 * 1024;

/// Result of [`resolve_declarations`]: owned declarations + shared custom-property map.
type ResolvedDeclarations = (
    Vec<(String, Declaration)>,
    Option<Arc<HashMap<String, String>>>,
);

/// Returns whether `value` contains a `var()` function (case-insensitive).
///
/// Allocation-free: the hot path (`compute` for documents without custom properties) must
/// not lowercase the whole value for every declaration of every element.
pub(crate) fn contains_var(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 4 {
        return false;
    }
    bytes
        .windows(4)
        .any(|window| window[..3].eq_ignore_ascii_case(b"var") && window[3] == b'(')
}

/// Returns whether style computation needs the custom-property machinery for this element.
///
/// `false` means: no local `--*` declarations and no declaration value references `var()`.
/// In that case the parent's [`Arc`] custom-property map is reused as-is (cheap clone) and
/// no declaration is cloned, sorted or scanned for substitution.
pub(crate) fn needs_resolution(cascaded: &HashMap<String, &Declaration>) -> bool {
    cascaded.keys().any(|name| name.starts_with("--"))
        || cascaded
            .values()
            .any(|declaration| contains_var(&declaration.raw_value))
}

/// Expands every `var()` reference in `value` using `custom`.
///
/// Returns `None` when a required variable is missing/cyclic without a fallback, or when the
/// substitution depth exceeds [`MAX_SUBSTITUTION_DEPTH`]; callers then treat the declaration
/// as invalid at computed-value time (inherited property → parent value, otherwise initial).
pub fn expand_variables(value: &str, custom: &HashMap<String, String>) -> Option<String> {
    let mut visiting = HashSet::new();
    let mut budget = value
        .len()
        .saturating_mul(64)
        .clamp(4096, MAX_SUBSTITUTION_BYTES);
    expand_inner(value, custom, 0, &mut visiting, &mut budget)
}

/// Reserves `bytes` from the substitution budget (returns false when exhausted).
fn spend(budget: &mut usize, bytes: usize) -> bool {
    if bytes > *budget {
        return false;
    }
    *budget -= bytes;
    true
}

fn expand_inner(
    value: &str,
    custom: &HashMap<String, String>,
    depth: usize,
    visiting: &mut HashSet<String>,
    budget: &mut usize,
) -> Option<String> {
    if depth > MAX_SUBSTITUTION_DEPTH {
        return None;
    }
    // Charge the work itself, not only the emitted bytes: a chain that expands to the empty
    // string (e.g. `var(--missing,)`) used to cost zero and recursed ~2^30 times.
    if !spend(budget, 1) {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    let mut out = String::with_capacity(value.len());
    let mut index = 0usize;
    while index < value.len() {
        if let Some(relative) = lower[index..].find("var(") {
            let start = index + relative;
            let literal = &value[index..start];
            if !spend(budget, literal.len()) {
                return None;
            }
            out.push_str(literal);
            // Find the matching closing parenthesis (quotes and nesting aware).
            let mut cursor = start + 4;
            let mut depth_parens = 0usize;
            let mut quote: Option<char> = None;
            let mut end = None;
            for (offset, ch) in value[cursor..].char_indices() {
                match quote {
                    Some(active) => {
                        if ch == active {
                            quote = None;
                        }
                    }
                    None => match ch {
                        '\'' | '"' => quote = Some(ch),
                        '(' => depth_parens += 1,
                        ')' if depth_parens == 0 => {
                            end = Some(cursor + offset);
                            break;
                        }
                        ')' => depth_parens -= 1,
                        _ => {}
                    },
                }
            }
            let Some(end) = end else {
                // Unbalanced var(): invalid value.
                return None;
            };
            let inner = &value[cursor..end];
            let (name, fallback) = split_first_comma(inner);
            let name = name.trim();
            let substituted = if name.starts_with("--") && !name.is_empty() {
                if let Some(raw) = custom.get(name) {
                    if visiting.contains(name) {
                        fallback.as_deref().and_then(|fallback| {
                            expand_inner(fallback, custom, depth + 1, visiting, budget)
                        })
                    } else {
                        visiting.insert(name.to_string());
                        let result = expand_inner(raw, custom, depth + 1, visiting, budget);
                        visiting.remove(name);
                        result.or_else(|| {
                            fallback.as_deref().and_then(|fallback| {
                                expand_inner(fallback, custom, depth + 1, visiting, budget)
                            })
                        })
                    }
                } else {
                    fallback.as_deref().and_then(|fallback| {
                        expand_inner(fallback, custom, depth + 1, visiting, budget)
                    })
                }
            } else {
                None
            };
            let substituted = substituted?;
            // The recursive expansion already charged the substituted bytes; charging them
            // again here rejected legitimate values larger than half the budget.
            out.push_str(&substituted);
            index = end + 1;
            cursor = index;
            let _ = cursor;
        } else {
            let tail = &value[index..];
            if !spend(budget, tail.len()) {
                return None;
            }
            out.push_str(tail);
            break;
        }
    }
    Some(out)
}

/// Splits `arg, fallback` on the first top-level comma (quotes/parentheses aware).
fn split_first_comma(inner: &str) -> (String, Option<String>) {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for (offset, ch) in inner.char_indices() {
        match quote {
            Some(active) => {
                if ch == active {
                    quote = None;
                }
            }
            None => match ch {
                '\'' | '"' => quote = Some(ch),
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    return (
                        inner[..offset].to_string(),
                        Some(inner[offset + 1..].to_string()),
                    );
                }
                _ => {}
            },
        }
    }
    (inner.to_string(), None)
}

fn normalize_custom_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("initial")
        || trimmed.eq_ignore_ascii_case("unset")
        || trimmed.eq_ignore_ascii_case("inherit")
    {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Merges inherited/cascaded custom properties and expands `var()` in all declarations.
///
/// Returns the owned, name-sorted declarations used by
/// [`crate::properties::ComputedStyle::compute`] plus the element's custom-property map for
/// descendants. The map is `None` when the element (and its ancestors) declare no custom
/// properties; otherwise it is shared through [`Arc`] (copy-on-write only when this element
/// adds/removes its own `--*` declarations). Callers must use
/// [`needs_resolution`] first and skip this function entirely for the common case.
pub(crate) fn resolve_declarations(
    cascaded: &HashMap<String, &Declaration>,
    inherited: Option<&Arc<HashMap<String, String>>>,
) -> ResolvedDeclarations {
    let own_custom = cascaded.keys().any(|name| name.starts_with("--"));
    // Copy-on-write: clone the inherited map only when the element declares custom props.
    let mut custom_owned: Option<HashMap<String, String>> =
        own_custom.then(|| inherited.map(|map| (**map).clone()).unwrap_or_default());
    if let Some(custom) = custom_owned.as_mut() {
        for (name, declaration) in cascaded {
            if !name.starts_with("--") {
                continue;
            }
            match normalize_custom_value(&declaration.raw_value) {
                Some(value) => {
                    custom.insert(name.clone(), value);
                }
                None => {
                    custom.remove(name);
                }
            }
        }
    }
    let lookup: Option<&HashMap<String, String>> = custom_owned
        .as_ref()
        .or_else(|| inherited.map(|map| map.as_ref()));
    let empty_map = HashMap::new();
    let lookup_map = lookup.unwrap_or(&empty_map);

    // 2. Substitute var() in normal declarations. Keep unexpanded shorthands for a second
    //    pass so a shorthand containing var() expands after substitution.
    let mut names: Vec<&String> = cascaded.keys().collect();
    names.sort();
    let mut resolved: Vec<(String, Declaration)> = Vec::with_capacity(cascaded.len());
    let mut pending_shorthands: Vec<Declaration> = Vec::new();
    for name in names {
        if name.starts_with("--") {
            continue;
        }
        let declaration = cascaded[name];
        let value = declaration.raw_value.trim();
        if contains_var(value) {
            let Some(expanded) = expand_variables(value, lookup_map) else {
                continue; // invalid at computed-value time
            };
            let expanded_declaration = Declaration {
                name: name.clone(),
                value: Vec::new(),
                important: declaration.important,
                raw_value: expanded,
            };
            let longhands =
                crate::shorthand::expand_declarations(std::slice::from_ref(&expanded_declaration));
            if longhands.len() == 1 && longhands[0].name == *name {
                resolved.push((name.clone(), expanded_declaration));
            } else {
                pending_shorthands.push(expanded_declaration);
            }
        } else {
            resolved.push((name.clone(), (*declaration).clone()));
        }
    }
    // Shorthand expansions apply only where no explicit longhand won the cascade (simplified
    // CSS cascade order; documented limitation).
    for declaration in pending_shorthands {
        for longhand in crate::shorthand::expand_declarations(std::slice::from_ref(&declaration)) {
            if !resolved.iter().any(|(name, _)| *name == longhand.name) {
                resolved.push((longhand.name.clone(), longhand));
            }
        }
    }
    let custom_properties = match custom_owned {
        Some(map) if map.is_empty() => None,
        Some(map) => Some(Arc::new(map)),
        None => inherited.cloned(),
    };
    (resolved, custom_properties)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitution_budget_stops_exponential_expansion() {
        let mut custom = HashMap::new();
        custom.insert("--a0".to_string(), "x".to_string());
        for index in 1..=31 {
            custom.insert(
                format!("--a{index}"),
                format!("var(--a{}) var(--a{})", index - 1, index - 1),
            );
        }
        assert!(
            expand_variables("var(--a31)", &custom).is_none(),
            "exponential expansion is cut by the byte budget"
        );
        assert_eq!(
            expand_variables("var(--a3)", &custom).as_deref(),
            Some("x x x x x x x x")
        );
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn empty_output_recursion_is_bounded() {
        let mut custom = HashMap::new();
        custom.insert("--a0".to_string(), "var(--missing,)".to_string());
        for index in 1..=30 {
            custom.insert(
                format!("--a{index}"),
                format!("var(--a{})var(--a{})", index - 1, index - 1),
            );
        }
        assert!(
            expand_variables("var(--a30)", &custom).is_none(),
            "an empty-output doubling chain must stop at the byte budget"
        );
    }

    #[test]
    fn a_single_large_custom_value_still_expands() {
        let mut custom = HashMap::new();
        let value = "g".repeat(600);
        custom.insert("--long".to_string(), value.clone());
        assert_eq!(
            expand_variables("var(--long)", &custom).as_deref(),
            Some(value.as_str())
        );
    }
}
