//! Import maps (subset of the HTML standard's import maps): exact and prefix mappings plus
//! `null` fallbacks. The map is installed with [`crate::JsRuntime::set_import_map_json`] and
//! consulted **before** the host module resolver (`set_module_resolver`), so a bare specifier
//! can be remapped to a URL without extra host plumbing.
//!
//! Supported shape:
//! ```json
//! {"imports":{"react":"https://cdn.example/react.js","utils/":"https://cdn.example/utils/",
//!             "blocked":null}}
//! ```
//! Exact keys are matched first, then the **longest prefix** ending in `/`; a `null` target
//! blocks the specifier ([`JsError::ImportMapBlocked`]). `scopes`/`integrity` are ignored
//! (documented subset); relative values are used verbatim (callers should pass absolute
//! URLs). Invalid JSON or non-object roots are typed errors.

use std::collections::BTreeMap;

use crate::error::JsError;

/// Parsed import map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportMap {
    exact: BTreeMap<String, Option<String>>,
    prefixes: BTreeMap<String, Option<String>>,
}

impl ImportMap {
    /// Parses an import map from its JSON source.
    pub fn from_json(source: &str) -> Result<Self, JsError> {
        let value = Json::parse(source)
            .map_err(|error| JsError::ImportMap(format!("invalid JSON: {error}")))?;
        let root = value
            .as_object()
            .ok_or_else(|| JsError::ImportMap("root is not an object".to_string()))?;
        let mut map = Self::default();
        if let Some(imports) = root
            .iter()
            .find(|(key, _)| key == "imports")
            .map(|(_, value)| value)
        {
            let imports = imports
                .as_object()
                .ok_or_else(|| JsError::ImportMap("`imports` is not an object".to_string()))?;
            for (specifier, target) in imports {
                let target = match target {
                    Json::Null => None,
                    Json::String(value) => Some(value.clone()),
                    _ => {
                        return Err(JsError::ImportMap(format!(
                            "mapping for `{specifier}` must be a string or null"
                        )));
                    }
                };
                if specifier.ends_with('/') {
                    map.prefixes.insert(specifier.clone(), target);
                } else {
                    map.exact.insert(specifier.clone(), target);
                }
            }
        }
        Ok(map)
    }

    /// Resolves `specifier`: `Some(Some(url))` when mapped, `Some(None)` when explicitly
    /// blocked, `None` when the map has no entry (host resolver takes over).
    pub fn resolve(&self, specifier: &str) -> Option<Option<String>> {
        if let Some(target) = self.exact.get(specifier) {
            return Some(target.clone());
        }
        let mut best: Option<(&String, &Option<String>)> = None;
        for (prefix, target) in &self.prefixes {
            if specifier.starts_with(prefix)
                && best.is_none_or(|(current, _)| prefix.len() > current.len())
            {
                best = Some((prefix, target));
            }
        }
        best.map(|(prefix, target)| {
            target
                .as_ref()
                .map(|base| format!("{base}{}", &specifier[prefix.len()..]))
        })
    }

    /// Number of exact mappings.
    pub fn exact_len(&self) -> usize {
        self.exact.len()
    }

    /// Number of prefix mappings.
    pub fn prefix_len(&self) -> usize {
        self.prefixes.len()
    }
}

/// Minimal JSON value used by the import-map parser.
#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }

    fn parse(text: &str) -> Result<Self, String> {
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
const MAX_JSON_DEPTH: usize = 128;

struct Parser<'a> {
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
        let mut out = String::new();
        loop {
            match self.next() {
                None => return Err("unterminated string".to_string()),
                Some(b'"') => break,
                Some(b'\\') => match self.next() {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'u') => {
                        let hex = std::str::from_utf8(
                            self.bytes
                                .get(self.position..self.position + 4)
                                .ok_or("truncated \\u")?,
                        )
                        .map_err(|_| "invalid \\u".to_string())?;
                        let code =
                            u32::from_str_radix(hex, 16).map_err(|_| "invalid \\u".to_string())?;
                        self.position += 4;
                        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    _ => return Err("invalid escape".to_string()),
                },
                Some(byte) => out.push(byte as char),
            }
        }
        Ok(out)
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
