//! Content-based automatic minimum size of flex items.
//!
//! CSS Flexbox 1 §4.5: for a row flex item whose `min-width` is `auto` and that
//! is not a scroll container, the automatic minimum size is its min-content
//! contribution (CSS Sizing 3 §5). The contribution is capped by the space left
//! on the line so a genuinely unbreakable word still wraps instead of pushing
//! the line past the viewport.

use super::*;

/// Fills `min_content_main` (uncapped) and `auto_min_main` (capped) for every
/// eligible item in each line.
pub(super) fn resolve_automatic_minimums(
    items: &mut [Item<'_>],
    lines: &[Vec<usize>],
    row: bool,
    container_main: f32,
    gap: f32,
    measurer: &dyn TextMeasurer,
) {
    if !row {
        return;
    }
    for line in lines {
        for &index in line {
            let item = &items[index];
            let min_width_auto = matches!(item.style.min_width, Length::Auto)
                && item.style.calc_lengths.min_width.is_none();
            let scroll_container =
                !matches!(item.style.overflow, Overflow::Visible | Overflow::Clip);
            if !min_width_auto || scroll_container {
                continue;
            }
            let min_content = crate::block::intrinsic_content_widths(
                items[index].child,
                container_main,
                measurer,
            )
            .0
            .max(0.0);
            let others: f32 = line
                .iter()
                .filter(|&&other| other != index)
                .map(|&other| items[other].outer_main(row))
                .sum();
            let gaps = gap * (line.len().saturating_sub(1) as f32);
            let cap = (container_main - others - gaps - item.edges_main(row)).max(0.0);
            let item = &mut items[index];
            item.min_content_main = min_content;
            item.auto_min_main = min_content.min(cap);
        }
    }
}
