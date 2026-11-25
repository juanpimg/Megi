//! Font-family resolution, query and per-glyph fallback logic.

use super::*;

impl FontDatabase {
    /// Resolves a CSS family list that contains an unsatisfied generic against the
    /// shared system database, using only the **named** fallbacks from
    /// [`Self::expand_families`].
    ///
    /// `query_face` ranks `Family::Serif` (and the other coarse generic classes)
    /// above explicit names, and fontdb's class can match symbol fonts such as
    /// "Standard Symbols PS" on Linux. Dropping the generic leaves the documented
    /// family-name list (DejaVu Serif, Liberation Serif, …) in CSS order, which is
    /// what a browser's generic fallback must select (CSS Fonts 4 §5.2).
    pub(crate) fn system_generic_font(
        families: &[&str],
        weight: u16,
        style: FontStyle,
        stretch: Stretch,
    ) -> Option<Font> {
        let system = system_fallback_db();
        if system.is_empty() {
            return None;
        }
        let names: Vec<&str> = Self::expand_families(families)
            .into_iter()
            .filter_map(|family| match family {
                Family::Name(name) => Some(name),
                _ => None,
            })
            .collect();
        if names.is_empty() {
            return None;
        }
        let id = system.query_face(&names, weight, style, stretch)?;
        system.build_font(id).ok()
    }

    pub(crate) fn expand_families<'a>(families: &'a [&'a str]) -> Vec<Family<'a>> {
        let mut expanded: Vec<Family<'a>> = Vec::new();
        for family in families {
            // A quoted family name is always literal, never a generic keyword (#1239).
            let quoted = family.starts_with(QUOTED_FAMILY_PREFIX);
            let literal = unmark_family(family);
            let keyword = literal.to_ascii_lowercase();
            if !quoted && keyword == "sans-serif" {
                // CSS Fonts 4 §3.1 / fontconfig: `sans-serif` names the platform
                // default UI face (Noto Sans on Linux). fontdb's `Family::SansSerif`
                // alias can resolve to a different installed face (e.g. Nimbus Sans
                // Narrow) and outrank the named fallbacks, so try the platform
                // defaults first and keep the generic only as a last resort (#parity).
                for name in SANS_FALLBACKS {
                    expanded.push(Family::Name(name));
                }
                expanded.push(Family::SansSerif);
                continue;
            }
            let (generic, fallbacks): (Option<Family<'a>>, &[&'static str]) = if quoted {
                (None, &[] as &[&'static str])
            } else {
                match keyword.as_str() {
                    "sans-serif" => unreachable!("handled above"),
                    // CSS Fonts 4 §3.1: system-ui is the platform UI font. Do not
                    // use the `SansSerif` generic here: fontdb maps it to "Arial",
                    // which would outrank Noto Sans when MS fonts are installed.
                    // The literal name is still appended below as a last resort.
                    "system-ui" | "ui-sans-serif" => (None, SYSTEM_UI_FALLBACKS),
                    "serif" | "ui-serif" => (Some(Family::Serif), SERIF_FALLBACKS),
                    "monospace" | "ui-monospace" => (Some(Family::Monospace), MONO_FALLBACKS),
                    "cursive" => (Some(Family::Cursive), CURSIVE_FALLBACKS),
                    "fantasy" => (Some(Family::Fantasy), FANTASY_FALLBACKS),
                    "emoji" => (
                        Some(Family::SansSerif),
                        &["Noto Color Emoji", "Apple Color Emoji", "Segoe UI Emoji"] as &[&str],
                    ),
                    "math" => (
                        Some(Family::Serif),
                        &["STIX Two Math", "Cambria Math", "Latin Modern Math"] as &[&str],
                    ),
                    "fangsong" => (None, &["FangSong", "STFangsong"] as &[&str]),
                    _ => (None, &[] as &[&'static str]),
                }
            };
            if let Some(generic) = generic {
                expanded.push(generic);
            }
            for name in fallbacks {
                expanded.push(Family::Name(name));
            }
            if generic.is_none() || !GENERIC_FAMILIES.contains(&keyword.as_str()) {
                // Explicit family name (also keeps e.g. "emoji"/"math" as names).
                expanded.push(Family::Name(literal));
            }
        }
        expanded
    }

    /// Resolves an `@font-face` alias list to the best matching registered face.
    pub(crate) fn alias_face(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        stretch: Stretch,
    ) -> Option<fontdb::ID> {
        let wanted_style = DbStyle::from(style);
        for family in families {
            let family = unmark_family(family);
            let Some(ids) = self.aliases.get(&family.to_ascii_lowercase()) else {
                continue;
            };
            let mut best: Option<(i32, fontdb::ID)> = None;
            for id in ids {
                let Some(face) = self.db.face(*id) else {
                    continue;
                };
                let style_penalty = match (face.style, wanted_style) {
                    (a, b) if a == b => 0u32,
                    (DbStyle::Italic, DbStyle::Oblique) | (DbStyle::Oblique, DbStyle::Italic) => {
                        100
                    }
                    _ => 1000,
                };
                let weight_penalty = i32::from(face.weight.0.abs_diff(weight));
                let stretch_penalty = if face.stretch == stretch { 0i32 } else { 100 };
                let score = style_penalty as i32 + weight_penalty + stretch_penalty;
                if best.is_none_or(|(best_score, _)| score < best_score) {
                    best = Some((score, *id));
                }
            }
            if let Some((_, id)) = best {
                return Some(id);
            }
        }
        None
    }

    pub(crate) fn resolve_family_name<'a>(&'a self, name: &'a str) -> &'a str {
        let name = unmark_family(name);
        self.order
            .iter()
            .filter_map(|id| self.db.face(*id))
            .flat_map(|face| face.families.iter())
            .find(|(registered, _)| registered.eq_ignore_ascii_case(name))
            .map(|(registered, _)| registered.as_str())
            .unwrap_or(name)
    }

    pub(crate) fn query_face(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        stretch: Stretch,
    ) -> Option<fontdb::ID> {
        if let Some(id) = self.alias_face(families, weight, style, stretch) {
            return Some(id);
        }
        let mut expanded = Self::expand_families(families);
        if expanded.is_empty() {
            return None;
        }
        for fam in &mut expanded {
            if let Family::Name(name) = fam {
                *name = self.resolve_family_name(name);
            }
        }
        self.db.query(&Query {
            families: &expanded,
            weight: Weight(weight),
            stretch,
            style: DbStyle::from(style),
        })
    }

    /// Resolves the `system-ui`/`ui-sans-serif` generic when this database only
    /// holds bundled/web faces (CSS Fonts 4 §3.1). A bundled-only database cannot
    /// see Noto Sans, so ask the shared system DB and return an owned [`Font`];
    /// fontdb IDs are never passed between databases. `sans-serif` callers do
    /// not use this path; a bundled-only DB resolves its own bundled face.
    pub(crate) fn system_ui_font(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        stretch: Stretch,
    ) -> Option<Font> {
        if self.system_loaded {
            return None;
        }
        let ui_index = families.iter().position(|family| {
            !family.starts_with(QUOTED_FAMILY_PREFIX)
                && matches!(
                    unmark_family(family).to_ascii_lowercase().as_str(),
                    "system-ui" | "ui-sans-serif"
                )
        })?;
        // CSS Fonts 4 §3.1 lists are ordered: a face matched earlier wins.
        if ui_index > 0
            && self
                .query_face(&families[..ui_index], weight, style, stretch)
                .is_some()
        {
            return None;
        }
        let system = system_fallback_db();
        let id = system.query_face(families, weight, style, stretch)?;
        system.build_font(id).ok()
    }

    pub(crate) fn last_resort_face(
        &self,
        weight: u16,
        style: FontStyle,
        stretch: Stretch,
    ) -> Option<fontdb::ID> {
        let wanted_style = DbStyle::from(style);
        let mut best: Option<(i32, fontdb::ID)> = None;
        for face in self.db.faces() {
            let style_penalty = match (face.style, wanted_style) {
                (a, b) if a == b => 0u32,
                (DbStyle::Italic, DbStyle::Oblique) | (DbStyle::Oblique, DbStyle::Italic) => 100,
                _ => 1000,
            };
            let weight_penalty = i32::from(face.weight.0.abs_diff(weight));
            let stretch_penalty = if face.stretch == stretch { 0i32 } else { 100 };
            let score = style_penalty as i32 + weight_penalty + stretch_penalty;
            if best.is_none_or(|(best_score, _)| score < best_score) {
                best = Some((score, face.id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Scans all loaded faces (not yet in `chain`) for a glyph matching `ch`, invoked
    /// only when the initial fallback chain lacks `ch` (e.g. CJK, Arabic, emoji).
    /// When `self` only loaded bundled/web fonts, falls back to a cached system font DB
    /// so missing scripts render without altering primary `serif`/`sans-serif`/`monospace`
    /// resolution for bundled fonts.
    pub(crate) fn find_fallback_face_for_char(
        &self,
        ch: char,
        chain: &[Font],
        weight: u16,
        style: FontStyle,
    ) -> Option<(Font, u16)> {
        if let Some(found) = self.scan_faces_for_char(ch, chain, weight, style) {
            return Some(found);
        }
        if !self.system_loaded {
            return system_fallback_db().scan_faces_for_char(ch, chain, weight, style);
        }
        None
    }

    pub(crate) fn scan_faces_for_char(
        &self,
        ch: char,
        chain: &[Font],
        weight: u16,
        style: FontStyle,
    ) -> Option<(Font, u16)> {
        let wanted_style = DbStyle::from(style);
        let mut candidates: Vec<(u32, fontdb::ID)> =
            self.db
                .faces()
                .filter(|face| {
                    !chain
                        .iter()
                        .any(|f| f.info().post_script_name == face.post_script_name)
                })
                .map(|face| {
                    let style_penalty = match (face.style, wanted_style) {
                        (a, b) if a == b => 0u32,
                        (DbStyle::Italic, DbStyle::Oblique)
                        | (DbStyle::Oblique, DbStyle::Italic) => 100,
                        _ => 1000,
                    };
                    let weight_distance = (face.weight.0 as i32 - weight as i32).unsigned_abs();
                    (style_penalty + weight_distance, face.id)
                })
                .collect();
        candidates.sort_by_key(|(score, _)| *score);
        for (_, id) in candidates {
            if let Ok(font) = self.build_font(id)
                && let Some(glyph_id) = font.glyph_id(ch)
            {
                return Some((font, glyph_id));
            }
        }
        None
    }

    /// Builds the ordered fallback chain (primary first) for per-glyph fallback.
    pub(crate) fn fallback_chain(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        primary: Font,
    ) -> Vec<Font> {
        let cache_key = (
            families
                .iter()
                .map(|family| family.to_ascii_lowercase())
                .collect::<Vec<_>>(),
            weight,
            style_cache_key(style),
        );
        if let Some(cached) = self.fallback_cache_lock().get(&cache_key).cloned() {
            return (*cached).clone();
        }
        let mut chain: Vec<Font> = vec![primary];
        // Dedup by the underlying face bytes instead of the PostScript name:
        // distinct subset faces can share a PS name and must not be dropped (#612).
        let mut seen = std::collections::HashSet::new();
        seen.insert(std::sync::Arc::as_ptr(&chain[0].data) as usize);

        for family in families {
            if let Ok(font) = self.font(&[*family], weight, style, Stretch::Normal)
                && seen.insert(std::sync::Arc::as_ptr(&font.data) as usize)
            {
                chain.push(font);
            }
        }

        if chain.len() == 1 {
            let wanted_style = DbStyle::from(style);
            let mut extra = 0usize;
            for face in self.db.faces() {
                if extra >= MAX_EXTRA_FALLBACKS {
                    break;
                }
                // Score by weight distance and style proximity instead of requiring
                // an exact match: a family with only a regular face should still
                // provide fallback glyphs for a bold request. Italic and oblique are
                // close cousins (small penalty) so an italic request can still fall
                // back to an oblique face (and vice versa) before reaching a
                // normal face, instead of treating every mismatch as equally distant
                // (#616).
                let style_penalty = match (face.style, wanted_style) {
                    (a, b) if a == b => 0u32,
                    (DbStyle::Italic, DbStyle::Oblique) | (DbStyle::Oblique, DbStyle::Italic) => {
                        100
                    }
                    _ => 1000,
                };
                let weight_distance = (face.weight.0 as i32 - weight as i32).unsigned_abs();
                if weight_distance + style_penalty > 1300 {
                    continue;
                }
                if let Ok(font) = self.build_font(face.id) {
                    // Dedup by the underlying face bytes instead of the PostScript
                    // name so distinct subset faces sharing a PS name survive (#612).
                    if seen.insert(std::sync::Arc::as_ptr(&font.data) as usize) {
                        chain.push(font);
                        extra += 1;
                    }
                }
            }
        }
        self.fallback_cache_lock()
            .insert(cache_key, Arc::new(chain.clone()));
        chain
    }
}
