//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

impl CssTokenizer {
    /// Creates a tokenizer from a raw string, performing §3.3 preprocessing.
    pub fn new(input: &str) -> Self {
        Self {
            input: preprocess_css_input(input),
            pos: 0,
        }
    }

    #[inline]
    pub(crate) fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    #[inline]
    pub(crate) fn peek2(&self) -> Option<char> {
        self.input.get(self.pos + 1).copied()
    }

    #[inline]
    pub(crate) fn peek3(&self) -> Option<char> {
        self.input.get(self.pos + 2).copied()
    }

    #[inline]
    pub(crate) fn consume(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    #[inline]
    pub(crate) fn reconsume(&mut self) {
        if self.pos > 0 {
            self.pos -= 1;
        }
    }

    /// Consumes comments per §4.3.2.
    pub(crate) fn consume_comments(&mut self) {
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
    pub(crate) fn consume_escaped_code_point(&mut self) -> char {
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
    pub(crate) fn consume_ident_sequence(&mut self) -> String {
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
    pub(crate) fn consume_number(&mut self) -> (f64, NumberType, Option<char>) {
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
    pub(crate) fn consume_numeric_token(&mut self) -> CssToken {
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
    pub(crate) fn consume_string_token(&mut self, ending_code_point: char) -> CssToken {
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
    pub(crate) fn consume_remnants_of_bad_url(&mut self) {
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
    pub(crate) fn consume_url_token(&mut self) -> CssToken {
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
