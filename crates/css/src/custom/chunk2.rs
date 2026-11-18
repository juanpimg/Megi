//! Sized submodule of the parent facade (verbatim move).

use super::*;

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
    // Bound the amplification (issue #687): a map beyond MAX_CUSTOM_PROPERTIES is not
    // carried forward, so a single document cannot force quadratic clone work.
    let mut custom_owned: Option<HashMap<String, String>> = own_custom.then(|| match inherited {
        Some(map) if map.len() <= MAX_CUSTOM_PROPERTIES => (**map).clone(),
        _ => HashMap::new(),
    });
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
        Some(map) if map.len() > MAX_CUSTOM_PROPERTIES => None,
        Some(map) => Some(Arc::new(map)),
        None => inherited.cloned(),
    };
    (resolved, custom_properties)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn var_substitution_skips_strings_and_other_functions() {
        let mut custom = HashMap::new();
        custom.insert("--x".to_string(), "1".to_string());
        let value = r#"myvar(--x) "var(--x)" 'var(--x)' var(--x)"#;
        assert_eq!(
            expand_variables(value, &custom).as_deref(),
            Some(r#"myvar(--x) "var(--x)" 'var(--x)' 1"#)
        );
    }

    #[test]
    fn escaped_quote_inside_a_fallback_keeps_the_comma_split_intact() {
        // #684: the fallback string contains an escaped quote, so the quoting state must
        // not close early — otherwise the comma lands inside the string and the whole
        // declaration is dropped as invalid.
        let mut custom = HashMap::new();
        custom.insert("--x".to_string(), "1".to_string());
        let double = r#"var(--missing, "a\"b,c")"#;
        assert_eq!(
            expand_variables(double, &custom)
                .map(|value| value.trim().to_string())
                .as_deref(),
            Some(r#""a\"b,c""#),
            "escaped double quote must not split the fallback"
        );
        let single = r#"var(--missing, 'a\'b,c')"#;
        assert_eq!(
            expand_variables(single, &custom)
                .map(|value| value.trim().to_string())
                .as_deref(),
            Some(r#"'a\'b,c'"#),
            "escaped single quote must not split the fallback"
        );
    }

    #[test]
    fn escaped_paren_inside_a_fallback_does_not_end_the_var_function() {
        // The parenthesis scan has the same escape rule: `\)` is literal text, not the
        // closing paren of `var(`.
        let mut custom = HashMap::new();
        custom.insert("--x".to_string(), "1".to_string());
        let value = r#"var(--missing, "a\)b") var(--x)"#;
        assert_eq!(
            expand_variables(value, &custom)
                .map(|value| value.trim().to_string())
                .as_deref(),
            Some(r#""a\)b" 1"#)
        );
    }

    #[test]
    fn escaped_quotes_do_not_leak_string_state() {
        let mut custom = HashMap::new();
        custom.insert("--x".to_string(), "1".to_string());
        let value = r#""a\" var(--x)" var(--x)"#;
        assert_eq!(
            expand_variables(value, &custom).as_deref(),
            Some(r#""a\" var(--x)" 1"#)
        );
    }
}

#[cfg(test)]
pub(crate) mod token_tests {
    use super::*;

    #[test]
    fn var_substitution_skips_hash_at_keyword_url_and_comments() {
        let mut custom = HashMap::new();
        custom.insert("--x".to_string(), "1".to_string());
        let value = r#"#var(--x) @var(--x) url(var(--x)) /* var(--x) */ var(--x)"#;
        assert_eq!(
            expand_variables(value, &custom).as_deref(),
            Some(r#"#var(--x) @var(--x) url(var(--x)) /* var(--x) */ 1"#)
        );
    }

    #[test]
    fn parsed_strings_stay_opaque_to_substitution() {
        let declarations = crate::parser::parse_list_of_declarations(
            "--x: 1; content: \"a\\\" var(--x)\" var(--x);",
        );

        let custom: HashMap<String, String> = declarations
            .iter()
            .filter(|declaration| declaration.name == "--x")
            .map(|declaration| ("--x".to_string(), declaration.raw_value.clone()))
            .collect();

        let value = &declarations[1].raw_value;

        assert_eq!(
            expand_variables(value, &custom).as_deref(),
            Some(r#""a\" var(--x)" 1"#)
        );
    }
}
