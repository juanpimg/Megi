//! Inline box frame state and text fragment placement (CSS 2.2 §9.4.2, §10.8.1).
use super::*;

/// Metadata for an active inline box that may span across line boundaries.
#[derive(Clone, Debug)]
pub(crate) struct ActiveBoxInfo {
    pub(crate) node_id: NodeId,
    pub(crate) style: Arc<ComputedStyle>,
    pub(crate) is_first_fragment: bool,
    pub(crate) is_pseudo: bool,
}

/// Context parameters for formatting a single line in an IFC.
pub(crate) struct LineContext<'a> {
    pub(crate) container_x: f32,
    pub(crate) line_y: f32,
    pub(crate) container_width: f32,
    pub(crate) container_style: &'a ComputedStyle,
    pub(crate) strut_ascent: f32,
    pub(crate) strut_descent: f32,
    /// `true` when this line must be justified (soft-wrapped line of a
    /// `text-align: justify` paragraph; the last line and lines before forced breaks
    /// are never justified, CSS 2.2 §16.2).
    pub(crate) justify: bool,
}

/// An inline box fragment being assembled while formatting a line.
pub(crate) struct ActiveBoxFrame {
    pub(crate) node_id: NodeId,
    pub(crate) style: Arc<ComputedStyle>,
    pub(crate) is_first_fragment: bool,
    pub(crate) is_pseudo: bool,
    pub(crate) start_x: f32,
    pub(crate) children: Vec<LayoutBox>,
    /// `(subtree top, subtree baseline, subtree height)` for `top`/`bottom` aligned
    /// inline boxes; `None` for every other box.
    pub(crate) anchor: Option<(f32, f32, f32)>,
}

/// Applies CSS `text-overflow: ellipsis` to a text run that exceeds its available width
/// (CSS UI §4.6.1 / #1929). Only `white-space: nowrap` runs are eligible; the ellipsis is
/// inserted in place of the trailing characters so the run fits the clipped line.
///
/// Widths are distributed evenly across characters (the line builder measures runs, not
/// individual glyphs), which keeps the result deterministic and conservative.
pub(crate) fn ellipsize_text(
    text: &str,
    width: f32,
    style: &ComputedStyle,
    available_width: f32,
) -> (String, f32) {
    if style.text_overflow != megi_css::TextOverflow::Ellipsis
        || style.white_space != WhiteSpace::Nowrap
        || width <= available_width
    {
        return (text.to_owned(), width);
    }
    let char_count = text.chars().count();
    if char_count == 0 {
        return (text.to_owned(), width);
    }
    let per_char = width / char_count as f32;
    if !per_char.is_finite() || per_char <= 0.0 {
        return (text.to_owned(), width);
    }
    // Reserve the width of one character for the ellipsis itself.
    let ellipsis_width = per_char;
    let budget = (available_width - ellipsis_width).max(0.0);
    let keep = (budget / per_char).floor() as usize;
    let keep = keep.min(char_count.saturating_sub(1));
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    (out, per_char * (keep as f32 + 1.0))
}

/// Fast path for a line made only of text from one text node (no inline boxes,
/// atomic boxes, vertical alignment or justification).
///
/// The generic path performs two passes over the tokens, tracks aligned subtrees and
/// merges adjacent fragments; for plain text lines (the overwhelming majority on
/// text-heavy pages) the result is a line box with exactly one merged text fragment.
/// Building that fragment directly keeps the output tree and geometry identical while
/// skipping the per-token bookkeeping.
pub(crate) fn try_format_simple_text_line(
    tokens: &[InlineToken],
    text_pool: &str,
    styles: &[Arc<ComputedStyle>],
    ctx: &LineContext<'_>,
) -> Option<LayoutBox> {
    if tokens.is_empty() || ctx.justify {
        return None;
    }
    let mut style_id: Option<u32> = None;
    let mut width = 0.0_f32;
    let mut ascent = 0.0_f32;
    let mut descent = 0.0_f32;
    let mut text = String::new();
    for token in tokens {
        let (token_style, token_width, token_ascent, token_descent) = match token {
            InlineToken::Word {
                start,
                end,
                width,
                style,
                ascent,
                descent,
            } => {
                text.push_str(&text_pool[*start..*end]);
                (*style, *width, *ascent, *descent)
            }
            InlineToken::Space {
                text: character,
                width,
                style,
                ascent,
                descent,
                ..
            } => {
                if *character != '\0' {
                    let mut buffer = [0u8; 4];
                    text.push_str(character.encode_utf8(&mut buffer));
                }
                (*style, *width, *ascent, *descent)
            }
            InlineToken::HyphenationPoint { style } => (*style, 0.0, 0.0, 0.0),
            _ => return None,
        };
        match style_id {
            Some(existing) if existing != token_style => return None,
            Some(_) => {}
            None => style_id = Some(token_style),
        }
        width += token_width;
        ascent = ascent.max(token_ascent);
        descent = descent.max(token_descent);
    }
    let style_id = style_id?;
    let max_ascent = ctx.strut_ascent.max(ascent);
    let max_descent = ctx.strut_descent.max(descent);
    let line_height = max_ascent + max_descent;
    let remaining_width = (ctx.container_width - width).max(0.0);
    let align_offset = match ctx.container_style.text_align {
        TextAlign::Left | TextAlign::Justify => 0.0,
        TextAlign::Right => remaining_width,
        TextAlign::Center => remaining_width / 2.0,
    };
    let box_h = ascent + descent;
    let y = ctx.line_y + (max_ascent - ascent).clamp(0.0, (line_height - box_h).max(0.0));
    let (text, width) = ellipsize_text(
        &text,
        width,
        styles[style_id as usize].as_ref(),
        (ctx.container_width - align_offset).max(0.0),
    );
    let mut text_box = LayoutBox::with_shared_style(
        BoxType::TextNode(text),
        Some(styles[style_id as usize].clone()),
    );
    text_box.dimensions.content = Rect::new(ctx.container_x + align_offset, y, width, box_h);
    text_box.baseline = ascent;
    let mut line_box = LayoutBox::new(BoxType::LineBox, None);
    line_box.dimensions.content = Rect::new(
        ctx.container_x,
        ctx.line_y,
        ctx.container_width,
        line_height,
    );
    line_box.baseline = max_ascent;
    line_box.children = vec![text_box];
    Some(line_box)
}

/// Positions a text run on the line and appends it to the innermost active box (or to
/// the line itself), merging with the previous fragment when possible.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_text_fragment(
    text: &str,
    width: f32,
    style_id: u32,
    styles: &[Arc<ComputedStyle>],
    ascent: f32,
    descent: f32,
    active_stack: &mut [ActiveBoxFrame],
    line_children: &mut Vec<LayoutBox>,
    curr_x: &mut f32,
    ctx: &LineContext<'_>,
    line_height: f32,
    baseline_y_in_line: f32,
) {
    let style = styles[style_id as usize].as_ref();
    let available = (ctx.container_width - *curr_x).max(0.0);
    let (text, width) = ellipsize_text(text, width, style, available);
    let box_h = ascent + descent;

    // Vertical alignment comes from the innermost inline box; text runs themselves
    // have no `vertical-align`. Inside a `top`/`bottom` anchored subtree the text is
    // placed relative to the subtree's own baseline.
    let anchor_pos = active_stack
        .iter()
        .rposition(|frame| frame.anchor.is_some());
    let innermost_align = active_stack
        .last()
        .map(|frame| frame.style.vertical_align)
        .unwrap_or(VerticalAlign::Baseline);
    let vertical_align = match anchor_pos {
        Some(position) if position + 1 == active_stack.len() => VerticalAlign::Baseline,
        _ => innermost_align,
    };
    let (a_prime, _) = align_extents(
        vertical_align,
        ascent,
        descent,
        ctx.container_style.font_size,
        ctx.strut_ascent,
        ctx.strut_descent,
    );

    let y = match anchor_pos.and_then(|position| active_stack[position].anchor) {
        Some((_, anchor_baseline, _)) => (anchor_baseline - a_prime).clamp(
            ctx.line_y,
            (ctx.line_y + line_height - box_h).max(ctx.line_y),
        ),
        None => {
            ctx.line_y + (baseline_y_in_line - a_prime).clamp(0.0, (line_height - box_h).max(0.0))
        }
    };

    let target_list = if let Some(active) = active_stack.last_mut() {
        &mut active.children
    } else {
        line_children
    };

    if let Some(last) = target_list.last_mut()
        && let BoxType::TextNode(ref mut last_text) = last.box_type
        && last.style.as_deref() == Some(style)
        && (last.baseline - a_prime).abs() <= 0.01
        && (last.dimensions.content.y - y).abs() <= 0.01
    {
        last_text.push_str(&text);
        last.dimensions.content.width += width;
        *curr_x += width;
        return;
    }

    let mut text_box = LayoutBox::with_shared_style(
        BoxType::TextNode(text),
        Some(styles[style_id as usize].clone()),
    );
    text_box.dimensions.content.x = *curr_x;
    text_box.dimensions.content.y = y;
    text_box.dimensions.content.width = width;
    text_box.dimensions.content.height = box_h;
    // Distance from the top of this fragment to the glyph baseline (paint places
    // glyphs at `content.y + baseline`).
    text_box.baseline = a_prime;

    *curr_x += width;
    target_list.push(text_box);
}

/// Closes the inline boxes still open at a line break (split across lines).
///
/// Interior split fragments have zero right edges per CSS 2.2 §9.4.2; the surviving
/// box metadata is returned to reopen the box on the next line.
pub(crate) fn close_remaining_boxes(
    active_stack: &mut Vec<ActiveBoxFrame>,
    line_children: &mut Vec<LayoutBox>,
    curr_x: f32,
    container_width: f32,
    line_y: f32,
    line_height: f32,
) -> Vec<ActiveBoxInfo> {
    let mut outgoing_active = Vec::new();
    while let Some(frame) = active_stack.pop() {
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

        // Interior split: right edges are 0.0 per CSS 2.2 §9.4.2
        let box_w = (curr_x - (frame.start_x + left_margin + left_border + left_padding)).max(0.0);

        let mut inline_box = LayoutBox::with_shared_style(
            BoxType::InlineNode(frame.node_id),
            Some(frame.style.clone()),
        );
        inline_box.is_pseudo = frame.is_pseudo;
        inline_box.dimensions.margin.left = left_margin;
        inline_box.dimensions.margin.right = 0.0;
        inline_box.dimensions.border.left = left_border;
        inline_box.dimensions.border.right = 0.0;
        inline_box.dimensions.padding.left = left_padding;
        inline_box.dimensions.padding.right = 0.0;

        inline_box.dimensions.content.x = frame.start_x + left_margin + left_border + left_padding;
        inline_box.dimensions.content.y = frame.anchor.map_or(line_y, |(top, _, _)| top);
        inline_box.dimensions.content.width = box_w;
        inline_box.dimensions.content.height =
            frame.anchor.map_or(line_height, |(_, _, height)| height);
        inline_box.children = frame.children;

        outgoing_active.push(ActiveBoxInfo {
            node_id: frame.node_id,
            style: frame.style,
            is_first_fragment: false,
            is_pseudo: frame.is_pseudo,
        });

        if let Some(parent) = active_stack.last_mut() {
            parent.children.push(inline_box);
        } else {
            line_children.push(inline_box);
        }
    }
    outgoing_active.reverse();
    outgoing_active
}
