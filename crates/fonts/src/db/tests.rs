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
mod system_ui_tests {
    use super::*;

    /// CSS Fonts 4 §3.1: both `system-ui` and `sans-serif` name the platform's
    /// default font on Linux fontconfig, i.e. Noto Sans (#parity).
    #[test]
    fn system_ui_and_sans_serif_use_the_system_default() {
        let db = FontDatabase::from_system();
        if db.is_empty() {
            return; // fontless CI container: constructor must simply not panic
        }
        let ui_id = db
            .query_face(&["system-ui"], 400, FontStyle::Normal, Stretch::Normal)
            .expect("system-ui must resolve against the system font DB");
        let ui = db.build_font(ui_id).expect("system-ui face must build");
        assert_eq!(
            ui.family_name(),
            "Noto Sans",
            "system-ui must resolve to the system default UI font, got {}",
            ui.family_name()
        );

        let sans_id = db
            .query_face(&["sans-serif"], 400, FontStyle::Normal, Stretch::Normal)
            .expect("sans-serif must resolve against the system font DB");
        let sans = db.build_font(sans_id).expect("sans-serif face must build");
        assert_eq!(
            sans.family_name(),
            "Noto Sans",
            "sans-serif must use the system default face, got {}",
            sans.family_name()
        );
    }

    /// CSS Fonts 4 §3.1 / fontconfig: on Linux `sans-serif` is aliased to the
    /// platform default (Noto Sans here), just like Chrome. A system-backed DB
    /// must therefore resolve the generic to Noto Sans rather than DejaVu.
    #[test]
    fn sans_serif_uses_system_default_noto_sans() {
        let db = FontDatabase::from_system();
        if db.is_empty() {
            return; // fontless CI container: constructor must simply not panic
        }
        let sans = db
            .query_face(&["sans-serif"], 400, FontStyle::Normal, Stretch::Normal)
            .expect("sans-serif must resolve against the system font DB");
        let face = db.build_font(sans).expect("sans-serif face must build");
        assert_eq!(
            face.family_name(),
            "Noto Sans",
            "sans-serif must use the system default face (fontconfig alias), got {}",
            face.family_name()
        );
    }

    /// The render pipeline uses a bundled-only DB; the generic must still find
    /// the system face there, while a quoted name stays a literal family (#1239).
    #[test]
    fn bundled_database_resolves_system_ui_but_not_quoted_generic() {
        let db = FontDatabase::new_with_bundled_fonts();
        let sans = db
            .font(&["sans-serif"], 400, FontStyle::Normal, Stretch::Normal)
            .expect("sans-serif resolves against bundled fonts");
        assert_eq!(sans.family_name(), "DejaVu Sans");

        let quoted = format!("{QUOTED_FAMILY_PREFIX}system-ui");
        let literal = db
            .font(&[&quoted], 400, FontStyle::Normal, Stretch::Normal)
            .expect("quoted system-ui falls back to the default face");
        assert_eq!(literal.family_name(), "DejaVu Sans");

        if system_fallback_db().is_empty() {
            return; // fontless CI container: bundled fallback is the best we can do
        }
        let ui = db
            .font(&["system-ui"], 400, FontStyle::Normal, Stretch::Normal)
            .expect("system-ui resolves through the system DB");
        assert_eq!(ui.family_name(), "Noto Sans");
    }

    /// CSS Fonts 4 §3.1 family lists are ordered: a face available earlier in
    /// the list must win over the trailing `system-ui` generic.
    #[test]
    fn earlier_available_family_wins_over_system_ui() {
        let db = FontDatabase::new_with_bundled_fonts();
        let direct = db
            .font(&["DejaVu Sans"], 400, FontStyle::Normal, Stretch::Normal)
            .expect("bundled face");
        let ordered = db
            .font(
                &["DejaVu Sans", "system-ui"],
                400,
                FontStyle::Normal,
                Stretch::Normal,
            )
            .expect("earlier family must resolve");
        assert_eq!(
            ordered, direct,
            "system-ui must not outrank an earlier available family"
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
