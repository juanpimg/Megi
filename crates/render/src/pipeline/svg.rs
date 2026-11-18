//! Direct navigation to an SVG document through a synthetic one-image HTML page.
//!
//! Moved verbatim from `lib.rs`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use megi_core::NodeId;
use megi_net::ResourceLoader;

use super::styles::{collect_styles, resolve_styles_with_state};
use super::url::LoadedDocument;
use crate::svg_assets;
use crate::{RenderError, RenderOptions, RenderOutput, RenderStats, RenderTimings};

/// Renders a directly-navigated SVG document through a synthetic one-image HTML page.
///
/// The SVG bytes are embedded as a `data:` URL (no second network fetch) and rendered by the
/// normal pipeline, so the result is the SVG at its intrinsic size in the top-left corner of
/// a white viewport (documented simplification: no viewport-filling root-SVG sizing,
/// centering or scrollbars yet). Corrupt SVG payloads become the regular image placeholder
/// plus a warning instead of a hard error.
pub(crate) fn render_svg_document(
    loaded: &LoadedDocument,
    options: &RenderOptions,
    fetch_time: Duration,
    total_start: Instant,
) -> Result<RenderOutput, RenderError> {
    let parse = megi_html::parse_document_with_options(
        "<!doctype html><html><head><style>html,body{margin:0;padding:0;background-color:#ffffff}img{display:block}</style></head><body><img alt=\"\"></body></html>",
        false,
        &options.parse_options,
    );
    let document = parse.document;
    let img_node = document.get_elements_by_tag_name("img").into_iter().next();

    let media_context = megi_css::MediaContext::with_viewport(
        options.viewport.width as f32,
        options.viewport.height as f32,
    );
    let mut warnings = Vec::new();
    let mut loader = ResourceLoader::with_options(options.fetch_options.clone());
    let (stylesheet_collection, style_summary, _font_faces) = collect_styles(
        &document,
        &loaded.url,
        &mut loader,
        options,
        &media_context,
        None,
        &mut warnings,
    )?;
    let (styles, pseudo_styles) = resolve_styles_with_state(
        &document,
        &stylesheet_collection,
        megi_css::StateContext::new(),
        options.time_ms,
        (
            options.viewport.width as f32,
            options.viewport.height as f32,
        ),
    );

    let mut image_store = megi_paint::ImageStore::new();
    let mut image_sizes: HashMap<NodeId, (f32, f32)> = HashMap::new();
    let (mut images_loaded, mut images_failed, mut svgs_rendered) = (0usize, 0usize, 0usize);
    if let Some(node_id) = img_node {
        match svg_assets::render(&loaded.bytes) {
            Ok(rendered) => {
                let (width, height) = rendered.raster.dimensions();
                let pixels = u64::from(width).saturating_mul(u64::from(height));
                if pixels > options.max_total_decoded_pixels {
                    images_failed = 1;
                    warnings.push(format!(
                        "skipping image {}: decoded pixel budget exhausted ({pixels} > {} pixels)",
                        loaded.url, options.max_total_decoded_pixels
                    ));
                } else {
                    image_sizes.insert(node_id, rendered.intrinsic);
                    image_store.insert(node_id, Arc::new(rendered.raster));
                    images_loaded = 1;
                    svgs_rendered = 1;
                }
            }
            Err(error) => {
                images_failed = 1;
                warnings.push(format!(
                    "skipping image {}: SVG render failed ({error})",
                    loaded.url
                ));
            }
        }
    }

    // #1665: use only the bundled (deterministic) fonts, matching the normal render path in
    // `webfonts.rs`. System fonts vary per host and made the same SVG produce different PNGs.
    let font_db = megi_fonts::FontDatabase::new_with_bundled_fonts();
    let measurer = megi_layout::FontTextMeasurer::from_database(font_db);
    let viewport = megi_layout::Rect::new(
        0.0,
        0.0,
        options.viewport.width as f32,
        options.viewport.height as f32,
    );
    let layout_root = megi_layout::layout_document_with_measurer_and_pseudos(
        &document,
        &styles,
        &pseudo_styles,
        viewport,
        &measurer,
        Some(&image_sizes),
    )
    .ok_or(RenderError::LayoutFailed)?;

    let empty_backgrounds = megi_paint::BackgroundImages::new();
    let (display_list, hit_regions) = megi_paint::build_display_list_with_hit_regions(
        &layout_root,
        viewport,
        &image_store,
        &empty_backgrounds,
    );
    let display_items = display_list.items.len();
    let hit_region_count = hit_regions.len();

    let pixmap = megi_raster::rasterize_display_list_with_font_database(
        &display_list,
        options.viewport.width,
        options.viewport.height,
        megi_paint::RenderConfig::default(),
        measurer.database(),
    );
    let png = if options.encode_png {
        let png = pixmap.to_png();
        if png.is_empty() {
            return Err(RenderError::LayoutFailed);
        }
        png
    } else {
        Vec::new()
    };

    Ok(RenderOutput {
        png,
        pixmap,
        document,
        styles,
        layout: layout_root,
        display_list,
        hit_regions,
        document_url: loaded.url.clone(),
        location_actions: Vec::new(),
        history_actions: Vec::new(),
        stats: RenderStats {
            document_bytes: loaded.bytes.len(),
            charset: "utf-8".to_owned(),
            status: loaded.status,
            external_stylesheets: style_summary.external_stylesheets,
            inline_stylesheets: style_summary.inline_stylesheets,
            css_bytes: style_summary.css_bytes,
            imports_loaded: style_summary.imports_loaded,
            imports_skipped: style_summary.imports_skipped,
            max_import_depth: style_summary.max_import_depth,
            media_skipped: style_summary.media_skipped,
            images_loaded,
            images_failed,
            image_bytes: 0,
            background_images_loaded: 0,
            background_images_failed: 0,
            background_bytes: 0,
            svgs_rendered,
            inline_svgs: 0,
            fonts_loaded: 0,
            fonts_failed: 0,
            font_bytes: 0,
            scripts_executed: 0,
            scripts_failed: 0,
            script_bytes: 0,
            console: Vec::new(),
            console_errors: 0,
            script_tasks: 0,
            script_loop_iterations: 0,
            script_virtual_time_ms: 0,
            script_events: 0,
            script_timers_pending: 0,
            location_actions: 0,
            script_limited: false,
            history_actions: 0,
            pushed_used: 0,
            pushed_discarded: 0,
            display_items,
            hit_regions: hit_region_count,
            cookies_stored: 0,
            content_encoding: loaded.content_encoding.clone(),
            referrer_policy: Some(
                megi_net::ReferrerPolicy::from_headers(&loaded.headers)
                    .unwrap_or_default()
                    .as_str()
                    .to_owned(),
            ),
            limit_violations: parse.limit_violations,
            cache_layout_hit: false,
            timings: RenderTimings {
                fetch: fetch_time,
                total: total_start.elapsed(),
                ..RenderTimings::default()
            },
            warnings,
        },
    })
}
