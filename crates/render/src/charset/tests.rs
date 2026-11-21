#[cfg(test)]
mod content_type_tests {
    use super::super::charset_from_content_type;

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
#[cfg(test)]
mod unit_tests {
    use super::super::*;

    #[test]
    fn utf8_bom_is_stripped() {
        let decoded = decode_html(b"\xEF\xBB\xBF<p>hi</p>", "text/html").expect("decode");
        assert_eq!(decoded.text, "<p>hi</p>");
        assert_eq!(decoded.charset, "utf-8");
    }

    #[test]
    fn invalid_bytes_after_bom_decode_lossily_without_the_bom() {
        // #1647: the charset fuzz oracle used to assert that a BOM input never starts
        // with U+FFFD, but lossy decoding of invalid bytes *after* a stripped BOM must
        // legitimately produce U+FFFD. The BOM itself must still be gone (U+FEFF).
        let decoded = decode_html(b"\xEF\xBB\xBF\xFF\xFE<p>hi</p>", "text/html").expect("decode");
        assert_eq!(decoded.charset, "utf-8");
        assert!(
            !decoded.text.starts_with('\u{FEFF}'),
            "the UTF-8 BOM must be stripped"
        );
        assert!(
            decoded.text.starts_with('\u{FFFD}'),
            "invalid bytes after the BOM must decode to the replacement character"
        );
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
            decode_html(b"<html></html>", "text/html; charset=shift_jis"),
            Err(RenderError::UnsupportedCharset(label)) if label == "shift_jis"
        ));
        assert!(matches!(
            decode_html(b"\xFF\xFE<\x00", "text/html"),
            Err(RenderError::UnsupportedCharset(_))
        ));
    }

    #[test]
    fn iso_8859_15_decodes_euro_and_accented_bytes() {
        // ISO-8859-15 (Latin-9) differs from Windows-1252 in a few slots (#1917).
        let decoded = decode_html(
            b"<p>caf\xE9 \xA4 \xB8</p>",
            "text/html; charset=iso-8859-15",
        )
        .expect("iso-8859-15 must decode, not abort the render");
        assert_eq!(decoded.charset, "iso-8859-15");
        assert!(decoded.text.contains("caf\u{e9}"), "{}", decoded.text);
        assert!(
            decoded.text.contains('\u{20ac}'),
            "euro sign: {}",
            decoded.text
        );
        assert!(
            decoded.text.contains('\u{17e}'),
            "z caron: {}",
            decoded.text
        );
    }

    #[test]
    fn gbk_alias_decodes_chinese_bytes() {
        // GB2312 bytes for 中文 (D6 D0 CE C4)
        let decoded = decode_html(
            b"<html>\xD6\xD0\xCE\xC4</html>",
            "text/html; charset=gb2312",
        )
        .expect("gb2312 must decode (#1910)");
        assert_eq!(decoded.charset, "gbk");
        assert!(decoded.text.contains('\u{4e2d}'), "{}", decoded.text);
        assert!(decoded.text.contains('\u{6587}'), "{}", decoded.text);
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
    use super::super::*;

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
            decode_stylesheet(b"@charset \"shift_jis\";", "text/css"),
            Err(RenderError::UnsupportedCharset(label)) if label == "shift_jis"
        ));
        assert!(matches!(
            decode_stylesheet(b"\xFF\xFE\x00", "text/css"),
            Err(RenderError::UnsupportedCharset(_))
        ));
    }
}

#[cfg(test)]
mod charset_prescan_tests {
    use super::super::*;

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
