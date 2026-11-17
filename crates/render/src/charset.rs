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
        other => return Err(RenderError::UnsupportedCharset(other.to_owned())),
    };
    Ok(DecodedHtml {
        text,
        charset: normalized,
    })
}

/// Normalizes an encoding label into `utf-8` or `windows-1252`.
fn normalize_charset_label(label: &str) -> Result<String, RenderError> {
    let label = label
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match label.as_str() {
        "utf-8" | "utf8" | "unicode-1-1-utf-8" => Ok("utf-8".to_owned()),
        "us-ascii" | "ascii" | "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1"
        | "windows-1252" | "cp1252" | "x-cp1252" => Ok("windows-1252".to_owned()),
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

#[cfg(test)]
mod tests {
    use super::charset_from_content_type;

    #[test]
    fn parses_charset_parameter_with_optional_whitespace() {
        assert_eq!(
            charset_from_content_type("text/html; charset=utf-8").as_deref(),
            Some("utf-8")
        );
        assert_eq!(
            charset_from_content_type("text/html;charset = UTF-8").as_deref(),
            Some("UTF-8"),
            "RFC 9110 BWS around '=' is allowed"
        );
        assert_eq!(
            charset_from_content_type("text/html; charset =\twindows-1252").as_deref(),
            Some("windows-1252")
        );
        assert_eq!(
            charset_from_content_type("text/html; charset=\"iso-8859-1\"").as_deref(),
            Some("iso-8859-1")
        );
        assert_eq!(
            charset_from_content_type("text/html; foo=bar; charset=windows-1252; q=1").as_deref(),
            Some("windows-1252")
        );
    }

    #[test]
    fn ignores_missing_or_lookalike_charset_parameters() {
        assert_eq!(charset_from_content_type("text/html"), None);
        assert_eq!(charset_from_content_type("text/html; xcharset=utf-8"), None);
        assert_eq!(charset_from_content_type("text/html; charset="), None);
        assert_eq!(charset_from_content_type("text/html; charset = "), None);
    }
}

/// Finds `charset` only where it starts an attribute, not inside another attribute's value.
fn find_charset_attribute(tag: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(pos) = tag[from..].find("charset") {
        let index = from + pos;
        if index == 0 || tag.as_bytes()[index - 1].is_ascii_whitespace() {
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
fn decode_windows_1252(bytes: &[u8]) -> String {
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

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn utf8_bom_is_stripped() {
        let decoded = decode_html(b"\xEF\xBB\xBF<p>hi</p>", "text/html").expect("decode");
        assert_eq!(decoded.text, "<p>hi</p>");
        assert_eq!(decoded.charset, "utf-8");
    }

    #[test]
    fn unterminated_meta_tag_does_not_panic() {
        // A truncated `<meta` used to slice one byte past the prescan window (#1212).
        let decoded = decode_html(b"<!DOCTYPE html><meta charset=utf-8", "text/html")
            .expect("truncated meta must not panic");
        assert_eq!(decoded.charset, "utf-8");
    }

    #[test]
    fn charset_substring_without_equals_does_not_abort_scan() {
        let decoded = decode_html(
            b"<meta name=\"x\" content=\"charset utf-8\"><meta charset=windows-1252>",
            "text/html",
        )
        .expect("scan must continue past a non-declaration");
        assert_eq!(decoded.charset, "windows-1252");
    }

    #[test]
    fn content_type_charset_wins_over_meta() {
        let decoded = decode_html(
            b"<meta charset=\"utf-8\"><p>caf\xE9</p>",
            "text/html; charset=windows-1252",
        )
        .expect("decode");
        assert_eq!(decoded.charset, "windows-1252");
        assert!(decoded.text.contains("café"));
    }

    #[test]
    fn meta_charset_is_used_without_header() {
        let decoded =
            decode_html(b"<meta charset=windows-1252><p>caf\xE9</p>", "text/html").expect("decode");
        assert_eq!(decoded.charset, "windows-1252");
        assert!(decoded.text.contains("café"));
    }

    #[test]
    fn unsupported_encoding_is_a_typed_error() {
        assert!(matches!(
            decode_html(b"<html></html>", "text/html; charset=gbk"),
            Err(RenderError::UnsupportedCharset(label)) if label == "gbk"
        ));
        assert!(matches!(
            decode_html(b"\xFF\xFE<\x00", "text/html"),
            Err(RenderError::UnsupportedCharset(_))
        ));
    }

    #[test]
    fn latin1_alias_decodes_accented_bytes() {
        let decoded = decode_html(b"<p>caf\xE9</p>", "text/html; charset=latin1").expect("decode");
        assert_eq!(decoded.charset, "windows-1252");
        assert!(decoded.text.contains('é'));
    }
}

#[cfg(test)]
mod stylesheet_charset_tests {
    use super::*;

    #[test]
    fn utf8_bom_is_stripped_and_wins_over_declarations() {
        let decoded = decode_stylesheet(
            b"\xEF\xBB\xBF@charset \"windows-1252\";.a{color:red}",
            "text/css; charset=windows-1252",
        )
        .expect("decode");
        assert_eq!(decoded.text, "@charset \"windows-1252\";.a{color:red}");
        assert_eq!(decoded.charset, "utf-8", "BOM wins over every declaration");
    }

    #[test]
    fn charset_declaration_decodes_legacy_bytes() {
        let bytes = b"@charset \"windows-1252\";p::after{content:\"caf\xE9\"}";
        let decoded = decode_stylesheet(bytes, "text/css").expect("decode");
        assert_eq!(decoded.charset, "windows-1252");
        assert!(decoded.text.contains("café"));
    }

    #[test]
    fn content_type_charset_wins_over_declaration() {
        // CSS Syntax §3.2: the HTTP charset is the fallback's first choice.
        let bytes = b"@charset \"windows-1252\";p::after{content:\"caf\xE9\"}";
        let decoded = decode_stylesheet(bytes, "text/css; charset=utf-8").expect("decode");
        assert_eq!(decoded.charset, "utf-8");
    }

    #[test]
    fn loose_charset_declaration_is_not_recognized() {
        // Leading whitespace and single quotes are valid CSS but not a valid encoding declaration.
        let bytes = b" @charset 'windows-1252';p{color:red}";
        let decoded = decode_stylesheet(bytes, "text/css").expect("decode");
        assert_eq!(decoded.charset, "utf-8");
        assert_eq!(decoded.text.as_bytes()[0], b' ');
    }

    #[test]
    fn unsupported_encoding_is_a_typed_error() {
        assert!(matches!(
            decode_stylesheet(b"@charset \"gbk\";", "text/css"),
            Err(RenderError::UnsupportedCharset(label)) if label == "gbk"
        ));
        assert!(matches!(
            decode_stylesheet(b"\xFF\xFE\x00", "text/css"),
            Err(RenderError::UnsupportedCharset(_))
        ));
    }
}

#[cfg(test)]
mod charset_prescan_tests {
    use super::*;

    #[test]
    fn commented_meta_charset_is_ignored() {
        let bytes = b"<!-- <meta charset=windows-1252> --><meta charset=utf-8>";
        assert_eq!(charset_from_meta(bytes).as_deref(), Some("utf-8"));
    }

    #[test]
    fn charset_inside_another_attribute_is_ignored() {
        let bytes = b"<meta name=\"charset=utf-8\"><meta charset=windows-1252>";
        assert_eq!(charset_from_meta(bytes).as_deref(), Some("windows-1252"));
    }

    #[test]
    fn metadata_tag_is_not_a_meta_tag() {
        let bytes = b"<metadata charset=utf-8><meta charset=windows-1252>";
        assert_eq!(charset_from_meta(bytes).as_deref(), Some("windows-1252"));
    }
}
