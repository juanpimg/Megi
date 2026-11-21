use super::*;

/// Explicit-stack task for the iterative inline-content traversal.
enum InlineTask<'a> {
    Visit {
        layout_box: &'a LayoutBox,
        inherited_hidden: bool,
        inherited_decoration: TextDecoration,
        alpha: f32,
        ordinal: u32,
        /// Nearest element `NodeId` (for text/decoration hit regions).
        owner: Option<megi_core::NodeId>,
    },
    PopClip,
    PopRoundedGroup,
}
/// Paints in-flow inline content (step 6): inline boxes, text runs with their
/// decorations and list markers, in tree order.
///
/// Iterative: explicit stack, so deep inline nesting cannot overflow the stack.
pub(super) fn paint_inline_content(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    inherited_hidden: bool,
    inherited_decoration: TextDecoration,
    alpha_scale: f32,
) {
    let owner = layout_box.node_id();
    let mut stack: Vec<InlineTask<'_>> =
        Vec::with_capacity(layout_box.children.len().saturating_mul(2).max(8));
    push_inline_children(
        layout_box,
        inherited_hidden,
        inherited_decoration,
        alpha_scale,
        owner,
        &mut stack,
        resources.effects_present,
    );
    while let Some(task) = stack.pop() {
        match task {
            InlineTask::PopClip => pop_clip_item(list, resources),
            InlineTask::PopRoundedGroup => pop_rounded_group(list, resources),
            InlineTask::Visit {
                layout_box: child,
                inherited_hidden,
                inherited_decoration,
                alpha,
                ordinal,
                owner,
            } => {
                let own_owner = child.node_id().or(owner);
                let parent_decoration = inherited_decoration
                    | child
                        .style
                        .as_ref()
                        .map_or(TextDecoration::NONE, |style| style.text_decoration);
                let hidden = effective_hidden(child, inherited_hidden);
                // #1713: the box's own background/border is not clipped by its
                // own overflow; emit it before the clip that restricts descendants.
                if !hidden
                    && matches!(
                        child.box_type,
                        BoxType::Replaced(_) | BoxType::InlineNode(_)
                    )
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
                let clipped = child.clip;
                if let Some(rect) = clipped {
                    push_clip_item(list, resources, rect);
                    stack.push(InlineTask::PopClip);
                }

                match &child.box_type {
                    BoxType::TextNode(text) if !text.trim().is_empty() => {
                        if !hidden {
                            paint_text_run(
                                child,
                                text,
                                list,
                                resources,
                                own_owner,
                                alpha,
                                layout_box.dimensions.content.width,
                            );
                            // `text-decoration` is empty for the vast majority of
                            // runs; skip the decoration helper entirely (R129).
                            let decoration = parent_decoration
                                | child
                                    .style
                                    .as_ref()
                                    .map_or(TextDecoration::NONE, |style| style.text_decoration);
                            if !decoration.is_empty() {
                                paint_text_decorations(
                                    child, decoration, list, resources, own_owner, alpha,
                                );
                            }
                        }
                    }
                    BoxType::TextNode(_) | BoxType::LineBreak => {}
                    BoxType::Replaced(node_id) => {
                        if !hidden {
                            let replaced_radius = child
                                .style
                                .as_ref()
                                .map(|style| style.border_radius)
                                .filter(|radii| !radii.is_zero());
                            if let Some(radii) = replaced_radius {
                                // Replaced content is clipped by `border-radius`
                                // (CSS Backgrounds 3 §5.5).
                                push_rounded_group(
                                    list,
                                    resources,
                                    child.dimensions.content,
                                    radii,
                                );
                                paint_replaced_element(child, *node_id, list, resources, alpha);
                                pop_rounded_group(list, resources);
                            } else {
                                paint_replaced_element(child, *node_id, list, resources, alpha);
                            }
                        }
                    }
                    BoxType::InlineNode(_) => {
                        if !hidden && let Some((rect, radii)) = overflow_rounded_clip(child) {
                            push_rounded_group(list, resources, rect, radii);
                            stack.push(InlineTask::PopRoundedGroup);
                        }
                        push_inline_children(
                            child,
                            hidden,
                            parent_decoration,
                            alpha,
                            own_owner,
                            &mut stack,
                            resources.effects_present,
                        );
                    }
                    BoxType::BlockNode(_) | BoxType::AnonymousBlock | BoxType::LineBox => {
                        if is_list_item(child) && !hidden {
                            paint_marker(child, ordinal, list, resources, alpha);
                        }
                        if !hidden && let Some((rect, radii)) = overflow_rounded_clip(child) {
                            push_rounded_group(list, resources, rect, radii);
                            stack.push(InlineTask::PopRoundedGroup);
                        }
                        push_inline_children(
                            child,
                            hidden,
                            parent_decoration,
                            alpha,
                            own_owner,
                            &mut stack,
                            resources.effects_present,
                        );
                    }
                    // Table internals and inline-block wrappers: backgrounds and
                    // borders are emitted by the block-background pass, but their
                    // inline content (line boxes, text runs, decorations) must be
                    // visited here as well or table cells' text never paints
                    // (reported by the layout lane, task-53).
                    BoxType::InlineBlock(_)
                    | BoxType::Table(_)
                    | BoxType::TableRowGroup(_)
                    | BoxType::TableRow(_)
                    | BoxType::TableCell(_)
                    | BoxType::TableCaption(_)
                    | BoxType::TableColumn(_)
                    | BoxType::AnonymousTableRow
                    | BoxType::AnonymousTableCell => {
                        if !hidden && let Some((rect, radii)) = overflow_rounded_clip(child) {
                            push_rounded_group(list, resources, rect, radii);
                            stack.push(InlineTask::PopRoundedGroup);
                        }
                        push_inline_children(
                            child,
                            hidden,
                            parent_decoration,
                            alpha,
                            own_owner,
                            &mut stack,
                            resources.effects_present,
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}
/// Pushes the inline tasks of `parent`'s children (reversed, so the stack pops
/// them in tree order), computing per-box hidden/decoration/alpha state and
/// `list-item` ordinals.
///
/// Allocation-free: a single backwards pass derives the ordinals directly
/// instead of collecting intermediate task vectors (this hot path runs for
/// every box of the layout tree).
fn push_inline_children<'a>(
    parent: &'a LayoutBox,
    inherited_hidden: bool,
    inherited_decoration: TextDecoration,
    alpha_scale: f32,
    owner: Option<megi_core::NodeId>,
    stack: &mut Vec<InlineTask<'a>>,
    effects_present: bool,
) {
    let parent_decoration = inherited_decoration
        | parent
            .style
            .as_ref()
            .map_or(TextDecoration::NONE, |style| style.text_decoration);
    // 1-based ordinal of the last list item in this parent, decremented while
    // walking the children backwards.
    let mut next_ordinal = parent
        .children
        .iter()
        .filter(|child| {
            // Cheap flag first: `phase()` only needs to run for list items here.
            is_list_item(child) && phase(child, effects_present) == Phase::InFlow
        })
        .count() as u32;

    for child in parent.children.iter().rev() {
        if phase(child, effects_present) != Phase::InFlow {
            continue;
        }
        let ordinal = if is_list_item(child) {
            let ordinal = next_ordinal.max(1);
            next_ordinal = next_ordinal.saturating_sub(1);
            ordinal
        } else {
            0
        };
        let hidden = effective_hidden(child, inherited_hidden);
        let alpha = alpha_scale * child.opacity.clamp(0.0, 1.0);
        stack.push(InlineTask::Visit {
            layout_box: child,
            inherited_hidden: hidden,
            inherited_decoration: parent_decoration,
            alpha,
            ordinal,
            owner: child.node_id().or(owner),
        });
    }
}
