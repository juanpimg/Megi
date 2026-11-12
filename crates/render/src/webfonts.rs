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
fn face_source(face: &FontFaceRule) -> Option<&str> {
    // Prefer WOFF2, then any declared source (TTF/OTF fallbacks are accepted).
    face.woff2_source().or_else(|| {
        face.sources
            .iter()
            .find(|(_, format)| {
                format.is_empty()
                    || format.eq_ignore_ascii_case("truetype")
                    || format.eq_ignore_ascii_case("opentype")
                    || format.eq_ignore_ascii_case("ttf")
                    || format.eq_ignore_ascii_case("otf")
            })
            .map(|(url, _)| url.as_str())
    })
}

/// Fetches, decodes and registers every usable `@font-face` source.
pub fn load_webfonts(
    faces: &[LocatedFontFace],
    loader: &mut ResourceLoader,
    no_network: bool,
    warnings: &mut Vec<String>,
) -> LoadedWebFonts {
    let mut database = FontDatabase::new_with_bundled_and_system_fonts();
    let mut seen: HashSet<String> = HashSet::new();
    let mut loaded = 0usize;
    let mut failed = 0usize;
    let mut bytes = 0usize;

    for (base_url, face) in faces {
        if loaded + failed >= MAX_WEB_FONTS {
            warnings.push(format!(
                "skipping @font-face \"{}\": max web fonts per render ({MAX_WEB_FONTS}) reached",
                face.family
            ));
            break;
        }
        let Some(source) = face_source(face) else {
            failed += 1;
            warnings.push(format!(
                "skipping @font-face \"{}\": no supported src",
                face.family
            ));
            continue;
        };
        let url = match base_url.join(source) {
            Ok(url) => url,
            Err(error) => {
                failed += 1;
                warnings.push(format!(
                    "skipping @font-face \"{}\" source {source}: {error}",
                    face.family
                ));
                continue;
            }
        };
        if !seen.insert(url.cache_key()) {
            continue;
        }
        if no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
            failed += 1;
            warnings.push(format!(
                "skipping web font {url} for \"{}\" (--no-network is set)",
                face.family
            ));
            continue;
        }
        let resource = match loader.load(&url) {
            Ok(resource) => resource,
            Err(error) => {
                failed += 1;
                warnings.push(format!(
                    "cannot load web font {url} for \"{}\": {error}",
                    face.family
                ));
                continue;
            }
        };
        if resource.bytes.len() > MAX_WEB_FONT_BYTES {
            failed += 1;
            warnings.push(format!(
                "web font {url} is too large ({} bytes > {MAX_WEB_FONT_BYTES})",
                resource.bytes.len()
            ));
            continue;
        }
        if bytes + resource.bytes.len() > MAX_WEB_FONT_TOTAL_BYTES {
            failed += 1;
            warnings.push(format!(
                "web font {url} exceeds the total budget ({MAX_WEB_FONT_TOTAL_BYTES} bytes)"
            ));
            continue;
        }
        let sfnt = match fonts::decode_web_font(&resource.bytes) {
            Ok(sfnt) => sfnt,
            Err(error) => {
                failed += 1;
                warnings.push(format!(
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
            }
            Err(error) => {
                failed += 1;
                warnings.push(format!(
                    "cannot register web font {url} for \"{}\": {error}",
                    face.family
                ));
            }
        }
    }

    LoadedWebFonts {
        database,
        loaded,
        failed,
        bytes,
    }
}
