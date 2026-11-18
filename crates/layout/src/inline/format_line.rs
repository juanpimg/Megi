//! Line formatting: baseline, line box height and fragment assembly (CSS 2.2 §10.8).
use super::*;

/// Formats a single line into a [`BoxType::LineBox`] [`LayoutBox`].
pub(crate) fn format_line(
    tokens: Vec<InlineToken>,
    text_pool: &str,
    styles: &[Arc<ComputedStyle>],
    ctx: &LineContext<'_>,
    measurer: &dyn TextMeasurer,
    incoming_active_boxes: Vec<ActiveBoxInfo>,
) -> (LayoutBox, Vec<ActiveBoxInfo>) {
    let LineContext {
        container_x,
        line_y,
        container_width,
        container_style,
        strut_ascent,
        strut_descent,
        justify,
    } = *ctx;

    // Plain single-text-node lines skip the generic two-pass bookkeeping.
    if incoming_active_boxes.is_empty()
        && let Some(line_box) = try_format_simple_text_line(&tokens, text_pool, styles, ctx)
    {
        return (line_box, incoming_active_boxes);
    }

    // 1. Baseline and line box height (CSS 2.2 §10.8.1), while also accumulating the
    // line content width needed by text-align/justify (single pass over the tokens).
    let mut line_content_width = 0.0_f32;
    let mut justify_space_count = 0usize;
    //
    // Baseline-aligned content (baseline/sub/super/length/middle/text-top/text-bottom)
    // determines the line box ascent and descent. `vertical-align: top|bottom` boxes
    // are collected as *aligned subtrees*: their extents are measured over all their
    // baseline-aligned descendants and they are anchored to the line box edges, which
    // can make the line box taller than the baseline-aligned content.
    let mut max_ascent = strut_ascent;
    let mut max_descent = strut_descent;
    let mut frame_aligns: Vec<VerticalAlign> = Vec::new();
    // Open `top`/`bottom` aligned subtrees: (BoxStart index, is_top, ascent, descent).
    let mut open_anchors: Vec<(usize, bool, f32, f32)> = Vec::new();
    // Aligned-subtree extents keyed by the index of their `BoxStart` token.
    let mut anchor_extents: Vec<(usize, bool, f32, f32)> = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        line_content_width += token_advance(token, styles, container_width);
        if justify && matches!(token, InlineToken::Space { text, .. } if *text != '\0') {
            justify_space_count += 1;
        }
        match token {
            InlineToken::BoxStart { style, .. } => {
                let style = &styles[*style as usize];
                let metrics = metrics_styled(measurer, style);
                let (ascent, descent) = line_box_extents(&metrics);
                // The box itself always contributes to every open aligned subtree.
                for anchor in &mut open_anchors {
                    anchor.2 = anchor.2.max(ascent);
                    anchor.3 = anchor.3.max(descent);
                }
                let is_top = matches!(style.vertical_align, VerticalAlign::Top);
                let is_bottom = matches!(style.vertical_align, VerticalAlign::Bottom);
                if open_anchors.is_empty() && !is_top && !is_bottom {
                    let (a, d) = align_extents(
                        style.vertical_align,
                        ascent,
                        descent,
                        container_style.font_size,
                        strut_ascent,
                        strut_descent,
                    );
                    max_ascent = max_ascent.max(a);
                    max_descent = max_descent.max(d);
                }
                if is_top || is_bottom {
                    open_anchors.push((index, is_top, ascent, descent));
                }
                frame_aligns.push(style.vertical_align);
            }
            InlineToken::BoxEnd { .. } => {
                if let Some(vertical_align) = frame_aligns.pop()
                    && matches!(vertical_align, VerticalAlign::Top | VerticalAlign::Bottom)
                    && let Some((start, is_top, ascent, descent)) = open_anchors.pop()
                {
                    anchor_extents.push((start, is_top, ascent, descent));
                }
            }
            InlineToken::Word {
                ascent, descent, ..
            }
            | InlineToken::Space {
                ascent, descent, ..
            } => {
                let (ascent, descent) = (*ascent, *descent);
                if !open_anchors.is_empty() {
                    // Inside a top/bottom aligned subtree: only the subtree extent grows.
                    for anchor in &mut open_anchors {
                        anchor.2 = anchor.2.max(ascent);
                        anchor.3 = anchor.3.max(descent);
                    }
                } else {
                    let vertical_align = frame_aligns
                        .last()
                        .copied()
                        .unwrap_or(VerticalAlign::Baseline);
                    let (a, d) = align_extents(
                        vertical_align,
                        ascent,
                        descent,
                        container_style.font_size,
                        strut_ascent,
                        strut_descent,
                    );
                    max_ascent = max_ascent.max(a);
                    max_descent = max_descent.max(d);
                }
            }
            InlineToken::Atomic {
                baseline,
                height,
                vertical_align,
                ..
            } => {
                let ascent = *baseline;
                let descent = (*height - *baseline).max(0.0);
                if !open_anchors.is_empty() {
                    for anchor in &mut open_anchors {
                        anchor.2 = anchor.2.max(ascent);
                        anchor.3 = anchor.3.max(descent);
                    }
                } else if matches!(vertical_align, VerticalAlign::Top | VerticalAlign::Bottom) {
                    anchor_extents.push((
                        index,
                        *vertical_align == VerticalAlign::Top,
                        ascent,
                        descent,
                    ));
                } else {
                    let (a, d) = align_extents(
                        *vertical_align,
                        ascent,
                        descent,
                        container_style.font_size,
                        strut_ascent,
                        strut_descent,
                    );
                    max_ascent = max_ascent.max(a);
                    max_descent = max_descent.max(d);
                }
            }
            InlineToken::HyphenationPoint { .. } | InlineToken::ForcedBreak => {}
        }
    }
    // Boxes still open at the end of the line are split across lines; their aligned
    // subtree still contributes to this line box.
    while let Some((start, is_top, ascent, descent)) = open_anchors.pop() {
        anchor_extents.push((start, is_top, ascent, descent));
    }

    // 2. Horizontal text-align offset and justify distribution (CSS 2.2 §16.2).
    let remaining_width = (container_width - line_content_width).max(0.0);
    let align_offset = match container_style.text_align {
        TextAlign::Left | TextAlign::Justify => 0.0,
        TextAlign::Right => remaining_width,
        TextAlign::Center => remaining_width / 2.0,
    };
    let justify_extra_per_space = if justify_space_count > 0 {
        remaining_width / justify_space_count as f32
    } else {
        0.0
    };

    let mut max_top_height: f32 = 0.0;
    let mut max_bottom_height: f32 = 0.0;
    for (_, is_top, ascent, descent) in &anchor_extents {
        let height = ascent + descent;
        if *is_top {
            max_top_height = max_top_height.max(height);
        } else {
            max_bottom_height = max_bottom_height.max(height);
        }
    }
    let line_height = (max_ascent + max_descent)
        .max(max_top_height)
        .max(max_bottom_height);
    let baseline_y_in_line = max_ascent;

    // 4. Fragment assembly using active boxes stack.
    let mut active_stack: Vec<ActiveBoxFrame> = Vec::with_capacity(incoming_active_boxes.len() + 2);
    let mut line_children: Vec<LayoutBox> = Vec::with_capacity(tokens.len() / 2 + 1);
    let mut curr_x = container_x + align_offset;

    // Reopen boxes that were split from previous lines.
    for prev in incoming_active_boxes {
        let anchor = anchor_for_style(&prev.style, measurer, line_y, line_height);
        active_stack.push(ActiveBoxFrame {
            node_id: prev.node_id,
            style: prev.style,
            is_first_fragment: prev.is_first_fragment,
            is_pseudo: prev.is_pseudo,
            start_x: curr_x,
            children: Vec::new(),
            anchor,
        });
    }

    for (index, token) in tokens.into_iter().enumerate() {
        match token {
            InlineToken::BoxStart {
                node_id,
                style,
                is_pseudo,
            } => {
                let style_arc = styles[style as usize].clone();
                let style = style_arc.as_ref();
                let left_margin = resolve_length(
                    style.margin.left,
                    style.calc_lengths.margin_left,
                    container_width,
                );
                let left_border = style.border_width.left;
                let left_padding = resolve_length(
                    style.padding.left,
                    style.calc_lengths.padding_left,
                    container_width,
                );
                let left_edges = left_margin + left_border + left_padding;
                let start_x = curr_x;
                curr_x += left_edges;

                let anchor = match anchor_extents
                    .iter()
                    .find(|(anchor_index, ..)| *anchor_index == index)
                {
                    Some((_, is_top, ascent, descent)) => {
                        let height = ascent + descent;
                        let top = if *is_top {
                            line_y
                        } else {
                            line_y + line_height - height
                        };
                        Some((top, top + ascent, height))
                    }
                    None => anchor_for_style(style, measurer, line_y, line_height),
                };

                active_stack.push(ActiveBoxFrame {
                    node_id,
                    style: style_arc,
                    is_first_fragment: true,
                    is_pseudo,
                    start_x,
                    children: Vec::new(),
                    anchor,
                });
            }
            InlineToken::BoxEnd {
                node_id, is_pseudo, ..
            } => {
                if let Some(pos) = active_stack
                    .iter()
                    .rposition(|b| b.node_id == node_id && b.is_pseudo == is_pseudo)
                {
                    let frame = active_stack.remove(pos);
                    let right_padding = resolve_length(
                        frame.style.padding.right,
                        frame.style.calc_lengths.padding_right,
                        container_width,
                    );
                    let right_border = frame.style.border_width.right;
                    let right_margin = resolve_length(
                        frame.style.margin.right,
                        frame.style.calc_lengths.margin_right,
                        container_width,
                    );
                    let right_edges = right_padding + right_border + right_margin;

                    let left_margin = if frame.is_first_fragment {
                        resolve_length(
                            frame.style.margin.left,
                            frame.style.calc_lengths.margin_left,
                            container_width,
                        )
                    } else {
                        0.0
                    };
                    let left_border = if frame.is_first_fragment {
                        frame.style.border_width.left
                    } else {
                        0.0
                    };
                    let left_padding = if frame.is_first_fragment {
                        resolve_length(
                            frame.style.padding.left,
                            frame.style.calc_lengths.padding_left,
                            container_width,
                        )
                    } else {
                        0.0
                    };

                    let box_w = (curr_x
                        - (frame.start_x + left_margin + left_border + left_padding))
                        .max(0.0);
                    curr_x += right_edges;

                    let mut inline_box = LayoutBox::with_shared_style(
                        BoxType::InlineNode(frame.node_id),
                        Some(frame.style.clone()),
                    );
                    inline_box.is_pseudo = frame.is_pseudo;
                    inline_box.dimensions.margin.left = left_margin;
                    inline_box.dimensions.margin.right = right_margin;
                    inline_box.dimensions.border.left = left_border;
                    inline_box.dimensions.border.right = right_border;
                    inline_box.dimensions.padding.left = left_padding;
                    inline_box.dimensions.padding.right = right_padding;

                    inline_box.dimensions.content.x =
                        frame.start_x + left_margin + left_border + left_padding;
                    inline_box.dimensions.content.y =
                        frame.anchor.map_or(line_y, |(top, _, _)| top);
                    inline_box.dimensions.content.width = box_w;
                    inline_box.dimensions.content.height =
                        frame.anchor.map_or(line_height, |(_, _, height)| height);
                    inline_box.children = frame.children;

                    if let Some(parent_frame) = active_stack.last_mut() {
                        parent_frame.children.push(inline_box);
                    } else {
                        line_children.push(inline_box);
                    }
                }
            }
            InlineToken::Word {
                start,
                end,
                width,
                style,
                ascent,
                descent,
            } => {
                push_text_fragment(
                    &text_pool[start..end],
                    width,
                    style,
                    styles,
                    ascent,
                    descent,
                    &mut active_stack,
                    &mut line_children,
                    &mut curr_x,
                    ctx,
                    line_height,
                    baseline_y_in_line,
                );
            }
            InlineToken::Space {
                text,
                width,
                style,
                ascent,
                descent,
                ..
            } => {
                let mut buf = [0u8; 4];
                let rendered: &str = if text == '\0' {
                    ""
                } else {
                    text.encode_utf8(&mut buf)
                };
                push_text_fragment(
                    rendered,
                    width + justify_extra_per_space,
                    style,
                    styles,
                    ascent,
                    descent,
                    &mut active_stack,
                    &mut line_children,
                    &mut curr_x,
                    ctx,
                    line_height,
                    baseline_y_in_line,
                );
            }
            InlineToken::Atomic {
                mut layout_box,
                width,
                height,
                baseline,
                vertical_align,
            } => {
                // Line-level `top`/`bottom` anchor first, then an enclosing inline box
                // anchor, then the shared line baseline.
                let line_anchor = anchor_extents
                    .iter()
                    .find(|(anchor_index, ..)| *anchor_index == index)
                    .map(|(_, is_top, ascent, descent)| {
                        let anchor_height = ascent + descent;
                        let top = if *is_top {
                            line_y
                        } else {
                            line_y + line_height - anchor_height
                        };
                        (top, top + ascent, anchor_height)
                    });
                let anchor_pos = active_stack
                    .iter()
                    .rposition(|frame| frame.anchor.is_some());
                let innermost_align = active_stack
                    .last()
                    .map(|frame| frame.style.vertical_align)
                    .unwrap_or(VerticalAlign::Baseline);
                let effective_align = match anchor_pos {
                    Some(position) if position + 1 == active_stack.len() => VerticalAlign::Baseline,
                    Some(_) => innermost_align,
                    None => vertical_align,
                };
                let (a_prime, _) = align_extents(
                    effective_align,
                    baseline,
                    height - baseline,
                    container_style.font_size,
                    strut_ascent,
                    strut_descent,
                );
                let y = if let Some((top, _, _)) = line_anchor {
                    top
                } else if let Some(frame_baseline) =
                    anchor_pos.and_then(|position| active_stack[position].anchor)
                {
                    frame_baseline.1 - a_prime
                } else {
                    line_y
                        + (baseline_y_in_line - a_prime).clamp(0.0, (line_height - height).max(0.0))
                };

                let margin_box = layout_box.dimensions.margin_box();
                let dx = curr_x - margin_box.x;
                let dy = y - margin_box.y;
                crate::block::translate_box(&mut layout_box, dx, dy);

                let target_list = if let Some(active) = active_stack.last_mut() {
                    &mut active.children
                } else {
                    &mut line_children
                };
                target_list.push(*layout_box);
                curr_x += width;
            }
            InlineToken::HyphenationPoint { .. } | InlineToken::ForcedBreak => {}
        }
    }

    // 5. Handle any inline boxes that remain open at line break (split across lines).
    let outgoing_active = close_remaining_boxes(
        &mut active_stack,
        &mut line_children,
        curr_x,
        container_width,
        line_y,
        line_height,
    );

    // 6. Assemble the LineBox.
    //
    // Line boxes are pure formatting artifacts: they must not carry a painting style
    // (no background, border or padding), otherwise paint duplicates the container's
    // decoration once per line (audit finding F-02).
    let mut line_box = LayoutBox::new(BoxType::LineBox, None);
    line_box.dimensions.content = Rect::new(container_x, line_y, container_width, line_height);
    line_box.baseline = baseline_y_in_line;
    line_box.children = line_children;

    (line_box, outgoing_active)
}
