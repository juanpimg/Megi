use super::*;

/// Explicit-stack task for the iterative block-background traversal.
enum BlockTask<'a> {
    Visit {
        layout_box: &'a LayoutBox,
        inherited_hidden: bool,
        alpha_scale: f32,
    },
    PopClip,
    PopRoundedGroup,
}
/// Paints background and borders of in-flow block-level descendants (step 4).
///
/// Iterative: uses an explicit stack instead of recursion so deeply nested
/// documents cannot overflow the native stack.
pub(super) fn paint_block_backgrounds(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    inherited_hidden: bool,
    alpha_scale: f32,
) {
    let mut stack: Vec<BlockTask<'_>> =
        Vec::with_capacity(layout_box.children.len().saturating_mul(2).max(8));
    push_block_children(
        layout_box,
        inherited_hidden,
        alpha_scale,
        &mut stack,
        resources.effects_present,
    );
    while let Some(task) = stack.pop() {
        match task {
            BlockTask::PopClip => pop_clip_item(list, resources),
            BlockTask::PopRoundedGroup => pop_rounded_group(list, resources),
            BlockTask::Visit {
                layout_box: child,
                inherited_hidden,
                alpha_scale,
            } => {
                let hidden = effective_hidden(child, inherited_hidden);
                let alpha = alpha_scale * child.opacity.clamp(0.0, 1.0);
                // #1713: paint the box's own background/border before pushing the
                // clip it inherits/generates; CSS 2.2 §11.1.1 excludes it from clipping.
                // Inline-blocks are inline-level (`is_block()` is false) but still
                // own a background/border box. The inline pass only visits their
                // line boxes, so paint it here (#1884: Wikipedia wordmark sprite).
                if (child.is_block() || matches!(child.box_type, BoxType::InlineBlock(_)))
                    && !hidden
                {
                    let bg_clip = own_background_clip(child);
                    if let Some(rect) = bg_clip {
                        push_clip_item(list, resources, rect);
                    }
                    paint_background_border(child, list, skip, resources, false, alpha);
                    if bg_clip.is_some() {
                        pop_clip_item(list, resources);
                    }
                }
                if let Some(rect) = child.clip {
                    push_clip_item(list, resources, rect);
                    stack.push(BlockTask::PopClip);
                }
                // `overflow: hidden` + radius clips the descendant subtree.
                if !hidden && let Some((rect, radii)) = overflow_rounded_clip(child) {
                    push_rounded_group(list, resources, rect, radii);
                    stack.push(BlockTask::PopRoundedGroup);
                }
                push_block_children(child, hidden, alpha, &mut stack, resources.effects_present);
            }
        }
    }
}
fn push_block_children<'a>(
    parent: &'a LayoutBox,
    inherited_hidden: bool,
    alpha_scale: f32,
    stack: &mut Vec<BlockTask<'a>>,
    effects_present: bool,
) {
    for child in parent.children.iter().rev() {
        if phase(child, effects_present) != Phase::InFlow {
            continue;
        }
        stack.push(BlockTask::Visit {
            layout_box: child,
            inherited_hidden,
            alpha_scale,
        });
    }
}
/// Paints non-positioned floats (step 5), in tree order.
///
/// Iterative; nested floats recurse only up to [`MAX_FLOAT_DEPTH`].
pub(super) fn paint_floats(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    inherited_hidden: bool,
    alpha_scale: f32,
    depth: usize,
) {
    let mut stack: Vec<&LayoutBox> = Vec::with_capacity(layout_box.children.len().max(8));
    for child in layout_box.children.iter().rev() {
        stack.push(child);
    }
    while let Some(child) = stack.pop() {
        match phase(child, resources.effects_present) {
            Phase::Float => paint_float(
                child,
                list,
                skip,
                resources,
                inherited_hidden,
                alpha_scale,
                depth,
            ),
            Phase::InFlow => {
                for grandchild in child.children.iter().rev() {
                    stack.push(grandchild);
                }
            }
            Phase::Negative | Phase::Zero | Phase::Positive | Phase::TopLayer => {}
        }
    }
}
/// Paints one float: its background/border and its in-flow content (a float
/// behaves like a stacking context for painting purposes).
fn paint_float(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    inherited_hidden: bool,
    alpha_scale: f32,
    depth: usize,
) {
    let hidden = effective_hidden(layout_box, inherited_hidden);
    let alpha = alpha_scale * layout_box.opacity.clamp(0.0, 1.0);
    // #1713: own background before the clip (see Visit above).
    if !hidden {
        let bg_clip = own_background_clip(layout_box);
        if let Some(rect) = bg_clip {
            push_clip_item(list, resources, rect);
        }
        paint_background_border(layout_box, list, skip, resources, false, alpha);
        if bg_clip.is_some() {
            pop_clip_item(list, resources);
        }
    }
    let clipped = layout_box.clip;
    if let Some(rect) = clipped {
        push_clip_item(list, resources, rect);
    }
    let overflow_clip = if hidden {
        None
    } else {
        overflow_rounded_clip(layout_box)
    };
    if let Some((rect, radii)) = overflow_clip {
        push_rounded_group(list, resources, rect, radii);
    }
    paint_block_backgrounds(layout_box, list, skip, resources, hidden, alpha);
    if depth < MAX_FLOAT_DEPTH {
        paint_floats(layout_box, list, skip, resources, hidden, alpha, depth + 1);
    }
    paint_inline_content(
        layout_box,
        list,
        skip,
        resources,
        hidden,
        TextDecoration::NONE,
        alpha,
    );
    if overflow_clip.is_some() {
        pop_rounded_group(list, resources);
    }
    if clipped.is_some() {
        pop_clip_item(list, resources);
    }
}
