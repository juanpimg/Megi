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

mod load;
mod query;
mod shape;

#[cfg(test)]
mod tests;

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

/// CSS Fonts 4 §3.1: `system-ui` is the default UI font of the platform, not the
/// `sans-serif` generic. Linux fontconfig (and therefore Chrome) maps it to
/// Noto Sans; keep that first so a system font DB resolves the same face. The
/// remaining names cover other platforms.
const SYSTEM_UI_FALLBACKS: &[&str] = &[
    "Noto Sans",
    "Segoe UI",
    "Helvetica Neue",
    "DejaVu Sans",
    "Liberation Sans",
    "FreeSans",
    "Arial",
];
/// CSS Fonts 4 §3.1 / fontconfig: the generic `sans-serif` names the system's
/// default sans face, which on Linux (and in Chrome) is Noto Sans. Keep it first
/// so a system font DB matches Chrome; DejaVu Sans stays as a fallback for
/// databases that do not carry Noto.
const SANS_FALLBACKS: &[&str] = &[
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
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

/// Deterministic names for fontdb's coarse generic families. fontdb 0.24
/// rewrites these from fontconfig aliases inside `load_system_fonts()`; on
/// CachyOS that maps `sans-serif` to "Nimbus Sans Narrow" and `serif` to
/// "Standard Symbols PS", so a CSS list with no installed named family paints
/// a symbol/narrow face (CSS Fonts 4 §5.2). Pinning them keeps generic
/// resolution host-independent while host faces remain available by name.
const GENERIC_SANS_FAMILY: &str = "DejaVu Sans";
const GENERIC_SERIF_FAMILY: &str = "DejaVu Serif";
const GENERIC_MONOSPACE_FAMILY: &str = "DejaVu Sans Mono";
const GENERIC_CURSIVE_FAMILY: &str = "Comic Sans MS";
const GENERIC_FANTASY_FAMILY: &str = "Impact";

/// Maximum number of same-weight faces added to the per-glyph fallback chain
/// when the requested family list has a single entry.
const MAX_EXTRA_FALLBACKS: usize = 8;

/// Maximum number of faces loaded via `load_font_data` / `load_font_data_family` (#497).
const MAX_LOADED_FONT_FACES: usize = 256;

/// Maximum aggregate bytes loaded via `load_font_data` / `load_font_data_family` (#497).
const MAX_LOADED_FONT_BYTES: usize = 64 * 1024 * 1024;

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn style_cache_key(style: FontStyle) -> u8 {
    match style {
        FontStyle::Normal => 0,
        FontStyle::Italic => 1,
        FontStyle::Oblique => 2,
    }
}

fn is_combining_mark(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x05BF
            | 0x05C1..=0x05C2
            | 0x05C4..=0x05C5
            | 0x05C7
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0670
            | 0x06D6..=0x06DC
            | 0x06DF..=0x06E4
            | 0x06E7..=0x06E8
            | 0x06EA..=0x06ED
            | 0x0900..=0x0903
            | 0x093A..=0x094F
            | 0x0951..=0x0957
            | 0x0962..=0x0963
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0xFE00..=0xFE0F
            | 0xFE20..=0xFE2F
            | 0xE0100..=0xE01EF
    )
}

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
type FallbackChainKey = (Vec<String>, u16, u8);
type FallbackChainMap = HashMap<FallbackChainKey, Arc<Vec<Font>>>;

pub struct FontDatabase {
    db: Database,
    cache: Mutex<HashMap<fontdb::ID, Arc<Vec<u8>>>>,
    fallback_cache: Mutex<FallbackChainMap>,
    order: Vec<fontdb::ID>,
    /// CSS `@font-face` family aliases (lowercased) → faces registered under that name,
    /// regardless of the families in the font's own `name` table.
    aliases: HashMap<String, Vec<fontdb::ID>>,
    /// Content-dedup map: `(byte_len, fnv1a_64)` → registered `fontdb::ID` lists (#497).
    loaded_hashes: HashMap<(usize, u64), Vec<Vec<fontdb::ID>>>,
    loaded_bytes: usize,
    loaded_faces: usize,
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

/// Internal marker prepended to family names that were quoted in the source. Quoted
/// names are always literal family names, never generic keywords (#1239); the marker
/// keeps that distinction through the `&[&str]` font-database API.
const QUOTED_FAMILY_PREFIX: &str = "\u{1}";

/// Returns the literal name of a possibly marked family string.
fn unmark_family(name: &str) -> &str {
    name.strip_prefix(QUOTED_FAMILY_PREFIX).unwrap_or(name)
}

/// Lazily-loaded system font database used for per-glyph fallback scripts
/// (CJK, Arabic, emoji). Shared by the fallback scan and by `font_by_ref`, so a
/// face selected during shaping can be rebuilt during rasterization (#1977).
pub(crate) fn system_fallback_db() -> &'static FontDatabase {
    static SYSTEM_FALLBACK_DB: std::sync::OnceLock<FontDatabase> = std::sync::OnceLock::new();
    SYSTEM_FALLBACK_DB.get_or_init(FontDatabase::from_system)
}

/// Parses a CSS `font-family` value into a priority list of family names.
///
/// Splits on commas, trims ASCII whitespace and strips single/double quotes
/// around family names (CSS Fonts 3 §3.1). Returns `["sans-serif"]` when the
/// value contains no usable family (CSS initial `font-family` is UA-defined).
/// Appends the default generic family when the list has no generic entry.
///
/// CSS Fonts 3 §5.2 requires the UA to fall back to its default font when none
/// of the requested families is available. The low-level
/// [`crate::FontDatabase::font`] keeps returning `FontNotFound` for callers that
/// need a controlled error, but browser layers (layout measurement and text
/// rasterization) must always resolve to a real face: otherwise an unavailable
/// explicit family such as `Arial` degrades to the historical 8×8 bitmap stub,
/// which has uppercase-only glyphs and no digits (#1976).
pub fn ensure_generic_fallback(families: &mut Vec<String>) {
    let has_generic = families.iter().any(|name| {
        !name.starts_with(QUOTED_FAMILY_PREFIX)
            && GENERIC_FAMILIES.contains(&unmark_family(name).to_ascii_lowercase().as_str())
    });
    if !has_generic {
        families.push("sans-serif".to_owned());
    }
}

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
        let trimmed = part.trim();
        // Strip at most one matched quote pair (CSS Fonts 3); mismatched quotes
        // are kept literal instead of being trimmed away (#605).
        let was_quoted = trimmed.len() >= 2
            && ((trimmed.starts_with('\'') && trimmed.ends_with('\''))
                || (trimmed.starts_with('"') && trimmed.ends_with('"')));
        let trimmed = if was_quoted {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        };
        let trimmed = trimmed.trim();
        if !trimmed.is_empty() {
            // Unescape CSS backslash sequences (e.g. \" -> ").
            let mut unescaped = String::with_capacity(trimmed.len());
            let mut chars = trimmed.chars();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    if let Some(next) = chars.next() {
                        unescaped.push(next);
                    } else {
                        // Trailing backslash: keep it.
                        unescaped.push(ch);
                    }
                } else {
                    unescaped.push(ch);
                }
            }
            let name = if unescaped.len() > MAX_ENTRY_LEN {
                // Truncate on a char boundary to avoid panicking on multi-byte
                // code points straddling the cap (#1228).
                let mut end = MAX_ENTRY_LEN;
                while end > 0 && !unescaped.is_char_boundary(end) {
                    end -= 1;
                }
                &unescaped[..end]
            } else {
                &unescaped
            };
            // Mark quoted names so generic-keyword detection skips them (#1239).
            families.push(if was_quoted {
                format!("{QUOTED_FAMILY_PREFIX}{name}")
            } else {
                name.to_string()
            });
        }
    }
    if families.is_empty() {
        families.push("sans-serif".to_string());
    }
    families
}
