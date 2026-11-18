//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

pub(crate) fn consume_qualified_rule_at(
    stream: &mut TokenStream,
    stop_token: Option<&CssToken>,
    nested: bool,
    depth: usize,
) -> Option<Rule> {
    let mut prelude = Vec::new();
    loop {
        let next = stream.peek();
        if next.is_eof() || stop_token == Some(next) {
            return None;
        }
        if next == &CssToken::RightBrace && nested {
            return None;
        }
        if next == &CssToken::LeftBrace {
            stream.consume(); // '{'
            let (declarations, child_rules) = if depth + 1 >= MAX_RULE_NESTING {
                // #1412: past the cap, drop the deep block iteratively instead of recursing.
                // `stream` is just after '{'; skip_to_matching consumes through the matching '}'.
                // The rule at the cap is still emitted (empty), so at most MAX_RULE_NESTING
                // rule levels are retained.
                skip_to_matching(stream, b'}');
                (Vec::new(), Vec::new())
            } else {
                consume_block_contents_at(stream, depth + 1)
            };
            if stream.peek() == &CssToken::RightBrace {
                stream.consume(); // '}'
            }
            return Some(Rule::Style(StyleRule {
                prelude,
                declarations,
                child_rules,
            }));
        }
        prelude.push(consume_component_value(stream));
    }
}

/// Consumes a stylesheet's contents (§5.5.1).
pub fn consume_stylesheet_contents(stream: &mut TokenStream) -> Vec<Rule> {
    let mut rules = Vec::new();
    loop {
        stream.discard_whitespace();
        match stream.peek() {
            CssToken::Eof => break,
            CssToken::Cdo | CssToken::Cdc => {
                stream.consume();
            }
            CssToken::AtKeyword(_) => {
                if let Some(at) = consume_at_rule(stream, false) {
                    rules.push(Rule::At(at));
                }
            }
            _ => {
                if let Some(rule) = consume_qualified_rule(stream, None, false) {
                    rules.push(rule);
                } else if !stream.is_empty() {
                    stream.consume();
                }
            }
        }
    }
    rules
}

/// Parses a complete CSS stylesheet from a string (§5.4.3).
///
/// `@import` rules at the beginning of the sheet are parsed into [`Stylesheet::imports`]
/// (URL kept as authored plus a reusable media query list). Imports appearing after any other
/// rule are ignored, per CSS Cascading and Inheritance Level 4 §6.4.3. No I/O is performed.
pub fn parse_stylesheet(input: &str) -> Stylesheet {
    let mut stream = TokenStream::from_input(input);
    let rules = consume_stylesheet_contents(&mut stream);
    let mut imports: Vec<crate::rule::ImportRule> = Vec::new();
    let mut font_faces: Vec<crate::rule::FontFaceRule> = Vec::new();
    let mut keyframes: Vec<crate::rule::KeyframesRule> = Vec::new();
    let mut kept_rules = Vec::with_capacity(rules.len());
    let mut seen_non_import = false;
    for rule in rules {
        match &rule {
            Rule::At(at) if at.name.eq_ignore_ascii_case("import") => {
                // Imports are only valid before any other rule; late, malformed and
                // unsupported imports are ignored and never reach the cascade.
                if !seen_non_import && let Some(import) = parse_import_rule(at) {
                    imports.push(import);
                }
            }
            Rule::At(at) if at.name.eq_ignore_ascii_case("font-face") => {
                seen_non_import = true;
                if let Some(rule) = parse_font_face_rule(at) {
                    font_faces.push(rule);
                }
            }
            Rule::At(at)
                if at.name.eq_ignore_ascii_case("keyframes")
                    || at.name.eq_ignore_ascii_case("-webkit-keyframes") =>
            {
                seen_non_import = true;
                if let Some(rule) = parse_keyframes_rule(at) {
                    keyframes.push(rule);
                }
            }
            _ => {
                seen_non_import = true;
                kept_rules.push(rule);
            }
        }
    }
    Stylesheet {
        imports,
        font_faces,
        keyframes,
        rules: kept_rules,
    }
}

/// Parses an `@keyframes <name> { <selector> { ... } ... }` rule (CSS Animations 1 §3).
///
/// Invalid keyframe selectors are skipped (the rest of the rule is kept); a rule with no
/// usable keyframe, an empty name or no block returns `None`.
pub(crate) fn parse_keyframes_rule(at: &AtRule) -> Option<crate::rule::KeyframesRule> {
    let name = strip_quotes(serialize_component_values(&at.prelude).trim());
    if name.is_empty() {
        return None;
    }
    let block = at.block.as_ref()?;
    let mut frames = Vec::new();
    let mut selector_values: Vec<ComponentValue> = Vec::new();
    for value in &block.values {
        match value {
            ComponentValue::Block(frame_block) => {
                let selector = serialize_component_values(&selector_values)
                    .trim()
                    .to_string();
                selector_values.clear();
                let Some(offsets) = parse_keyframe_offsets(&selector) else {
                    continue;
                };
                let declarations =
                    parse_list_of_declarations(&serialize_component_values(&frame_block.values));
                if !declarations.is_empty() {
                    frames.push(crate::rule::Keyframe {
                        offsets,
                        declarations,
                    });
                }
            }
            other => selector_values.push(other.clone()),
        }
    }
    if frames.is_empty() {
        return None;
    }
    Some(crate::rule::KeyframesRule { name, frames })
}

/// Parses a keyframe selector list (`from`, `to`, percentages; comma-separated).
pub(crate) fn parse_keyframe_offsets(selector: &str) -> Option<Vec<f32>> {
    let mut offsets = Vec::new();
    for part in selector.split(',') {
        let part = part.trim();
        let offset = if part.eq_ignore_ascii_case("from") {
            0.0
        } else if part.eq_ignore_ascii_case("to") {
            1.0
        } else {
            let percent = part.strip_suffix('%')?.trim().parse::<f32>().ok()?;
            percent / 100.0
        };
        if !offset.is_finite() || !(0.0..=1.0).contains(&offset) {
            return None;
        }
        offsets.push(offset);
    }
    (!offsets.is_empty()).then_some(offsets)
}

/// Parses an `@font-face` at-rule (CSS Fonts 4 §4.1). Missing `font-family`/`src` or a
/// `src` with no usable URL makes the rule invalid (returns `None`), and unknown
/// descriptors (`font-display`, `unicode-range`, ...) are ignored.
pub(crate) fn parse_font_face_rule(at: &AtRule) -> Option<crate::rule::FontFaceRule> {
    let block = at.block.as_ref()?;
    let text = serialize_component_values(&block.values);
    let declarations = parse_list_of_declarations(&text);
    let mut family = None;
    let mut sources: Vec<(String, String)> = Vec::new();
    let mut weight = String::from("400");
    let mut style = String::from("normal");
    for declaration in declarations {
        let name = declaration.name.to_ascii_lowercase();
        match name.as_str() {
            "font-family" => {
                let value = strip_quotes(declaration.raw_value.trim());
                if !value.is_empty() {
                    family = Some(value);
                }
            }
            "src" => sources = parse_font_sources(&declaration.value),
            "font-weight" => weight = normalize_font_weight(declaration.raw_value.trim()),
            "font-style" => style = declaration.raw_value.trim().to_ascii_lowercase(),
            _ => {}
        }
    }
    let family = family?;
    if sources.is_empty() {
        return None;
    }
    Some(crate::rule::FontFaceRule {
        family,
        sources,
        weight,
        style,
    })
}

pub(crate) fn strip_quotes(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0];
        let last = bytes[value.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

pub(crate) fn normalize_font_weight(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "normal" | "" => String::from("400"),
        "bold" => String::from("700"),
        other => other.to_string(),
    }
}

pub(crate) fn function_string_argument(
    values: &[crate::component_value::ComponentValue],
) -> Option<String> {
    use crate::component_value::ComponentValue;
    use crate::token::CssToken;
    for value in values {
        if let ComponentValue::PreservedToken(token) = value
            && let CssToken::String(text) | CssToken::Url(text) = token
        {
            return Some(text.clone());
        }
    }
    None
}

/// Parses the `src` descriptor into `(url, format)` entries in authored order.
pub(crate) fn parse_font_sources(
    values: &[crate::component_value::ComponentValue],
) -> Vec<(String, String)> {
    use crate::component_value::ComponentValue;
    use crate::token::CssToken;
    let mut sources = Vec::new();
    let mut pending_url: Option<String> = None;
    for value in values {
        match value {
            ComponentValue::Function(function) if function.name.eq_ignore_ascii_case("url") => {
                if let Some(url) = function_string_argument(&function.values) {
                    pending_url = Some(url);
                }
            }
            ComponentValue::Function(function) if function.name.eq_ignore_ascii_case("format") => {
                let format = function_string_argument(&function.values).unwrap_or_default();
                if let Some(url) = pending_url.take() {
                    sources.push((url, format));
                }
            }
            ComponentValue::PreservedToken(CssToken::Url(url)) => {
                pending_url = Some(url.clone());
            }
            ComponentValue::PreservedToken(CssToken::Comma) => {
                if let Some(url) = pending_url.take() {
                    sources.push((url, String::new()));
                }
            }
            _ => {}
        }
    }
    if let Some(url) = pending_url {
        sources.push((url, String::new()));
    }
    sources
}

/// Parses the prelude of an `@import` at-rule into an [`crate::rule::ImportRule`].
///
/// Accepted forms: `@import "<url>" [media-list];` and `@import url("<url>") [media-list];`
/// (also unquoted `url(<url>)`). `layer()`/`supports()` conditions are not supported yet and
/// make the import invalid.
pub(crate) fn parse_import_rule(at: &AtRule) -> Option<crate::rule::ImportRule> {
    let mut url = None;
    let mut rest_start = at.prelude.len();
    for (index, component) in at.prelude.iter().enumerate() {
        if component.is_whitespace() {
            continue;
        }
        url = import_url(component);
        rest_start = index + 1;
        break;
    }
    let url = url?;
    let media_text = serialize_component_values(&at.prelude[rest_start..]);
    let media = crate::media::parse_media_query_list(&media_text);
    Some(crate::rule::ImportRule { url, media })
}

/// Extracts the URL of an `@import` prelude component.
pub(crate) fn import_url(component: &ComponentValue) -> Option<String> {
    match component {
        ComponentValue::PreservedToken(CssToken::String(url)) => Some(url.clone()),
        ComponentValue::PreservedToken(CssToken::Url(url)) => Some(url.clone()),
        ComponentValue::Function(function) if function.name.eq_ignore_ascii_case("url") => {
            let inner = serialize_component_values(&function.values);
            let inner = inner.trim();
            let quoted = inner.len() >= 2
                && ((inner.starts_with('"') && inner.ends_with('"'))
                    || (inner.starts_with('\'') && inner.ends_with('\'')));
            if quoted {
                Some(inner[1..inner.len() - 1].to_string())
            } else if inner.is_empty() {
                None
            } else {
                Some(inner.to_string())
            }
        }
        _ => None,
    }
}

/// Parses a list of declarations from a string (e.g. for HTML `style="..."` attributes) (§5.4.5).
pub fn parse_list_of_declarations(input: &str) -> Vec<Declaration> {
    let mut stream = TokenStream::from_input(input);
    let mut decls = Vec::new();
    while !stream.is_empty() {
        stream.discard_whitespace();
        if stream.peek() == &CssToken::Semicolon {
            stream.consume();
            continue;
        }
        if matches!(stream.peek(), CssToken::AtKeyword(_)) {
            consume_at_rule(&mut stream, true);
            continue;
        }
        if let Some(decl) = consume_declaration(&mut stream) {
            decls.push(decl);
        } else if !stream.is_empty() {
            stream.consume();
        }
    }
    decls
}
