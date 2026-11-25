use super::*;

/// Computes the drawn rect of a replaced element image for `object-fit` /
/// `object-position` against its content box (CSS Images 3 §5.5).
///
/// `fill` returns the content box unchanged, which keeps the historical
/// stretched-image output byte-identical (goldens).
fn object_fit_rect(
    content: Rect,
    image_width: f32,
    image_height: f32,
    fit: megi_css::ObjectFit,
    position: megi_css::BackgroundPosition,
) -> Rect {
    if image_width <= 0.0 || image_height <= 0.0 {
        return content;
    }
    let scale = match fit {
        megi_css::ObjectFit::Fill => {
            return content;
        }
        megi_css::ObjectFit::None => 1.0,
        megi_css::ObjectFit::Contain => {
            (content.width / image_width).min(content.height / image_height)
        }
        megi_css::ObjectFit::Cover => {
            (content.width / image_width).max(content.height / image_height)
        }
        megi_css::ObjectFit::ScaleDown => {
            let contain = (content.width / image_width).min(content.height / image_height);
            contain.min(1.0)
        }
    };
    let drawn_width = image_width * scale;
    let drawn_height = image_height * scale;
    let resolve = |value: megi_css::Length, area: f32, drawn: f32| -> f32 {
        match value {
            megi_css::Length::Px(px) => px,
            megi_css::Length::Percent(pct) => (area - drawn) * (pct / 100.0),
            megi_css::Length::Auto => 0.0,
        }
    };
    Rect::new(
        content.x + resolve(position.x, content.width, drawn_width),
        content.y + resolve(position.y, content.height, drawn_height),
        drawn_width,
        drawn_height,
    )
}
/// Emits the paint of a replaced element: its decoded image if available,
/// otherwise a documented gray placeholder box (CSS 2.2 §10.6.2 intrinsic size
/// is handled by layout; paint only stretches the bitmap to `content`).
pub(super) fn paint_replaced_element(
    layout_box: &LayoutBox,
    node_id: megi_core::NodeId,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    alpha_scale: f32,
) {
    let rect = layout_box.dimensions.content;
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    resources.record(rect, Some(node_id), HitKind::Image);
    let fit = layout_box
        .style
        .as_ref()
        .map_or(megi_css::ObjectFit::Fill, |style| style.object_fit);
    let position = layout_box.style.as_ref().map_or(
        megi_css::BackgroundPosition {
            x: megi_css::Length::Percent(50.0),
            y: megi_css::Length::Percent(50.0),
        },
        |style| style.object_position,
    );
    let image_opt = if layout_box.is_pseudo {
        layout_box
            .style
            .as_ref()
            .and_then(|s| s.content.image_url())
            .and_then(|url| resources.backgrounds.get(url))
    } else {
        resources.images.get(&node_id)
    };
    match image_opt {
        Some(image) if !image.is_empty() => {
            let start = list.len();
            let drawn =
                object_fit_rect(rect, image.width as f32, image.height as f32, fit, position);
            // `object-fit: fill` is the historical stretched image: its drawn
            // rect is the content box, so emit it without a clip and keep the
            // existing goldens byte-identical. Every other fit can exceed the
            // content box: intrinsic size (`none`), scaled size (`cover`,
            // `contain`, `scale-down`) or an out-of-range `object-position`.
            // CSS Images 3 §4.2 requires clipping to the content box (R166,
            // #2296).
            if fit == megi_css::ObjectFit::Fill {
                list.push(DisplayItem::Image {
                    rect,
                    image: std::sync::Arc::clone(image),
                });
            } else {
                push_clip_item(list, resources, rect);
                list.push(DisplayItem::Image {
                    rect: drawn,
                    image: std::sync::Arc::clone(image),
                });
                pop_clip_item(list, resources);
            }
            if alpha_scale < 1.0 && list.len() > start {
                let items: Vec<DisplayItem> = list.items.drain(start..).collect();
                let group_rect = items_bounds(&items).unwrap_or(rect);
                list.push(DisplayItem::OpacityGroup {
                    rect: group_rect,
                    opacity: alpha_scale,
                    items,
                });
            }
        }
        _ => {
            list.push(DisplayItem::SolidColor {
                rect,
                color: scaled(IMAGE_PLACEHOLDER, alpha_scale),
            });
        }
    }
}
