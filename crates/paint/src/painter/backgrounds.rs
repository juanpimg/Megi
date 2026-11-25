use super::*;

/// Builds the four computed border sides of a box (CSS order: top, right,
/// bottom, left), zeroing the width of `none`/`hidden` sides (CSS 2.2 §8.5.3).
fn border_sides(
    style: &megi_css::ComputedStyle,
    border: megi_layout::EdgeSizes,
) -> [BorderSide; 4] {
    let widths = [border.top, border.right, border.bottom, border.left];
    let styles = [
        style.border_style.top,
        style.border_style.right,
        style.border_style.bottom,
        style.border_style.left,
    ];
    let colors = [
        style.border_color.top,
        style.border_color.right,
        style.border_color.bottom,
        style.border_color.left,
    ];

    let mut sides = [BorderSide::NONE; 4];
    for (index, side) in sides.iter_mut().enumerate() {
        let width = if matches!(styles[index], BorderStyle::None | BorderStyle::Hidden) {
            0.0
        } else if widths[index].is_finite() {
            widths[index].max(0.0)
        } else {
            0.0
        };
        *side = BorderSide::new(width, styles[index], colors[index]);
    }
    sides
}
/// True when a background layer asks to be clipped to the element's glyphs.
fn background_clips_to_text(style: &megi_css::ComputedStyle) -> bool {
    style
        .background_layers
        .iter()
        .any(|layer| layer.clip == megi_css::BackgroundClip::Text)
}
/// Emits the background and borders of an element box.
pub(super) fn paint_background_border(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    is_root: bool,
    alpha_scale: f32,
) {
    let is_element = layout_box.node_id().is_some();
    if !is_element {
        return;
    }
    let Some(style) = layout_box.style.as_ref() else {
        return;
    };

    let node_id = layout_box.node_id();
    let border_box = layout_box.dimensions.border_box();
    let padding_box = layout_box.dimensions.padding_box();
    let content_box = layout_box.dimensions.content;
    if alpha_scale > 0.0 {
        resources.record(border_box, node_id, HitKind::Background);
    }
    // `backdrop-filter` filters the already-painted backdrop inside the box
    // before this element paints its own background/border/content (R149).
    if !style.backdrop_filter.is_empty() {
        list.push(DisplayItem::BackdropFilter {
            rect: border_box,
            filters: style.backdrop_filter.clone(),
        });
    }

    if let Some(filtered) = inline_filtered_solid(layout_box) {
        if filtered.a > 0 && !(!layout_box.is_pseudo && is_root && skip.root_background) {
            list.push(DisplayItem::SolidColor {
                rect: border_box,
                color: scaled(filtered, alpha_scale),
            });
        }
        return;
    }

    let background_is_propagated = !layout_box.is_pseudo
        && ((is_root && skip.root_background)
            || layout_box.node_id().is_some() && layout_box.node_id() == skip.body_node);
    // Fast path: most boxes in real documents paint no background, border,
    // shadow or radius. Detect that with cheap flags before any feature
    // preparation (layers, gradients, masks, shadows, radii) so text-heavy
    // pages do not pay per-box feature setup (R129).
    let has_bg_color = style.background_color.a > 0;
    let has_legacy_image = style.background_image.is_some();
    let has_explicit_layers = style.background_geometry_explicit;
    let has_shadows = !style.box_shadows.is_empty();
    // Avoid building the four border sides when no width can paint.
    let border = layout_box.dimensions.border;
    let has_border_width =
        border.top > 0.0 || border.right > 0.0 || border.bottom > 0.0 || border.left > 0.0;
    let sides = if has_border_width {
        border_sides(style, border)
    } else {
        [BorderSide::NONE; 4]
    };
    let has_border = sides.iter().any(BorderSide::is_visible);
    let paints_background =
        !background_is_propagated && (has_bg_color || has_legacy_image || has_explicit_layers);
    if !paints_background && !has_shadows && !has_border && style.backdrop_filter.is_empty() {
        return;
    }

    let radii = style.border_radius;
    let has_radius = !radii.is_zero();
    let rounded_group = !background_is_propagated && has_radius;
    // CSS Masking 1 §6.1: the decoded `mask-image` is applied to the box's
    // background (colour + image layers) as an alpha mask. A URL that was not
    // fetched/decoded simply paints the unmasked background, as before.
    let mask_bitmap = if background_is_propagated {
        None
    } else {
        style
            .background_layers
            .first()
            .and_then(|layer| layer.mask_image.as_deref())
            .and_then(|url| resources.backgrounds.get(url))
            .filter(|image| !image.is_empty())
            .map(std::sync::Arc::clone)
    };
    // `background-clip: text`: the background color/images are composited through
    // the glyph mask of the box's text descendants, scoped strictly to the
    // layers whose `background-clip` is `text` (#446).
    let text_mask_runs = if !background_is_propagated
        && background_clips_to_text(style)
        && resources.claim_text_mask_collector()
    {
        // Bound the number of collecting boxes per pass (#658): once the cap is
        // reached, skip collection so a hostile tree cannot turn one render into
        // O(N * subtree). The budget belongs to this pass alone (#2296).
        Some(std::sync::Arc::new(collect_text_mask_runs(layout_box)))
    } else {
        None
    };

    // Outer `box-shadow` layers are painted behind the background, last shadow
    // first so the first authored shadow ends up on top (CSS Backgrounds 3 §6).
    // They are never clipped to `background-clip: text` (#446).
    if !background_is_propagated {
        for shadow in style
            .box_shadows
            .iter()
            .rev()
            .filter(|shadow| !shadow.inset)
            .take(MAX_SHADOWS)
        {
            list.push(DisplayItem::BoxShadow {
                rect: border_box,
                radii,
                shadow: megi_css::BoxShadow {
                    color: scaled(shadow.color, alpha_scale),
                    ..*shadow
                },
            });
        }
    }
    if rounded_group {
        push_rounded_group(list, resources, border_box, radii);
    }
    let mask_group_active = mask_bitmap.is_some();
    if let Some(mask) = mask_bitmap {
        push_mask_group(list, resources, border_box, mask);
    }
    let mut text_mask_active = false;
    let color_clips_to_text = style
        .background_layers
        .last()
        .is_some_and(|layer| layer.clip == megi_css::BackgroundClip::Text);
    if !background_is_propagated && style.background_color.a > 0 {
        // CSS Backgrounds 3 §2.5: the colour is clipped like the bottom-most
        // layer. Synthetic styles that only fill the legacy scalar fields keep
        // the border box so existing golden fixtures stay byte-identical.
        // `background-clip: text` still resolves to the border box and relies
        // on the text mask group below, so it is intentionally not applied here.
        let color_rect = if style.background_geometry_explicit {
            let clip = style
                .background_layers
                .last()
                .map_or(megi_css::BackgroundClip::BorderBox, |layer| layer.clip);
            crate::background::background_clip_rect(clip, border_box, padding_box, content_box)
        } else {
            border_box
        };
        if color_clips_to_text
            && let Some(runs) = text_mask_runs.as_ref()
            && !text_mask_active
        {
            push_text_mask_group(list, resources, border_box, std::sync::Arc::clone(runs));
            text_mask_active = true;
        }
        if has_radius {
            list.push(DisplayItem::RoundedRect {
                rect: color_rect,
                radii,
                color: scaled(style.background_color, alpha_scale),
            });
        } else {
            list.push(DisplayItem::SolidColor {
                rect: color_rect,
                color: scaled(style.background_color, alpha_scale),
            });
        }
    }

    // `background-image`, on top of the color; missing URL => nothing painted.
    if !background_is_propagated {
        if style.background_geometry_explicit {
            // Full CSS Backgrounds 3 geometry for every layer, painted
            // bottom-to-top (CSS lists the top layer first).
            let fallback_layer;
            let layers: &[megi_css::BackgroundLayer] = if style.background_layers.is_empty() {
                // Synthetic styles/tests may only populate the legacy scalar
                // fields; treat them as a single layer with CSS initial geometry.
                fallback_layer = vec![megi_css::BackgroundLayer {
                    image: style.background_image.clone(),
                    repeat: style.background_repeat,
                    size: style.background_size,
                    position: style.background_position,
                    ..megi_css::BackgroundLayer::default()
                }];
                &fallback_layer
            } else {
                &style.background_layers
            };
            for layer in layers.iter().rev() {
                if layer.image.is_none() && layer.gradient.is_none() {
                    continue;
                }
                let layer_clips_to_text = layer.clip == megi_css::BackgroundClip::Text;
                if layer_clips_to_text && !text_mask_active {
                    if let Some(runs) = text_mask_runs.as_ref() {
                        push_text_mask_group(
                            list,
                            resources,
                            border_box,
                            std::sync::Arc::clone(runs),
                        );
                        text_mask_active = true;
                    }
                } else if !layer_clips_to_text && text_mask_active {
                    pop_text_mask_group(list, resources);
                    text_mask_active = false;
                }
                let positioning_area =
                    if crate::background::uses_viewport_positioning(layer.attachment) {
                        resources.viewport
                    } else {
                        crate::background::background_origin_rect(
                            layer.origin,
                            border_box,
                            padding_box,
                            content_box,
                        )
                    };
                let clip_rect = crate::background::background_clip_rect(
                    layer.clip,
                    border_box,
                    padding_box,
                    content_box,
                );
                if let Some(url) = layer.image.as_deref() {
                    let Some(image) = resources.backgrounds.get(url) else {
                        continue;
                    };
                    if image.is_empty() {
                        continue;
                    }
                    let tiles = crate::background::background_tiles_with_origin(
                        positioning_area,
                        clip_rect,
                        image.dimensions(),
                        layer.repeat,
                        layer.size,
                        layer.position,
                    );
                    if tiles.is_empty() {
                        continue;
                    }
                    push_clip_item(list, resources, clip_rect);
                    for tile in tiles {
                        list.push(DisplayItem::Image {
                            rect: tile,
                            image: std::sync::Arc::clone(image),
                        });
                    }
                    pop_clip_item(list, resources);
                } else if let Some(gradient) = layer.gradient.as_ref() {
                    let tiles = crate::background::background_gradient_tiles(
                        positioning_area,
                        clip_rect,
                        layer.repeat,
                        layer.size,
                        layer.position,
                    );
                    if tiles.is_empty() {
                        continue;
                    }
                    push_clip_item(list, resources, clip_rect);
                    for tile in tiles {
                        list.push(DisplayItem::Gradient {
                            rect: tile,
                            gradient: std::sync::Arc::clone(gradient),
                        });
                    }
                    pop_clip_item(list, resources);
                }
            }
        } else if let Some(url) = style.background_image.as_deref()
            && let Some(image) = resources.backgrounds.get(url)
            && !image.is_empty()
        {
            if text_mask_active {
                pop_text_mask_group(list, resources);
                text_mask_active = false;
            }
            // Legacy compatibility: documents that only set `background-image`
            // (all Phase 4/5/6 golden fixtures) keep the historical behaviour of
            // stretching the bitmap over the whole padding box. Authored
            // background properties opt into CSS geometry above.
            list.push(DisplayItem::Image {
                rect: padding_box,
                image: std::sync::Arc::clone(image),
            });
        }
    }

    if text_mask_active {
        pop_text_mask_group(list, resources);
    }
    if mask_group_active {
        pop_mask_group(list, resources);
    }
    if rounded_group {
        pop_rounded_group(list, resources);
    }

    // Inset shadows are painted above the background but below the border and content.
    if !background_is_propagated {
        for shadow in style
            .box_shadows
            .iter()
            .rev()
            .filter(|shadow| shadow.inset)
            .take(MAX_SHADOWS)
        {
            list.push(DisplayItem::BoxShadow {
                rect: border_box,
                radii,
                shadow: megi_css::BoxShadow {
                    color: scaled(shadow.color, alpha_scale),
                    ..*shadow
                },
            });
        }
    }

    if has_border {
        resources.record(border_box, node_id, HitKind::Border);
        let sides = sides.map(|side| BorderSide {
            color: scaled(side.color, alpha_scale),
            ..side
        });
        let solid = sides
            .iter()
            .filter(|side| side.is_visible())
            .all(|side| matches!(side.style, BorderStyle::Solid));
        if has_radius && solid {
            list.push(DisplayItem::RoundedBorder {
                rect: border_box,
                sides,
                radii,
            });
        } else {
            list.push(DisplayItem::Border {
                rect: border_box,
                sides,
            });
        }
    }
}
