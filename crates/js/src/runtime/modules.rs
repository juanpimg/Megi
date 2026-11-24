use super::*;

/// Splits a source line on `;` at top level (outside strings and brackets), keeping the
/// delimiter. Bundlers emit several module statements per line (#1879).
fn split_top_level_statements(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0usize;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            current.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ';' if depth == 0 => {
                current.push(ch);
                out.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

pub(crate) fn transform_module(source: &str, url: &str) -> Result<ParsedModule, JsError> {
    let quoted_url = module_quote(url);
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut body_lines: Vec<String> = Vec::new();
    for raw_line in source.lines() {
        // #1879: bundled modules often put several statements on one line
        // (`export const a=1;export const b=2;`). Process each top-level statement.
        for raw_line in split_top_level_statements(raw_line) {
            let line = raw_line.replace("import.meta.url", &quoted_url);
            let trimmed = line.trim();
            // CSS-style minifiers drop the space after the keyword: `import{...}`,
            // `import"spec"`, `export{...}`. ES grammar allows no whitespace here,
            // but only when the next token is not an identifier (#1920/#1936).
            let import_rest = if let Some(rest) = trimmed.strip_prefix("import ") {
                Some(rest)
            } else if let Some(rest) = trimmed.strip_prefix("import") {
                let next = rest.chars().next();
                if matches!(next, Some('{' | '"' | '\'' | '*')) {
                    Some(rest)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(import_rest) = import_rest {
                let rest = import_rest.trim().trim_end_matches(';').trim();
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
                } else if head.starts_with("* as ") || head.starts_with("*as ") {
                    ImportBinding::Namespace(
                        head.strip_prefix("* as ")
                            .or_else(|| head.strip_prefix("*as "))
                            .map(str::trim)
                            .unwrap_or_else(|| head.trim())
                            .to_string(),
                    )
                } else if head.starts_with('{') {
                    ImportBinding::Named(parse_import_named_list(&head))
                } else if let Some((default_name, named)) = head.split_once(',') {
                    let mut bindings =
                        vec![(default_name.trim().to_string(), "default".to_string())];
                    bindings.extend(parse_import_named_list(named));
                    ImportBinding::Named(bindings)
                } else {
                    ImportBinding::Default(head.to_string())
                };
                imports.push(ImportSpec { specifier, binding });
                continue;
            }
            let export_rest = if let Some(rest) = trimmed.strip_prefix("export ") {
                Some(rest)
            } else if let Some(rest) = trimmed.strip_prefix("export") {
                let next = rest.chars().next();
                if matches!(next, Some('{' | '*' | '"' | '\''))
                    || next.is_some_and(|c| c.is_ascii_alphabetic())
                {
                    Some(rest)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(rest) = export_rest {
                let rest = rest.trim();
                if let Some(default) = rest.strip_prefix("default ") {
                    let default = default.trim().trim_end_matches(';').trim();
                    body_lines.push(format!("const __megi_default = ({default});"));
                    exports.push(("default".to_string(), "__megi_default".to_string()));
                    continue;
                }
                if rest.starts_with('{') {
                    let mapping = parse_named_list(rest.trim_end_matches(';'));
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
    }
    let mut body = body_lines.join("\n");
    let dynamic_imports = rewrite_dynamic_imports(&mut body, url)?;
    Ok(ParsedModule {
        imports,
        body,
        exports,
        dynamic_imports,
    })
}

/// Rewrites literal `import("specifier")` calls into `__MEGI_DYNAMIC_k__` placeholders and
/// returns the literal specifiers. Non-literal arguments are rewritten to a native
/// `__megiDynamicImport(expr, base_url)` call resolved at runtime (F6).
pub(crate) fn rewrite_dynamic_imports(
    body: &mut String,
    url: &str,
) -> Result<Vec<String>, JsError> {
    let mut specifiers: Vec<String> = Vec::new();
    let mut output = String::with_capacity(body.len());
    // #1850: search a masked copy, where strings/template literals and comments are
    // spaces, so an `import(...)` written inside one of them is not mistaken for a
    // real dynamic import. `mask_js_literals` preserves byte length, so every offset
    // found here maps directly onto the original source.
    let masked = mask_js_literals(body);
    let mut cursor = 0usize;
    while let Some(relative) = masked[cursor..].find("import") {
        let index = cursor + relative;
        let after = &body[index + "import".len()..];
        if !after.trim_start().starts_with('(') {
            output.push_str(&body[cursor..index + "import".len()]);
            cursor = index + "import".len();
            continue;
        }
        let mut inner = after.trim_start();
        inner = &inner[1..];
        inner = inner.trim_start();
        let Some(quote) = inner.chars().next().filter(|ch| *ch == '\'' || *ch == '"') else {
            // F6-IMPORTMAP-SCOPES: a runtime (non-literal) specifier is resolved when the
            // call executes, through the host module resolver/import map. Rewrite the whole
            // call into a native helper carrying the current module's base URL.
            let Some(end) = matching_paren(inner) else {
                return Err(JsError::DynamicImport(
                    "unterminated import() call".to_string(),
                ));
            };
            let expression = inner[..end].trim();
            if expression.is_empty() {
                return Err(JsError::DynamicImport(
                    "empty import() specifier".to_string(),
                ));
            }
            let replacement = format!("__megiDynamicImport({expression}, {})", module_quote(url));
            let consumed = index + "import".len() + (after.len() - inner.len()) + end + 1;
            output.push_str(&body[cursor..index]);
            output.push_str(&replacement);
            cursor = consumed.min(body.len());
            continue;
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
        cursor = body.len() - after_literal[paren_offset + 1..].len();
    }
    output.push_str(&body[cursor..]);
    *body = output;
    Ok(specifiers)
}

/// Finds the index of the `)` matching the first `(` in `expression` (which starts just
/// after the opening paren), honouring nested brackets and quoted strings.
fn matching_paren(expression: &str) -> Option<usize> {
    let mut depth = 1usize;
    let mut quote: Option<char> = None;
    for (offset, ch) in expression.char_indices() {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
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
