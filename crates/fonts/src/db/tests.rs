//! Unit tests for the font database.

use super::*;

mod quoted_family_tests {
    use super::*;

    #[test]
    fn generic_fallback_is_appended_only_when_needed() {
        let mut explicit = parse_font_families("Arial");
        ensure_generic_fallback(&mut explicit);
        assert_eq!(explicit.last().map(String::as_str), Some("sans-serif"));

        let mut with_generic = parse_font_families("Arial, serif");
        ensure_generic_fallback(&mut with_generic);
        assert_eq!(with_generic.len(), 2);
        assert_eq!(with_generic.last().map(String::as_str), Some("serif"));

        // A quoted generic is a literal family name, so the fallback is still needed.
        let mut quoted = parse_font_families("\"serif\"");
        ensure_generic_fallback(&mut quoted);
        assert_eq!(quoted.len(), 2);
        assert_eq!(quoted.last().map(String::as_str), Some("sans-serif"));
    }

    #[test]
    fn quoted_generic_name_stays_a_literal_family() {
        let unquoted = parse_font_families("serif");
        let quoted = parse_font_families("\"serif\"");
        assert!(!unquoted[0].starts_with(QUOTED_FAMILY_PREFIX));
        assert!(quoted[0].starts_with(QUOTED_FAMILY_PREFIX));
        assert_eq!(unmark_family(&quoted[0]), "serif");

        let unquoted_refs: Vec<&str> = vec![&unquoted[0]];
        let quoted_refs: Vec<&str> = vec![&quoted[0]];
        let expanded_unquoted = FontDatabase::expand_families(&unquoted_refs);
        let expanded_quoted = FontDatabase::expand_families(&quoted_refs);
        assert!(
            expanded_unquoted
                .iter()
                .any(|family| matches!(family, Family::Serif)),
            "unquoted serif must select the generic face"
        );
        assert!(
            !expanded_quoted
                .iter()
                .any(|family| matches!(family, Family::Serif)),
            "quoted serif must not select the generic face (#1239)"
        );
        assert!(
            expanded_quoted
                .iter()
                .any(|family| matches!(family, Family::Name(name) if *name == "serif")),
            "quoted serif must be looked up as a literal family name"
        );
    }
}

#[cfg(test)]
mod identity_digest_tests {
    use super::*;

    #[test]
    fn digest_changes_with_face_identity_not_just_count() {
        let mut a = FontDatabase::new();
        let mut b = FontDatabase::new();
        a.load_font_data(BUNDLED_FONTS[0].1.to_vec())
            .expect("regular");
        b.load_font_data(BUNDLED_FONTS[1].1.to_vec()).expect("bold");
        assert_eq!(a.order.len(), b.order.len(), "same number of faces");
        assert_ne!(
            a.identity_digest(),
            b.identity_digest(),
            "different faces with the same count must have different digests (#1218)"
        );
    }
}
