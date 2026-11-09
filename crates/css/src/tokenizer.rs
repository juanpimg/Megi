//! # W3C CSS Syntax Module Level 3 Tokenizer (§4)
//!
//! Complete, normative implementation of CSS tokenization according to:
//! <https://www.w3.org/TR/css-syntax-3/#tokenization>.

use crate::token::{CssToken, HashType, NumberType};

/// Preprocesses the raw CSS input string per W3C CSS Syntax Level 3 §3.3.
///
/// - Replaces `\r\n`, `\r`, and `\x0C` (FF) with `\n`.
/// - Replaces `\0` and Unicode surrogates (`0xD800..=0xDFFF`) with `\u{FFFD}`.
pub fn preprocess_css_input(input: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\x0C' => out.push('\n'),
            '\0' => out.push('\u{FFFD}'),
            other => {
                let cp = other as u32;
                if (0xD800..=0xDFFF).contains(&cp) {
                    out.push('\u{FFFD}');
                } else {
                    out.push(other);
                }
            }
        }
    }
    out
}

#[inline]
pub fn is_whitespace(ch: char) -> bool {
    matches!(ch, '\n' | '\t' | ' ')
}

#[inline]
pub fn is_digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

#[inline]
pub fn is_hex_digit(ch: char) -> bool {
    ch.is_ascii_hexdigit()
}

#[inline]
pub fn is_letter(ch: char) -> bool {
    ch.is_ascii_alphabetic()
}

#[inline]
pub fn is_non_ascii_ident(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x00B7
            | 0x00C0..=0x00D6
            | 0x00D8..=0x00F6
            | 0x00F8..=0x037D
            | 0x037F..=0x1FFF
            | 0x200C
            | 0x200D
            | 0x203F
            | 0x2040
            | 0x2070..=0x218F
            | 0x2C00..=0x2FEF
            | 0x3001..=0xD7FF
            | 0xF900..=0xFDCF
            | 0xFDF0..=0xFFFD
            | 0x10000..=0x10FFFF
    )
}

#[inline]
pub fn is_ident_start(ch: char) -> bool {
    is_letter(ch) || is_non_ascii_ident(ch) || ch == '_'
}

#[inline]
pub fn is_ident(ch: char) -> bool {
    is_ident_start(ch) || is_digit(ch) || ch == '-'
}

#[inline]
pub fn is_non_printable(ch: char) -> bool {
    let cp = ch as u32;
    cp <= 0x08 || cp == 0x0B || (0x0E..=0x1F).contains(&cp) || cp == 0x7F
}

#[inline]
pub fn is_valid_escape(cp1: Option<char>, cp2: Option<char>) -> bool {
    cp1 == Some('\\') && cp2 != Some('\n') && cp2.is_some()
}

#[inline]
pub fn starts_ident_sequence(cp1: Option<char>, cp2: Option<char>, cp3: Option<char>) -> bool {
    match cp1 {
        Some('-') => {
            cp2.is_some_and(is_ident_start) || cp2 == Some('-') || is_valid_escape(cp2, cp3)
        }
        Some(c) if is_ident_start(c) => true,
        Some('\\') => is_valid_escape(cp1, cp2),
        _ => false,
    }
}

#[inline]
pub fn starts_number(cp1: Option<char>, cp2: Option<char>, cp3: Option<char>) -> bool {
    match cp1 {
        Some('+' | '-') => {
            cp2.is_some_and(is_digit) || (cp2 == Some('.') && cp3.is_some_and(is_digit))
        }
        Some('.') => cp2.is_some_and(is_digit),
        Some(c) if is_digit(c) => true,
        _ => false,
    }
}

/// Tokenizer for W3C CSS Syntax Level 3.
pub struct CssTokenizer {
    input: Vec<char>,
    pos: usize,
}

impl CssTokenizer {
    /// Creates a tokenizer from a raw string, performing §3.3 preprocessing.
    pub fn new(input: &str) -> Self {
        Self {
            input: preprocess_css_input(input),
            pos: 0,
        }
    }

    #[inline]
    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    #[inline]
    fn peek2(&self) -> Option<char> {
        self.input.get(self.pos + 1).copied()
    }

    #[inline]
    fn peek3(&self) -> Option<char> {
        self.input.get(self.pos + 2).copied()
    }

    #[inline]
    fn consume(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    #[inline]
    fn reconsume(&mut self) {
        if self.pos > 0 {
            self.pos -= 1;
        }
    }

    /// Consumes comments per §4.3.2.
    fn consume_comments(&mut self) {
        while self.peek() == Some('/') && self.peek2() == Some('*') {
            self.consume(); // '/'
            self.consume(); // '*'
            while let Some(ch) = self.consume() {
                if ch == '*' && self.peek() == Some('/') {
                    self.consume(); // '/'
                    break;
                }
            }
        }
    }

    /// Consumes an escaped code point per §4.3.7. Assumes `\` was consumed.
    fn consume_escaped_code_point(&mut self) -> char {
        match self.consume() {
            Some(ch) if is_hex_digit(ch) => {
                let mut hex_str = String::with_capacity(6);
                hex_str.push(ch);
                while hex_str.len() < 6 && self.peek().is_some_and(is_hex_digit) {
                    hex_str.push(self.consume().unwrap());
                }
                if self.peek().is_some_and(is_whitespace) {
                    self.consume();
                }
                let value = u32::from_str_radix(&hex_str, 16).unwrap_or(0);
                if value == 0 || (0xD800..=0xDFFF).contains(&value) || value > 0x10FFFF {
                    '\u{FFFD}'
                } else {
                    char::from_u32(value).unwrap_or('\u{FFFD}')
                }
            }
            Some(ch) => ch,
            None => '\u{FFFD}',
        }
    }

    /// Consumes an ident sequence per §4.3.12.
    fn consume_ident_sequence(&mut self) -> String {
        let mut result = String::new();
        while let Some(ch) = self.consume() {
            if is_ident(ch) {
                result.push(ch);
            } else if is_valid_escape(Some(ch), self.peek()) {
                result.push(self.consume_escaped_code_point());
            } else {
                self.reconsume();
                break;
            }
        }
        result
    }

    /// Consumes a number per §4.3.13.
    fn consume_number(&mut self) -> (f64, NumberType, Option<char>) {
        let mut type_flag = NumberType::Integer;
        let mut repr = String::new();
        let mut sign = None;

        if let Some(ch @ ('+' | '-')) = self.peek() {
            sign = Some(ch);
            repr.push(self.consume().unwrap());
        }

        while self.peek().is_some_and(is_digit) {
            repr.push(self.consume().unwrap());
        }

        if self.peek() == Some('.') && self.peek2().is_some_and(is_digit) {
            type_flag = NumberType::Number;
            repr.push(self.consume().unwrap()); // '.'
            while self.peek().is_some_and(is_digit) {
                repr.push(self.consume().unwrap());
            }
        }

        if let Some('e' | 'E') = self.peek() {
            let next2 = self.peek2();
            let next3 = self.peek3();
            let has_exp = next2.is_some_and(is_digit)
                || (matches!(next2, Some('+' | '-')) && next3.is_some_and(is_digit));
            if has_exp {
                type_flag = NumberType::Number;
                repr.push(self.consume().unwrap()); // 'e' or 'E'
                if let Some(s @ ('+' | '-')) = self.peek() {
                    repr.push(s);
                    self.consume();
                }
                while self.peek().is_some_and(is_digit) {
                    repr.push(self.consume().unwrap());
                }
            }
        }

        let value = repr.parse::<f64>().unwrap_or(0.0);
        (value, type_flag, sign)
    }

    /// Consumes a numeric token per §4.3.3.
    fn consume_numeric_token(&mut self) -> CssToken {
        let (value, type_flag, sign) = self.consume_number();
        if starts_ident_sequence(self.peek(), self.peek2(), self.peek3()) {
            let unit = self.consume_ident_sequence();
            CssToken::Dimension {
                value,
                type_flag,
                sign,
                unit,
            }
        } else if self.peek() == Some('%') {
            self.consume();
            CssToken::Percentage { value, sign }
        } else {
            CssToken::Number {
                value,
                type_flag,
                sign,
            }
        }
    }

    /// Consumes a string token per §4.3.5. Assumes initial quote was consumed.
    fn consume_string_token(&mut self, ending_code_point: char) -> CssToken {
        let mut string = String::new();
        while let Some(ch) = self.consume() {
            if ch == ending_code_point {
                return CssToken::String(string);
            }
            match ch {
                '\n' => {
                    self.reconsume();
                    return CssToken::BadString;
                }
                '\\' => {
                    if self.peek().is_none() {
                        // EOF after backslash
                    } else if self.peek() == Some('\n') {
                        self.consume(); // Escaped newline line continuation
                    } else {
                        string.push(self.consume_escaped_code_point());
                    }
                }
                other => string.push(other),
            }
        }
        CssToken::String(string)
    }

    /// Consumes remnants of a bad url per §4.3.15.
    fn consume_remnants_of_bad_url(&mut self) {
        while let Some(ch) = self.consume() {
            if ch == ')' {
                break;
            }
            if is_valid_escape(Some(ch), self.peek()) {
                self.consume_escaped_code_point();
            }
        }
    }

    /// Consumes a url token per §4.3.6. Assumes `url(` was consumed.
    fn consume_url_token(&mut self) -> CssToken {
        while self.peek().is_some_and(is_whitespace) {
            self.consume();
        }
        let mut url = String::new();
        while let Some(ch) = self.consume() {
            if ch == ')' {
                return CssToken::Url(url);
            }
            if is_whitespace(ch) {
                while self.peek().is_some_and(is_whitespace) {
                    self.consume();
                }
                if self.peek() == Some(')') {
                    self.consume();
                    return CssToken::Url(url);
                }
                if self.peek().is_none() {
                    return CssToken::Url(url);
                }
                self.consume_remnants_of_bad_url();
                return CssToken::BadUrl;
            }
            match ch {
                '"' | '\'' | '(' => {
                    self.consume_remnants_of_bad_url();
                    return CssToken::BadUrl;
                }
                '\\' => {
                    if is_valid_escape(Some(ch), self.peek()) {
                        url.push(self.consume_escaped_code_point());
                    } else {
                        self.consume_remnants_of_bad_url();
                        return CssToken::BadUrl;
                    }
                }
                other if is_non_printable(other) => {
                    self.consume_remnants_of_bad_url();
                    return CssToken::BadUrl;
                }
                other => url.push(other),
            }
        }
        CssToken::Url(url)
    }

    /// Consumes an ident-like token per §4.3.4.
    fn consume_ident_like_token(&mut self) -> CssToken {
        let string = self.consume_ident_sequence();
        if string.eq_ignore_ascii_case("url") && self.peek() == Some('(') {
            self.consume(); // '('
            let mut ws_offset = 0;
            while self
                .input
                .get(self.pos + ws_offset)
                .copied()
                .is_some_and(is_whitespace)
            {
                ws_offset += 1;
            }
            let next_non_ws = self.input.get(self.pos + ws_offset).copied();
            if next_non_ws == Some('"') || next_non_ws == Some('\'') {
                CssToken::Function(string)
            } else {
                self.consume_url_token()
            }
        } else if self.peek() == Some('(') {
            self.consume(); // '('
            CssToken::Function(string)
        } else {
            CssToken::Ident(string)
        }
    }

    /// Consumes a token per §4.3.1.
    pub fn next_token(&mut self) -> CssToken {
        self.consume_comments();

        let Some(ch) = self.consume() else {
            return CssToken::Eof;
        };

        if is_whitespace(ch) {
            while self.peek().is_some_and(is_whitespace) {
                self.consume();
            }
            return CssToken::Whitespace;
        }

        match ch {
            '"' | '\'' => self.consume_string_token(ch),
            '#' => {
                if is_ident(self.peek().unwrap_or('\0'))
                    || is_valid_escape(self.peek(), self.peek2())
                {
                    let type_flag =
                        if starts_ident_sequence(self.peek(), self.peek2(), self.peek3()) {
                            HashType::Id
                        } else {
                            HashType::Unrestricted
                        };
                    let value = self.consume_ident_sequence();
                    CssToken::Hash { value, type_flag }
                } else {
                    CssToken::Delim('#')
                }
            }
            '(' => CssToken::LeftParen,
            ')' => CssToken::RightParen,
            '[' => CssToken::LeftBracket,
            ']' => CssToken::RightBracket,
            '{' => CssToken::LeftBrace,
            '}' => CssToken::RightBrace,
            ',' => CssToken::Comma,
            ':' => CssToken::Colon,
            ';' => CssToken::Semicolon,
            '+' => {
                if starts_number(Some('+'), self.peek(), self.peek2()) {
                    self.reconsume();
                    self.consume_numeric_token()
                } else {
                    CssToken::Delim('+')
                }
            }
            '-' => {
                if starts_number(Some('-'), self.peek(), self.peek2()) {
                    self.reconsume();
                    self.consume_numeric_token()
                } else if self.peek() == Some('-') && self.peek2() == Some('>') {
                    self.consume(); // '-'
                    self.consume(); // '>'
                    CssToken::Cdc
                } else if starts_ident_sequence(Some('-'), self.peek(), self.peek2()) {
                    self.reconsume();
                    self.consume_ident_like_token()
                } else {
                    CssToken::Delim('-')
                }
            }
            '.' => {
                if starts_number(Some('.'), self.peek(), self.peek2()) {
                    self.reconsume();
                    self.consume_numeric_token()
                } else {
                    CssToken::Delim('.')
                }
            }
            '<' => {
                if self.peek() == Some('!')
                    && self.peek2() == Some('-')
                    && self.peek3() == Some('-')
                {
                    self.consume(); // '!'
                    self.consume(); // '-'
                    self.consume(); // '-'
                    CssToken::Cdo
                } else {
                    CssToken::Delim('<')
                }
            }
            '@' => {
                if starts_ident_sequence(self.peek(), self.peek2(), self.peek3()) {
                    let value = self.consume_ident_sequence();
                    CssToken::AtKeyword(value)
                } else {
                    CssToken::Delim('@')
                }
            }
            '\\' => {
                if is_valid_escape(Some('\\'), self.peek()) {
                    self.reconsume();
                    self.consume_ident_like_token()
                } else {
                    CssToken::Delim('\\')
                }
            }
            c if is_digit(c) => {
                self.reconsume();
                self.consume_numeric_token()
            }
            c if is_ident_start(c) => {
                self.reconsume();
                self.consume_ident_like_token()
            }
            other => CssToken::Delim(other),
        }
    }

    /// Tokenizes the entire input into a vector of tokens ending with [`CssToken::Eof`].
    pub fn tokenize_all(&mut self) -> Vec<CssToken> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            let is_eof = tok.is_eof();
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        tokens
    }
}

/// Convenience helper to tokenize an input string.
pub fn tokenize(input: &str) -> Vec<CssToken> {
    CssTokenizer::new(input).tokenize_all()
}
