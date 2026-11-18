//! Sized submodule of the parent facade (verbatim move).

use super::*;

/// Maximum nested `var()` substitutions.
pub const MAX_SUBSTITUTION_DEPTH: usize = 32;

/// Maximum bytes a single `var()` expansion may produce (about 64× the input, clamped).
///
/// Every reference re-expands its variable, so a chain like `--aN: var(--aN-1) var(--aN-1)`
/// is exponential: 31 doublings asked for ~4 GiB. The budget bounds the work and memory.
pub const MAX_SUBSTITUTION_BYTES: usize = 1024 * 1024;

/// Maximum number of custom properties retained in a single element's map.
///
/// Bounds the O(N*P) clone amplification when many elements each declare `--*`
/// properties (issue #687): once the inherited map exceeds this cap it is not
/// carried forward, so cascade resolution stays linear in the document size.
pub const MAX_CUSTOM_PROPERTIES: usize = 256;

/// Result of [`resolve_declarations`]: owned declarations + shared custom-property map.
pub(crate) type ResolvedDeclarations = (
    Vec<(String, Declaration)>,
    Option<Arc<HashMap<String, String>>>,
);

/// Returns whether `value` contains a `var()` function (case-insensitive).
///
/// Allocation-free: the hot path (`compute` for documents without custom properties) must
/// not lowercase the whole value for every declaration of every element.
pub(crate) fn contains_var(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 4 {
        return false;
    }
    bytes
        .windows(4)
        .any(|window| window[..3].eq_ignore_ascii_case(b"var") && window[3] == b'(')
}

/// Returns whether style computation needs the custom-property machinery for this element.
///
/// `false` means: no local `--*` declarations and no declaration value references `var()`.
/// In that case the parent's [`Arc`] custom-property map is reused as-is (cheap clone) and
/// no declaration is cloned, sorted or scanned for substitution.
pub(crate) fn needs_resolution(cascaded: &HashMap<String, &Declaration>) -> bool {
    cascaded.keys().any(|name| name.starts_with("--"))
        || cascaded
            .values()
            .any(|declaration| contains_var(&declaration.raw_value))
}

/// Expands every `var()` reference in `value` using `custom`.
///
/// Returns `None` when a required variable is missing/cyclic without a fallback, or when the
/// substitution depth exceeds [`MAX_SUBSTITUTION_DEPTH`]; callers then treat the declaration
/// as invalid at computed-value time (inherited property → parent value, otherwise initial).
pub fn expand_variables(value: &str, custom: &HashMap<String, String>) -> Option<String> {
    let mut visiting = HashSet::new();
    let mut budget = value
        .len()
        .saturating_mul(64)
        .clamp(4096, MAX_SUBSTITUTION_BYTES);
    expand_inner(value, custom, 0, &mut visiting, &mut budget)
}

/// Reserves `bytes` from the substitution budget (returns false when exhausted).
pub(crate) fn spend(budget: &mut usize, bytes: usize) -> bool {
    if bytes > *budget {
        return false;
    }
    *budget -= bytes;
    true
}

/// Returns the byte index of the next `var(` function token outside strings and comments.
///
/// The scan is token-aware: `"var(--x)"` inside a string and `myvar(--x)` (a different
/// function) are not substitutions (CSS Syntax: strings are opaque and function names are
/// whole idents).
pub(crate) fn next_var_function(value: &str, from: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut index = from;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(active) => {
                if byte == b'\\' {
                    index += 2;
                    continue;
                }
                if byte == active {
                    quote = None;
                }
                index += 1;
            }
            None => {
                if byte == b'\'' || byte == b'"' {
                    quote = Some(byte);
                    index += 1;
                    continue;
                }
                if bytes[index..].starts_with(b"/*") {
                    let Some(offset) = value[index + 2..].find("*/") else {
                        return None; // unterminated comment: no function token after it
                    };
                    index += 2 + offset + 2;
                    continue;
                }
                if is_ident_byte(byte) {
                    let start = index;
                    while index < bytes.len() && is_ident_byte(bytes[index]) {
                        // Consume continuation bytes of a multi-byte UTF-8 sequence
                        // so the slice stays on char boundaries.
                        if bytes[index] >= 0xC0 {
                            index += 1;
                            while index < bytes.len() && (bytes[index] & 0xC0) == 0x80 {
                                index += 1;
                            }
                        } else {
                            index += 1;
                        }
                    }
                    let name = &value[start..index];
                    if bytes.get(index) == Some(&b'(') {
                        if name.eq_ignore_ascii_case("url") {
                            // A URL token swallows its body: `url(var(--x))` is one token.
                            index = skip_function(value, index)?;
                            continue;
                        }
                        if name.eq_ignore_ascii_case("var")
                            && (start == 0 || !is_ident_byte(bytes[start - 1]))
                        {
                            return Some(start);
                        }
                    }
                    continue;
                }
                if byte == b'#' || byte == b'@' {
                    // `#var(`/`@var(` are hash/at-keyword tokens followed by a block, not
                    // Function tokens.
                    index += 1;
                    while index < bytes.len() && is_ident_byte(bytes[index]) {
                        if bytes[index] >= 0xC0 {
                            index += 1;
                            while index < bytes.len() && (bytes[index] & 0xC0) == 0x80 {
                                index += 1;
                            }
                        } else {
                            index += 1;
                        }
                    }
                    continue;
                }
                index += 1;
            }
        }
    }
    None
}

/// Whether `byte` can be part of a CSS identifier (so `myvar(` is not `var(`).
pub(crate) fn is_ident_byte(byte: u8) -> bool {
    // Accept ASCII ident chars and any byte that is part of a valid UTF-8
    // sequence (lead or continuation). This keeps byte-level scanning safe
    // while still rejecting pure ASCII punctuation.
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte >= 0x80
}

/// Skips a balanced function body starting at the `(` byte; returns the index after `)`.
pub(crate) fn skip_function(value: &str, open: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut index = open + 1;
    let mut depth = 1usize;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(active) => {
                if byte == b'\\' {
                    index += 2;
                    continue;
                }
                if byte == active {
                    quote = None;
                }
                index += 1;
            }
            None => match byte {
                b'\'' | b'"' => {
                    quote = Some(byte);
                    index += 1;
                }
                b'(' => {
                    depth += 1;
                    index += 1;
                }
                b')' => {
                    depth -= 1;
                    index += 1;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                _ => index += 1,
            },
        }
    }
    None
}

pub(crate) fn expand_inner(
    value: &str,
    custom: &HashMap<String, String>,
    depth: usize,
    visiting: &mut HashSet<String>,
    budget: &mut usize,
) -> Option<String> {
    if depth > MAX_SUBSTITUTION_DEPTH {
        return None;
    }
    // Charge the work itself, not only the emitted bytes: a chain that expands to the empty
    // string (e.g. `var(--missing,)`) used to cost zero and recursed ~2^30 times.
    if !spend(budget, 1) {
        return None;
    }
    let mut out = String::with_capacity(value.len());
    let mut index = 0usize;
    while index < value.len() {
        if let Some(start) = next_var_function(value, index) {
            let literal = &value[index..start];
            if !spend(budget, literal.len()) {
                return None;
            }
            out.push_str(literal);
            if out.len() > MAX_SUBSTITUTION_BYTES {
                return None;
            }
            // Find the matching closing parenthesis (quotes and nesting aware).
            let mut cursor = start + 4;
            let mut depth_parens = 0usize;
            let mut quote: Option<char> = None;
            // A backslash consumes the next code point (CSS Syntax 3 §4.3.7), so an escaped
            // quote or parenthesis must not change the state (#684).
            let mut escaped = false;
            let mut end = None;
            for (offset, ch) in value[cursor..].char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                match quote {
                    Some(active) => {
                        if ch == '\\' {
                            escaped = true;
                        } else if ch == active {
                            quote = None;
                        }
                    }
                    None => match ch {
                        '\\' => escaped = true,
                        '\'' | '"' => quote = Some(ch),
                        '(' => depth_parens += 1,
                        ')' if depth_parens == 0 => {
                            end = Some(cursor + offset);
                            break;
                        }
                        ')' => depth_parens -= 1,
                        _ => {}
                    },
                }
            }
            let Some(end) = end else {
                // Unbalanced var(): invalid value.
                return None;
            };
            let inner = &value[cursor..end];
            let (name, fallback) = split_first_comma(inner);
            let name = name.trim();
            let substituted = if name.starts_with("--") && !name.is_empty() {
                if let Some(raw) = custom.get(name) {
                    if visiting.contains(name) {
                        fallback.as_deref().and_then(|fallback| {
                            expand_inner(fallback, custom, depth + 1, visiting, budget)
                        })
                    } else {
                        visiting.insert(name.to_string());
                        let result = expand_inner(raw, custom, depth + 1, visiting, budget);
                        visiting.remove(name);
                        result.or_else(|| {
                            fallback.as_deref().and_then(|fallback| {
                                expand_inner(fallback, custom, depth + 1, visiting, budget)
                            })
                        })
                    }
                } else {
                    fallback.as_deref().and_then(|fallback| {
                        expand_inner(fallback, custom, depth + 1, visiting, budget)
                    })
                }
            } else {
                None
            };
            let substituted = substituted?;
            // The recursive expansion already charged the substituted bytes; charging them
            // again here rejected legitimate values larger than half the budget.
            out.push_str(&substituted);
            if out.len() > MAX_SUBSTITUTION_BYTES {
                return None;
            }
            index = end + 1;
            cursor = index;
            let _ = cursor;
        } else {
            let tail = &value[index..];
            if !spend(budget, tail.len()) {
                return None;
            }
            out.push_str(tail);
            if out.len() > MAX_SUBSTITUTION_BYTES {
                return None;
            }
            break;
        }
    }
    Some(out)
}

/// Splits `arg, fallback` on the first top-level comma (quotes/parentheses aware).
pub(crate) fn split_first_comma(inner: &str) -> (String, Option<String>) {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (offset, ch) in inner.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(active) => {
                if ch == '\\' {
                    escaped = true;
                } else if ch == active {
                    quote = None;
                }
            }
            None => match ch {
                '\\' => escaped = true,
                '\'' | '"' => quote = Some(ch),
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    return (
                        inner[..offset].to_string(),
                        Some(inner[offset + 1..].to_string()),
                    );
                }
                _ => {}
            },
        }
    }
    (inner.to_string(), None)
}

pub(crate) fn normalize_custom_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("initial")
        || trimmed.eq_ignore_ascii_case("unset")
        || trimmed.eq_ignore_ascii_case("inherit")
    {
        None
    } else {
        Some(trimmed.to_string())
    }
}
