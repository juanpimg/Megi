//! WHATWG percent-encoding sets and escape normalization.

// ---------------------------------------------------------------------------
// Percent encoding (WHATWG encode sets)
// ---------------------------------------------------------------------------

const HEX_DIGITS: [u8; 16] = *b"0123456789ABCDEF";

pub(super) fn push_encoded_byte(out: &mut String, byte: u8) {
    out.push('%');
    out.push(HEX_DIGITS[(byte >> 4) as usize] as char);
    out.push(HEX_DIGITS[(byte & 0x0F) as usize] as char);
}

pub(super) fn encode_with(input: &str, requires_encoding: impl Fn(u8) -> bool) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii() {
            let byte = ch as u8;
            if requires_encoding(byte) {
                push_encoded_byte(&mut out, byte);
            } else {
                out.push(ch);
            }
        } else {
            let mut buffer = [0u8; 4];
            for byte in ch.encode_utf8(&mut buffer).bytes() {
                push_encoded_byte(&mut out, byte);
            }
        }
    }
    out
}

/// WHATWG *path percent-encode set*: C0 controls, space, `" # < > ? ` { }` and non-ASCII.
pub(super) fn path_requires_encoding(byte: u8) -> bool {
    !(0x20..=0x7E).contains(&byte)
        || matches!(
            byte,
            b' ' | b'"' | b'#' | b'<' | b'>' | b'?' | b'`' | b'{' | b'}'
        )
}

/// WHATWG query percent-encode set. For special schemes the query state uses the
/// *special-query* set, which additionally escapes `'` (URL §4.4, #1406).
pub(super) fn query_requires_encoding(byte: u8, special: bool) -> bool {
    !(0x20..=0x7E).contains(&byte)
        || matches!(byte, b' ' | b'"' | b'#' | b'<' | b'>')
        || (special && byte == b'\'')
}

/// WHATWG *fragment percent-encode set*.
pub(super) fn fragment_requires_encoding(byte: u8) -> bool {
    !(0x20..=0x7E).contains(&byte) || matches!(byte, b' ' | b'"' | b'<' | b'>' | b'`')
}

/// WHATWG *userinfo percent-encode set*.
pub(super) fn userinfo_requires_encoding(byte: u8) -> bool {
    path_requires_encoding(byte)
        || matches!(
            byte,
            b'/' | b':' | b';' | b'=' | b'@' | b'[' | b'\\' | b']' | b'^' | b'|'
        )
}

/// Rewrites `%XX` escapes to uppercase and encodes a `%` that does not start a valid escape as
/// `%25`, so an invalid escape can never reach the wire or a cache key (#641).
pub(super) fn normalize_escapes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex_value);
            let low = bytes.get(index + 2).copied().and_then(hex_value);
            match (high, low) {
                (Some(high), Some(low)) => {
                    push_encoded_byte(&mut out, (high << 4) | low);
                    index += 3;
                }
                _ => {
                    push_encoded_byte(&mut out, b'%');
                    index += 1;
                }
            }
            continue;
        }
        let ch = input[index..]
            .chars()
            .next()
            .expect("index on char boundary");
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

pub(super) fn encode_path(input: &str) -> String {
    encode_with(&normalize_escapes(input), path_requires_encoding)
}

pub(super) fn encode_query(input: &str, special: bool) -> String {
    encode_with(input, |byte| query_requires_encoding(byte, special))
}

pub(super) fn encode_fragment(input: &str) -> String {
    encode_with(input, fragment_requires_encoding)
}

pub(super) fn encode_userinfo(input: &str) -> String {
    encode_with(input, userinfo_requires_encoding)
}

pub(super) fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
