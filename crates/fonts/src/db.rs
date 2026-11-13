//! Font discovery/selection database with CSS family fallback.
//!
//! [`FontDatabase`] wraps [`fontdb::Database`] (system fonts + byte/file-loaded
//! fonts) and resolves CSS `font-family` lists with `font-weight`/`font-style`.
//! It also implements the documented last-resort and per-glyph fallback rules
//! used by the rest of Megi.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

use fontdb::{Database, Family, Query, Source, Stretch, Style as DbStyle, Weight};

use crate::error::FontError;
use crate::font::Font;
use crate::types::{FontMetrics, FontRefInfo, FontStyle, GlyphMask, ShapedRun};

/// Generic CSS families. Requesting one of these always falls back to a real
/// face (documented "last-resort" rule) instead of returning `FontNotFound`.
const GENERIC_FAMILIES: &[&str] = &[
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "ui-sans-serif",
    "ui-serif",
    "ui-monospace",
    "emoji",
    "math",
    "fangsong",
];

/// Preferred system families tried after the OS-specific generic default.
const SANS_FALLBACKS: &[&str] = &[
    "DejaVu Sans",
    "Liberation Sans",
    "Noto Sans",
    "FreeSans",
    "Arial",
    "Helvetica",
];
const SERIF_FALLBACKS: &[&str] = &[
    "DejaVu Serif",
    "Liberation Serif",
    "Noto Serif",
    "FreeSerif",
    "Times New Roman",
];
const MONO_FALLBACKS: &[&str] = &[
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
    "FreeMono",
    "Courier New",
];
const CURSIVE_FALLBACKS: &[&str] = &["Comic Sans MS", "DejaVu Sans"];
const FANTASY_FALLBACKS: &[&str] = &["Impact", "DejaVu Sans"];

/// Maximum number of same-weight faces added to the per-glyph fallback chain
/// when the requested family list has a single entry.
const MAX_EXTRA_FALLBACKS: usize = 8;

/// A database of system and explicitly loaded font faces.
///
/// # Selection contract
/// - `families` is a CSS `font-family` list in priority order; unknown names are
///   skipped and the next entry is tried.
/// - Generic families (`sans-serif`, `serif`, `monospace`, …) expand to the OS
///   default first and then to a list of common free families (DejaVu,
///   Liberation, Noto, …). If nothing matches, the database falls back to any
///   available face (documented last resort) instead of failing.
/// - An explicitly unknown family list with no generic entry returns
///   [`FontError::FontNotFound`] (controlled error, no panic).
/// - Per-glyph fallback: when the primary face lacks a glyph, the remaining
///   requested families (and, best effort, other same-weight faces) are tried in
///   order.
///
/// The database is `Send + Sync`; the internal byte cache is behind a `Mutex`.
pub struct FontDatabase {
    db: Database,
    cache: Mutex<HashMap<fontdb::ID, Arc<Vec<u8>>>>,
    order: Vec<fontdb::ID>,
    /// CSS `@font-face` family aliases (lowercased) → faces registered under that name,
    /// regardless of the families in the font's own `name` table.
    aliases: HashMap<String, Vec<fontdb::ID>>,
    system_loaded: bool,
}

impl std::fmt::Debug for FontDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontDatabase")
            .field("faces", &self.order.len())
            .field("system_loaded", &self.system_loaded)
            .finish()
    }
}

impl Default for FontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Bundled deterministic fonts used for reproducible rendering and tests:
/// `(file_name, bytes)` for DejaVu Sans regular + bold (Bitstream Vera / DejaVu
/// license in `assets/LICENSE-DejaVu.txt`).
pub const BUNDLED_FONTS: &[(&str, &[u8])] = &[
    ("DejaVuSans.ttf", include_bytes!("../assets/DejaVuSans.ttf")),
    (
        "DejaVuSans-Bold.ttf",
        include_bytes!("../assets/DejaVuSans-Bold.ttf"),
    ),
];

/// Parses a CSS `font-family` value into a priority list of family names.
///
/// Splits on commas, trims ASCII whitespace and strips single/double quotes
/// around family names (CSS Fonts 3 §3.1). Returns `["sans-serif"]` when the
/// value contains no usable family (CSS initial `font-family` is UA-defined).
pub fn parse_font_families(font_family: &str) -> Vec<String> {
    const MAX_FAMILIES: usize = 256;
    const MAX_ENTRY_LEN: usize = 256;
    // Split on top-level commas, respecting quoted strings.
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for ch in font_family.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if in_single {
            current.push(ch);
            if ch == '\\' {
                escaped = true;
            } else if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            current.push(ch);
            if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_double = false;
            }
            continue;
        }
        match ch {
            '\'' => {
                in_single = true;
                current.push(ch);
            }
            '"' => {
                in_double = true;
                current.push(ch);
            }
            ',' => {
                parts.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    let mut families = Vec::new();
    for part in parts {
        if families.len() >= MAX_FAMILIES {
            break;
        }
        let trimmed = part.trim().trim_matches(|ch| ch == '\'' || ch == '"');
        let trimmed = trimmed.trim();
        if !trimmed.is_empty() {
            let name = if trimmed.len() > MAX_ENTRY_LEN {
                &trimmed[..MAX_ENTRY_LEN]
            } else {
                trimmed
            };
            families.push(name.to_string());
        }
    }
    if families.is_empty() {
        families.push("sans-serif".to_string());
    }
    families
}

impl FontDatabase {
    /// Loads the deterministic bundled fonts (DejaVu Sans regular + bold) from
    /// `assets/` via `include_bytes!`.
    ///
    /// This is the recommended constructor for **reproducible rendering and
    /// tests**: results do not depend on which fonts are installed on the host.
    /// No system font is loaded.
    pub fn load_bundled() -> Result<Self, FontError> {
        let mut database = Self::new();
        for (_name, bytes) in BUNDLED_FONTS {
            database.load_font_data(bytes.to_vec())?;
        }
        Ok(database)
    }

    /// Infallible variant of [`FontDatabase::load_bundled`]: the bundled bytes are
    /// validated at compile time, so this degrades to an empty database instead of
    /// failing if they could ever be corrupt.
    pub fn new_with_bundled_fonts() -> Self {
        Self::load_bundled().unwrap_or_else(|_| Self::new())
    }

    /// Loads the deterministic bundled fonts first (preserving bundled DejaVu priority
    /// for Latin and CSS generic families), then loads installed system fonts so
    /// per-glyph fallback can render CJK, Arabic, Hebrew, and emoji scripts.
    pub fn new_with_bundled_and_system_fonts() -> Self {
        let mut db = Self::new_with_bundled_fonts();
        db.load_system_fonts();
        db
    }

    /// Creates an empty database (no system fonts, no loaded bytes).
    pub fn new() -> Self {
        Self {
            db: Database::new(),
            cache: Mutex::new(HashMap::new()),
            order: Vec::new(),
            aliases: HashMap::new(),
            system_loaded: false,
        }
    }

    /// Creates a database pre-loaded with the fonts installed on the system.
    ///
    /// This scans the standard font directories and is the only potentially slow
    /// constructor (~tens of ms). It never fails: an empty result simply means the
    /// environment has no discoverable fonts.
    pub fn from_system() -> Self {
        let mut database = Self::new();
        database.load_system_fonts();
        database
    }

    /// Loads the system font directories if they were not loaded yet.
    pub fn load_system_fonts(&mut self) {
        if self.system_loaded {
            return;
        }
        self.db.load_system_fonts();
        self.system_loaded = true;
        self.refresh_order();
    }

    /// Loads one font file (TTF/OTF/TTC) from disk.
    pub fn load_font_file(&mut self, path: impl AsRef<Path>) -> Result<(), FontError> {
        let path = path.as_ref();
        // Reject non-regular files (devices, FIFOs, ...) and cap the read size.
        let metadata = std::fs::metadata(path).map_err(|err| FontError::Io {
            path: path.display().to_string(),
            reason: err.to_string(),
        })?;
        if !metadata.is_file() {
            return Err(FontError::Io {
                path: path.display().to_string(),
                reason: "not a regular file".to_string(),
            });
        }
        const MAX_FONT_FILE_BYTES: u64 = 64 * 1024 * 1024;
        if metadata.len() > MAX_FONT_FILE_BYTES {
            return Err(FontError::Io {
                path: path.display().to_string(),
                reason: format!("font file too large ({} bytes)", metadata.len()),
            });
        }
        let bytes = std::fs::read(path).map_err(|err| FontError::Io {
            path: path.display().to_string(),
            reason: err.to_string(),
        })?;
        self.load_font_data(bytes)
    }

    /// Loads font bytes (a single face or a font collection) into the database.
    ///
    /// The data is validated with `ttf-parser` before being registered; malformed
    /// input returns [`FontError::InvalidFont`] and leaves the database unchanged.
    pub fn load_font_data(&mut self, data: Vec<u8>) -> Result<(), FontError> {
        if data.is_empty() {
            return Err(FontError::InvalidFont {
                reason: "empty font data".to_string(),
            });
        }
        if let Err(err) = ttf_parser::Face::parse(&data, 0) {
            return Err(FontError::InvalidFont {
                reason: err.to_string(),
            });
        }
        let shared: Arc<Vec<u8>> = Arc::new(data);
        let ids = self.db.load_font_source(Source::Binary(
            shared.clone() as Arc<dyn AsRef<[u8]> + Send + Sync>
        ));
        if ids.is_empty() {
            return Err(FontError::InvalidFont {
                reason: "fontdb registered no face for the supplied data".to_string(),
            });
        }
        {
            let mut cache = self.cache_lock();
            for id in ids.iter() {
                cache.insert(*id, shared.clone());
            }
        }
        self.order.extend(ids.iter().copied());
        Ok(())
    }

    /// Loads raw sfnt (TTF/OTF) bytes under a CSS family **alias**.
    ///
    /// This is the `@font-face` entry point: the CSS `font-family` descriptor is the
    /// alias (it may differ from the family in the font's `name` table), and
    /// [`Self::font`]/[`Self::measure_text`]/[`Self::shape`] resolve it in priority
    /// order with the rest of the CSS family list. WOFF2 bytes must be decoded first
    /// with [`crate::decode_web_font`].
    ///
    /// Malformed data returns [`FontError::InvalidFont`] and leaves the database
    /// unchanged.
    pub fn load_font_data_family(
        &mut self,
        family_alias: &str,
        bytes_ttf: Vec<u8>,
    ) -> Result<(), FontError> {
        let alias = family_alias.trim();
        if alias.is_empty() {
            return Err(FontError::InvalidFont {
                reason: "empty @font-face family alias".to_string(),
            });
        }
        let first_new = self.order.len();
        self.load_font_data(bytes_ttf)?;
        let ids: Vec<fontdb::ID> = self.order[first_new..].to_vec();
        if ids.is_empty() {
            return Err(FontError::InvalidFont {
                reason: "fontdb registered no face for the supplied @font-face data".to_string(),
            });
        }
        self.aliases
            .entry(alias.to_ascii_lowercase())
            .or_default()
            .extend(ids);
        Ok(())
    }

    /// True if the family was registered through [`Self::load_font_data_family`].
    pub fn has_alias(&self, family: &str) -> bool {
        self.aliases.contains_key(&family.to_ascii_lowercase())
    }

    fn refresh_order(&mut self) {
        self.order = self.db.faces().map(|face| face.id).collect();
    }

    /// Number of registered faces.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// True when no face is registered.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Sorted, deduplicated English family names available in the database.
    pub fn family_names(&self) -> Vec<String> {
        let mut names: BTreeSet<String> = self
            .order
            .iter()
            .filter_map(|id| self.db.face(*id))
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect();
        names.extend(self.aliases.keys().cloned());
        names.into_iter().collect()
    }

    /// True if a face with this family name (ASCII case-insensitive per CSS Fonts 3)
    /// or an `@font-face` alias is registered.
    pub fn has_family(&self, family: &str) -> bool {
        if self.aliases.contains_key(&family.to_ascii_lowercase()) {
            return true;
        }
        self.order
            .iter()
            .filter_map(|id| self.db.face(*id))
            .any(|face| {
                face.families
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case(family))
            })
    }

    fn cache_lock(&self) -> std::sync::MutexGuard<'_, HashMap<fontdb::ID, Arc<Vec<u8>>>> {
        match self.cache.lock() {
            Ok(guard) => guard,
            // A poisoned lock only means another thread panicked while holding it;
            // the cache itself is still consistent, so reuse the data.
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn face_data(&self, id: fontdb::ID) -> Result<Arc<Vec<u8>>, FontError> {
        if let Some(data) = self.cache_lock().get(&id).cloned() {
            return Ok(data);
        }
        let data = self
            .db
            .with_face_data(id, |bytes, _index| Arc::new(bytes.to_vec()))
            .ok_or_else(|| FontError::InvalidFont {
                reason: "fontdb has no data for the requested face".to_string(),
            })?;
        self.cache_lock().insert(id, data.clone());
        Ok(data)
    }

    fn build_font(&self, id: fontdb::ID) -> Result<Font, FontError> {
        let face = self.db.face(id).ok_or_else(|| FontError::InvalidFont {
            reason: "fontdb face id is no longer registered".to_string(),
        })?;
        let info = FontRefInfo {
            family: face
                .families
                .first()
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| face.post_script_name.clone()),
            post_script_name: face.post_script_name.clone(),
            weight: face.weight.0,
            style: face.style.into(),
            monospaced: face.monospaced,
        };
        let index = face.index;
        let data = self.face_data(id)?;
        Font::new(data, index, info)
    }

    fn expand_families<'a>(families: &'a [&'a str]) -> Vec<Family<'a>> {
        let mut expanded: Vec<Family<'a>> = Vec::new();
        for family in families {
            let keyword = family.to_ascii_lowercase();
            let (generic, fallbacks): (Option<Family<'a>>, &[&'static str]) = match keyword.as_str()
            {
                "sans-serif" | "system-ui" | "ui-sans-serif" => {
                    (Some(Family::SansSerif), SANS_FALLBACKS)
                }
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
            };
            if let Some(generic) = generic {
                expanded.push(generic);
            }
            for name in fallbacks {
                expanded.push(Family::Name(name));
            }
            if generic.is_none() || !GENERIC_FAMILIES.contains(&keyword.as_str()) {
                // Explicit family name (also keeps e.g. "emoji"/"math" as names).
                expanded.push(Family::Name(family));
            }
        }
        expanded
    }

    /// Resolves an `@font-face` alias list to the best matching registered face.
    fn alias_face(&self, families: &[&str], weight: u16, style: FontStyle) -> Option<fontdb::ID> {
        let wanted_style = DbStyle::from(style);
        for family in families {
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
                    (DbStyle::Italic, DbStyle::Oblique)
                    | (DbStyle::Oblique, DbStyle::Italic) => 100,
                    _ => 1000,
                };
                let weight_penalty = i32::from(face.weight.0.abs_diff(weight));
                let score = style_penalty as i32 + weight_penalty;
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

    fn resolve_family_name<'a>(&'a self, name: &'a str) -> &'a str {
        self.order
            .iter()
            .filter_map(|id| self.db.face(*id))
            .flat_map(|face| face.families.iter())
            .find(|(registered, _)| registered.eq_ignore_ascii_case(name))
            .map(|(registered, _)| registered.as_str())
            .unwrap_or(name)
    }

    fn query_face(&self, families: &[&str], weight: u16, style: FontStyle) -> Option<fontdb::ID> {
        if let Some(id) = self.alias_face(families, weight, style) {
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
            stretch: Stretch::Normal,
            style: DbStyle::from(style),
        })
    }

    fn last_resort_face(&self, weight: u16, style: FontStyle) -> Option<fontdb::ID> {
        let wanted_style = DbStyle::from(style);
        let mut best: Option<(i32, fontdb::ID)> = None;
        for face in self.db.faces() {
            let style_penalty = match (face.style, wanted_style) {
                    (a, b) if a == b => 0u32,
                    (DbStyle::Italic, DbStyle::Oblique)
                    | (DbStyle::Oblique, DbStyle::Italic) => 100,
                    _ => 1000,
                };
            let weight_penalty = i32::from(face.weight.0.abs_diff(weight));
            let score = style_penalty as i32 + weight_penalty;
            if best.is_none_or(|(best_score, _)| score < best_score) {
                best = Some((score, face.id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Resolves a CSS family list + weight/style to a concrete font face.
    ///
    /// Unknown explicit families produce [`FontError::FontNotFound`]; generic
    /// families and the empty list use the documented last-resort face. Returns
    /// [`FontError::NoFontAvailable`] when the database is empty.
    pub fn font(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
    ) -> Result<Font, FontError> {
        if let Some(id) = self.query_face(families, weight, style) {
            return self.build_font(id);
        }
        let uses_generic = families
            .iter()
            .any(|family| GENERIC_FAMILIES.contains(&family.to_ascii_lowercase().as_str()));
        if families.is_empty() || uses_generic {
            return match self.last_resort_face(weight, style) {
                Some(id) => self.build_font(id),
                None => Err(FontError::NoFontAvailable),
            };
        }
        Err(FontError::FontNotFound {
            families: families
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        })
    }

    /// Re-resolves a previously observed [`FontRefInfo`] to a [`Font`].
    pub fn font_by_ref(&self, info: &FontRefInfo) -> Result<Font, FontError> {
        let id = self
            .order
            .iter()
            .filter_map(|id| self.db.face(*id))
            .find(|face| face.post_script_name == info.post_script_name)
            .map(|face| face.id)
            .ok_or_else(|| FontError::FontNotFound {
                families: info.family.clone(),
            })?;
        self.build_font(id)
    }

    /// Metrics of the resolved face at `size_px`.
    pub fn metrics(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        size_px: f32,
    ) -> Result<FontMetrics, FontError> {
        self.font(families, weight, style)?.metrics(size_px)
    }

    /// Fallback-aware, unshaped text width in px at `size_px`.
    ///
    /// Sums real `hmtx` advances, choosing for each character the first face in
    /// the fallback chain that has a glyph. Kerning/ligatures are **not** applied;
    /// use [`FontDatabase::shape`] for shaped width.
    pub fn measure_text(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        size_px: f32,
        text: &str,
    ) -> Result<f32, FontError> {
        Font::validate_size(size_px)?;
        let primary = self.font(families, weight, style)?;
        let mut chain = self.fallback_chain(families, weight, style, primary);
        let mut memo: HashMap<char, (usize, Option<u16>)> = HashMap::new();
        let mut width = 0.0f32;
        for ch in text.chars() {
            let (font_index, glyph_id) = *memo.entry(ch).or_insert_with(|| {
                for (index, font) in chain.iter().enumerate() {
                    if let Some(glyph_id) = font.glyph_id(ch) {
                        return (index, Some(glyph_id));
                    }
                }
                if let Some((font, glyph_id)) =
                    self.find_fallback_face_for_char(ch, &chain, weight, style)
                {
                    let index = chain.len();
                    chain.push(font);
                    return (index, Some(glyph_id));
                }
                (0, None)
            });
            let advance = match glyph_id {
                Some(glyph_id) => chain[font_index].glyph_advance(glyph_id, size_px)?,
                None => chain[0].glyph_advance(0, size_px)?,
            };
            width += advance;
        }
        Ok(width)
    }

    /// Full OpenType shaping with per-glyph font fallback.
    ///
    /// Consecutive characters that resolve to different fonts are shaped as
    /// separate segments (kerning/ligatures do not cross a fallback boundary —
    /// documented limitation). Only LTR horizontal text is supported in this
    /// phase; bidi reordering is not applied.
    pub fn shape(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        size_px: f32,
        text: &str,
    ) -> Result<ShapedRun, FontError> {
        Font::validate_size(size_px)?;
        let primary = self.font(families, weight, style)?;
        let metrics = primary.metrics(size_px)?;
        let mut chain = self.fallback_chain(families, weight, style, primary);
        let mut fonts: Vec<FontRefInfo> = chain.iter().map(|font| font.info().clone()).collect();

        if text.is_empty() {
            return Ok(ShapedRun {
                glyphs: Vec::new(),
                width: 0.0,
                ascent: metrics.ascent,
                descent: metrics.descent,
                fonts: vec![fonts[0].clone()],
            });
        }

        let mut glyphs = Vec::new();
        let mut memo: HashMap<char, usize> = HashMap::new();
        // Current segment: (font index in `chain`, byte start, segment text).
        let mut segment: Option<(usize, usize, String)> = None;
        for (byte_index, ch) in text.char_indices() {
            let font_index = *memo.entry(ch).or_insert_with(|| {
                for (index, font) in chain.iter().enumerate() {
                    if font.has_glyph(ch) {
                        return index;
                    }
                }
                if let Some((font, _)) = self.find_fallback_face_for_char(ch, &chain, weight, style)
                {
                    let index = chain.len();
                    fonts.push(font.info().clone());
                    chain.push(font);
                    return index;
                }
                0
            });
            match &mut segment {
                Some((current, _start, buffer)) if *current == font_index => buffer.push(ch),
                Some((current, start, buffer)) => {
                    glyphs.extend(chain[*current].shape_segment(
                        size_px,
                        buffer,
                        *start as u32,
                        *current,
                    )?);
                    segment = Some((font_index, byte_index, ch.to_string()));
                }
                None => segment = Some((font_index, byte_index, ch.to_string())),
            }
        }
        if let Some((font_index, start, buffer)) = segment {
            glyphs.extend(chain[font_index].shape_segment(
                size_px,
                &buffer,
                start as u32,
                font_index,
            )?);
        }
        // Only emit faces that actually shaped at least one glyph, remapping the
        // glyph.font indices so unused fallback faces do not travel with the run
        // (issue #610).
        let mut used: Vec<bool> = vec![false; fonts.len()];
        for glyph in &glyphs {
            if let Some(slot) = used.get_mut(glyph.font) {
                *slot = true;
            }
        }
        // The primary face (index 0) is always retained so an empty run still
        // carries its metrics face.
        if let Some(slot) = used.first_mut() {
            *slot = true;
        }
        let mut remap: Vec<usize> = vec![0; fonts.len()];
        let mut compact: Vec<FontRefInfo> = Vec::new();
        for (index, info) in fonts.into_iter().enumerate() {
            if used[index] {
                remap[index] = compact.len();
                compact.push(info);
            }
        }
        for glyph in &mut glyphs {
            glyph.font = remap
                .get(glyph.font)
                .copied()
                .unwrap_or(0);
        }
        let width = glyphs.iter().map(|glyph| glyph.x_advance).sum();
        Ok(ShapedRun {
            glyphs,
            width,
            ascent: metrics.ascent,
            descent: metrics.descent,
            fonts: compact,
        })
    }

    /// Rasterizes a glyph of the primary face into an alpha coverage mask.
    ///
    /// `glyph_id` is interpreted in the primary face selected by the family list.
    /// For character-driven rendering with fallback use
    /// [`FontDatabase::rasterize_char`].
    pub fn rasterize_glyph(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        size_px: f32,
        glyph_id: u16,
    ) -> Result<GlyphMask, FontError> {
        self.font(families, weight, style)?
            .rasterize_glyph(size_px, glyph_id)
    }

    /// Rasterizes the glyph for `ch`, applying per-glyph fallback.
    pub fn rasterize_char(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        size_px: f32,
        ch: char,
    ) -> Result<GlyphMask, FontError> {
        Font::validate_size(size_px)?;
        let primary = self.font(families, weight, style)?;
        if let Some(glyph_id) = primary.glyph_id(ch) {
            return primary.rasterize_glyph(size_px, glyph_id);
        }
        let chain = self.fallback_chain(families, weight, style, primary);
        for font in &chain {
            if let Some(glyph_id) = font.glyph_id(ch) {
                return font.rasterize_glyph(size_px, glyph_id);
            }
        }
        if let Some((font, glyph_id)) = self.find_fallback_face_for_char(ch, &chain, weight, style)
        {
            return font.rasterize_glyph(size_px, glyph_id);
        }
        // No face has the glyph: render the primary face's .notdef box.
        chain[0].rasterize_glyph(size_px, 0)
    }

    /// Scans all loaded faces (not yet in `chain`) for a glyph matching `ch`, invoked
    /// only when the initial fallback chain lacks `ch` (e.g. CJK, Arabic, emoji).
    /// When `self` only loaded bundled/web fonts, falls back to a cached system font DB
    /// so missing scripts render without altering primary `serif`/`sans-serif`/`monospace`
    /// resolution for bundled fonts.
    fn find_fallback_face_for_char(
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
            static SYSTEM_FALLBACK_DB: std::sync::OnceLock<FontDatabase> =
                std::sync::OnceLock::new();
            let sys_db = SYSTEM_FALLBACK_DB.get_or_init(FontDatabase::from_system);
            return sys_db.scan_faces_for_char(ch, chain, weight, style);
        }
        None
    }

    fn scan_faces_for_char(
        &self,
        ch: char,
        chain: &[Font],
        weight: u16,
        style: FontStyle,
    ) -> Option<(Font, u16)> {
        let wanted_style = DbStyle::from(style);
        let mut candidates: Vec<(u32, fontdb::ID)> = self
            .db
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
    fn fallback_chain(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        primary: Font,
    ) -> Vec<Font> {
        let mut chain: Vec<Font> = vec![primary];
        // Dedup by the underlying face bytes instead of the PostScript name:
        // distinct subset faces can share a PS name and must not be dropped (#612).
        let mut seen = std::collections::HashSet::new();
        seen.insert(std::sync::Arc::as_ptr(&chain[0].data) as usize);

        for family in families {
            if let Ok(font) = self.font(&[*family], weight, style)
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
                    (DbStyle::Italic, DbStyle::Oblique)
                    | (DbStyle::Oblique, DbStyle::Italic) => 100,
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
        chain
    }
}

impl FontDatabase {
    /// Convenience alias kept for readability at call sites that resolve one face.
    pub fn resolve(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
    ) -> Result<Font, FontError> {
        self.font(families, weight, style)
    }
}
