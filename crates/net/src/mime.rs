//! MIME type sniffing (WHATWG MIME Sniffing Standard §5 subset).
//!
//! [`sniff`] inspects the leading bytes of a payload and returns a [`MimeHint`]. It is
//! used when a resource arrives without a usable `Content-Type` header (especially for
//! `file://` loads); [`resolve_content_type`] implements the header-first policy used by
//! [`crate::ResourceLoader`].
//!
//! What is covered (see <https://mimesniff.spec.whatwg.org/>):
//!
//! - §5.2 HTML sniffing: BOM/whitespace skip plus the normative `<!DOCTYPE HTML`,
//!   `<HTML`, `<HEAD`, `<SCRIPT`, ... byte patterns with the required tag-terminating
//!   byte;
//! - the image/document signatures this engine cares about right now: PNG, JPEG, GIF,
//!   WebP and PDF;
//! - XML declaration detection and a bounded `<svg` scan inside the first 1 KiB to
//!   distinguish SVG from generic XML;
//! - a `@charset "` CSS probe (the sniffable subset of CSS) and a conservative JSON
//!   heuristic;
//! - the §5 "text or binary" fallback (valid UTF-8 without NUL/control bytes → text).
//!
//! Limitations: no font, audio, video or multipart signatures, no UTF-16/UTF-8 BOM
//! transcoding beyond skipping, and the JSON heuristic is intentionally simple (JSON is
//! not sniffable in the standard; it is a convenience for resource hints).

/// A coarse MIME hint inferred from payload bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MimeHint {
    /// `text/html`
    Html,
    /// `application/xml`
    Xml,
    /// `image/svg+xml`
    Svg,
    /// `text/css`
    Css,
    /// `application/json` (heuristic)
    Json,
    /// `image/png`
    Png,
    /// `image/jpeg`
    Jpeg,
    /// `image/gif`
    Gif,
    /// `image/webp`
    Webp,
    /// `application/pdf`
    Pdf,
    /// `text/plain`
    Text,
    /// `application/octet-stream`
    Binary,
}

impl MimeHint {
    /// Returns the canonical MIME type string for this hint.
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Html => "text/html",
            Self::Xml => "application/xml",
            Self::Svg => "image/svg+xml",
            Self::Css => "text/css",
            Self::Json => "application/json",
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
            Self::Pdf => "application/pdf",
            Self::Text => "text/plain",
            Self::Binary => "application/octet-stream",
        }
    }
}

/// Returns the canonical MIME type string for a [`MimeHint`] (free function form).
pub const fn mime_type(hint: MimeHint) -> &'static str {
    hint.mime_type()
}

/// Sniffs the MIME type of `bytes` (WHATWG MIME Sniffing §5 subset).
///
/// Returns the most specific hint the payload supports, falling back to [`MimeHint::Text`]
/// for valid UTF-8 without control bytes and [`MimeHint::Binary`] otherwise.
pub fn sniff(bytes: &[u8]) -> MimeHint {
    if bytes.is_empty() {
        return MimeHint::Text;
    }
    if let Some(image) = image_signature(bytes) {
        return image;
    }
    if is_html(bytes) {
        return MimeHint::Html;
    }

    let start = skip_bom_and_whitespace(bytes, 1024);
    let head = &start[..start.len().min(1024)];
    if starts_with_ignore_ascii_case(head, b"<?xml") {
        if contains_ignore_ascii_case(head, b"<svg") {
            return MimeHint::Svg;
        }
        return MimeHint::Xml;
    }
    if starts_with_ignore_ascii_case(head, b"<svg") {
        return MimeHint::Svg;
    }
    if starts_with_ignore_ascii_case(head, b"@charset \"") {
        return MimeHint::Css;
    }
    if looks_like_json(head) {
        return MimeHint::Json;
    }
    if is_text(bytes) {
        MimeHint::Text
    } else {
        MimeHint::Binary
    }
}

/// Resolves the effective content type for a resource.
///
/// A non-empty `Content-Type` header wins (the standard's header-first policy); when it is
/// missing or empty the payload is sniffed and the canonical MIME type is returned.
pub fn resolve_content_type(headers: &[(String, String)], body: &[u8]) -> String {
    let header = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty());
    match header {
        Some(value) => value.to_owned(),
        None => sniff(body).mime_type().to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Matches the image/document signatures supported by the engine.
fn image_signature(bytes: &[u8]) -> Option<MimeHint> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.starts_with(PNG) {
        return Some(MimeHint::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(MimeHint::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(MimeHint::Gif);
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(MimeHint::Webp);
    }
    if bytes.starts_with(b"%PDF-") {
        return Some(MimeHint::Pdf);
    }
    None
}

/// WHATWG §5.2 "HTML sniffing": skip BOM + whitespace and match the normative prefixes.
fn is_html(bytes: &[u8]) -> bool {
    let start = skip_bom_and_whitespace(bytes, 144);
    const PREFIXES: [&[u8]; 17] = [
        b"<!DOCTYPE HTML",
        b"<HTML",
        b"<HEAD",
        b"<SCRIPT",
        b"<IFRAME",
        b"<H1",
        b"<DIV",
        b"<FONT",
        b"<TABLE",
        b"<A",
        b"<STYLE",
        b"<TITLE",
        b"<B",
        b"<BODY",
        b"<BR",
        b"<P",
        b"<!--",
    ];
    for prefix in PREFIXES {
        if prefix == b"<!--" {
            if start.starts_with(prefix) {
                return true;
            }
            continue;
        }
        if start.len() >= prefix.len()
            && start[..prefix.len()].eq_ignore_ascii_case(prefix)
            && start
                .get(prefix.len())
                .is_some_and(|byte| matches!(byte, 0x09 | 0x0A | 0x0C | 0x0D | 0x20 | b'>'))
        {
            return true;
        }
    }
    false
}

/// Skips an optional UTF-8 BOM and ASCII whitespace without consuming more than `limit` bytes.
fn skip_bom_and_whitespace(bytes: &[u8], limit: usize) -> &[u8] {
    let mut index = 0usize;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        index = 3;
    }
    while index < bytes.len()
        && index < limit
        && matches!(bytes[index], 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
    {
        index += 1;
    }
    &bytes[index.min(bytes.len())..]
}

fn starts_with_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

fn contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Conservative JSON heuristic: the payload starts with `{`/`[` and a JSON-ish token.
fn looks_like_json(bytes: &[u8]) -> bool {
    let Some(first) = bytes.first() else {
        return false;
    };
    if !matches!(first, b'{' | b'[') {
        return false;
    }
    let rest = &bytes[1..];
    let rest = rest
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c'))
        .map_or(&rest[..0], |index| &rest[index..]);
    rest.first().is_some_and(|byte| {
        matches!(
            byte,
            b'"' | b'}' | b']' | b'{' | b'[' | b'-' | b'0'..=b'9' | b'n' | b't' | b'f'
        )
    })
}

/// WHATWG §5 "text or binary" fallback: valid UTF-8 without NUL/undesirable control bytes.
fn is_text(bytes: &[u8]) -> bool {
    if std::str::from_utf8(bytes).is_err() {
        return false;
    }
    !bytes
        .iter()
        .any(|byte| *byte == 0 || (*byte < 0x20 && !matches!(byte, 0x09 | 0x0A | 0x0C | 0x0D)))
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn signatures_win_over_text() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nrest"), MimeHint::Png);
        assert_eq!(sniff(b"\xFF\xD8\xFF\xE0"), MimeHint::Jpeg);
        assert_eq!(sniff(b"GIF89a"), MimeHint::Gif);
        assert_eq!(sniff(b"%PDF-1.7"), MimeHint::Pdf);
    }

    #[test]
    fn empty_payload_is_text() {
        assert_eq!(sniff(b""), MimeHint::Text);
        assert_eq!(mime_type(MimeHint::Text), "text/plain");
    }
}
