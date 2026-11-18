/// Minimal JSON value used by the import-map parser.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub(crate) fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }

    pub(crate) fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value.as_str()),
            _ => None,
        }
    }

    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            position: 0,
            depth: 0,
        };
        let value = parser.value()?;
        parser.skip_whitespace();
        if parser.position != parser.bytes.len() {
            return Err("trailing characters".to_string());
        }
        Ok(value)
    }
}

/// Maximum object/array nesting accepted by the import-map JSON parser.
///
/// Import maps arrive inside `<script type="importmap">`; without a bound, a ~100 KB page
/// of `[` overflows the host stack in the recursive descent below.
pub(crate) const MAX_JSON_DEPTH: usize = 128;

pub(crate) struct Parser<'a> {
    bytes: &'a [u8],
    position: usize,
    depth: usize,
}

impl Parser<'_> {
    fn value(&mut self) -> Result<Json, String> {
        if self.depth >= MAX_JSON_DEPTH {
            return Err("nesting too deep".to_string());
        }
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => {
                self.depth += 1;
                let value = self.object();
                self.depth -= 1;
                value
            }
            Some(b'[') => {
                self.depth += 1;
                let value = self.array();
                self.depth -= 1;
                value
            }
            Some(b'"') => self.string().map(Json::String),
            Some(b't') => {
                self.literal("true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.literal("false")?;
                Ok(Json::Bool(false))
            }
            Some(b'n') => {
                self.literal("null")?;
                Ok(Json::Null)
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err("unexpected character".to_string()),
            None => Err("unexpected end".to_string()),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.position += 1;
        let mut entries = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.position += 1;
            return Ok(Json::Object(entries));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            if self.next() != Some(b':') {
                return Err("expected ':'".to_string());
            }
            let value = self.value()?;
            entries.push((key, value));
            self.skip_whitespace();
            match self.next() {
                Some(b',') => continue,
                Some(b'}') => break,
                _ => return Err("expected ',' or '}'".to_string()),
            }
        }
        Ok(Json::Object(entries))
    }

    fn array(&mut self) -> Result<Json, String> {
        self.position += 1;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.position += 1;
            return Ok(Json::Array(values));
        }
        loop {
            values.push(self.value()?);
            self.skip_whitespace();
            match self.next() {
                Some(b',') => continue,
                Some(b']') => break,
                _ => return Err("expected ',' or ']'".to_string()),
            }
        }
        Ok(Json::Array(values))
    }

    fn string(&mut self) -> Result<String, String> {
        if self.next() != Some(b'"') {
            return Err("expected string".to_string());
        }
        let mut out: Vec<u8> = Vec::new();
        loop {
            match self.next() {
                None => return Err("unterminated string".to_string()),
                Some(b'"') => break,
                Some(b'\\') => match self.next() {
                    Some(b'"') => out.push(b'"'),
                    Some(b'\\') => out.push(b'\\'),
                    Some(b'/') => out.push(b'/'),
                    Some(b'b') => out.push(0x08),
                    Some(b'f') => out.push(0x0C),
                    Some(b'n') => out.push(b'\n'),
                    Some(b'r') => out.push(b'\r'),
                    Some(b't') => out.push(b'\t'),
                    Some(b'u') => {
                        let mut code = self.read_hex4()?;
                        let mut character = '\u{FFFD}';
                        if (0xD800..=0xDBFF).contains(&code) {
                            // High surrogate: combine it with a following low surrogate.
                            if self.bytes.get(self.position..self.position + 2) == Some(b"\\u") {
                                let saved = self.position;
                                self.position += 2;
                                let low = self.read_hex4()?;
                                if (0xDC00..=0xDFFF).contains(&low) {
                                    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                    character = char::from_u32(code).unwrap_or('\u{FFFD}');
                                } else {
                                    self.position = saved;
                                }
                            }
                        } else {
                            character = char::from_u32(code).unwrap_or('\u{FFFD}');
                        }
                        let mut buffer = [0u8; 4];
                        out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                    }
                    _ => return Err("invalid escape".to_string()),
                },
                // Raw bytes are UTF-8: keep them and decode the complete string at the end.
                Some(byte) => out.push(byte),
            }
        }
        String::from_utf8(out).map_err(|_| "invalid UTF-8 in string".to_string())
    }

    /// Reads exactly four hex digits of a `\uXXXX` escape.
    fn read_hex4(&mut self) -> Result<u32, String> {
        let bytes = self
            .bytes
            .get(self.position..self.position + 4)
            .ok_or("truncated \\u".to_string())?;
        let mut code = 0u32;
        for byte in bytes {
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'f' => u32::from(byte - b'a') + 10,
                b'A'..=b'F' => u32::from(byte - b'A') + 10,
                _ => return Err("invalid \\u".to_string()),
            };
            code = code * 16 + digit;
        }
        self.position += 4;
        Ok(code)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.position;
        while self.peek().is_some_and(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E')
        }) {
            self.position += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.position])
            .ok()
            .and_then(|text| text.parse::<f64>().ok())
            .map(Json::Number)
            .ok_or_else(|| "invalid number".to_string())
    }

    fn literal(&mut self, literal: &str) -> Result<(), String> {
        if self.bytes[self.position..].starts_with(literal.as_bytes()) {
            self.position += literal.len();
            Ok(())
        } else {
            Err(format!("expected `{literal}`"))
        }
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }
    fn next(&mut self) -> Option<u8> {
        let byte = self.peek();
        if byte.is_some() {
            self.position += 1;
        }
        byte
    }
}
