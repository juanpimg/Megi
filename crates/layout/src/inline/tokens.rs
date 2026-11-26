//! Inline token stream, style table and text-node tokenization.
use super::tokenize_text::tokenize_text;
use super::*;

/// Pushes a style handle into the per-IFC table and returns its id.
fn push_style(styles: &mut Vec<Arc<ComputedStyle>>, style: Arc<ComputedStyle>) -> u32 {
    let id = styles.len() as u32;
    styles.push(style);
    id
}

/// Resolves a CSS [`Length`] (and optional `calc()` value) into pixels relative to containing block width.
pub(crate) fn resolve_length(
    len: Length,
    calc: Option<megi_css::calc::CalcValue>,
    cb_width: f32,
) -> f32 {
    crate::calc::length(len, calc, cb_width)
}

/// Recursively flattens inline boxes into [`InlineToken`]s.
///
/// Atomic inline-level boxes (`inline-block`, replaced elements) are laid out eagerly
/// at a provisional origin and emitted as [`InlineToken::Atomic`]: the line breaker
/// treats them as unbreakable units with a known margin box and baseline.
#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_inline_tokens(
    boxes: &mut [LayoutBox],
    measurer: &dyn TextMeasurer,
    text_pool: &mut String,
    styles: &mut Vec<Arc<ComputedStyle>>,
    tokens: &mut Vec<InlineToken>,
    container_width: f32,
    definite_height: Option<f32>,
    pos: crate::block::PositioningContext,
    out_of_flow: &mut Vec<LayoutBox>,
) {
    for child in boxes.iter_mut() {
        // CSS 2.2 §9.6/§10.3.7: an absolutely positioned box with a definite
        // `left`/`right` is placed against its containing block and must not advance
        // the inline cursor. Left in the token stream it becomes an atomic item after
        // the preceding text, widening the parent inline box and shifting decorations
        // such as `a::after { position:absolute; left:0; width:100% }`. Boxes with auto
        // horizontal offsets keep their static in-flow position (and their height
        // contribution), which also preserves existing line-box geometry.
        let is_out_of_flow = child.style.as_ref().is_some_and(|style| {
            matches!(
                style.position,
                megi_css::Position::Absolute | megi_css::Position::Fixed
            ) && (!matches!(style.left, Length::Auto) || !matches!(style.right, Length::Auto))
        });
        if is_out_of_flow {
            out_of_flow.push(std::mem::replace(
                child,
                LayoutBox::new(BoxType::LineBreak, None),
            ));
            continue;
        }
        match &child.box_type {
            BoxType::InlineNode(node_id) => {
                let node_id = *node_id;
                let is_pseudo = child.is_pseudo;
                // The inline box is consumed into tokens; move its style instead of
                // cloning it (the surviving fragments re-use the same `Arc`).
                let style = child
                    .style
                    .take()
                    .unwrap_or_else(|| Arc::new(ComputedStyle::initial()));
                let style_id = push_style(styles, style);
                tokens.push(InlineToken::BoxStart {
                    node_id,
                    style: style_id,
                    is_pseudo,
                });
                collect_inline_tokens(
                    &mut child.children,
                    measurer,
                    text_pool,
                    styles,
                    tokens,
                    container_width,
                    definite_height,
                    pos,
                    out_of_flow,
                );
                tokens.push(InlineToken::BoxEnd {
                    node_id,
                    style: style_id,
                    is_pseudo,
                });
            }
            BoxType::TextNode(txt) => {
                // The text box is a formatting artifact consumed into tokens; moving its
                // style avoids one `ComputedStyle` clone per text node.
                let style = child
                    .style
                    .take()
                    .unwrap_or_else(|| Arc::new(ComputedStyle::initial()));
                let style_id = push_style(styles, style);
                tokenize_text(
                    txt,
                    &styles[style_id as usize],
                    style_id,
                    measurer,
                    text_pool,
                    tokens,
                );
            }
            // `<br>` forces a line break (CSS 2.2 §16.6.1). It has no width and no edges.
            BoxType::LineBreak => {
                tokens.push(InlineToken::ForcedBreak);
            }
            BoxType::InlineBlock(_)
            | BoxType::Replaced(_)
            | BoxType::InlineTable(_)
            | BoxType::BlockNode(_)
            | BoxType::Table(_) => {
                let atomic = crate::block::layout_atomic_inline(
                    std::mem::replace(child, LayoutBox::new(BoxType::LineBreak, None)),
                    container_width,
                    definite_height,
                    measurer,
                    pos,
                );
                let vertical_align = atomic
                    .layout_box
                    .style
                    .as_ref()
                    .map_or(VerticalAlign::Baseline, |style| style.vertical_align);
                tokens.push(InlineToken::Atomic {
                    layout_box: Box::new(atomic.layout_box),
                    width: atomic.width,
                    height: atomic.height,
                    baseline: atomic.baseline,
                    vertical_align,
                });
            }
            _ => {}
        }
    }
}
