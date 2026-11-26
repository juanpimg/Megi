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

    /// CSS Fonts 4 §5.2: fontdb rewrites its coarse generic family names from
    /// fontconfig aliases during `load_system_fonts()`. On CachyOS that selects
    /// "Nimbus Sans Narrow" for sans-serif and "Standard Symbols PS" for serif,
    /// so a CSS list with no installed named family paints a symbol face. Pin
    /// the generics to documented deterministic families after the host scan;
    /// host fonts stay available for named families and per-glyph fallback.
    #[test]
    fn system_load_pins_coarse_generics_to_deterministic_families() {
        let db = FontDatabase::from_system();
        if db.is_empty() {
            return; // fontless CI container: nothing to pin
        }
        assert_eq!(
            db.db.family_name(&Family::SansSerif),
            "DejaVu Sans",
            "fontconfig must not override the deterministic sans-serif face"
        );
        assert_eq!(
            db.db.family_name(&Family::Serif),
            "DejaVu Serif",
            "fontconfig must not map serif to a symbol font"
        );
        assert_eq!(
            db.db.family_name(&Family::Monospace),
            "DejaVu Sans Mono",
            "fontconfig must not override the deterministic monospace face"
        );
    }

    /// CSS Fonts 4 §3.1 / §5.2: both platform generics must select the first
    /// installed family from the documented named fallback list. Comparing two
    /// host databases proves nothing; pin the expected family explicitly so a
    /// fontconfig alias such as "Nimbus Sans Narrow" cannot pass unnoticed.
    #[test]
    fn system_ui_and_sans_serif_use_the_system_default() {
        let db = FontDatabase::from_system();
        if db.is_empty() {
            return; // fontless CI container: constructor must simply not panic
        }
        let family_for = |families: &[&str]| -> String {
            let id = db
                .query_face(families, 400, FontStyle::Normal, Stretch::Normal)
                .expect("generic must resolve against the system font DB");
            db.build_font(id)
                .expect("face must build")
                .family_name()
                .to_owned()
        };
        let first_named = |names: &[&str]| -> Option<String> {
            names.iter().find_map(|name| {
                let id = db.query_face(&[name], 400, FontStyle::Normal, Stretch::Normal)?;
                db.build_font(id)
                    .ok()
                    .map(|font| font.family_name().to_owned())
            })
        };
        let ui = family_for(&["system-ui"]);
        let sans = family_for(&["sans-serif"]);
        assert_eq!(
            ui,
            first_named(SYSTEM_UI_FALLBACKS).expect("a documented UI family must be installed"),
            "system-ui must use the first named fallback"
        );
        assert_eq!(
            sans,
            first_named(SANS_FALLBACKS).expect("a documented sans family must be installed"),
            "sans-serif must use the first named fallback"
        );
        assert_eq!(ui, sans, "both generics name the platform default face");
        assert!(
            SANS_FALLBACKS.contains(&sans.as_str()),
            "a fontconfig alias leaked into sans-serif: {sans}"
        );
        // A fresh database must agree: earlier queries must not mutate fallbacks.
        let fresh = FontDatabase::from_system();
        let fresh_id = fresh
            .query_face(&["sans-serif"], 400, FontStyle::Normal, Stretch::Normal)
            .expect("fresh sans-serif resolution");
        let fresh_sans = fresh
            .build_font(fresh_id)
            .expect("fresh sans-serif face must build");
        assert_eq!(sans, fresh_sans.family_name(), "resolution is stable");
    }

    /// CSS Fonts 4 §3.1: `sans-serif` names the platform default — Noto Sans
    /// when installed, otherwise the next documented fallback (DejaVu Sans).
    /// Never compare the database with itself: assert the ordered fallback so a
    /// host alias cannot silently change real-site typography (#fonts-parity).
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
        let expected = SANS_FALLBACKS
            .iter()
            .find_map(|name| {
                let id = db.query_face(&[name], 400, FontStyle::Normal, Stretch::Normal)?;
                db.build_font(id)
                    .ok()
                    .map(|font| font.family_name().to_owned())
            })
            .expect("a documented sans-serif fallback must be installed");
        assert_eq!(
            face.family_name(),
            expected,
            "sans-serif must use the first documented default family, got {}",
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

        let system = system_fallback_db();
        if system.is_empty() {
            return; // fontless CI container: bundled fallback is the best we can do
        }
        let ui = db
            .font(&["system-ui"], 400, FontStyle::Normal, Stretch::Normal)
            .expect("system-ui resolves through the system DB");
        let system_ui_id = system
            .query_face(&["system-ui"], 400, FontStyle::Normal, Stretch::Normal)
            .expect("system fallback DB resolves system-ui");
        let system_ui = system
            .build_font(system_ui_id)
            .expect("system-ui face must build");
        assert_eq!(
            ui.family_name(),
            system_ui.family_name(),
            "bundled DB must delegate system-ui to the host's system DB"
        );
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
mod generic_serif_tests {
    use super::*;

    /// Real-site typography (Wikipedia Vector 2022 h1):
    /// `'Linux Libertine','Georgia','Times','Source Serif 4',serif`. The render
    /// pipeline uses a bundled-only DB (DejaVu Sans + Bold), so the trailing
    /// `serif` generic must reach the shared system DB instead of degrading to
    /// the last-resort sans face (CSS Fonts 4 §5.2).
    #[test]
    fn bundled_database_resolves_serif_generic_from_system_fonts() {
        let system = system_fallback_db();
        if system.is_empty() {
            return; // fontless CI container: bundled fallback is the best we can do
        }
        let db = FontDatabase::new_with_bundled_fonts();
        let quoted = |name: &str| format!("{QUOTED_FAMILY_PREFIX}{name}");
        let libertine = quoted("Linux Libertine");
        let georgia = quoted("Georgia");
        let times = quoted("Times");
        let source_serif = quoted("Source Serif 4");
        let families: Vec<&str> = vec![&libertine, &georgia, &times, &source_serif, "serif"];
        // Expected face: first installed family in CSS order (explicit names,
        // then the documented `serif` fallbacks). fontdb's coarse `Family::Serif`
        // can select a symbol font, so assert against the named fallback list.
        let serif_family = [libertine.as_str(), &georgia, &times, &source_serif]
            .into_iter()
            .map(unmark_family)
            .chain(SERIF_FALLBACKS.iter().copied())
            .find_map(|name| {
                system
                    .font(&[name], 400, FontStyle::Normal, Stretch::Normal)
                    .ok()
            })
            .expect("a real serif fallback must be installed")
            .family_name()
            .to_owned();
        assert_ne!(serif_family, "DejaVu Sans", "test needs a real serif face");
        let font = db
            .font(&families, 400, FontStyle::Normal, Stretch::Normal)
            .expect("serif generic must resolve through the system fallback DB");
        assert_eq!(
            font.family_name(),
            serif_family,
            "a trailing generic serif must not degrade to the bundled sans last resort"
        );

        // CSS Fonts 4 §3.1: `sans-serif` keeps its deterministic bundled face (#parity).
        let sans = db
            .font(&["sans-serif"], 400, FontStyle::Normal, Stretch::Normal)
            .expect("sans-serif resolves against bundled fonts");
        assert_eq!(sans.family_name(), "DejaVu Sans");
    }

    /// A database with no faces must keep its documented contract: an unresolved
    /// generic reports `NoFontAvailable` instead of borrowing a system face
    /// (`crates/fonts/tests/font_engine/database.rs`).
    #[test]
    fn empty_database_does_not_borrow_system_generic_faces() {
        let db = FontDatabase::new();
        assert!(matches!(
            db.font(&["serif"], 400, FontStyle::Normal, Stretch::Normal),
            Err(FontError::NoFontAvailable)
        ));
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
