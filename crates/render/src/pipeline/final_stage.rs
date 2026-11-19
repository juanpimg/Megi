//! Final pipeline stage: layout, paint, raster, stats assembly and stage orchestration.
//!
//! The stage bodies are moved verbatim from `lib.rs`; only signatures and state (un)packing
//! were added to keep the file within size caps.

#[cfg(feature = "js")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "js")]
use std::time::Duration;
use std::time::Instant;

#[cfg(feature = "js")]
use megi_core::Document;
#[cfg(feature = "js")]
use megi_net::{ResourceLoader, Scheme, Url};

use super::state::{ParsedDocumentInput, ResourceStageOutput, ScriptStats};
#[cfg(feature = "js")]
use super::url::FrameRenderContext;
#[cfg(feature = "js")]
use crate::RenderOptions;
#[cfg(feature = "js")]
use crate::frames;
use crate::{RenderError, RenderOutput, RenderStats, RenderTimings};

#[cfg(feature = "js")]
pub(crate) fn render_pre_scripted_document(
    document: Document,
    document_url: Url,
    options: &RenderOptions,
    frame_depth: usize,
    frame_ctx: FrameRenderContext,
    budget: &mut frames::FrameBudget,
) -> Result<RenderOutput, RenderError> {
    let total_start = Instant::now();
    #[cfg(feature = "js")]
    let opaque_origin = frame_ctx
        .sandbox_flags
        .is_some_and(|flags| !flags.allow_same_origin);
    #[cfg(not(feature = "js"))]
    let opaque_origin = false;

    let cookie_jar = if opaque_origin {
        None
    } else {
        options.cookie_jar.clone().or_else(|| {
            options
                .cookies
                .then(|| Arc::new(Mutex::new(megi_net::CookieJar::new())))
        })
    };
    let subresource_origin = if opaque_origin {
        None
    } else {
        options
            .origin
            .clone()
            .or_else(|| match document_url.scheme() {
                Scheme::Http | Scheme::Https => {
                    megi_net::Origin::from_url(&document_url).map(|o| o.as_str())
                }
                Scheme::File | Scheme::Data => None,
            })
    };
    let mut subresource_fetch_options = options.fetch_options.clone();
    subresource_fetch_options.cookie_jar = cookie_jar.clone();
    subresource_fetch_options.origin = subresource_origin;
    let mut loader = ResourceLoader::with_options(subresource_fetch_options);
    loader.set_allow_file(document_url.is_file());
    loader.set_mixed_content_context(Some(document_url.scheme()));
    let referrer_policy = megi_net::ReferrerPolicy::default();
    loader.set_referrer(Some(megi_net::ReferrerConfig::with_policy(
        document_url.clone(),
        referrer_policy,
    )));

    // #1632/frame budget: reuse the shared budget threaded through the recursion.
    render_parsed_document_inner(ParsedDocumentInput {
        budget,
        document,
        document_url,
        document_headers: &[],
        options,
        loader,
        cookie_jar,
        referrer_policy,
        source_mentions_svg: true,
        document_bytes: 0,
        charset: "utf-8".to_owned(),
        status: None,
        content_encoding: None,
        limit_violations: megi_html::LimitViolations::default(),
        cache: None,
        frame_depth,
        frame_ctx,
        fetch_time: Duration::ZERO,
        decode_parse_time: Duration::ZERO,
        total_start,
    })
}

/// Runs the four pipeline stages in order over one parsed document.
pub(crate) fn render_parsed_document_inner(
    input: ParsedDocumentInput<'_>,
) -> Result<RenderOutput, RenderError> {
    let (input, script_ctx, script_stats) = super::script::run_script_stage(input)?;
    let (input, resources) = super::resources::run_resource_stage(input, script_ctx)?;
    finish_render(input, script_stats, resources)
}

/// Lays out, paints, rasterizes and assembles the final `RenderOutput`.
fn finish_render(
    input: ParsedDocumentInput<'_>,
    script: ScriptStats,
    resources: ResourceStageOutput,
) -> Result<RenderOutput, RenderError> {
    let ParsedDocumentInput {
        document,
        document_url,
        document_headers: _,
        options,
        loader,
        cookie_jar,
        referrer_policy,
        source_mentions_svg: _,
        document_bytes,
        charset,
        status,
        content_encoding,
        limit_violations,
        cache,
        frame_depth: _,
        frame_ctx: _,
        budget: _,
        fetch_time,
        decode_parse_time,
        total_start,
    } = input;
    let ScriptStats {
        styles_pre_script,
        script_bytes,
        scripting_time,
        scripts_executed,
        scripts_failed,
        console,
        script_tasks,
        script_loop_iterations,
        script_virtual_time_ms,
        script_events,
        script_timers_pending,
        event_loop_time,
        location_actions,
        history_actions,
        script_limited,
    } = script;
    let ResourceStageOutput {
        warnings,
        inline_svg_report,
        style_summary,
        styles,
        pseudo_styles,
        styles_time,
        fonts_time,
        images_time,
        backgrounds_time,
        loaded_webfonts,
        loaded_images,
        loaded_backgrounds,
    } = resources;

    let layout_start = Instant::now();
    // Mix font identities (not just the count) so a caller-shared cache cannot reuse
    // metrics computed with a different font file (#1218).
    let font_digest = loaded_webfonts.database.identity_digest();
    let measurer = megi_layout::FontTextMeasurer::from_database(loaded_webfonts.database);
    let viewport = megi_layout::Rect::new(
        0.0,
        0.0,
        options.viewport.width as f32,
        options.viewport.height as f32,
    );
    let (layout_root, cache_layout_hit) = if let Some(cache) = cache {
        cache
            .get_or_compute_layout_with_pseudos(
                &document,
                &styles,
                &pseudo_styles,
                viewport,
                &measurer,
                &loaded_images.sizes,
                font_digest,
            )
            .ok_or(RenderError::LayoutFailed)?
    } else {
        let root = megi_layout::layout_document_with_measurer_and_pseudos(
            &document,
            &styles,
            &pseudo_styles,
            viewport,
            &measurer,
            Some(&loaded_images.sizes),
        )
        .ok_or(RenderError::LayoutFailed)?;
        (root, false)
    };
    let layout_time = layout_start.elapsed();

    let paint_start = Instant::now();
    // Additive hit-testing: the painter records `HitRegion`s while emitting the same
    // display-list items, so the rasterized pixels/PNG are byte-identical.
    let (display_list, hit_regions) = megi_paint::build_display_list_with_hit_regions(
        &layout_root,
        viewport,
        &loaded_images.store,
        &loaded_backgrounds.images,
    );
    let display_items = display_list.items.len();
    let paint_time = paint_start.elapsed();

    let raster_start = Instant::now();
    let pixmap = megi_raster::rasterize_display_list_with_font_database(
        &display_list,
        options.viewport.width,
        options.viewport.height,
        megi_paint::RenderConfig::default(),
        measurer.database(),
    );
    let raster_time = raster_start.elapsed();
    // Encode the deterministic PNG eagerly unless the caller asked to skip it because it
    // will paint overlays on the pixmap and encode once at the end (`--chrome`/`--find`).
    let png = if options.encode_png {
        let png = pixmap.to_png();
        if png.is_empty() {
            return Err(RenderError::LayoutFailed);
        }
        png
    } else {
        Vec::new()
    };

    let hit_region_count = hit_regions.len();
    let cookies_stored = cookie_jar
        .as_ref()
        .map(|jar| {
            jar.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .len()
        })
        .unwrap_or(0);
    let referrer_policy_name = Some(referrer_policy.as_str().to_owned());
    let console_errors = console
        .iter()
        .filter(|entry| entry.level == "error")
        .count();
    let total = total_start.elapsed();
    let location_action_count = location_actions.len();
    let history_action_count = history_actions.len();
    Ok(RenderOutput {
        png,
        pixmap,
        document,
        styles,
        layout: layout_root,
        display_list,
        hit_regions,
        document_url,
        location_actions,
        history_actions,
        stats: RenderStats {
            document_bytes,
            charset,
            status,
            external_stylesheets: style_summary.external_stylesheets,
            inline_stylesheets: style_summary.inline_stylesheets,
            css_bytes: style_summary.css_bytes,
            imports_loaded: style_summary.imports_loaded,
            imports_skipped: style_summary.imports_skipped,
            max_import_depth: style_summary.max_import_depth,
            media_skipped: style_summary.media_skipped,
            images_loaded: loaded_images.loaded,
            images_failed: loaded_images.failed,
            image_bytes: loaded_images.downloaded_bytes,
            background_images_loaded: loaded_backgrounds.loaded,
            background_images_failed: loaded_backgrounds.failed,
            background_bytes: loaded_backgrounds.downloaded_bytes,
            svgs_rendered: loaded_images.svgs_rendered + loaded_backgrounds.svgs_rendered,
            inline_svgs: inline_svg_report.replaced,
            fonts_loaded: loaded_webfonts.loaded,
            fonts_failed: loaded_webfonts.failed,
            font_bytes: loaded_webfonts.bytes,
            scripts_executed,
            scripts_failed,
            script_bytes,
            console,
            console_errors,
            script_tasks,
            script_loop_iterations,
            script_virtual_time_ms,
            script_events,
            script_timers_pending,
            location_actions: location_action_count,
            script_limited,
            history_actions: history_action_count,
            pushed_used: loader.pushed_used(),
            pushed_discarded: loader.pushed_discarded(),
            display_items,
            hit_regions: hit_region_count,
            cookies_stored,
            content_encoding,
            referrer_policy: referrer_policy_name,
            limit_violations,
            cache_layout_hit,
            timings: RenderTimings {
                fetch: fetch_time,
                decode_parse: decode_parse_time,
                styles: styles_time,
                styles_pre_script,
                fonts: fonts_time,
                images: images_time,
                backgrounds: backgrounds_time,
                scripting: scripting_time
                    .checked_sub(event_loop_time)
                    .unwrap_or_default(),
                event_loop: event_loop_time,
                layout: layout_time,
                paint: paint_time,
                raster: raster_time,
                total,
            },
            warnings,
        },
    })
}
