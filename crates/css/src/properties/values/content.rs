//! CSS `content` generated-value types and counter parsing.

use crate::properties::parse::parse_image_value;
use std::collections::HashMap;

/// One item of a computed CSS `content` list (CSS Generated Content Level 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentItem {
    /// Literal quoted string (CSS escapes decoded).
    String(String),
    /// `attr(<name>)` evaluated against the originating DOM element's attribute.
    Attr(String),
    /// `counter(<name>)` evaluated against the current CSS counter scope.
    Counter(String),
    /// `open-quote` (`true`) or `close-quote` (`false`).
    Quote(bool),
    /// `url(<url>)` image content (raw CSS URL as authored).
    Url(String),
}

/// Computed value of the CSS `content` property (`::before` / `::after`, #1540).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Content {
    /// Initial value `normal` (generates no box on `::before`/`::after`).
    #[default]
    Normal,
    /// `none` (generates no box on `::before`/`::after`).
    None,
    /// Ordered list of content items (even `""` generates a box).
    Items(Vec<ContentItem>),
}

impl Content {
    /// Parses CSS `content` (`normal`, `none`, quoted strings, `attr(...)`, `counter(...)`, `open-quote`, `close-quote`, `url(...)`).
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.eq_ignore_ascii_case("normal") {
            return Some(Self::Normal);
        }
        if trimmed.eq_ignore_ascii_case("none") {
            return Some(Self::None);
        }

        let mut items = Vec::new();
        let bytes = trimmed.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            // Optional alt-text separator (`/` "alt text" in CSS Content 3): stop before fallback alt text.
            if bytes[i] == b'/' {
                break;
            }
            if bytes[i] == b'"' || bytes[i] == b'\'' {
                let quote = bytes[i];
                i += 1;
                let mut out = String::new();
                let mut closed = false;
                let rest = &trimmed[i..];
                let mut chars = rest.char_indices().peekable();
                while let Some((offset, ch)) = chars.next() {
                    if ch as u32 == u32::from(quote) {
                        i += offset + ch.len_utf8();
                        closed = true;
                        break;
                    }
                    if ch == '\\' {
                        match chars.peek().copied() {
                            Some((_, '\n')) => {
                                chars.next();
                            }
                            Some((_, c)) if c.is_ascii_hexdigit() => {
                                let mut code: u32 = 0;
                                let mut digits = 0;
                                while digits < 6 {
                                    let Some(&(_, h)) = chars.peek() else {
                                        break;
                                    };
                                    if !h.is_ascii_hexdigit() {
                                        break;
                                    }
                                    chars.next();
                                    code = code * 16 + h.to_digit(16).unwrap_or(0);
                                    digits += 1;
                                }
                                // Optional single whitespace terminates hex escape.
                                if chars
                                    .peek()
                                    .is_some_and(|&(_, ws)| ws.is_ascii_whitespace())
                                {
                                    chars.next();
                                }
                                if let Some(decoded) = char::from_u32(code).filter(|&c| c != '\0') {
                                    out.push(decoded);
                                } else {
                                    out.push('\u{FFFD}');
                                }
                            }
                            Some((_, c)) => {
                                chars.next();
                                out.push(c);
                            }
                            None => {}
                        }
                    } else {
                        out.push(ch);
                    }
                }
                if !closed {
                    return None;
                }
                items.push(ContentItem::String(out));
                continue;
            }

            // Read keyword or function token (`attr(...)`, `counter(...)`, `url(...)`, `open-quote`, ...).
            let start = i;
            let mut has_paren = false;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'/' {
                if bytes[i] == b'(' {
                    has_paren = true;
                    i += 1;
                    let mut depth = 1usize;
                    let mut quote_in_fn: Option<u8> = None;
                    while i < bytes.len() && depth > 0 {
                        let b = bytes[i];
                        if let Some(q) = quote_in_fn {
                            if b == b'\\' && i + 1 < bytes.len() {
                                i += 2;
                                continue;
                            }
                            if b == q {
                                quote_in_fn = None;
                            }
                            i += 1;
                            continue;
                        }
                        if b == b'"' || b == b'\'' {
                            quote_in_fn = Some(b);
                        } else if b == b'(' {
                            depth += 1;
                        } else if b == b')' {
                            depth -= 1;
                        }
                        i += 1;
                    }
                    if depth != 0 {
                        return None;
                    }
                    break;
                }
                i += 1;
            }
            let token = &trimmed[start..i];
            if has_paren {
                let lower = token.to_ascii_lowercase();
                if lower.starts_with("attr(") && token.ends_with(')') {
                    let inner = token[5..token.len() - 1].trim();
                    if inner.is_empty() {
                        return None;
                    }
                    items.push(ContentItem::Attr(inner.to_string()));
                } else if lower.starts_with("counter(") && token.ends_with(')') {
                    let inner = token[8..token.len() - 1].trim();
                    let name = inner.split(',').next().map(str::trim).unwrap_or("");
                    if name.is_empty() {
                        return None;
                    }
                    items.push(ContentItem::Counter(name.to_string()));
                } else if lower.starts_with("url(") && token.ends_with(')') {
                    let url = parse_image_value(token)?;
                    items.push(ContentItem::Url(url));
                } else {
                    return None;
                }
            } else {
                match token.to_ascii_lowercase().as_str() {
                    "open-quote" => items.push(ContentItem::Quote(true)),
                    "close-quote" => items.push(ContentItem::Quote(false)),
                    "no-open-quote" | "no-close-quote" => {
                        items.push(ContentItem::String(String::new()));
                    }
                    _ => return None,
                }
            }
        }
        if items.is_empty() {
            None
        } else {
            Some(Self::Items(items))
        }
    }

    /// Returns `true` if this `content` generates a pseudo-element box (i.e. neither `normal` nor `none`).
    pub fn generates_box(&self) -> bool {
        matches!(self, Self::Items(_))
    }

    /// Returns the first `url(...)` item in `content`, if any.
    pub fn image_url(&self) -> Option<&str> {
        let Self::Items(items) = self else {
            return None;
        };
        items.iter().find_map(|item| match item {
            ContentItem::Url(url) => Some(url.as_str()),
            _ => None,
        })
    }

    /// Resolves textual content items against an originating DOM element and counter map.
    pub fn resolve_text(
        &self,
        doc: &megi_core::Document,
        originating: megi_core::NodeId,
        counters: &HashMap<String, i32>,
    ) -> Option<String> {
        let Self::Items(items) = self else {
            return None;
        };
        let element = doc.node(originating).as_element();
        let mut out = String::new();
        for item in items {
            match item {
                ContentItem::String(s) => out.push_str(s),
                ContentItem::Attr(name) => {
                    if let Some(el) = element
                        && let Some(val) = el.get_attribute(name)
                    {
                        out.push_str(val);
                    }
                }
                ContentItem::Counter(name) => {
                    let value = counters.get(name).copied().unwrap_or(0);
                    out.push_str(&value.to_string());
                }
                ContentItem::Quote(open) => {
                    out.push(if *open { '\u{201C}' } else { '\u{201D}' });
                }
                ContentItem::Url(_) => {}
            }
        }
        Some(out)
    }
}

/// Parses `counter-reset` / `counter-increment` declaration values (`none` or `<name> [<integer>]`).
pub(crate) fn parse_counter_list(raw: &str, default_value: i32) -> Vec<(String, i32)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let tokens: Vec<&str> = trimmed.split_ascii_whitespace().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let name = tokens[i];
        let value = if i + 1 < tokens.len() {
            if let Ok(parsed) = tokens[i + 1].parse::<i32>() {
                i += 2;
                parsed
            } else {
                i += 1;
                default_value
            }
        } else {
            i += 1;
            default_value
        };
        result.push((name.to_string(), value));
    }
    result
}
