//! # W3C CSS Syntax Module Level 3 Parser (§5)
//!
//! Normative implementation of parsing stylesheets, rules, declarations,
//! and component values per <https://www.w3.org/TR/css-syntax-3/#parser-entry-points>.

use crate::component_value::{ComponentValue, FunctionCall, SimpleBlock};
use crate::rule::{AtRule, Declaration, Rule, StyleRule, Stylesheet};
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

/// Consumes a component value from the stream (§5.5.8).
pub fn consume_component_value(stream: &mut TokenStream) -> ComponentValue {
    match stream.consume() {
        open @ (CssToken::LeftBrace | CssToken::LeftBracket | CssToken::LeftParen) => {
            let block = consume_simple_block(stream, open);
            ComponentValue::Block(block)
        }
        CssToken::Function(name) => {
            let func = consume_function(stream, name);
            ComponentValue::Function(func)
        }
        tok => ComponentValue::PreservedToken(tok),
    }
}

/// Consumes a simple block from the stream (§5.5.9).
pub fn consume_simple_block(stream: &mut TokenStream, associated_token: CssToken) -> SimpleBlock {
    let mirror = match associated_token {
        CssToken::LeftBracket => CssToken::RightBracket,
        CssToken::LeftParen => CssToken::RightParen,
        _ => CssToken::RightBrace,
    };
    let mut values = Vec::new();
    loop {
        if stream.peek() == &mirror {
            stream.consume();
            break;
        }
        if stream.peek().is_eof() {
            break;
        }
        values.push(consume_component_value(stream));
    }
    SimpleBlock {
        associated_token,
        values,
    }
}

/// Consumes a function from the stream (§5.5.10).
pub fn consume_function(stream: &mut TokenStream, name: String) -> FunctionCall {
    let mut values = Vec::new();
    loop {
        if matches!(stream.peek(), CssToken::RightParen | CssToken::Eof) {
            if !stream.peek().is_eof() {
                stream.consume();
            }
            break;
        }
        values.push(consume_component_value(stream));
    }
    FunctionCall { name, values }
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
pub fn parse_stylesheet(input: &str) -> Stylesheet {
    let mut stream = TokenStream::from_input(input);
    let rules = consume_stylesheet_contents(&mut stream);
    Stylesheet { rules }
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
