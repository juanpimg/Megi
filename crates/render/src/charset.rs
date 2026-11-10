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
pub fn charset_from_content_type(content_type: &str) -> Option<String> {
    let lower = content_type.to_ascii_lowercase();
    let index = lower.find("charset=")?;
    let value = &content_type[index + "charset=".len()..];
    let value = value
        .trim_start_matches([' ', '"', '\''])
        .split([';', ' ', '"', '\''])
        .next()
        .unwrap_or("");
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

/// Prescans the first 1024 bytes for `<meta charset>` / `<meta http-equiv>` charset.
fn charset_from_meta(bytes: &[u8]) -> Option<String> {
    let window = &bytes[..bytes.len().min(META_PRESCAN_BYTES)];
    let lower: Vec<u8> = window.to_ascii_lowercase();
    let haystack = String::from_utf8_lossy(&lower);
    let start = haystack.find("charset")?;
    let rest = &haystack[start + "charset".len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?;
    let rest = rest.trim_start_matches([' ', '"', '\'']);
    let label: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .collect();
    if label.is_empty() { None } else { Some(label) }
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
