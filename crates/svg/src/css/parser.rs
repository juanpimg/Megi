use super::{
    Declaration, MAX_DECLARATIONS_PER_RULE, MAX_RULES, MAX_SELECTOR_PARTS, MAX_SELECTORS_PER_RULE,
    MAX_STYLESHEET_BYTES, Selector, SimpleSelector, StyleRule,
};

/// Parses a stylesheet into rules, skipping unsupported/invalid constructs.
pub(crate) fn parse_stylesheet(text: &str) -> Vec<StyleRule> {
    if text.len() > MAX_STYLESHEET_BYTES {
        return Vec::new();
    }
    let stripped = strip_comments(text);
    let bytes = stripped.as_bytes();
    let mut rules = Vec::new();
    let mut position = 0usize;
    let mut order = 0usize;
    while position < bytes.len() && rules.len() < MAX_RULES {
        while position < bytes.len() && bytes[position].is_ascii_whitespace() {
            position += 1;
        }
        if position >= bytes.len() {
            break;
        }
        // At-rules: skip the whole block (not supported in this subset).
        if bytes[position] == b'@' {
            position = skip_block(bytes, position);
            continue;
        }
        let selector_start = position;
        let mut block_start = None;
        while position < bytes.len() {
            if bytes[position] == b'{' {
                block_start = Some(position);
                break;
            }
            if bytes[position] == b'}' || bytes[position] == b';' {
                break;
            }
            position += 1;
        }
        let Some(block_start) = block_start else {
            position += 1;
            continue;
        };
        let selector_text = stripped[selector_start..block_start].trim();
        let block_end = find_block_end(bytes, block_start + 1);
        let declaration_text = &stripped[block_start + 1..block_end];
        position = (block_end + 1).min(bytes.len());
        if selector_text.is_empty() || selector_text.len() > 2048 {
            continue;
        }
        let mut selectors = Vec::new();
        for raw in selector_text.split(',') {
            if selectors.len() >= MAX_SELECTORS_PER_RULE {
                break;
            }
            if let Some(selector) = parse_selector(raw) {
                selectors.push(selector);
            }
        }
        if selectors.is_empty() {
            continue;
        }
        let declarations = parse_declarations(declaration_text);
        if declarations.is_empty() {
            continue;
        }
        rules.push(StyleRule {
            selectors,
            declarations,
            order,
        });
        order += 1;
    }
    rules
}

/// Parses a `style="..."` attribute into declarations.
pub(crate) fn parse_declarations(text: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    for raw in text.split(';') {
        if declarations.len() >= MAX_DECLARATIONS_PER_RULE {
            break;
        }
        let Some((name, value)) = raw.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name.is_empty() || value.is_empty() {
            continue;
        }
        declarations.push(Declaration {
            name,
            value: value.to_owned(),
        });
    }
    declarations
}

fn parse_selector(raw: &str) -> Option<Selector> {
    let mut parts = Vec::new();
    // `>` is treated as a descendant combinator (documented approximation).
    for compound in raw.split(|c: char| c.is_ascii_whitespace() || c == '>') {
        if compound.is_empty() {
            continue;
        }
        if parts.len() >= MAX_SELECTOR_PARTS {
            // Fail closed: reject over-long selectors instead of silently truncating.
            return None;
        }
        parts.push(parse_simple_selector(compound)?);
    }
    if parts.is_empty() {
        return None;
    }
    Some(Selector { parts })
}

fn parse_simple_selector(raw: &str) -> Option<SimpleSelector> {
    let bytes = raw.as_bytes();
    let mut index = 0usize;
    let mut tag = None;
    let mut id = None;
    let mut classes = Vec::new();
    if bytes.first().is_some_and(|byte| *byte == b'*') {
        index += 1;
    } else if bytes
        .first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'-'))
    {
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'-'))
        {
            index += 1;
        }
        tag = Some(raw[start..index].to_ascii_lowercase());
    }
    while index < bytes.len() {
        let marker = bytes[index];
        if !matches!(marker, b'.' | b'#') {
            return None;
        }
        index += 1;
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'-'))
        {
            index += 1;
        }
        if start == index {
            return None;
        }
        let name = raw[start..index].to_owned();
        if marker == b'#' {
            if id.is_some() {
                return None;
            }
            id = Some(name);
        } else {
            classes.push(name);
        }
    }
    if tag.is_none() && id.is_none() && classes.is_empty() {
        return None;
    }
    Some(SimpleSelector { tag, id, classes })
}

fn strip_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] == b'/' && bytes.get(position + 1) == Some(&b'*') {
            position += 2;
            while position + 1 < bytes.len()
                && !(bytes[position] == b'*' && bytes[position + 1] == b'/')
            {
                position += 1;
            }
            position = (position + 2).min(bytes.len());
        } else {
            let ch = text[position..].chars().next().unwrap_or('\0');
            result.push(ch);
            position += ch.len_utf8();
        }
    }
    result
}

fn find_block_end(bytes: &[u8], start: usize) -> usize {
    let mut depth = 1usize;
    let mut position = start;
    while position < bytes.len() {
        match bytes[position] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return position;
                }
            }
            _ => {}
        }
        position += 1;
    }
    bytes.len()
}

fn skip_block(bytes: &[u8], start: usize) -> usize {
    let mut position = start;
    while position < bytes.len() && bytes[position] != b'{' && bytes[position] != b';' {
        position += 1;
    }
    if position < bytes.len() && bytes[position] == b'{' {
        find_block_end(bytes, position + 1) + 1
    } else {
        position + 1
    }
}
