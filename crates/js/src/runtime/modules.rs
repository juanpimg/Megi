use super::*;

pub(crate) fn transform_module(source: &str, url: &str) -> Result<ParsedModule, JsError> {
    let quoted_url = module_quote(url);
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut body_lines: Vec<String> = Vec::new();
    for raw_line in source.lines() {
        let line = raw_line.replace("import.meta.url", &quoted_url);
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            let rest = trimmed
                .trim_start_matches("import ")
                .trim()
                .trim_end_matches(';')
                .trim();
            if rest.starts_with('"') || rest.starts_with('\'') {
                let specifier = rest.trim_matches(['"', '\'']).to_string();
                imports.push(ImportSpec {
                    specifier,
                    binding: ImportBinding::SideEffect,
                });
                continue;
            }
            let Some((head, specifier)) = import_specifier(rest) else {
                return Err(JsError::ModuleSyntax(format!(
                    "unsupported import statement in {url}: {trimmed}"
                )));
            };
            let binding = if head.is_empty() {
                ImportBinding::SideEffect
            } else if head.starts_with("* as ") {
                ImportBinding::Namespace(
                    head.strip_prefix("* as ")
                        .map(str::trim)
                        .unwrap_or_else(|| head.trim())
                        .to_string(),
                )
            } else if head.starts_with('{') {
                ImportBinding::Named(parse_import_named_list(&head))
            } else if let Some((default_name, named)) = head.split_once(',') {
                let mut bindings = vec![(default_name.trim().to_string(), "default".to_string())];
                bindings.extend(parse_import_named_list(named));
                ImportBinding::Named(bindings)
            } else {
                ImportBinding::Default(head.to_string())
            };
            imports.push(ImportSpec { specifier, binding });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("export ") {
            let rest = rest.trim();
            if let Some(default) = rest.strip_prefix("default ") {
                let default = default.trim().trim_end_matches(';').trim();
                body_lines.push(format!("const __megi_default = ({default});"));
                exports.push(("default".to_string(), "__megi_default".to_string()));
                continue;
            }
            if rest.starts_with('{') {
                let mapping = parse_named_list(rest);
                for (local, exported) in mapping {
                    exports.push((exported, local));
                }
                continue;
            }
            let names = export_declared_names(rest);
            if names.is_empty() {
                return Err(JsError::ModuleSyntax(format!(
                    "unsupported export statement in {url}: {trimmed}"
                )));
            }
            for name in names {
                exports.push((name.clone(), name));
            }
            body_lines.push(rest.to_string());
            continue;
        }
        body_lines.push(line);
    }
    let mut body = body_lines.join("\n");
    let dynamic_imports = rewrite_dynamic_imports(&mut body)?;
    Ok(ParsedModule {
        imports,
        body,
        exports,
        dynamic_imports,
    })
}

/// Rewrites literal `import("specifier")` calls into `__MEGI_DYNAMIC_k__` placeholders and
/// returns the specifiers. Non-literal arguments are a documented subset limitation.
pub(crate) fn rewrite_dynamic_imports(body: &mut String) -> Result<Vec<String>, JsError> {
    let mut specifiers: Vec<String> = Vec::new();
    let mut output = String::with_capacity(body.len());
    let mut rest = body.as_str();
    while let Some(index) = rest.find("import") {
        let after = &rest[index + "import".len()..];
        let mut spaces = after.len() - after.trim_start().len();
        let trimmed = after.trim_start();
        if !trimmed.starts_with('(') {
            output.push_str(&rest[..index + "import".len()]);
            rest = after;
            continue;
        }
        spaces += 0;
        let mut inner = &trimmed[1..];
        inner = inner.trim_start();
        let Some(quote) = inner.chars().next().filter(|ch| *ch == '\'' || *ch == '"') else {
            return Err(JsError::DynamicImport(
                "only string-literal import() specifiers are supported".to_string(),
            ));
        };
        let Some(close) = inner[1..].find(quote) else {
            return Err(JsError::DynamicImport(
                "unterminated import() specifier".to_string(),
            ));
        };
        let specifier = inner[1..1 + close].to_string();
        let after_literal = &inner[1 + close + 1..];
        let Some(paren_offset) = after_literal.find(')') else {
            return Err(JsError::DynamicImport(
                "unterminated import() call".to_string(),
            ));
        };
        if !after_literal[..paren_offset].trim().is_empty() {
            return Err(JsError::DynamicImport(
                "only one string-literal argument is supported".to_string(),
            ));
        }
        let position = specifiers
            .iter()
            .position(|existing| *existing == specifier)
            .unwrap_or_else(|| {
                specifiers.push(specifier.clone());
                specifiers.len() - 1
            });
        output.push_str(&format!("__MEGI_DYNAMIC_{position}__"));
        rest = &after_literal[paren_offset + 1..];
        let _ = spaces;
    }
    output.push_str(rest);
    *body = output;
    Ok(specifiers)
}

/// Replaces the contents of JS strings, template literals and comments with spaces,
/// preserving byte length and newlines. Textual module scans then only see executable
/// code, so `await` or `import(` inside a string/comment cannot be misdetected (#1145).
/// Template substitutions (`${…}`) inside a template are masked too: this is part of the
/// documented module-preprocessor subset.
pub(crate) fn mask_js_literals(source: &str) -> String {
    let mut bytes = source.as_bytes().to_vec();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'/' && i + 1 < bytes.len() && (bytes[i + 1] == b'/' || bytes[i + 1] == b'*')
        {
            let block = bytes[i + 1] == b'*';
            bytes[i] = b' ';
            bytes[i + 1] = b' ';
            i += 2;
            if block {
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    if bytes[i] != b'\n' {
                        bytes[i] = b' ';
                    }
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    bytes[i] = b' ';
                    bytes[i + 1] = b' ';
                    i += 2;
                }
            } else {
                while i < bytes.len() && bytes[i] != b'\n' {
                    bytes[i] = b' ';
                    i += 1;
                }
            }
        } else if matches!(bytes[i], b'\'' | b'"' | b'`') {
            let quote = bytes[i];
            bytes[i] = b' ';
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    bytes[i] = b' ';
                    i += 1;
                    if i < bytes.len() {
                        bytes[i] = b' ';
                        i += 1;
                    }
                    continue;
                }
                if bytes[i] == quote {
                    bytes[i] = b' ';
                    i += 1;
                    break;
                }
                if bytes[i] != b'\n' {
                    bytes[i] = b' ';
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    // Every replaced byte is ASCII, so masking cannot break UTF-8 sequences.
    String::from_utf8(bytes).expect("masking preserves UTF-8")
}

/// Detects top-level `await` (the synchronous module evaluator cannot suspend).
pub(crate) fn module_has_top_level_await(body: &str) -> bool {
    let masked = mask_js_literals(body);
    let mut depth = 0i32;
    for line in masked.lines() {
        let trimmed = line.trim();
        if depth <= 0
            && (trimmed.starts_with("await ")
                || trimmed.starts_with("export default await ")
                || trimmed.contains("= await "))
        {
            return true;
        }
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
    }
    false
}

pub(crate) fn module_error(error: boa_engine::JsError) -> JsError {
    classify_error(&error.to_string())
}

pub(crate) fn resolve_module(
    context: &Context,
    specifier: &str,
    base: &str,
) -> Result<String, JsError> {
    // Clone the Arc provider out of the borrow before invoking it: a resolver may
    // re-enter JavaScript, which must not borrow the host again.
    let (import_target, resolver) = {
        let host = host_ref(context).map_err(|error| JsError::Internal(error.to_string()))?;
        (
            host.import_map
                .as_ref()
                .and_then(|map| map.resolve_with_base(specifier, Some(base))),
            host.module_resolver.clone(),
        )
    };
    if let Some(target) = import_target {
        let Some(target) = target else {
            return Err(JsError::ImportMapBlocked(format!(
                "{specifier} (base {base})"
            )));
        };
        if !target.contains("://")
            && !target.starts_with("data:")
            && let Some(resolver) = &resolver
            && let Some(abs) = resolver(&target, base)?
        {
            return Ok(abs);
        }
        return Ok(target);
    }
    let resolved = match &resolver {
        Some(resolver) => resolver(specifier, base)?,
        None => None,
    };
    resolved.ok_or_else(|| JsError::ModuleNotFound(format!("{specifier} (base {base})")))
}

pub(crate) fn load_module(context: &Context, url: &str) -> Result<String, JsError> {
    let loader = {
        host_ref(context)
            .map_err(|error| JsError::Internal(error.to_string()))?
            .module_loader
            .clone()
    };
    let loaded = match &loader {
        Some(loader) => Some(loader(url)?),
        None => None,
    };
    loaded.ok_or_else(|| JsError::ModuleNotFound(url.to_string()))
}

#[cfg(test)]
mod module_scan_tests {
    use super::*;

    #[test]
    pub(crate) fn await_in_strings_and_comments_is_not_top_level_await() {
        assert!(module_has_top_level_await("await Promise.resolve(1);"));
        assert!(!module_has_top_level_await(
            "var s = 'await Promise.resolve(1)';"
        ));
        assert!(!module_has_top_level_await(
            "// await Promise.resolve(1)\nvar x = 1;"
        ));
        assert!(!module_has_top_level_await(
            "/* = await foo(); */ var x = 1;"
        ));
        assert!(!module_has_top_level_await("var s = `= await foo()`;"));
    }
}
