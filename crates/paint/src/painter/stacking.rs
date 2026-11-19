use super::*;

/// Paints the appendix E phases of one stacking context.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_stacking_context(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    inherited_hidden: bool,
    is_root: bool,
    depth: usize,
    contexts: &HashMap<*const LayoutBox, ContextBuckets<'_>>,
) {
    let start = list.len();
    let hidden = effective_hidden(layout_box, inherited_hidden);
    let opacity = layout_box.opacity.clamp(0.0, 1.0);

    // Step 2: background and borders of the element establishing the context.
    if !hidden {
        paint_background_border(layout_box, list, skip, resources, is_root, 1.0);
    }

    // `overflow: hidden`/`clip` + radius clips the descendant subtree of this
    // stacking context (all phases, including nested contexts and z-index).
    let overflow_clip = if hidden {
        None
    } else {
        overflow_rounded_clip(layout_box)
    };
    if let Some((rect, radii)) = overflow_clip {
        push_rounded_group(list, resources, rect, radii);
    }

    let clip = layout_box.clip;
    if let Some(rect) = clip {
        push_clip_item(list, resources, rect);
    }

    if depth >= MAX_STACKING_CONTEXT_DEPTH {
        // Depth guard: paint the remaining subtree as flat in-flow content
        // instead of recursing into further stacking contexts.
        paint_block_backgrounds(layout_box, list, skip, resources, hidden, 1.0);
        paint_floats(layout_box, list, skip, resources, hidden, 1.0, depth);
        paint_inline_content(
            layout_box,
            list,
            skip,
            resources,
            hidden,
            TextDecoration::NONE,
            1.0,
        );
        if clip.is_some() {
            pop_clip_item(list, resources);
        }
        if overflow_clip.is_some() {
            pop_rounded_group(list, resources);
        }
        return;
    }

    // Direct context children were bucketed once by `index_stacking_contexts`;
    // no per-context subtree rescan (#443).
    let empty = ContextBuckets::default();
    let buckets = contexts
        .get(&(layout_box as *const LayoutBox))
        .unwrap_or(&empty);
    let mut negative: Vec<&LayoutBox> = buckets.negative.to_vec();
    let mut positive: Vec<&LayoutBox> = buckets.positive.to_vec();
    negative.sort_by_key(|context| context.z_index);
    positive.sort_by_key(|context| context.z_index);

    // Step 3: negative z-index contexts.
    for &context in &negative {
        paint_stacking_context(
            context,
            list,
            skip,
            resources,
            hidden,
            false,
            depth + 1,
            contexts,
        );
    }

    // Steps 4-6: in-flow block backgrounds, floats, inline content.
    paint_block_backgrounds(layout_box, list, skip, resources, hidden, 1.0);
    paint_floats(layout_box, list, skip, resources, hidden, 1.0, depth);
    paint_inline_content(
        layout_box,
        list,
        skip,
        resources,
        hidden,
        TextDecoration::NONE,
        1.0,
    );

    // A positioned/floated replaced element is painted in its own stacking
    // context step (Appendix E step 8); the inline walker skips non-in-flow
    // children, so its bitmap would otherwise never be emitted (#1884).
    if !hidden
        && !matches!(phase(layout_box, resources.effects_present), Phase::InFlow)
        && let Some(node_id) = layout_box.node_id()
        && resources.images.get(&node_id).is_some()
    {
        paint_replaced_element(layout_box, node_id, list, resources, opacity);
    }

    // Step 7: positioned z-index auto/0 and z-index 0 contexts.
    for &context in &buckets.zero {
        paint_stacking_context(
            context,
            list,
            skip,
            resources,
            hidden,
            false,
            depth + 1,
            contexts,
        );
    }

    // Step 8: positive z-index contexts.
    for &context in &positive {
        paint_stacking_context(
            context,
            list,
            skip,
            resources,
            hidden,
            false,
            depth + 1,
            contexts,
        );
    }

    if clip.is_some() {
        pop_clip_item(list, resources);
    }
    if overflow_clip.is_some() {
        pop_rounded_group(list, resources);
    }

    // Filter / blend group: the whole subtree is rasterized offscreen, the
    // filters are applied, and the result is composited against the backdrop.
    // Boxes handled by the inline filtered-solid fast path already applied their
    // filters, so they must not be wrapped again (R166).
    if resources.effects_present
        && inline_filtered_solid(layout_box).is_none()
        && let Some(style) = layout_box.style.as_ref()
        && (!style.filter.is_empty() || style.mix_blend_mode != megi_css::MixBlendMode::Normal)
        && list.len() > start
    {
        let filters = style.filter.clone();
        let blend = style.mix_blend_mode;
        let items: Vec<DisplayItem> = list.items.drain(start..).collect();
        let bounds = items_bounds(&items).unwrap_or_else(|| layout_box.dimensions.border_box());
        let padding = effect_padding(&filters);
        let rect = Rect::new(
            bounds.x - padding,
            bounds.y - padding,
            bounds.width + 2.0 * padding,
            bounds.height + 2.0 * padding,
        );
        list.push(DisplayItem::EffectGroup {
            rect,
            filters,
            blend,
            items,
        });
    }

    // Opacity group: the whole subtree is composited once with `opacity`.
    if opacity < 1.0 && list.len() > start {
        let items: Vec<DisplayItem> = list.items.drain(start..).collect();
        let rect = items_bounds(&items).unwrap_or_else(|| layout_box.dimensions.border_box());
        list.push(DisplayItem::OpacityGroup {
            rect,
            opacity,
            items,
        });
    }
}
/// Collects the stacking contexts that participate in the context rooted at
/// `layout_box` (nearest-context descendants, in tree order).
///
/// Iterative by design: a 20 000-level layout tree must not exhaust the stack.
/// Direct stacking-context children of one context, partitioned into Appendix E
/// phase buckets. Built once for the whole tree (#443).
#[derive(Default)]
pub(super) struct ContextBuckets<'a> {
    negative: Vec<&'a LayoutBox>,
    zero: Vec<&'a LayoutBox>,
    positive: Vec<&'a LayoutBox>,
}
/// Single-pass index of every stacking context and its direct context children.
///
/// The old code called `collect_context_boxes` on every context, and each call
/// walked the whole descendant subtree: N nested contexts cost O(N²) box visits
/// (#443). This visits each box exactly once and buckets direct context children
/// against their nearest ancestor context.
pub(super) fn index_stacking_contexts<'a>(
    root: &'a LayoutBox,
    effects_present: bool,
) -> (
    HashMap<*const LayoutBox, ContextBuckets<'a>>,
    Vec<&'a LayoutBox>,
) {
    let mut contexts = HashMap::new();
    let mut top_layer = Vec::new();
    let root_ptr = root as *const LayoutBox;
    contexts.insert(root_ptr, ContextBuckets::default());
    let mut stack: Vec<(&'a LayoutBox, *const LayoutBox, usize)> = root
        .children
        .iter()
        .rev()
        .map(|child| (child, root_ptr, 1))
        .collect();
    while let Some((node, owner, depth)) = stack.pop() {
        let node_phase = phase(node, effects_present);
        let node_ptr = node as *const LayoutBox;
        if node_phase == Phase::TopLayer {
            top_layer.push(node);
            contexts.entry(node_ptr).or_default();
            for child in node.children.iter().rev() {
                stack.push((child, node_ptr, 1));
            }
            continue;
        }
        let bucket = match node_phase {
            Phase::Negative => Some(0),
            Phase::Zero => Some(1),
            Phase::Positive => Some(2),
            Phase::Float | Phase::InFlow | Phase::TopLayer => None,
        };
        if let Some(bucket) = bucket
            && depth < MAX_STACKING_CONTEXT_DEPTH
        {
            contexts.entry(node_ptr).or_default();
            let owner_buckets = contexts.get_mut(&owner).expect("indexed owner context");
            match bucket {
                0 => owner_buckets.negative.push(node),
                1 => owner_buckets.zero.push(node),
                _ => owner_buckets.positive.push(node),
            }
            for child in node.children.iter().rev() {
                stack.push((child, node_ptr, depth + 1));
            }
        } else {
            // In-flow/float boxes, and contexts past the depth guard, keep the
            // same owner: their descendants are painted in the owner's passes.
            for child in node.children.iter().rev() {
                stack.push((child, owner, depth));
            }
        }
    }
    (contexts, top_layer)
}
