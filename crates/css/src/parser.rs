//! # W3C CSS Syntax Module Level 3 Parser (§5)
//!
//! Normative implementation of parsing stylesheets, rules, declarations,
//! and component values per <https://www.w3.org/TR/css-syntax-3/#parser-entry-points>.

use crate::component_value::{ComponentValue, FunctionCall, SimpleBlock};
use crate::rule::{AtRule, Declaration, Rule, StyleRule, Stylesheet, serialize_component_values};
use crate::token::CssToken;
use crate::tokenizer::tokenize;

/// A stream of CSS tokens or component values supporting lookahead, marks, and rollback (§5.3).
#[derive(Clone, Debug)]
pub struct TokenStream {
    tokens: Vec<CssToken>,
    pos: usize,
    marks: Vec<usize>,
}

impl TokenStream {
    pub fn new(tokens: Vec<CssToken>) -> Self {
        Self {
            tokens,
            pos: 0,
            marks: Vec::new(),
        }
    }

    pub fn from_input(input: &str) -> Self {
        Self::new(tokenize(input))
    }

    #[inline]
    pub fn peek(&self) -> &CssToken {
        self.tokens.get(self.pos).unwrap_or(&CssToken::Eof)
    }

    #[inline]
    pub fn consume(&mut self) -> CssToken {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            tok
        } else {
            CssToken::Eof
        }
    }

    #[inline]
    pub fn reconsume(&mut self) {
        if self.pos > 0 {
            self.pos -= 1;
        }
    }

    #[inline]
    pub fn discard_whitespace(&mut self) {
        while matches!(self.peek(), CssToken::Whitespace) {
            self.consume();
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        matches!(self.peek(), CssToken::Eof)
    }

    #[inline]
    pub fn mark(&mut self) {
        self.marks.push(self.pos);
    }

    #[inline]
    pub fn restore_mark(&mut self) {
        if let Some(pos) = self.marks.pop() {
            self.pos = pos;
        }
    }

    #[inline]
    pub fn discard_mark(&mut self) {
        self.marks.pop();
    }
}

/// Maximum nesting of simple blocks and functions accepted while parsing component values.
///
/// The grammar is recursive (§5.5.8-§5.5.10); without a bound a ~100 KB stylesheet of `(`
/// overflows the host stack before any declaration is validated. Deeper content is
/// consumed iteratively and dropped, so the surrounding declarations still parse.
pub const MAX_COMPONENT_NESTING: usize = 256;

/// Consumes a component value from the stream (§5.5.8).
pub fn consume_component_value(stream: &mut TokenStream) -> ComponentValue {
    consume_component_value_at(stream, 0)
}

fn consume_component_value_at(stream: &mut TokenStream, depth: usize) -> ComponentValue {
    match stream.consume() {
        open @ (CssToken::LeftBrace | CssToken::LeftBracket | CssToken::LeftParen) => {
            let block = consume_simple_block_at(stream, open, depth);
            ComponentValue::Block(block)
        }
        CssToken::Function(name) => {
            let func = consume_function_at(stream, name, depth);
            ComponentValue::Function(func)
        }
        tok => ComponentValue::PreservedToken(tok),
    }
}

/// Consumes a simple block from the stream (§5.5.9).
pub fn consume_simple_block(stream: &mut TokenStream, associated_token: CssToken) -> SimpleBlock {
    consume_simple_block_at(stream, associated_token, 0)
}

fn consume_simple_block_at(
    stream: &mut TokenStream,
    associated_token: CssToken,
    depth: usize,
) -> SimpleBlock {
    let mirror = match associated_token {
        CssToken::LeftBracket => CssToken::RightBracket,
        CssToken::LeftParen => CssToken::RightParen,
        _ => CssToken::RightBrace,
    };
    if depth >= MAX_COMPONENT_NESTING {
        skip_to_matching(stream, closing_byte(&mirror).unwrap_or(b'}'));
        return SimpleBlock {
            associated_token,
            values: Vec::new(),
        };
    }
    let mut values = Vec::new();
    loop {
        if stream.peek() == &mirror {
            stream.consume();
            break;
        }
        if stream.peek().is_eof() {
            break;
        }
        values.push(consume_component_value_at(stream, depth + 1));
    }
    SimpleBlock {
        associated_token,
        values,
    }
}

/// Consumes a function from the stream (§5.5.10).
pub fn consume_function(stream: &mut TokenStream, name: String) -> FunctionCall {
    consume_function_at(stream, name, 0)
}

fn consume_function_at(stream: &mut TokenStream, name: String, depth: usize) -> FunctionCall {
    if depth >= MAX_COMPONENT_NESTING {
        skip_to_matching(stream, b')');
        return FunctionCall {
            name,
            values: Vec::new(),
        };
    }
    let mut values = Vec::new();
    loop {
        if matches!(stream.peek(), CssToken::RightParen | CssToken::Eof) {
            if !stream.peek().is_eof() {
                stream.consume();
            }
            break;
        }
        values.push(consume_component_value_at(stream, depth + 1));
    }
    FunctionCall { name, values }
}

/// Iteratively consumes tokens up to the close that matches `mirror`, tracking nesting.
///
/// Used at [`MAX_COMPONENT_NESTING`] instead of recursing, so adversarial nesting costs a
/// single loop rather than stack frames.
fn skip_to_matching(stream: &mut TokenStream, mirror: u8) {
    // One byte per open block keeps a 1 MiB adversarial sheet at ~1 MiB of bookkeeping.
    let mut expected = vec![mirror];
    while let Some(&wanted) = expected.last() {
        let token = stream.consume();
        if token.is_eof() {
            break;
        }
        if let Some(close) = closing_byte(&token) {
            if close == wanted {
                expected.pop();
            }
            continue;
        }
        if let Some(open) = opening_byte(&token) {
            expected.push(open);
        }
    }
}

fn closing_byte(token: &CssToken) -> Option<u8> {
    match token {
        CssToken::RightBrace => Some(b'}'),
        CssToken::RightBracket => Some(b']'),
        CssToken::RightParen => Some(b')'),
        _ => None,
    }
}

fn opening_byte(token: &CssToken) -> Option<u8> {
    match token {
        CssToken::LeftBrace => Some(b'}'),
        CssToken::LeftBracket => Some(b']'),
        CssToken::LeftParen | CssToken::Function(_) => Some(b')'),
        _ => None,
    }
}

/// Consumes a list of component values until EOF or `stop_token` (§5.5.7).
pub fn consume_list_of_component_values(
    stream: &mut TokenStream,
    stop_token: Option<&CssToken>,
) -> Vec<ComponentValue> {
    let mut values = Vec::new();
    loop {
        let next = stream.peek();
        if next.is_eof() || stop_token == Some(next) {
            break;
        }
        values.push(consume_component_value(stream));
    }
    values
}

/// Consumes an at-rule from the stream (§5.5.2). Assumes `@` keyword was already seen.
pub fn consume_at_rule(stream: &mut TokenStream, _nested: bool) -> Option<AtRule> {
    let name = match stream.consume() {
        CssToken::AtKeyword(s) => s,
        _ => return None,
    };
    let mut prelude = Vec::new();
    loop {
        match stream.peek() {
            CssToken::Semicolon => {
                stream.consume();
                return Some(AtRule {
                    name,
                    prelude,
                    block: None,
                });
            }
            CssToken::Eof => {
                return Some(AtRule {
                    name,
                    prelude,
                    block: None,
                });
            }
            CssToken::LeftBrace => {
                let open = stream.consume();
                let block = consume_simple_block(stream, open);
                return Some(AtRule {
                    name,
                    prelude,
                    block: Some(block),
                });
            }
            _ => {
                prelude.push(consume_component_value(stream));
            }
        }
    }
}

/// Consumes a declaration from the stream (§5.5.6).
pub fn consume_declaration(stream: &mut TokenStream) -> Option<Declaration> {
    stream.discard_whitespace();
    let name = match stream.consume() {
        CssToken::Ident(s) => s,
        _ => return None,
    };
    stream.discard_whitespace();
    if !matches!(stream.consume(), CssToken::Colon) {
        return None;
    }
    stream.discard_whitespace();

    let mut values = Vec::new();
    while !matches!(
        stream.peek(),
        CssToken::Semicolon | CssToken::RightBrace | CssToken::Eof
    ) {
        values.push(consume_component_value(stream));
    }

    // Check for !important: last two non-whitespace component values are '!' followed by ident("important")
    let mut important = false;
    let non_ws_indices: Vec<usize> = values
        .iter()
        .enumerate()
        .filter(|(_, cv)| !cv.is_whitespace())
        .map(|(i, _)| i)
        .collect();

    if non_ws_indices.len() >= 2 {
        let last_idx = non_ws_indices[non_ws_indices.len() - 1];
        let prev_idx = non_ws_indices[non_ws_indices.len() - 2];
        let is_bang = matches!(
            &values[prev_idx],
            ComponentValue::PreservedToken(CssToken::Delim('!'))
        );
        let is_important = matches!(
            &values[last_idx],
            ComponentValue::PreservedToken(CssToken::Ident(s)) if s.eq_ignore_ascii_case("important")
        );
        if is_bang && is_important {
            important = true;
            values.truncate(prev_idx);
        }
    }

    // Trim trailing whitespace
    while values.last().is_some_and(|cv| cv.is_whitespace()) {
        values.pop();
    }

    Some(Declaration::new(name, values, important))
}

/// Consumes block contents (declarations and child rules) (§5.5.5).
pub fn consume_block_contents(stream: &mut TokenStream) -> (Vec<Declaration>, Vec<Rule>) {
    let mut decls = Vec::new();
    let mut rules = Vec::new();

    loop {
        stream.discard_whitespace();
        match stream.peek() {
            CssToken::RightBrace | CssToken::Eof => break,
            CssToken::Semicolon => {
                stream.consume();
            }
            CssToken::AtKeyword(_) => {
                if let Some(at) = consume_at_rule(stream, true) {
                    rules.push(Rule::At(at));
                }
            }
            _ => {
                stream.mark();
                if let Some(decl) = consume_declaration(stream) {
                    decls.push(decl);
                    stream.discard_mark();
                    if stream.peek() == &CssToken::Semicolon {
                        stream.consume();
                    }
                } else {
                    stream.restore_mark();
                    if let Some(rule) =
                        consume_qualified_rule(stream, Some(&CssToken::Semicolon), true)
                    {
                        rules.push(rule);
                    } else {
                        // Skip unparseable token to recover
                        stream.consume();
                    }
                }
            }
        }
    }

    (decls, rules)
}

/// Consumes a qualified rule from the stream (§5.5.3).
pub fn consume_qualified_rule(
    stream: &mut TokenStream,
    stop_token: Option<&CssToken>,
    nested: bool,
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
            let (declarations, child_rules) = consume_block_contents(stream);
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
            _ => {
                seen_non_import = true;
                kept_rules.push(rule);
            }
        }
    }
    Stylesheet {
        imports,
        font_faces,
        rules: kept_rules,
    }
}

/// Parses an `@font-face` at-rule (CSS Fonts 4 §4.1). Missing `font-family`/`src` or a
/// `src` with no usable URL makes the rule invalid (returns `None`), and unknown
/// descriptors (`font-display`, `unicode-range`, ...) are ignored.
fn parse_font_face_rule(at: &AtRule) -> Option<crate::rule::FontFaceRule> {
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

fn strip_quotes(value: &str) -> String {
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

fn normalize_font_weight(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "normal" | "" => String::from("400"),
        "bold" => String::from("700"),
        other => other.to_string(),
    }
}

fn function_string_argument(values: &[crate::component_value::ComponentValue]) -> Option<String> {
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
fn parse_font_sources(values: &[crate::component_value::ComponentValue]) -> Vec<(String, String)> {
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
fn parse_import_rule(at: &AtRule) -> Option<crate::rule::ImportRule> {
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
fn import_url(component: &ComponentValue) -> Option<String> {
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

#[cfg(test)]
mod serialization_tests {
    use super::*;

    #[test]
    fn string_serialization_keeps_escapes_inside_the_string() {
        let declarations = parse_list_of_declarations(r#"content: "a\"b";"#);
        assert_eq!(declarations[0].raw_value, r#""a\"b""#);
    }
}
