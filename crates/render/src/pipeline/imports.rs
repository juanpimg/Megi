//! Recursive `@import` loading, cascade ordering and stylesheet signatures.
//!
//! Moved verbatim from `lib.rs`.

use megi_core::Document;
use megi_net::{ResourceLoader, Url};

use super::state_stylesheet_text::load_stylesheet_text;
use super::styles::{StyleSummary, account_css_bytes};
use crate::{RenderError, RenderOptions};
use crate::{stylesheets, webfonts};

/// Mutable state shared by the recursive `@import` walk.
struct ImportWalk<'a> {
    loader: &'a mut ResourceLoader,
    options: &'a RenderOptions,
    media: &'a megi_css::MediaContext,
    base_url: &'a Url,
    csp: Option<&'a megi_net::csp::ContentSecurityPolicy>,
    warnings: &'a mut Vec<String>,
    visited: std::collections::HashSet<String>,
    loaded: usize,
    /// Loads attempted (successful or not), bounded by `max_stylesheets`.
    attempts: usize,
    /// Whether the attempt-cap warning was already pushed (one per walk).
    cap_warned: bool,
    /// Whether the CSS byte-budget warning was already pushed (one per walk, #654).
    budget_warned: bool,
    /// Total @import entries processed (including skipped), bounded to prevent
    /// unbounded CPU on trivial imports (#524).
    total_imports: usize,
    skipped: usize,
    max_depth: usize,
    css_bytes: &'a mut usize,
}

/// Computes a content-and-attribute signature of stylesheet-producing elements (`<base>`,
/// `<style>`, and `<link>`).
///
/// The post-script style pass can reuse the pre-script collection only when scripts did
/// not add, remove, reorder, or mutate any `<style>`, `<link>`, or `<base>` element,
/// including `<style>` text content and `<link>`/`<base>` attributes (#462).
pub(crate) fn stylesheet_signature(document: &Document) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mix = |hash: &mut u64, bytes: &[u8]| {
        for &byte in bytes {
            *hash ^= u64::from(byte);
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // Record delimiter so adjacent fields cannot collide.
        *hash ^= 0xff;
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };

    for node_id in document.descendants(document.root()) {
        let Some(element) = document.node(node_id).as_element() else {
            continue;
        };
        if element.namespace != megi_core::Namespace::Html {
            continue;
        }
        let local = element.local_name.as_str();
        if local.eq_ignore_ascii_case("base") {
            mix(&mut hash, b"B");
            if let Some(href) = element.get_attribute("href") {
                mix(&mut hash, href.as_bytes());
            }
        } else if local.eq_ignore_ascii_case("style") {
            mix(&mut hash, b"S");
            for attr_name in ["media", "type", "disabled", "nonce"] {
                if let Some(val) = element.get_attribute(attr_name) {
                    mix(&mut hash, attr_name.as_bytes());
                    mix(&mut hash, val.as_bytes());
                }
            }
            for desc_id in document.descendants(node_id) {
                if let megi_core::NodeKind::Text(text) = &document.node(desc_id).kind {
                    mix(&mut hash, text.as_bytes());
                }
            }
        } else if local.eq_ignore_ascii_case("link") {
            mix(&mut hash, b"L");
            for attr_name in [
                "rel",
                "href",
                "media",
                "type",
                "disabled",
                "crossorigin",
                "integrity",
            ] {
                if let Some(val) = element.get_attribute(attr_name) {
                    mix(&mut hash, attr_name.as_bytes());
                    mix(&mut hash, val.as_bytes());
                }
            }
        }
    }
    hash
}

pub(crate) fn build_author_stylesheet_collection(
    collected: &stylesheets::CollectedStyles,
    base_url: &Url,
    loader: &mut ResourceLoader,
    options: &RenderOptions,
    media: &megi_css::MediaContext,
    csp: Option<&megi_net::csp::ContentSecurityPolicy>,
    warnings: &mut Vec<String>,
) -> Result<
    (
        megi_css::StyleCollection,
        StyleSummary,
        Vec<webfonts::LocatedFontFace>,
    ),
    RenderError,
> {
    let mut collection = megi_css::create_user_agent_style_collection();
    collection.media = media.clone();
    let mut font_faces: Vec<webfonts::LocatedFontFace> = Vec::new();
    let mut summary = StyleSummary {
        external_stylesheets: 0,
        inline_stylesheets: 0,
        css_bytes: 0,
        imports_loaded: 0,
        imports_skipped: 0,
        max_import_depth: 0,
        media_skipped: collected.media_skipped,
        sheets_capped: false,
    };
    let mut walk = ImportWalk {
        loader,
        options,
        media,
        base_url,
        csp,
        warnings,
        visited: std::collections::HashSet::new(),
        loaded: 0,
        attempts: 0,
        cap_warned: false,
        budget_warned: false,
        total_imports: 0,
        skipped: 0,
        max_depth: 0,
        css_bytes: &mut summary.css_bytes,
    };

    for source in &collected.sources {
        match source {
            stylesheets::CssSource::Inline { text, nonce, .. } => {
                if let Some(policy) = walk.csp
                    && !policy.allows_inline_style(nonce.as_deref(), text)
                {
                    walk.warnings.push(
                        "skipping inline <style>: blocked by Content-Security-Policy".to_owned(),
                    );
                    continue;
                }
                summary.inline_stylesheets += 1;
                if !account_css_bytes(walk.css_bytes, text.len(), options) {
                    if !summary.sheets_capped {
                        summary.sheets_capped = true;
                        walk.warnings.push(format!(
                            "CSS byte budget reached ({}); further sheets are skipped",
                            options.max_total_css_bytes
                        ));
                    }
                    continue;
                }
                let sheet = megi_css::parse_stylesheet(text);
                font_faces.extend(
                    sheet
                        .font_faces
                        .iter()
                        .cloned()
                        .map(|face| (base_url.clone(), face)),
                );
                walk_imports(
                    &mut collection,
                    &sheet,
                    base_url,
                    0,
                    options.max_stylesheets,
                    false,
                    &mut walk,
                    &mut font_faces,
                )?;
                collection.add_stylesheet(&sheet, megi_css::Origin::Author);
            }
            stylesheets::CssSource::External {
                url,
                crossorigin,
                integrity,
                ..
            } => {
                if let Some(policy) = walk.csp
                    && !policy.allows_external_style(url, base_url)
                {
                    walk.warnings.push(format!(
                        "skipping stylesheet {url}: blocked by Content-Security-Policy"
                    ));
                    continue;
                }
                summary.external_stylesheets += 1;
                let total_sheets = summary.external_stylesheets + walk.loaded;
                if total_sheets > options.max_stylesheets {
                    // #654: degrade instead of failing the whole render.
                    if !summary.sheets_capped {
                        summary.sheets_capped = true;
                        walk.warnings.push(format!(
                            "stylesheet cap reached ({}); further sheets are skipped",
                            options.max_stylesheets
                        ));
                    }
                    continue;
                }
                let Some(text) = load_stylesheet_text(
                    url,
                    integrity.as_deref(),
                    &mut *walk.loader,
                    options,
                    &mut *walk.warnings,
                    *crossorigin,
                )?
                else {
                    continue;
                };
                if !account_css_bytes(walk.css_bytes, text.len(), options) {
                    if !summary.sheets_capped {
                        summary.sheets_capped = true;
                        walk.warnings.push(format!(
                            "CSS byte budget reached ({}); further sheets are skipped",
                            options.max_total_css_bytes
                        ));
                    }
                    continue;
                }
                // Register the top-level URL so an import cycle back to it is detected.
                walk.visited.insert(url.cache_key());
                let sheet = megi_css::parse_stylesheet(&text);
                font_faces.extend(
                    sheet
                        .font_faces
                        .iter()
                        .cloned()
                        .map(|face| (url.clone(), face)),
                );
                walk_imports(
                    &mut collection,
                    &sheet,
                    url,
                    0,
                    options.max_stylesheets,
                    *crossorigin,
                    &mut walk,
                    &mut font_faces,
                )?;
                collection.add_stylesheet(&sheet, megi_css::Origin::Author);
            }
        }
    }
    // css_bytes is shared with walk.
    summary.imports_loaded = walk.loaded;
    summary.imports_skipped = walk.skipped;
    summary.max_import_depth = walk.max_depth;
    Ok((collection, summary, font_faces))
}

/// Recursively loads and adds the `@import`ed sheets of `sheet` **before** its own rules.
///
/// Ordering per CSS: for each import in document order, first the imported sheet's own
/// imports, then the imported sheet's rules (via
/// [`megi_css::StyleCollection::add_import_stylesheet`], which also re-checks the media gate).
#[allow(clippy::too_many_arguments)]
fn walk_imports(
    collection: &mut megi_css::StyleCollection,
    sheet: &megi_css::Stylesheet,
    sheet_url: &Url,
    depth: usize,
    max_total_sheets: usize,
    use_cors: bool,
    walk: &mut ImportWalk<'_>,
    font_faces: &mut Vec<webfonts::LocatedFontFace>,
) -> Result<(), RenderError> {
    'imports: for import in &sheet.imports {
        // Bound total import entries (including skipped) to prevent unbounded
        // CPU on a sheet with thousands of trivial @imports (#524).
        const MAX_TOTAL_IMPORTS: usize = 10_000;
        if walk.total_imports >= MAX_TOTAL_IMPORTS {
            if !walk.cap_warned {
                walk.cap_warned = true;
                walk.warnings.push(format!(
                    "import cap reached ({MAX_TOTAL_IMPORTS}); further @imports are skipped"
                ));
            }
            break 'imports;
        }
        walk.total_imports += 1;
        let effective_depth = walk.options.max_import_depth.min(32);
        if depth >= effective_depth {
            walk.skipped += 1;
            walk.warnings.push(format!(
                "skipping @import \"{}\" at depth {depth}: max_import_depth={} exceeded",
                import.url, effective_depth
            ));
            continue;
        }
        let url = match sheet_url.join(&import.url) {
            Ok(url) => url,
            Err(error) => {
                walk.skipped += 1;
                walk.warnings.push(format!(
                    "skipping invalid @import \"{}\": {error}",
                    import.url
                ));
                continue;
            }
        };
        if let Some(policy) = walk.csp
            && !policy.allows_external_style(&url, walk.base_url)
        {
            walk.skipped += 1;
            walk.warnings.push(format!(
                "skipping @import {url}: blocked by Content-Security-Policy"
            ));
            continue;
        }
        if stylesheets::ensure_supported_scheme(&url).is_err() {
            walk.skipped += 1;
            walk.warnings
                .push(format!("skipping @import {url}: unsupported URL scheme"));
            continue;
        }
        if !megi_css::evaluate_media(&import.media, walk.media) {
            walk.skipped += 1;
            continue;
        }
        // Deduplicate by final URL (also breaks import cycles).
        if !walk.visited.insert(url.cache_key()) {
            walk.skipped += 1;
            continue;
        }
        // #654: the sheet cap skips further imports instead of failing the render.
        if walk.loaded + 1 > max_total_sheets {
            walk.skipped += 1;
            if !walk.cap_warned {
                walk.cap_warned = true;
                walk.warnings.push(format!(
                    "stylesheet cap reached ({max_total_sheets}); further imports are skipped"
                ));
            }
            continue;
        }
        // `loaded` only counts successes: failed imports would otherwise fetch without
        // limit (one request per unique URL in the stylesheet). Report the cap once.
        if walk.attempts >= max_total_sheets {
            walk.skipped += 1;
            if !walk.cap_warned {
                walk.cap_warned = true;
                walk.warnings.push(format!(
                    "skipping further @imports: more than {max_total_sheets} stylesheet loads attempted (cap)"
                ));
            }
            continue;
        }
        walk.attempts += 1;
        let Some(text) = load_stylesheet_text(
            &url,
            None,
            &mut *walk.loader,
            walk.options,
            &mut *walk.warnings,
            use_cors,
        )?
        else {
            continue;
        };
        if !account_css_bytes(walk.css_bytes, text.len(), walk.options) {
            walk.skipped += 1;
            if !walk.budget_warned {
                walk.budget_warned = true;
                walk.warnings.push(format!(
                    "CSS byte budget reached ({}); further imports are skipped",
                    walk.options.max_total_css_bytes
                ));
            }
            continue;
        }
        let imported = megi_css::parse_stylesheet(&text);
        walk.loaded += 1;
        walk.max_depth = walk.max_depth.max(depth + 1);
        font_faces.extend(
            imported
                .font_faces
                .iter()
                .cloned()
                .map(|face| (url.clone(), face)),
        );
        // Imported sheet's own imports first, then its rules at this cascade position.
        walk_imports(
            collection,
            &imported,
            &url,
            depth + 1,
            max_total_sheets,
            use_cors,
            walk,
            font_faces,
        )?;
        collection.add_import_stylesheet(import, &imported, megi_css::Origin::Author);
    }
    Ok(())
}
