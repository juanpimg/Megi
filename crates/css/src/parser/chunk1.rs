//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// A stream of CSS tokens or component values supporting lookahead, marks, and rollback (§5.3).
#[derive(Clone, Debug)]
pub struct TokenStream {
    pub(crate) tokens: Vec<CssToken>,
    pub(crate) pos: usize,
    pub(crate) marks: Vec<usize>,
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

/// Maximum nesting depth of qualified style rules (CSS Nesting).
///
/// `consume_qualified_rule` and `consume_block_contents` are mutually recursive; without a bound
/// a ~52 KB sheet of `a{a{a{…` overflows the 8 MB host stack before any declaration is validated
/// (#1412). Real author stylesheets nest only a handful of levels. Past this cap the deep block is
/// consumed iteratively and dropped, so the surrounding rules still parse.
pub const MAX_RULE_NESTING: usize = 32;

/// Consumes a component value from the stream (§5.5.8).
pub fn consume_component_value(stream: &mut TokenStream) -> ComponentValue {
    consume_component_value_at(stream, 0)
}

pub(crate) fn consume_component_value_at(stream: &mut TokenStream, depth: usize) -> ComponentValue {
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

pub(crate) fn consume_simple_block_at(
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

pub(crate) fn consume_function_at(
    stream: &mut TokenStream,
    name: String,
    depth: usize,
) -> FunctionCall {
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
pub(crate) fn skip_to_matching(stream: &mut TokenStream, mirror: u8) {
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

pub(crate) fn closing_byte(token: &CssToken) -> Option<u8> {
    match token {
        CssToken::RightBrace => Some(b'}'),
        CssToken::RightBracket => Some(b']'),
        CssToken::RightParen => Some(b')'),
        _ => None,
    }
}

pub(crate) fn opening_byte(token: &CssToken) -> Option<u8> {
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
    consume_block_contents_at(stream, 0)
}

pub(crate) fn consume_block_contents_at(
    stream: &mut TokenStream,
    depth: usize,
) -> (Vec<Declaration>, Vec<Rule>) {
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
                        consume_qualified_rule_at(stream, Some(&CssToken::Semicolon), true, depth)
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
    consume_qualified_rule_at(stream, stop_token, nested, 0)
}
