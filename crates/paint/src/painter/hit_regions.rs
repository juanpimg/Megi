use super::*;

/// Emits a `PushClip` display item and mirrors it into the hit-region clip stack.
pub(super) fn push_clip_item(list: &mut DisplayList, resources: &PaintResources<'_>, rect: Rect) {
    list.push(DisplayItem::PushClip { rect });
    resources.push_clip(rect);
}
/// Emits a `PopClip` display item and mirrors it into the hit-region clip stack.
pub(super) fn pop_clip_item(list: &mut DisplayList, resources: &PaintResources<'_>) {
    list.push(DisplayItem::PopClip);
    resources.pop_clip();
}
/// Emits a `PushRoundedGroup` display item and mirrors its bounding rect into
/// the hit-region clip stack.
pub(super) fn push_rounded_group(
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    rect: Rect,
    radii: megi_css::BorderRadii,
) {
    list.push(DisplayItem::PushRoundedGroup { rect, radii });
    resources.push_clip(rect);
}
/// Emits a `PopRoundedGroup` display item and pops its mirrored clip.
pub(super) fn pop_rounded_group(list: &mut DisplayList, resources: &PaintResources<'_>) {
    list.push(DisplayItem::PopRoundedGroup);
    resources.pop_clip();
}
/// Emits a `PushTextMaskGroup` display item and mirrors its bounding rect into
/// the hit-region clip stack.
pub(super) fn push_text_mask_group(
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    rect: Rect,
    runs: std::sync::Arc<Vec<TextMaskRun>>,
) {
    list.push(DisplayItem::PushTextMaskGroup { rect, runs });
    resources.push_clip(rect);
}
/// Emits a `PopTextMaskGroup` display item and pops its mirrored clip.
pub(super) fn pop_text_mask_group(list: &mut DisplayList, resources: &PaintResources<'_>) {
    list.push(DisplayItem::PopTextMaskGroup);
    resources.pop_clip();
}
/// Emits a `PushMaskGroup` display item (CSS Masking 1 §6.1) and mirrors its
/// bounding rect into the hit-region clip stack.
pub(super) fn push_mask_group(
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    rect: Rect,
    mask: std::sync::Arc<megi_image::RgbaImage>,
) {
    list.push(DisplayItem::PushMaskGroup { rect, mask });
    resources.push_clip(rect);
}
/// Emits a `PopMaskGroup` display item and pops its mirrored clip.
pub(super) fn pop_mask_group(list: &mut DisplayList, resources: &PaintResources<'_>) {
    list.push(DisplayItem::PopMaskGroup);
    resources.pop_clip();
}
