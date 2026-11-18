// ---------------------------------------------------------------------------
// ES modules (round 25 subset preprocessor + host resolver/loader)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) enum ImportBinding {
    SideEffect,
    Default(String),
    Namespace(String),
    Named(Vec<(String, String)>), // local, exported
}

#[derive(Clone, Debug)]
pub(crate) struct ImportSpec {
    pub(crate) specifier: String,
    pub(crate) binding: ImportBinding,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedModule {
    pub(crate) imports: Vec<ImportSpec>,
    pub(crate) body: String,
    pub(crate) exports: Vec<(String, String)>, // exported name, local expression
    /// Literal specifiers of dynamic `import()` calls (evaluated with the graph; the
    /// returned Promise resolves immediately — documented subset).
    pub(crate) dynamic_imports: Vec<String>,
}

pub(crate) fn module_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

pub(crate) fn import_specifier(rest: &str) -> Option<(String, String)> {
    // Returns (binding head, specifier). Import declarations must be single-line.
    let rest = rest.trim().trim_end_matches(';').trim();
    if let Some((head, tail)) = rest.rsplit_once(" from ") {
        let specifier = tail.trim().trim_matches(['"', '\'']);
        if specifier.is_empty() {
            return None;
        }
        let head = head.trim().trim_end_matches(',').trim();
        Some((head.to_string(), specifier.to_string()))
    } else {
        let specifier = rest.trim_matches(['"', '\'']);
        (!specifier.is_empty()).then(|| (String::new(), specifier.to_string()))
    }
}

pub(crate) fn parse_named_list(raw: &str) -> Vec<(String, String)> {
    let raw = raw.trim().trim_start_matches('{').trim_end_matches('}');
    raw.split(',')
        .filter_map(|item| {
            let item = item.trim();
            if item.is_empty() {
                return None;
            }
            if let Some((local, exported)) = item.split_once(" as ") {
                Some((local.trim().to_string(), exported.trim().to_string()))
            } else {
                Some((item.to_string(), item.to_string()))
            }
        })
        .collect()
}

pub(crate) fn parse_import_named_list(raw: &str) -> Vec<(String, String)> {
    parse_named_list(raw)
        .into_iter()
        .map(|(local, exported)| (exported, local))
        .collect()
}

pub(crate) fn export_declared_names(decl: &str) -> Vec<String> {
    let decl = decl.trim();
    if let Some(rest) = decl
        .strip_prefix("function ")
        .or_else(|| decl.strip_prefix("class "))
    {
        return rest
            .split(|ch: char| ch == '(' || ch == '{' || ch.is_whitespace())
            .next()
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .into_iter()
            .collect();
    }
    let rest = decl
        .strip_prefix("const ")
        .or_else(|| decl.strip_prefix("let "))
        .or_else(|| decl.strip_prefix("var "))
        .unwrap_or(decl);
    // Naive top-level declarator split; enough for the documented subset.
    rest.split(',')
        .filter_map(|declarator| {
            let name = declarator
                .split('=')
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches(['{', '[', '('])
                .trim();
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}
