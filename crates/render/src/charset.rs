//! Character-encoding detection and decoding for HTML documents.
//!
//! Supported encodings (documented subset, browsers support many more):
//!
//! 1. UTF-8 with BOM (`EF BB BF`): decoded as UTF-8 and the BOM is stripped.
//! 2. The `charset` parameter of the resource `Content-Type` header.
//! 3. The `<meta charset=...>` / `<meta http-equiv="Content-Type" content="...charset=...">`
//!    prescan over the first 1024 bytes (WHATWG HTML §13.2.3.2 simplification).
//! 4. UTF-8 fallback.
//!
//! Decodable labels: `utf-8`, `us-ascii`/`ascii` (mapped to windows-1252 like browsers),
//! `iso-8859-1`/`latin1`/`latin-1`/`windows-1252`/`cp1252` (single-byte Windows-1252).
//! Any other declared encoding fails with [`RenderError::UnsupportedCharset`] instead of
//! silently producing mojibake. UTF-16/UTF-32 and BOMs other than UTF-8 are explicitly
//! out of scope for this WP.

use crate::error::RenderError;

mod charset_gbk;

/// Maximum number of leading bytes scanned for a `<meta charset>` declaration.
const META_PRESCAN_BYTES: usize = 1024;

/// Decoded HTML text plus the encoding label that was used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedHtml {
    /// Decoded document text.
    pub text: String,
    /// Encoding actually used, normalized to `utf-8` or `windows-1252`.
    pub charset: String,
}

/// Decodes `bytes` into HTML text using the documented encoding subset.
pub fn decode_html(bytes: &[u8], content_type: &str) -> Result<DecodedHtml, RenderError> {
    // 1. BOM detection wins over any declaration.
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return Ok(DecodedHtml {
            text: String::from_utf8_lossy(rest).into_owned(),
            charset: "utf-8".to_owned(),
        });
    }
    if bytes.starts_with(b"\xFF\xFE") || bytes.starts_with(b"\xFE\xFF") {
        return Err(RenderError::UnsupportedCharset("utf-16".to_owned()));
    }

    // 2. Content-Type charset, then 3. `<meta charset>` prescan.
    let declared = charset_from_content_type(content_type)
        .or_else(|| charset_from_meta(bytes))
        .unwrap_or_else(|| "utf-8".to_owned());
    let normalized = normalize_charset_label(&declared)?;
    let text = match normalized.as_str() {
        "utf-8" => String::from_utf8_lossy(bytes).into_owned(),
        "windows-1252" => decode_windows_1252(bytes),
        "iso-8859-15" => decode_iso_8859_15(bytes),
        "gbk" => decode_gbk(bytes),
        other => return Err(RenderError::UnsupportedCharset(other.to_owned())),
    };
    Ok(DecodedHtml {
        text,
        charset: normalized,
    })
}

/// Normalizes an encoding label into `utf-8` or `windows-1252`.
pub(crate) fn normalize_charset_label(label: &str) -> Result<String, RenderError> {
    let label = label
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match label.as_str() {
        "utf-8" | "utf8" | "unicode-1-1-utf-8" => Ok("utf-8".to_owned()),
        "us-ascii" | "ascii" | "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1"
        | "windows-1252" | "cp1252" | "x-cp1252" => Ok("windows-1252".to_owned()),
        "iso-8859-15" | "iso8859-15" | "latin-9" | "latin9" | "l9" => Ok("iso-8859-15".to_owned()),
        "gbk" | "gb2312" | "gb18030" | "x-gbk" => Ok("gbk".to_owned()),
        other => Err(RenderError::UnsupportedCharset(other.to_owned())),
    }
}

/// Extracts the `charset` parameter from a `Content-Type` header value.
///
/// Parameter names are matched case-insensitively and RFC 9110 optional whitespace (BWS)
/// around `=` is accepted (`charset = "utf-8"`); only an exact `charset` parameter counts,
/// so strings such as `xcharset=utf-8` do not match.
pub fn charset_from_content_type(content_type: &str) -> Option<String> {
    // Parse parameters properly (RFC 9110 §5.6): split on ';', trim OWS around
    // name '=' value, handle quoted values.
    for part in content_type.split(';').skip(1) {
        let part = part.trim();
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case("charset") {
            continue;
        }
        let value = value.trim().trim_matches(['"', '\'']);
        if value.is_empty() {
            continue;
        }
        return Some(value.to_owned());
    }
    None
}

/// Decoded stylesheet text plus the encoding label that was used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedStylesheet {
    /// Decoded stylesheet text.
    pub text: String,
    /// Encoding actually used, normalized to `utf-8` or `windows-1252`.
    pub charset: String,
}

/// Decodes the byte stream of a CSS stylesheet per CSS Syntax §3.2.
///
/// Precedence: a UTF-8 BOM is stripped and wins over every declaration; otherwise the fallback
/// encoding is the `Content-Type` charset, then an exact `@charset "…";` declaration in the first
/// 1024 bytes, then UTF-8. Only the documented UTF-8/Windows-1252 subset is decodable; a UTF-16
/// BOM or an unsupported declared encoding yields [`RenderError::UnsupportedCharset`] instead of
/// silently producing mojibake.
pub fn decode_stylesheet(
    bytes: &[u8],
    content_type: &str,
) -> Result<DecodedStylesheet, RenderError> {
    // The decode algorithm gives precedence to a BOM over the fallback encoding.
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return Ok(DecodedStylesheet {
            text: String::from_utf8_lossy(rest).into_owned(),
            charset: "utf-8".to_owned(),
        });
    }
    if bytes.starts_with(b"\xFF\xFE") || bytes.starts_with(b"\xFE\xFF") {
        return Err(RenderError::UnsupportedCharset("utf-16".to_owned()));
    }

    let declared = charset_from_content_type(content_type)
        .or_else(|| charset_from_css_declaration(bytes))
        .unwrap_or_else(|| "utf-8".to_owned());
    let normalized = normalize_charset_label(&declared)?;
    let text = match normalized.as_str() {
        "utf-8" => String::from_utf8_lossy(bytes).into_owned(),
        "windows-1252" => decode_windows_1252(bytes),
        "iso-8859-15" => decode_iso_8859_15(bytes),
        "gbk" => decode_gbk(bytes),
        other => return Err(RenderError::UnsupportedCharset(other.to_owned())),
    };
    Ok(DecodedStylesheet {
        text,
        charset: normalized,
    })
}

/// Recognizes the exact `@charset "LABEL";` byte sequence from CSS Syntax §3.2.
///
/// The declaration must be at the very start of the stream, use double quotes and no whitespace
/// before the semicolon, and its label bytes must be printable ASCII (excluding `"` and control
/// characters) — anything else is not an encoding declaration.
fn charset_from_css_declaration(bytes: &[u8]) -> Option<String> {
    const PREFIX: &[u8] = b"@charset \"";
    let window = &bytes[..bytes.len().min(META_PRESCAN_BYTES)];
    let rest = window.strip_prefix(PREFIX)?;
    let end = rest.iter().position(|&byte| byte == b'"')?;
    if rest.get(end + 1) != Some(&b';') {
        return None;
    }
    let label = &rest[..end];
    if label.is_empty()
        || !label
            .iter()
            .all(|&byte| (0x01..=0x11).contains(&byte) || (0x23..=0x7F).contains(&byte))
    {
        return None;
    }
    Some(String::from_utf8_lossy(label).into_owned())
}

fn find_charset_attribute(tag: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(pos) = tag[from..].find("charset") {
        let index = from + pos;
        // WHATWG HTML §13.2.3.2: a `;` parameter delimiter also introduces `charset`,
        // including the compact `text/html;charset=...` form (#2378).
        if index == 0
            || tag.as_bytes()[index - 1].is_ascii_whitespace()
            || tag.as_bytes()[index - 1] == b';'
        {
            return Some(index);
        }
        from = index + 1;
    }
    None
}

/// Prescans the first 1024 bytes for `<meta charset>` / `<meta http-equiv>` charset.
fn charset_from_meta(bytes: &[u8]) -> Option<String> {
    let window = &bytes[..bytes.len().min(META_PRESCAN_BYTES)];
    let lower: Vec<u8> = window.to_ascii_lowercase();
    let haystack = String::from_utf8_lossy(&lower);
    // Only scan inside real `<meta>` tags, skipping comments and tag-like names such as
    // `<metadata>` so a commented-out declaration cannot win over a later real one (#1450).
    let mut search_start = 0;
    loop {
        if let Some(comment_pos) = haystack[search_start..].find("<!--") {
            let comment_start = search_start + comment_pos;
            if let Some(close) = haystack[comment_start + 4..].find("-->") {
                search_start = comment_start + 4 + close + 3;
                continue;
            }
        }
        let Some(meta_pos) = haystack[search_start..].find("<meta") else {
            break;
        };
        let meta_start = search_start + meta_pos;
        if haystack
            .as_bytes()
            .get(meta_start + 5)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        {
            search_start = meta_start + 5;
            continue;
        }
        // A `<meta` without a closing `>` is truncated input: stop scanning instead of
        // slicing one byte past the end of the window (#1212).
        let Some(i) = haystack[meta_start..].find('>') else {
            break;
        };
        let tag_end = meta_start + i;
        let tag = &haystack[meta_start..=tag_end];
        if let Some(charset_pos) = find_charset_attribute(tag) {
            let rest = &tag[charset_pos + "charset".len()..];
            let rest = rest.trim_start();
            // A `charset` substring without `=` (e.g. content="charset ...") is not a
            // declaration: try the next `<meta>` rather than aborting the whole scan.
            let Some(rest) = rest.strip_prefix('=') else {
                search_start = tag_end + 1;
                continue;
            };
            let rest = rest.trim_start_matches([' ', '"', '\'']);
            let label: String = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
                .collect();
            if !label.is_empty() {
                return Some(label);
            }
        }
        search_start = tag_end + 1;
    }
    None
}

/// Windows-1252 decode table for the `0x80..=0x9F` range (the rest matches Latin-1).
const WINDOWS_1252_C1: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

/// Decodes single-byte Windows-1252 (a superset of ISO-8859-1 in practice).
pub(crate) fn decode_windows_1252(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        let code_point = if (0x80..=0x9F).contains(&byte) {
            u32::from(WINDOWS_1252_C1[(byte - 0x80) as usize])
        } else {
            u32::from(byte)
        };
        out.push(char::from_u32(code_point).unwrap_or('\u{FFFD}'));
    }
    out
}

/// Decodes GB2312 (the common subset of GBK) using the embedded level-1 table.
/// Bytes outside the table map to U+FFFD rather than aborting the render (#1910).
pub fn decode_gbk(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 {
            out.push(char::from(b));
            i += 1;
            continue;
        }
        if (0xB0..=0xD7).contains(&b) && i + 1 < bytes.len() {
            let trail = bytes[i + 1];
            if (0xA1..=0xFE).contains(&trail) {
                let row = charset_gbk::GB2312_LEVEL1[(b - 0xB0) as usize];
                let glyph = row.chars().nth((trail - 0xA1) as usize);
                if let Some(ch) = glyph.filter(|c| *c != '\u{FFFD}') {
                    out.push(ch);
                    i += 2;
                    continue;
                }
            }
        }
        out.push('\u{FFFD}');
        i += 1;
    }
    out
}

/// Decodes ISO-8859-15 / Latin-9. Identical to Latin-1 for 0x00-0x7F and 0xA0-0xFF
/// except eight slots (€ Š š Ž ž Œ œ Ÿ); bytes 0x80-0x9F are C1 controls and are
/// kept as their Unicode code points (WHATWG Encoding: replacement for unmapped).
fn decode_iso_8859_15(bytes: &[u8]) -> String {
    const REPLACEMENTS: [(u8, u32); 8] = [
        (0xA4, 0x20AC), // EURO SIGN
        (0xA6, 0x0160), // LATIN CAPITAL LETTER S WITH CARON
        (0xA8, 0x0161), // LATIN SMALL LETTER S WITH CARON
        (0xB4, 0x017D), // LATIN CAPITAL LETTER Z WITH CARON
        (0xB8, 0x017E), // LATIN SMALL LETTER Z WITH CARON
        (0xBC, 0x0152), // LATIN CAPITAL LIGATURE OE
        (0xBD, 0x0153), // LATIN SMALL LIGATURE OE
        (0xBE, 0x0178), // LATIN CAPITAL LETTER Y WITH DIAERESIS
    ];
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        let code_point = REPLACEMENTS
            .iter()
            .find(|(b, _)| *b == byte)
            .map(|(_, cp)| *cp)
            .unwrap_or(u32::from(byte));
        out.push(char::from_u32(code_point).unwrap_or('\u{FFFD}'));
    }
    out
}

#[cfg(test)]
mod tests;
