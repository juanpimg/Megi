//! Minimal, dependency-free XML subset parser for SVG documents.
//!
//! The parser is deliberately small: it ignores the prolog, `DOCTYPE`,
//! processing instructions, comments and CDATA (whose content is discarded),
//! decodes the basic XML entities and keeps a tree of elements plus their
//! non-whitespace text. Namespaces are tolerated but not interpreted: callers
//! use [`local_name`] to ignore prefixes. Every limit is explicit and every
//! malformed input produces an [`SvgError`], never a panic.

use crate::{Limits, SvgError};

/// A parsed XML element.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct XmlElement {
    /// Tag name including any namespace prefix.
    pub name: String,
    /// Raw attributes in document order (values entity-decoded).
    pub attributes: Vec<(String, String)>,
    /// Child nodes in document order.
    pub children: Vec<XmlNode>,
}

impl XmlElement {
    /// Returns the value of the first attribute with the given local name.
    pub fn attribute(&self, local: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(name, _)| local_name(name).eq_ignore_ascii_case(local))
            .map(|(_, value)| value.as_str())
    }

    /// Returns the element's local name (namespace prefix stripped, lowercased).
    pub fn local_name_lower(&self) -> String {
        local_name(&self.name).to_ascii_lowercase()
    }
}

/// A child node: either an element or non-whitespace text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum XmlNode {
    Element(XmlElement),
    Text(String),
}

/// Strips an XML namespace prefix (`svg:rect` → `rect`).
pub(crate) fn local_name(name: &str) -> &str {
    match name.rsplit_once(':') {
        Some((_, local)) => local,
        None => name,
    }
}

/// Parses the document bytes and returns the root element.
pub(crate) fn parse_document(bytes: &[u8], limits: &Limits) -> Result<XmlElement, SvgError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| SvgError::Xml(format!("input is not valid UTF-8: {error}")))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let chars: Vec<char> = text.chars().collect();
    let mut parser = Parser {
        chars: &chars,
        position: 0,
        elements: 0,
        limits,
    };
    parser.skip_misc()?;
    if parser.position >= parser.chars.len() {
        return Err(SvgError::Empty);
    }
    if parser.chars[parser.position] != '<' {
        return Err(SvgError::Xml("expected a root element".to_owned()));
    }
    let root = parser.parse_element(0)?;
    parser.skip_misc()?;
    if parser.position < parser.chars.len() {
        return Err(SvgError::Xml(
            "trailing content after the root element".to_owned(),
        ));
    }
    Ok(root)
}

struct Parser<'a> {
    chars: &'a [char],
    position: usize,
    elements: usize,
    limits: &'a Limits,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.position + offset).copied()
    }

    fn starts_with(&self, needle: &str) -> bool {
        needle
            .chars()
            .enumerate()
            .all(|(index, expected)| self.peek_at(index).is_some_and(|found| found == expected))
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.position += 1;
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(is_xml_whitespace) {
            self.position += 1;
        }
    }

    fn skip_until(&mut self, needle: &str) -> Result<(), SvgError> {
        while self.position < self.chars.len() {
            if self.starts_with(needle) {
                self.position += needle.chars().count();
                return Ok(());
            }
            self.position += 1;
        }
        Err(SvgError::Xml(format!(
            "unterminated construct, expected {needle:?}"
        )))
    }

    /// Skips whitespace, comments, processing instructions and the prolog/DOCTYPE.
    fn skip_misc(&mut self) -> Result<(), SvgError> {
        loop {
            self.skip_whitespace();
            if self.starts_with("<!--") {
                self.position += 4;
                self.skip_until("-->")?;
            } else if self.starts_with("<?") {
                self.position += 2;
                self.skip_until("?>")?;
            } else if self.starts_with("<!") {
                self.skip_markup_declaration()?;
            } else {
                return Ok(());
            }
        }
    }

    fn skip_markup_declaration(&mut self) -> Result<(), SvgError> {
        // `<!DOCTYPE ...>` (possibly with an internal subset) or another `<!...>`.
        self.position += 2;
        if self.starts_with("[CDATA[") {
            self.position += 7;
            return self.skip_until("]]>");
        }
        let mut bracket_depth = 0usize;
        let mut quote: Option<char> = None;
        while let Some(ch) = self.bump() {
            match quote {
                Some(active) if ch == active => quote = None,
                Some(_) => {}
                None => match ch {
                    '"' | '\'' => quote = Some(ch),
                    '[' => bracket_depth += 1,
                    ']' => bracket_depth = bracket_depth.saturating_sub(1),
                    '>' if bracket_depth == 0 => return Ok(()),
                    _ => {}
                },
            }
        }
        Err(SvgError::Xml("unterminated <!...> declaration".to_owned()))
    }

    fn parse_element(&mut self, depth: usize) -> Result<XmlElement, SvgError> {
        if depth > self.limits.max_depth {
            return Err(SvgError::Limit(format!(
                "element nesting deeper than {}",
                self.limits.max_depth
            )));
        }
        self.elements += 1;
        if self.elements > self.limits.max_elements {
            return Err(SvgError::Limit(format!(
                "more than {} elements",
                self.limits.max_elements
            )));
        }
        if self.bump() != Some('<') {
            return Err(SvgError::Xml("expected '<'".to_owned()));
        }
        if self.peek().is_some_and(|ch| matches!(ch, '!' | '?' | '/')) {
            return Err(SvgError::Xml("unexpected markup declaration".to_owned()));
        }
        let name = self.parse_name()?;
        let mut element = XmlElement {
            name,
            attributes: Vec::new(),
            children: Vec::new(),
        };
        // Attributes.
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('>') => {
                    self.position += 1;
                    break;
                }
                Some('/') => {
                    self.position += 1;
                    if self.bump() != Some('>') {
                        return Err(SvgError::Xml("expected '/>'".to_owned()));
                    }
                    return Ok(element);
                }
                None => return Err(SvgError::Xml("truncated start tag".to_owned())),
                _ => {
                    if element.attributes.len() >= self.limits.max_attributes {
                        return Err(SvgError::Limit(format!(
                            "more than {} attributes on one element",
                            self.limits.max_attributes
                        )));
                    }
                    let attribute_name = self.parse_name()?;
                    self.skip_whitespace();
                    if self.bump() != Some('=') {
                        return Err(SvgError::Xml(format!(
                            "attribute {attribute_name:?} is missing '='"
                        )));
                    }
                    self.skip_whitespace();
                    let quote = self
                        .bump()
                        .filter(|ch| matches!(ch, '"' | '\''))
                        .ok_or_else(|| {
                            SvgError::Xml(format!("attribute {attribute_name:?} is not quoted"))
                        })?;
                    let mut raw = String::new();
                    loop {
                        match self.bump() {
                            Some(ch) if ch == quote => break,
                            Some('&') => raw.push(self.parse_entity()?),
                            Some(ch) => raw.push(ch),
                            None => {
                                return Err(SvgError::Xml(format!(
                                    "unterminated value for attribute {attribute_name:?}"
                                )));
                            }
                        }
                    }
                    if !element
                        .attributes
                        .iter()
                        .any(|(existing, _)| existing.eq_ignore_ascii_case(&attribute_name))
                    {
                        element.attributes.push((attribute_name, raw));
                    }
                }
            }
        }
        // Children until the matching end tag.
        loop {
            if self.position >= self.chars.len() {
                return Err(SvgError::Xml(format!(
                    "unclosed element <{}>",
                    element.name
                )));
            }
            if self.starts_with("<!--") {
                self.position += 4;
                self.skip_until("-->")?;
                continue;
            }
            if self.starts_with("<?") {
                self.position += 2;
                self.skip_until("?>")?;
                continue;
            }
            if self.starts_with("<![CDATA[") {
                // CDATA content is kept as text: inline `<style>` blocks frequently use
                // `<![CDATA[ ... ]]>` and the CSS/selector layer needs that text.
                self.position += 9;
                let start = self.position;
                self.skip_until("]]>")?;
                let text: String = self.chars[start..self.position - 3].iter().collect();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    element.children.push(XmlNode::Text(trimmed.to_owned()));
                }
                continue;
            }
            if self.starts_with("<!") {
                self.skip_markup_declaration()?;
                continue;
            }
            if self.starts_with("</") {
                self.position += 2;
                let end_name = self.parse_name()?;
                self.skip_whitespace();
                if self.bump() != Some('>') {
                    return Err(SvgError::Xml(format!("malformed end tag </{end_name}>")));
                }
                if !end_name.eq_ignore_ascii_case(&element.name) {
                    return Err(SvgError::Xml(format!(
                        "mismatched end tag: expected </{}>, found </{end_name}>",
                        element.name
                    )));
                }
                return Ok(element);
            }
            if self.peek() == Some('<') {
                let child = self.parse_element(depth + 1)?;
                element.children.push(XmlNode::Element(child));
                continue;
            }
            // Text content.
            let mut raw = String::new();
            loop {
                match self.peek() {
                    Some('<') | None => break,
                    Some('&') => {
                        self.position += 1;
                        raw.push(self.parse_entity()?);
                    }
                    Some(ch) => {
                        self.position += 1;
                        raw.push(ch);
                    }
                }
            }
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                element.children.push(XmlNode::Text(trimmed.to_owned()));
            }
        }
    }

    fn parse_name(&mut self) -> Result<String, SvgError> {
        let start = self.position;
        while let Some(ch) = self.peek() {
            let valid = ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':');
            if !valid {
                break;
            }
            self.position += 1;
        }
        if start == self.position {
            return Err(SvgError::Xml(
                "expected an element or attribute name".to_owned(),
            ));
        }
        Ok(self.chars[start..self.position].iter().collect())
    }

    fn parse_entity(&mut self) -> Result<char, SvgError> {
        let start = self.position;
        let mut digits = String::new();
        while let Some(ch) = self.peek() {
            if ch == ';' {
                self.position += 1;
                break;
            }
            if digits.len() > 16 {
                return Err(SvgError::Xml("entity reference is too long".to_owned()));
            }
            digits.push(ch);
            self.position += 1;
        }
        if self.chars.get(start.saturating_sub(1)) != Some(&'&')
            || !self.chars[self.position - 1].eq(&';')
        {
            return Err(SvgError::Xml("unterminated entity reference".to_owned()));
        }
        let entity = digits.as_str();
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => {
                entity[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        // Unknown entities are kept literally (tolerant SVG parsing).
        Ok(decoded.unwrap_or('\u{fffd}'))
    }
}

fn is_xml_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parses_prolog_comments_attributes_and_entities() {
        let input = br##"<?xml version="1.0"?>
            <!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
            <!-- comment --><svg:svg xmlns:svg="http://www.w3.org/2000/svg" width="10">
              <svg:rect x="1" y="2" fill="#ff0000"/><title>a &amp; b</title>
            </svg:svg>"##;
        let root = parse_document(input, &Limits::default()).expect("parse");
        assert_eq!(local_name(&root.name), "svg");
        assert_eq!(root.attribute("width"), Some("10"));
        assert_eq!(root.children.len(), 2);
        match &root.children[0] {
            XmlNode::Element(element) => {
                assert_eq!(local_name(&element.name), "rect");
                assert_eq!(element.attribute("fill"), Some("#ff0000"));
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn test_truncated_and_mismatched_input_errors() {
        assert!(parse_document(b"<svg><rect", &Limits::default()).is_err());
        assert!(parse_document(b"<svg></g>", &Limits::default()).is_err());
        assert!(parse_document(b"not xml", &Limits::default()).is_err());
        assert!(parse_document(b"", &Limits::default()).is_err());
    }

    #[test]
    fn test_depth_limit_is_enforced() {
        let mut input = String::from("<svg>");
        for _ in 0..20 {
            input.push_str("<g>");
        }
        for _ in 0..20 {
            input.push_str("</g>");
        }
        input.push_str("</svg>");
        let limits = Limits {
            max_depth: 4,
            ..Limits::default()
        };
        assert!(matches!(
            parse_document(input.as_bytes(), &limits),
            Err(SvgError::Limit(_))
        ));
    }

    #[test]
    fn test_self_closing_and_cdata_are_tolerated() {
        let input = br##"<svg><rect width="1"/><![CDATA[.a{fill:red}]]></svg>"##;
        let root = parse_document(input, &Limits::default()).expect("parse");
        assert_eq!(root.children.len(), 2);
        assert!(matches!(&root.children[1], XmlNode::Text(text) if text == ".a{fill:red}"));
    }
}
