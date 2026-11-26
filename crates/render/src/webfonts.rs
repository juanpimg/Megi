//! Web-font (`@font-face`) collection, fetching and registration (Phase 6).
//!
//! ## Pipeline
//!
//! 1. The CSS layer exposes [`megi_css::FontFaceRule`]s per stylesheet (no I/O there).
//! 2. This module deduplicates sources by final URL, fetches them with the shared
//!    [`megi_net::ResourceLoader`] (so `--no-network` and the HTTP cache apply), decodes WOFF2
//!    into sfnt bytes with [`megi_fonts::decode_web_font`] and registers each face under its CSS
//!    `font-family` alias with [`megi_fonts::FontDatabase::load_font_data_family`].
//! 3. The resulting database starts from the deterministic bundled faces (optionally plus
//!    installed system faces in the opt-in Chrome-parity mode) and is used by **both** layout
//!    (`megi_layout::FontTextMeasurer`) and paint
//!    (`megi_paint::rasterize_display_list_with_font_database`).
//!
//! ## Limits (fixed, non-fatal warnings)
//!
//! - at most **128** font faces attempted per render;
//! - at most **8 MiB** per decoded source;
//! - at most **32 MiB** of accepted encoded bytes in total;
//! - `--no-network` skips `http(s)` sources with a warning and falls back to the bundled
//!   fonts; failed/decode-error faces also become non-fatal warnings.

use std::collections::HashSet;

use megi_css::FontFaceRule;
use megi_fonts::FontDatabase;
use megi_net::{ResourceLoader, Scheme, Url};

/// Maximum number of `@font-face` sources attempted per render.
pub const MAX_WEB_FONTS: usize = 128;
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
        || format.eq_ignore_ascii_case("woff")
        || format.eq_ignore_ascii_case("truetype")
        || format.eq_ignore_ascii_case("opentype")
        || format.eq_ignore_ascii_case("ttf")
        || format.eq_ignore_ascii_case("otf")
}

/// Candidate sources for one `@font-face`, in the order they must be tried.
///
/// WOFF2 comes first (smallest transfer), then WOFF1, then the remaining supported formats in authored
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
    load_webfonts_with_csp(faces, loader, no_network, limits, None, warnings)
}

/// Like [`load_webfonts_with_limits`] but enforces a Content Security Policy (`font-src`, #1526).
pub fn load_webfonts_with_csp(
    faces: &[LocatedFontFace],
    loader: &mut ResourceLoader,
    no_network: bool,
    limits: WebFontLimits,
    csp: Option<(&megi_net::csp::ContentSecurityPolicy, &Url)>,
    warnings: &mut Vec<String>,
) -> LoadedWebFonts {
    load_webfonts_with_system_fonts(faces, loader, no_network, limits, csp, false, warnings)
}

/// Like [`load_webfonts_with_csp`] but optionally loads installed system faces too.
///
/// CSS Fonts 4 §3.1: a system-backed database resolves generic `sans-serif` to the
/// platform default face (Noto Sans on Linux, like Chrome). `system_fonts = false`
/// (the default) keeps the bundled-only database so deterministic goldens do not change.
pub fn load_webfonts_with_system_fonts(
    faces: &[LocatedFontFace],
    loader: &mut ResourceLoader,
    no_network: bool,
    limits: WebFontLimits,
    csp: Option<(&megi_net::csp::ContentSecurityPolicy, &Url)>,
    system_fonts: bool,
    warnings: &mut Vec<String>,
) -> LoadedWebFonts {
    let mut database = if system_fonts {
        FontDatabase::new_with_bundled_and_system_fonts()
    } else {
        FontDatabase::new_with_bundled_fonts()
    };
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
            if let Some((policy, document_url)) = csp
                && !policy.allows_font(&url, document_url)
            {
                last_warning = Some(format!(
                    "skipping @font-face \"{}\" source {url}: blocked by Content-Security-Policy",
                    face.family
                ));
                continue;
            }
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
            let resource = match loader.load_checked(&url, true) {
                Ok(resource) => resource,
                Err(error) => {
                    last_warning = Some(format!(
                        "cannot load web font {url} for \"{}\": {error}",
                        face.family
                    ));
                    continue;
                }
            };
            // #2244: a 404 often returns an HTML error page; do not feed it to
            // the WOFF2 decoder (which would report a misleading "unknown magic").
            if resource.status >= 400 {
                last_warning = Some(format!(
                    "web font {url} for \"{}\" returned HTTP {}",
                    face.family, resource.status
                ));
                continue;
            }
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
            let sfnt = match megi_fonts::decode_web_font(&resource.bytes) {
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

#[cfg(test)]
mod supported_format_tests {
    use super::is_supported_format;

    #[test]
    fn woff1_is_a_supported_src_format() {
        // #1893: woff-only faces (postgresql.org) must reach the loader, not be discarded.
        assert!(is_supported_format("woff"));
        assert!(is_supported_format("WOFF"));
        assert!(is_supported_format("woff2"));
        assert!(is_supported_format("truetype"));
        assert!(!is_supported_format("unknown-format"));
    }

    #[test]
    fn docs_match_max_web_fonts_constant() {
        let webfonts_src = include_str!("webfonts.rs");
        let expected_line = format!(
            "//! - at most **{}** font faces attempted per render;",
            super::MAX_WEB_FONTS
        );
        assert!(
            webfonts_src.contains(&expected_line),
            "webfonts.rs doc comment must match MAX_WEB_FONTS={}",
            super::MAX_WEB_FONTS
        );
        let lib_src = include_str!("lib.rs");
        let expected_caps = format!("Caps: {} fonts,", super::MAX_WEB_FONTS);
        assert!(
            lib_src.contains(&expected_caps),
            "lib.rs doc comment must match MAX_WEB_FONTS={}",
            super::MAX_WEB_FONTS
        );
    }
}

#[cfg(test)]
mod system_fonts_parity_tests {
    use crate::{RenderOptions, Viewport, render_url};

    #[test]
    fn system_fonts_mode_resolves_generic_sans_serif_to_noto_sans() {
        // CSS Fonts 4 §3.1: `sans-serif` is the platform default face (Noto Sans
        // on Linux, like Chrome). The opt-in mode must use it; the default stays bundled.
        let page = |family: &str| {
            format!("data:text/html,<style>body{{font:32px {family}}}</style><p>Handgloves</p>")
        };
        let opts = RenderOptions {
            viewport: Viewport::new(420, 120).expect("viewport"),
            ..RenderOptions::default()
        };
        let system_opts = RenderOptions {
            system_fonts: true,
            ..opts
        };
        let generic = render_url(&page("sans-serif"), &system_opts).expect("system render");
        let ui = render_url(&page("system-ui"), &system_opts).expect("system-ui render");
        // The platform default family is host-specific (Noto Sans on some CI
        // images, DejaVu Sans elsewhere). The host-independent invariant is
        // that both platform generics select the same face in opt-in mode.
        assert!(!generic.png.is_empty() && !ui.png.is_empty(), "valid PNGs");
        assert_eq!(
            generic.png, ui.png,
            "sans-serif and system-ui must resolve to the same platform default"
        );
    }
}
