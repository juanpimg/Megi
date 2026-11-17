//! Web-font (`@font-face`) collection, fetching and registration (Phase 6).
//!
//! ## Pipeline
//!
//! 1. The CSS layer exposes [`css::FontFaceRule`]s per stylesheet (no I/O there).
//! 2. This module deduplicates sources by final URL, fetches them with the shared
//!    [`net::ResourceLoader`] (so `--no-network` and the HTTP cache apply), decodes WOFF2
//!    into sfnt bytes with [`fonts::decode_web_font`] and registers each face under its CSS
//!    `font-family` alias with [`fonts::FontDatabase::load_font_data_family`].
//! 3. The resulting database starts from the deterministic bundled faces and is used by
//!    **both** layout (`layout::FontTextMeasurer`) and paint
//!    (`paint::rasterize_display_list_with_font_database`).
//!
//! ## Limits (fixed, non-fatal warnings)
//!
//! - at most **8** font faces attempted per render;
//! - at most **8 MiB** per decoded source;
//! - at most **32 MiB** of accepted encoded bytes in total;
//! - `--no-network` skips `http(s)` sources with a warning and falls back to the bundled
//!   fonts; failed/decode-error faces also become non-fatal warnings.

use std::collections::HashSet;

use css::FontFaceRule;
use fonts::FontDatabase;
use net::{ResourceLoader, Scheme, Url};

/// Maximum number of `@font-face` sources attempted per render.
pub const MAX_WEB_FONTS: usize = 8;
/// Maximum encoded bytes accepted for a single web font.
pub const MAX_WEB_FONT_BYTES: usize = 8 * 1024 * 1024;
/// Maximum encoded bytes accepted for all web fonts in one render.
pub const MAX_WEB_FONT_TOTAL_BYTES: usize = 32 * 1024 * 1024;

/// Defensive caps applied while loading `@font-face` web fonts (#531).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebFontLimits {
    /// Maximum number of `@font-face` sources attempted per render.
    pub max_web_fonts: usize,
    /// Maximum encoded bytes accepted for a single web font.
    pub max_web_font_bytes: usize,
    /// Maximum encoded bytes accepted for all web fonts in one render.
    pub max_total_web_font_bytes: usize,
}

impl Default for WebFontLimits {
    fn default() -> Self {
        Self {
            max_web_fonts: MAX_WEB_FONTS,
            max_web_font_bytes: MAX_WEB_FONT_BYTES,
            max_total_web_font_bytes: MAX_WEB_FONT_TOTAL_BYTES,
        }
    }
}

/// Result of the web-font stage.
pub struct LoadedWebFonts {
    /// Bundled faces plus every successfully decoded `@font-face` alias.
    pub database: FontDatabase,
    /// Faces registered under their CSS family.
    pub loaded: usize,
    /// Faces skipped/failed (network, caps, decode, invalid data).
    pub failed: usize,
    /// Encoded bytes of the successfully loaded fonts.
    pub bytes: usize,
}

/// A `@font-face` rule paired with the base URL of the stylesheet that declared it.
pub type LocatedFontFace = (Url, FontFaceRule);

/// Collects `@font-face` rules from all author sheets (including `@import`ed ones).
/// Returns them in cascade order, paired with the base URL used to resolve `src` URLs.
/// Supported `format()` values; an absent `format()` is accepted (the decoder sniffs).
fn is_supported_format(format: &str) -> bool {
    format.is_empty()
        || format.eq_ignore_ascii_case("woff2")
        || format.eq_ignore_ascii_case("truetype")
        || format.eq_ignore_ascii_case("opentype")
        || format.eq_ignore_ascii_case("ttf")
        || format.eq_ignore_ascii_case("otf")
}

/// Candidate sources for one `@font-face`, in the order they must be tried.
///
/// WOFF2 comes first (smallest transfer), then the remaining supported formats in authored
/// order. Trying the alternatives matters: a broken or rejected WOFF2 used to kill the whole
/// face even when a `truetype` fallback was declared right after it (#665).
fn face_sources(face: &FontFaceRule) -> Vec<&str> {
    let woff2 = face
        .sources
        .iter()
        .filter(|(_, format)| format.eq_ignore_ascii_case("woff2"))
        .map(|(url, _)| url.as_str());
    let rest = face
        .sources
        .iter()
        .filter(|(_, format)| !format.eq_ignore_ascii_case("woff2") && is_supported_format(format))
        .map(|(url, _)| url.as_str());
    woff2.chain(rest).collect()
}

/// Fetches, decodes and registers every usable `@font-face` source using default [`WebFontLimits`].
pub fn load_webfonts(
    faces: &[LocatedFontFace],
    loader: &mut ResourceLoader,
    no_network: bool,
    warnings: &mut Vec<String>,
) -> LoadedWebFonts {
    load_webfonts_with_limits(
        faces,
        loader,
        no_network,
        WebFontLimits::default(),
        warnings,
    )
}

/// Fetches, decodes and registers every usable `@font-face` source under explicit [`WebFontLimits`].
///
/// Faces are independent: one face that fails (bad URL, rejected by `--no-network`, oversized,
/// undecodable) no longer starves the faces declared after it. Only sources that actually reach
/// the loader count toward [`WebFontLimits::max_web_fonts`], and each face contributes one entry to
/// [`LoadedWebFonts::failed`] when none of its candidates worked (#665, #531).
pub fn load_webfonts_with_limits(
    faces: &[LocatedFontFace],
    loader: &mut ResourceLoader,
    no_network: bool,
    limits: WebFontLimits,
    warnings: &mut Vec<String>,
) -> LoadedWebFonts {
    let mut database = FontDatabase::new_with_bundled_fonts();
    let mut seen: HashSet<String> = HashSet::new();
    let mut loaded = 0usize;
    let mut failed = 0usize;
    let mut bytes = 0usize;
    let mut attempts = 0usize;

    'faces: for (index, (base_url, face)) in faces.iter().enumerate() {
        if attempts >= limits.max_web_fonts {
            let remaining = faces.len() - index;
            failed += remaining;
            warnings.push(format!(
                "skipping @font-face \"{}\": max web fonts per render ({}) reached",
                face.family, limits.max_web_fonts
            ));
            break;
        }
        let candidates = face_sources(face);
        if candidates.is_empty() {
            failed += 1;
            warnings.push(format!(
                "skipping @font-face \"{}\": no supported src",
                face.family
            ));
            continue;
        }
        let mut last_warning: Option<String> = None;
        for source in candidates {
            let url = match base_url.join(source) {
                Ok(url) => url,
                Err(error) => {
                    last_warning = Some(format!(
                        "skipping @font-face \"{}\" source {source}: {error}",
                        face.family
                    ));
                    continue;
                }
            };
            // Two rules may point at the same file for different descriptors (a common
            // unicode-range / weight split), so the key keeps the descriptor triple (#665).
            let key = format!(
                "{}|{}|{}|{}",
                url.cache_key(),
                face.family,
                face.weight,
                face.style
            );
            if !seen.insert(key) {
                continue;
            }
            if no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
                last_warning = Some(format!(
                    "skipping web font {url} for \"{}\" (--no-network is set)",
                    face.family
                ));
                continue;
            }
            if attempts >= limits.max_web_fonts {
                last_warning = Some(format!(
                    "skipping @font-face \"{}\": max web fonts per render ({}) reached",
                    face.family, limits.max_web_fonts
                ));
                break;
            }
            attempts += 1;
            let resource = match loader.load(&url) {
                Ok(resource) => resource,
                Err(error) => {
                    last_warning = Some(format!(
                        "cannot load web font {url} for \"{}\": {error}",
                        face.family
                    ));
                    continue;
                }
            };
            if resource.bytes.len() > limits.max_web_font_bytes {
                last_warning = Some(format!(
                    "web font {url} is too large ({} bytes > {})",
                    resource.bytes.len(),
                    limits.max_web_font_bytes
                ));
                continue;
            }
            if bytes + resource.bytes.len() > limits.max_total_web_font_bytes {
                last_warning = Some(format!(
                    "web font {url} exceeds the total budget ({} bytes)",
                    limits.max_total_web_font_bytes
                ));
                continue;
            }
            let sfnt = match fonts::decode_web_font(&resource.bytes) {
                Ok(sfnt) => sfnt,
                Err(error) => {
                    last_warning = Some(format!(
                        "cannot decode web font {url} for \"{}\": {error}",
                        face.family
                    ));
                    continue;
                }
            };
            match database.load_font_data_family(&face.family, sfnt) {
                Ok(()) => {
                    loaded += 1;
                    bytes += resource.bytes.len();
                    continue 'faces;
                }
                Err(error) => {
                    last_warning = Some(format!(
                        "cannot register web font {url} for \"{}\": {error}",
                        face.family
                    ));
                }
            }
        }
        failed += 1;
        if let Some(warning) = last_warning {
            warnings.push(warning);
        }
    }

    LoadedWebFonts {
        database,
        loaded,
        failed,
        bytes,
    }
}
