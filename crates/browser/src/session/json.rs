pub(crate) fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(crate) fn json_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e9 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

/// Minimal JSON value used by the session parser.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    pub(crate) fn as_u32(&self) -> Option<u32> {
        match self {
            Self::Number(number) if number.is_finite() && *number >= 0.0 => Some(*number as u32),
            _ => None,
        }
    }

    pub(crate) fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(number) => Some(*number),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    pub(crate) fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }
}

/// Recursive-descent JSON parser with a depth cap (rejects deeply nested input).
pub(crate) struct JsonParser<'a> {
    bytes: &'a [u8],
    position: usize,
    depth: usize,
}

impl<'a> JsonParser<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            position: 0,
            depth: 0,
        }
    }

    pub(crate) fn parse(mut self) -> Result<JsonValue, String> {
        self.skip_whitespace();
        let value = self.parse_value()?;
        self.skip_whitespace();
        if self.position != self.bytes.len() {
            return Err("trailing characters after JSON value".to_string());
        }
        Ok(value)
    }

    pub(crate) fn parse_value(&mut self) -> Result<JsonValue, String> {
        if self.depth > 64 {
            return Err("JSON nesting too deep".to_string());
        }
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b't') => {
                self.expect_literal("true")?;
                Ok(JsonValue::Bool(true))
            }
            Some(b'f') => {
                self.expect_literal("false")?;
                Ok(JsonValue::Bool(false))
            }
            Some(b'n') => {
                self.expect_literal("null")?;
                Ok(JsonValue::Null)
            }
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(_) => Err("invalid JSON value".to_string()),
            None => Err("unexpected end of JSON".to_string()),
        }
    }

    pub(crate) fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.depth += 1;
        self.position += 1; // '{'
        let mut entries = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.position += 1;
            self.depth -= 1;
            return Ok(JsonValue::Object(entries));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            if self.next() != Some(b':') {
                return Err("expected ':' in JSON object".to_string());
            }
            let value = self.parse_value()?;
            entries.push((key, value));
            self.skip_whitespace();
            match self.next() {
                Some(b',') => continue,
                Some(b'}') => break,
                _ => return Err("expected ',' or '}' in JSON object".to_string()),
            }
        }
        self.depth -= 1;
        Ok(JsonValue::Object(entries))
    }

    pub(crate) fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.depth += 1;
        self.position += 1; // '['
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.position += 1;
            self.depth -= 1;
            return Ok(JsonValue::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_whitespace();
            match self.next() {
                Some(b',') => continue,
                Some(b']') => break,
                _ => return Err("expected ',' or ']' in JSON array".to_string()),
            }
        }
        self.depth -= 1;
        Ok(JsonValue::Array(values))
    }

    pub(crate) fn parse_string(&mut self) -> Result<String, String> {
        if self.next() != Some(b'"') {
            return Err("expected JSON string".to_string());
        }
        let mut out = String::new();
        loop {
            match self.next() {
                None => return Err("unterminated JSON string".to_string()),
                Some(b'"') => break,
                Some(b'\\') => match self.next() {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'b') => out.push('\u{0008}'),
                    Some(b'f') => out.push('\u{000c}'),
                    Some(b'u') => {
                        let hex = self.take(4)?;
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|_| "invalid \\u escape".to_string())?;
                        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    _ => return Err("invalid JSON escape".to_string()),
                },
                Some(byte) if byte < 0x20 => {
                    return Err("control character in JSON string".to_string());
                }
                Some(byte) => {
                    // Decode the whole UTF-8 scalar: pushing each byte as a `char` turned
                    // every non-ASCII literal into Latin-1 mojibake ("café" -> "cafÃ©").
                    let start = self.position - 1;
                    let width = utf8_width(byte);
                    if width == 0 || start + width > self.bytes.len() {
                        return Err("invalid UTF-8 in JSON string".to_string());
                    }
                    let text = std::str::from_utf8(&self.bytes[start..start + width])
                        .map_err(|_| "invalid UTF-8 in JSON string".to_string())?;
                    out.push_str(text);
                    self.position = start + width;
                }
            }
        }
        Ok(out)
    }

    pub(crate) fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.position;
        if self.peek() == Some(b'-') {
            self.position += 1;
        }
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        if self.peek() == Some(b'.') {
            self.position += 1;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        if self.peek().is_some_and(|byte| byte == b'e' || byte == b'E') {
            self.position += 1;
            if self.peek().is_some_and(|byte| byte == b'+' || byte == b'-') {
                self.position += 1;
            }
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.position])
            .map_err(|_| "invalid number encoding".to_string())?;
        text.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|_| format!("invalid JSON number `{text}`"))
    }

    pub(crate) fn expect_literal(&mut self, literal: &str) -> Result<(), String> {
        if self.bytes[self.position..].starts_with(literal.as_bytes()) {
            self.position += literal.len();
            Ok(())
        } else {
            Err(format!("expected JSON literal `{literal}`"))
        }
    }

    pub(crate) fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    pub(crate) fn next(&mut self) -> Option<u8> {
        let byte = self.peek();
        if byte.is_some() {
            self.position += 1;
        }
        byte
    }

    pub(crate) fn take(&mut self, count: usize) -> Result<String, String> {
        let slice = self
            .bytes
            .get(self.position..self.position + count)
            .ok_or_else(|| "truncated JSON".to_string())?;
        self.position += count;
        String::from_utf8(slice.to_vec()).map_err(|_| "invalid UTF-8 escape".to_string())
    }
}

/// Length of the UTF-8 scalar starting with `byte` (0 for a continuation/invalid lead).
pub(crate) fn utf8_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 0,
    }
}
