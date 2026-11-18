//! Resource stage: styles, web fonts, images, frames and CSS backgrounds.
//!
//! The stage body is moved verbatim from `lib.rs`; only the surrounding function
//! signature and the state (un)packing were added to keep the file within size caps.

use std::time::Instant;

use super::imports::stylesheet_signature;
use super::state::{ParsedDocumentInput, ResourceStageOutput, ScriptContext};
use super::styles::{collect_styles, resolve_styles_with_state};
use crate::RenderError;
use crate::images::ImageLimits;
use crate::{backgrounds, frames, images, inline_svg, webfonts};

#[allow(unused_variables)] // input fields are threaded unchanged to the next stage
pub(crate) fn run_resource_stage<'a>(
    input: ParsedDocumentInput<'a>,
    script: ScriptContext,
) -> Result<(ParsedDocumentInput<'a>, ResourceStageOutput), RenderError> {
    let ParsedDocumentInput {
        mut document,
        document_url,
        document_headers,
        options,
        mut loader,
        cookie_jar,
        referrer_policy,
        source_mentions_svg,
        document_bytes,
        charset,
        status,
        content_encoding,
        limit_violations,
        cache,
        frame_depth,
        frame_ctx,
        budget,
        fetch_time,
        decode_parse_time,
        total_start,
    } = input;
    let ScriptContext {
        mut warnings,
        base_url,
        csp,
        media_context,
        reusable_stylesheets,
        mut pre_warnings,
        pre_scripted_frames,
        scripts_executed,
        #[cfg(feature = "js")]
        canvas_surfaces,
    } = script;

    // Inline `<svg>` elements become synthetic `<img>` data URLs after scripts (so DOM
    // mutations are honored) and before style resolution/layout; the existing SVG image
    // pipeline then supplies intrinsic size, caps and warnings.
    // Render-glue fast path: with no executed scripts the DOM is exactly what the parser
    // produced, so an inline <svg> can only exist if the source markup contains "<svg".
    // Skipping the full 30k-node scan is output-identical (bench document: no scripts/svg).
    let inline_svg_report = if scripts_executed == 0 && !source_mentions_svg {
        inline_svg::InlineSvgReport::default()
    } else {
        // Only replace as many inline SVGs as the image loader will actually fetch, so a
        // document cannot pay serialization cost for placeholders (#670).
        inline_svg::replace_inline_svgs(&mut document, options.max_images)
    };
    warnings.extend(inline_svg_report.warnings.iter().cloned());

    let styles_start = Instant::now();
    let (stylesheet_collection, style_summary, font_faces) = match reusable_stylesheets {
        Some((collection, summary, fonts, stylesheet_signature_before))
            if stylesheet_signature(&document) == stylesheet_signature_before =>
        {
            warnings.extend(pre_warnings);
            (collection, summary, fonts)
        }
        _ => {
            warnings.append(&mut pre_warnings);
            collect_styles(
                &document,
                &base_url,
                &mut loader,
                options,
                &media_context.clone(),
                csp.as_ref(),
                &mut warnings,
            )?
        }
    };
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
    let styles_time = styles_start.elapsed();

    let fonts_start = Instant::now();
    let loaded_webfonts = webfonts::load_webfonts_with_csp(
        &font_faces,
        &mut loader,
        options.no_network,
        webfonts::WebFontLimits {
            max_web_fonts: options.max_web_fonts,
            max_web_font_bytes: options.max_web_font_bytes,
            max_total_web_font_bytes: options.max_total_web_font_bytes,
        },
        csp.as_ref().map(|policy| (policy, &base_url)),
        &mut warnings,
    );
    let fonts_time = fonts_start.elapsed();

    let images_start = Instant::now();
    // The canvas blit loop (js feature) mutates this; harmless in a JS-free build.
    #[cfg_attr(not(feature = "js"), allow(unused_mut))]
    let mut loaded_images = images::load_images_with_context(
        &document,
        &base_url,
        &mut loader,
        options.no_network,
        &media_context,
        ImageLimits {
            max_images: options.max_images,
            max_image_bytes: options.max_image_bytes,
            max_total_bytes: options.max_total_image_bytes,
            max_total_decoded_pixels: options.max_total_decoded_pixels,
            max_image_dim: options.max_image_dim,
            device_pixel_ratio: options.device_pixel_ratio,
        },
        csp.as_ref(),
        &mut warnings,
    );
    let images_time = images_start.elapsed();

    // Blit script-painted canvas surfaces into the replaced-element image store (#1545). They
    // are keyed by `<canvas>` NodeId, so the painter's existing replaced-element path draws them.
    #[cfg(feature = "js")]
    {
        for (node, surface) in canvas_surfaces {
            if let Ok(rgba) =
                megi_image::RgbaImage::new(surface.width, surface.height, surface.pixels)
            {
                let size = (surface.width as f32, surface.height as f32);
                loaded_images.store.insert(node, std::sync::Arc::new(rgba));
                loaded_images.sizes.insert(node, size);
            }
        }
    }

    // Nested browsing contexts (#1544): render each <iframe>'s document into its own viewport
    // and blit it as a replaced element. Depth/count caps keep self-referential trees bounded.
    for frame in frames::collect_frames_with_context(
        &document,
        &base_url,
        options,
        &styles,
        csp.as_ref(),
        &pre_scripted_frames,
        frame_depth,
        &mut warnings,
        budget,
    )? {
        loaded_images
            .store
            .insert(frame.node, std::sync::Arc::new(frame.image));
        loaded_images.sizes.insert(frame.node, frame.size);
    }

    let backgrounds_start = Instant::now();
    let remaining_decoded_pixels = options
        .max_total_decoded_pixels
        .saturating_sub(loaded_images.decoded_pixels);
    let loaded_backgrounds = backgrounds::load_backgrounds_with_pseudos(
        &document,
        &styles,
        &pseudo_styles,
        &base_url,
        &mut loader,
        options.no_network,
        backgrounds::BackgroundLimits {
            max_backgrounds: options.max_backgrounds,
            max_background_bytes: options.max_background_bytes,
            max_total_bytes: options.max_total_background_bytes,
            max_total_decoded_pixels: remaining_decoded_pixels,
            max_image_dim: options.max_image_dim,
        },
        &mut warnings,
    );
    let backgrounds_time = backgrounds_start.elapsed();

    Ok((
        ParsedDocumentInput {
            document,
            document_url,
            document_headers,
            options,
            loader,
            cookie_jar,
            referrer_policy,
            source_mentions_svg,
            document_bytes,
            charset,
            status,
            content_encoding,
            limit_violations,
            cache,
            frame_depth,
            frame_ctx,
            budget,
            fetch_time,
            decode_parse_time,
            total_start,
        },
        ResourceStageOutput {
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
        },
    ))
}
