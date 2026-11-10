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

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::rule::Declaration;

/// Maximum nested `var()` substitutions.
pub const MAX_SUBSTITUTION_DEPTH: usize = 32;

/// Returns whether `value` contains a `var()` function (case-insensitive).
pub(crate) fn contains_var(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("var(")
}

/// Expands every `var()` reference in `value` using `custom`.
///
/// Returns `None` when a required variable is missing/cyclic without a fallback, or when the
/// substitution depth exceeds [`MAX_SUBSTITUTION_DEPTH`]; callers then treat the declaration
/// as invalid at computed-value time (inherited property → parent value, otherwise initial).
pub fn expand_variables(value: &str, custom: &HashMap<String, String>) -> Option<String> {
    let mut visiting = HashSet::new();
    expand_inner(value, custom, 0, &mut visiting)
}

fn expand_inner(
    value: &str,
    custom: &HashMap<String, String>,
    depth: usize,
    visiting: &mut HashSet<String>,
) -> Option<String> {
    if depth > MAX_SUBSTITUTION_DEPTH {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    let mut out = String::with_capacity(value.len());
    let mut index = 0usize;
    while index < value.len() {
        if let Some(relative) = lower[index..].find("var(") {
            let start = index + relative;
            out.push_str(&value[index..start]);
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
                            expand_inner(fallback, custom, depth + 1, visiting)
                        })
                    } else {
                        visiting.insert(name.to_string());
                        let result = expand_inner(raw, custom, depth + 1, visiting);
                        visiting.remove(name);
                        result.or_else(|| {
                            fallback.as_deref().and_then(|fallback| {
                                expand_inner(fallback, custom, depth + 1, visiting)
                            })
                        })
                    }
                } else {
                    fallback
                        .as_deref()
                        .and_then(|fallback| expand_inner(fallback, custom, depth + 1, visiting))
                }
            } else {
                None
            };
            out.push_str(&substituted?);
            index = end + 1;
            cursor = index;
            let _ = cursor;
        } else {
            out.push_str(&value[index..]);
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
/// Returns the declaration map (deterministic order) used by
/// [`crate::properties::ComputedStyle::compute`] plus the element's custom property map for
/// inheritance by descendants.
pub(crate) fn resolve_declarations(
    cascaded: &HashMap<String, &Declaration>,
    inherited: Option<&HashMap<String, String>>,
) -> (BTreeMap<String, Declaration>, HashMap<String, String>) {
    let mut custom: HashMap<String, String> = inherited.cloned().unwrap_or_default();
    // 1. Custom properties: `initial`/`unset` remove the declaration (custom props are
    //    inherited, so the inherited value wins); `inherit` keeps the inherited value.
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
    // 2. Substitute var() in normal declarations. Keep unexpanded shorthands for a second
    //    pass so a shorthand containing var() expands after substitution.
    let mut resolved: BTreeMap<String, Declaration> = BTreeMap::new();
    let mut pending_shorthands: Vec<(String, Declaration)> = Vec::new();
    let mut names: Vec<&String> = cascaded.keys().collect();
    names.sort();
    for name in names {
        if name.starts_with("--") {
            continue;
        }
        let declaration = cascaded[name];
        let value = declaration.raw_value.trim();
        if contains_var(value) {
            let Some(expanded) = expand_variables(value, &custom) else {
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
                resolved.insert(name.clone(), expanded_declaration);
            } else {
                pending_shorthands.push((name.clone(), expanded_declaration));
            }
        } else {
            resolved.insert(name.clone(), (*declaration).clone());
        }
    }
    // Shorthand expansions apply only where no explicit longhand won the cascade (simplified
    // CSS cascade order; documented limitation).
    for (_name, declaration) in pending_shorthands {
        for longhand in crate::shorthand::expand_declarations(std::slice::from_ref(&declaration)) {
            resolved.entry(longhand.name.clone()).or_insert(longhand);
        }
    }
    (resolved, custom)
}
