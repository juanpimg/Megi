//! Inline formatting context entry points (CSS 2.2 §9.4.2).
use super::*;

/// Lays out inline children inside a block container using [`DefaultTextMeasurer`].
pub fn layout_inline_formatting_context(layout_box: &mut LayoutBox) {
    let measurer = DefaultTextMeasurer;
    layout_inline_formatting_context_with_measurer(layout_box, &measurer);
}

/// Lays out inline children inside a block container with a custom [`TextMeasurer`].
pub fn layout_inline_formatting_context_with_measurer(
    layout_box: &mut LayoutBox,
    measurer: &dyn TextMeasurer,
) {
    let pos =
        crate::block::PositioningContext::from_containing_block(layout_box.dimensions.content);
    layout_inline_formatting_context_with_measurer_and_pos(layout_box, None, measurer, pos);
}

pub(crate) fn layout_inline_formatting_context_with_measurer_and_pos(
    layout_box: &mut LayoutBox,
    definite_height: Option<f32>,
    measurer: &dyn TextMeasurer,
    pos: crate::block::PositioningContext,
) {
    let container_width = layout_box.dimensions.content.width;
    let container_x = layout_box.dimensions.content.x;
    let container_y = layout_box.dimensions.content.y;
    let container_style = layout_box
        .style
        .take()
        .unwrap_or_else(|| Arc::new(ComputedStyle::initial()));

    // 1. Flatten children into a sequential stream of inline tokens. Atomic
    // inline-level boxes (inline-block/replaced) are laid out here so the line breaker
    // knows their margin box and baseline.
    // Rough one-pass estimate of the token/text volume so the per-IFC buffers are
    // allocated once instead of growing through several reallocations. Nested inline
    // boxes are underestimated on purpose: the estimate never needs to be exact.
    let mut estimated_tokens = 2usize;
    let mut estimated_text = 0usize;
    let mut control_direction = None;
    for child in &layout_box.children {
        if control_direction.is_none() {
            control_direction = bidi_control_direction(child);
        }
        match &child.box_type {
            BoxType::TextNode(txt) => {
                estimated_text += txt.len();
                estimated_tokens += txt.len() / 5 + 3;
            }
            BoxType::InlineNode(_) => estimated_tokens += 8,
            _ => estimated_tokens += 1,
        }
    }
    let mut tokens = Vec::with_capacity(estimated_tokens);
    let mut text_pool = String::with_capacity(estimated_text + estimated_text / 4);
    let mut styles: Vec<Arc<ComputedStyle>> = Vec::new();
    let mut inline_children = std::mem::take(&mut layout_box.children);
    let mut out_of_flow = Vec::new();
    collect_inline_tokens(
        &mut inline_children,
        measurer,
        &mut text_pool,
        &mut styles,
        &mut tokens,
        container_width,
        definite_height,
        pos,
        &mut out_of_flow,
    );

    // CSS 2.2 §9.6/§10.3.7: out-of-flow descendants are positioned after their
    // containing block's geometry is known, against the nearest positioned
    // ancestor (tracked by `pos`). The IFC container origin supplies the static
    // position when the box's offsets leave it unspecified.
    let place_out_of_flow = |layout_box: &mut LayoutBox, out_of_flow: Vec<LayoutBox>| {
        for mut child in out_of_flow {
            let fixed = child
                .style
                .as_ref()
                .is_some_and(|style| style.position == megi_css::Position::Fixed);
            let (cb, cb_height) = if fixed {
                (pos.viewport, Some(pos.viewport.height))
            } else {
                (pos.abs_cb, pos.abs_cb_height)
            };
            crate::block::layout_absolutely_positioned(
                &mut child,
                cb,
                cb_height,
                container_x,
                container_y,
                pos,
                measurer,
            );
            layout_box.children.push(child);
        }
    };

    if tokens.is_empty() {
        layout_box.children.clear();
        place_out_of_flow(layout_box, out_of_flow);
        layout_box.style = Some(container_style);
        return;
    }

    // 2. Break tokens into lines at valid soft wrap opportunities.
    // A flex item shrunk below its min-content width also allows breaks inside
    // an otherwise unbreakable word (CSS Flexbox 1 §4.5, min-width: auto -> 0).
    let lines = break_into_lines(
        tokens,
        &mut text_pool,
        &styles,
        container_width,
        measurer,
        pos.wrap_long_words,
    );

    if lines.is_empty() {
        layout_box.children.clear();
        place_out_of_flow(layout_box, out_of_flow);
        layout_box.style = Some(container_style);
        return;
    }

    // 3. Strut metrics for the containing block container per CSS 2.2 §10.8
    let strut_metrics = metrics_styled(measurer, &container_style);
    let strut_leading = strut_metrics.line_height - (strut_metrics.ascent + strut_metrics.descent);
    let strut_ascent = strut_metrics.ascent + strut_leading / 2.0;
    let strut_descent = strut_metrics.descent + strut_leading / 2.0;

    // 4. Construct LineBox layout boxes and resolve baseline / vertical-align
    let bidi_active = container_style.direction == megi_css::Direction::Rtl
        || container_style.unicode_bidi != megi_css::UnicodeBidi::Normal
        || crate::bidi::contains_rtl_or_bidi(&text_pool)
        || control_direction.is_some();
    let mut line_boxes = Vec::with_capacity(lines.len());
    let mut current_line_y = container_y;

    // Track active inline boxes across line breaks for box splitting
    let mut persistent_active_boxes: Vec<ActiveBoxInfo> = Vec::new();

    for mut line in lines {
        if bidi_active {
            reorder_line_tokens_bidi(
                &mut line.tokens,
                &mut text_pool,
                &styles,
                &container_style,
                control_direction,
            );
        }
        let ctx = LineContext {
            container_x,
            line_y: current_line_y,
            container_width,
            container_style: &container_style,
            strut_ascent,
            strut_descent,
            justify: line.justified && container_style.text_align == TextAlign::Justify,
        };
        let (line_box, new_active) = format_line(
            line.tokens,
            &text_pool,
            &styles,
            &ctx,
            measurer,
            persistent_active_boxes,
        );

        current_line_y += line_box.dimensions.content.height;
        persistent_active_boxes = new_active;
        line_boxes.push(line_box);
    }

    layout_box.children = line_boxes;
    place_out_of_flow(layout_box, out_of_flow);
    layout_box.style = Some(container_style);
}
