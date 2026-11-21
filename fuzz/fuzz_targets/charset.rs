#![no_main]
//! Fuzz target: HTML byte-stream decoding (CSS/HTML Encoding §13.2, #1427).
//!
//! `megi_render::charset::decode_html` handles a UTF-8 BOM, a `charset=` parameter in the
//! `Content-Type` header, a `<meta charset>` prescan and an UTF-8 fallback over untrusted
//! bytes. The other text targets only ever feed valid UTF-8, so the non-UTF-8 branches were
//! effectively unreachable. This target passes the raw bytes plus an input-derived content
//! type and checks the decoder's documented invariants:
//!
//! - the reported charset is one of the two normalized labels (`utf-8` / `windows-1252`);
//! - the decoded text is a valid `String` (lossy decoding must never produce invalid UTF-8);
//! - a recognized UTF-8 BOM is stripped from the decoded text.

use libfuzzer_sys::fuzz_target;

/// A set of content types biased toward exercising each detection branch.
const CONTENT_TYPES: &[&str] = &[
    "text/html",
    "text/html; charset=utf-8",
    "text/html; charset=windows-1252",
    "text/html; charset=iso-8859-1",
    "text/html; charset=latin1",
    "text/html; charset=gbk", // unsupported -> typed error
    "text/html; charset=\"UTF-8\"",
];

fuzz_target!(|data: &[u8]| {
    // Derive a content type from the leading bytes when enough are available; otherwise fall
    // back to the first entry. This keeps the input space small while covering all branches.
    let index = data
        .first()
        .map_or(0, |b| usize::from(*b) % CONTENT_TYPES.len());
    let content_type = CONTENT_TYPES[index];

    match megi_render::charset::decode_html(data, content_type) {
        Ok(decoded) => {
            // The decoder only ever reports these two normalized labels.
            assert!(
                decoded.charset == "utf-8" || decoded.charset == "windows-1252",
                "unexpected charset label: {}",
                decoded.charset
            );
            // A valid UTF-8 BOM wins and is stripped from the decoded text (#1647).
            if let Some(rest) = data.strip_prefix(b"\xEF\xBB\xBF") {
                assert!(
                    !decoded.text.starts_with('\u{FEFF}'),
                    "a UTF-8 BOM must be stripped from the decoded text"
                );
                // Lossy decoding of invalid bytes after the BOM legitimately yields U+FFFD;
                // only demand a clean decode when the post-BOM bytes are valid UTF-8.
                if std::str::from_utf8(rest).is_ok() {
                    assert!(
                        !decoded.text.starts_with('\u{FFFD}'),
                        "valid UTF-8 after a BOM must not decode to a replacement character"
                    );
                }
            }
            // `String` is always valid UTF-8; touch it so the invariant is observed.
            let _ = decoded.text.chars().count();
        }
        Err(_) => {
            // Unsupported charsets and UTF-16 BOMs are typed errors, never panics.
        }
    }

    // A fixed UTF-8 content type on the same bytes exercises the declared-charset path
    // independently of the derived one.
    let _ = megi_render::charset::decode_html(data, "text/html; charset=utf-8");
});
