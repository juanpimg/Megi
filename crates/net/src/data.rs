//! `data:` URL decoding (RFC 2397) and a dependency-free Base64 codec.
//!
//! Supported forms:
//!
//! ```text
//! data:[<mediatype>][;base64],<data>
//! ```
//!
//! - the mediatype defaults to `text/plain;charset=US-ASCII` when omitted;
//! - `;base64` (case-insensitive) selects Base64 decoding, otherwise the payload is
//!   percent-decoded into raw bytes (so `data:image/png,%89PNG...` works);
//! - a `#fragment` is not part of the payload and is ignored for decoding.
//!
//! The Base64 codec is implemented here to honor the crate's "no external crates for
//! core parsing" policy (only TLS transport pulls dependencies).

use crate::error::FetchError;
use crate::url::{Scheme, Url};

/// Decoded `data:` URL: resolved content type plus raw payload bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataUrl {
    /// Resolved MIME type, e.g. `text/html` or `text/css;charset=utf-8`.
    pub mime_type: String,
    /// Decoded payload bytes.
    pub bytes: Vec<u8>,
}

impl DataUrl {
    /// Interprets the payload as UTF-8 text when possible.
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }
}

/// Decodes a parsed `data:` [`Url`].
pub fn decode_data_url(url: &Url) -> Result<DataUrl, FetchError> {
    if url.scheme() != Scheme::Data {
        return Err(FetchError::UnsupportedScheme(
            url.scheme().as_str().to_owned(),
        ));
    }
    decode_data_payload(url.path())
}

/// Decodes a raw payload (everything after `data:`, without the fragment).
pub fn decode_data_payload(payload: &str) -> Result<DataUrl, FetchError> {
    let (meta, data) = payload.split_once(',').ok_or_else(|| {
        FetchError::InvalidDataUrl("missing `,` separating mediatype and data".to_owned())
    })?;

    // Split metadata into mediatype + parameters per RFC 2397.
    let mut segments = meta.split(';');
    let first = segments.next().unwrap_or("").trim();
    let mut mime_type = if first.is_empty() {
        "text/plain".to_owned()
    } else if first.contains('/') {
        first.to_ascii_lowercase()
    } else {
        // No `/` in the first segment: it is already a parameter, default the type.
        "text/plain".to_owned()
    };
    let mut base64 = false;
    let mut parameters: Vec<String> = Vec::new();
    if (first.is_empty() || !first.contains('/')) && !first.is_empty() {
        parameters.push(first.to_owned());
    }
    for segment in segments {
        let segment = segment.trim();
        if segment.eq_ignore_ascii_case("base64") {
            base64 = true;
        } else if !segment.is_empty() {
            parameters.push(segment.to_owned());
        }
    }
    for parameter in &parameters {
        mime_type.push(';');
        mime_type.push_str(parameter);
    }
    if mime_type.eq_ignore_ascii_case("text/plain") {
        mime_type = "text/plain;charset=US-ASCII".to_owned();
    }

    let bytes = if base64 {
        base64_decode(data).map_err(FetchError::InvalidDataUrl)?
    } else {
        percent_decode_bytes(data)
    };
    Ok(DataUrl { mime_type, bytes })
}

/// Decodes `%HH` escapes into raw bytes; invalid escapes are kept literally.
pub fn percent_decode_bytes(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            )
        {
            out.push((high * 16 + low) as u8);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    out
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes bytes as standard padded Base64 (RFC 4648).
pub fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_ALPHABET[(triple >> 18) as usize & 0x3F] as char);
        out.push(BASE64_ALPHABET[(triple >> 12) as usize & 0x3F] as char);
        out.push(if chunk.len() > 1 {
            BASE64_ALPHABET[(triple >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64_ALPHABET[triple as usize & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

/// Decodes standard Base64, accepting missing padding and skipping ASCII whitespace.
///
/// Returns a descriptive error for invalid characters or impossible lengths, so
/// malformed `data:` URLs produce [`FetchError::InvalidDataUrl`] instead of garbage.
pub fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    const MAX_DATA_URL_BYTES: usize = 64 * 1024 * 1024;
    if input.len() > MAX_DATA_URL_BYTES * 4 / 3 + 4 {
        return Err("data: URL payload too large".to_owned());
    }
    let mut sextets: Vec<u8> = Vec::with_capacity(input.len());
    let mut padding = 0usize;
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' => sextets.push(byte - b'A'),
            b'a'..=b'z' => sextets.push(byte - b'a' + 26),
            b'0'..=b'9' => sextets.push(byte - b'0' + 52),
            b'+' => sextets.push(62),
            b'/' => sextets.push(63),
            b'=' => padding += 1,
            b' ' | b'\t' | b'\r' | b'\n' => {}
            other => {
                return Err(format!(
                    "invalid base64 character 0x{other:02X} in data: URL"
                ));
            }
        }
        if padding > 0 && !matches!(byte, b'=' | b' ' | b'\t' | b'\r' | b'\n') {
            return Err("base64 data after the padding character".to_owned());
        }
    }
    if padding > 2 {
        return Err("too much base64 padding".to_owned());
    }
    if sextets.len() % 4 == 1 {
        return Err("invalid base64 length".to_owned());
    }
    let mut out = Vec::with_capacity(sextets.len() / 4 * 3 + 2);
    for chunk in sextets.chunks(4) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let b3 = chunk.get(3).copied().unwrap_or(0) as u32;
        let triple = (b0 << 18) | (b1 << 12) | (b2 << 6) | b3;
        out.push((triple >> 16) as u8);
        if chunk.len() > 2 {
            out.push((triple >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(triple as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn base64_round_trips_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_decode("Zm9vYmFy").expect("decode"), b"foobar");
        assert_eq!(base64_decode("Zg==").expect("decode"), b"f");
        assert_eq!(base64_decode("Zm8").expect("unpadded"), b"fo");
    }

    #[test]
    fn base64_rejects_invalid_input() {
        assert!(base64_decode("!!!!").is_err());
        assert!(base64_decode("A").is_err());
        assert!(base64_decode("Zg==Zg==").is_err());
    }

    #[test]
    fn decodes_text_html_data_url() {
        let decoded = decode_data_payload("text/html,<h1>Hi</h1>").expect("decode");
        assert_eq!(decoded.mime_type, "text/html");
        assert_eq!(decoded.bytes, b"<h1>Hi</h1>");
    }

    #[test]
    fn decodes_base64_css_without_charset_default() {
        let payload = format!("text/css;base64,{}", base64_encode(b"body{color:red}"));
        let decoded = decode_data_payload(&payload).expect("decode");
        assert_eq!(decoded.mime_type, "text/css");
        assert_eq!(decoded.bytes, b"body{color:red}");
    }

    #[test]
    fn defaults_mediatype_and_percent_decodes() {
        let decoded = decode_data_payload(",a%20b%3Cc%3E").expect("decode");
        assert_eq!(decoded.mime_type, "text/plain;charset=US-ASCII");
        assert_eq!(decoded.bytes, b"a b<c>");
    }
}
