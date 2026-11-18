//! Line breaking at soft wrap opportunities (CSS 2.2 §16.6.1).
use super::*;

/// A formatted line produced by [`break_into_lines`].
pub(crate) struct Line {
    /// Tokens that belong to this line (inline box fragments are split later by
    /// [`format_line`]).
    pub(crate) tokens: Vec<InlineToken>,
    /// `true` when the line ended at a soft wrap opportunity (and is therefore a
    /// candidate for `text-align: justify`; the last line and lines before a forced
    /// break are never justified, CSS 2.2 §16.2).
    pub(crate) justified: bool,
}

/// How a soft wrap opportunity materializes when the line is broken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SoftWrapKind {
    /// Break after a space or after an explicit hyphen (nothing extra is rendered).
    AtSpace,
    /// Break at a soft hyphen: a rendered `-` is appended to the line.
    AtSoftHyphen,
}

/// Advance contributed by a token to the current line width.
pub(crate) fn token_advance(
    token: &InlineToken,
    styles: &[Arc<ComputedStyle>],
    container_width: f32,
) -> f32 {
    match token {
        InlineToken::Word { width, .. }
        | InlineToken::Space { width, .. }
        | InlineToken::Atomic { width, .. } => *width,
        InlineToken::BoxStart { style, .. } => {
            let style = &styles[*style as usize];
            resolve_length(
                style.margin.left,
                style.calc_lengths.margin_left,
                container_width,
            ) + style.border_width.left
                + resolve_length(
                    style.padding.left,
                    style.calc_lengths.padding_left,
                    container_width,
                )
        }
        InlineToken::BoxEnd { style, .. } => {
            let style = &styles[*style as usize];
            resolve_length(
                style.margin.right,
                style.calc_lengths.margin_right,
                container_width,
            ) + style.border_width.right
                + resolve_length(
                    style.padding.right,
                    style.calc_lengths.padding_right,
                    container_width,
                )
        }
        InlineToken::HyphenationPoint { .. } | InlineToken::ForcedBreak => 0.0,
    }
}

fn trim_trailing_collapsible_spaces(line: &mut Vec<InlineToken>) {
    // Box ends and soft-hyphen markers are invisible and stay on the line; only
    // collapsible spaces are removed. Removing them from the back one by one shifted the
    // invisible suffix after every removal (quadratic on `word + (shy^n + space)^m`); find
    // the last visible token and drop the trailing spaces in one retain pass.
    let last_visible = line.iter().rposition(|token| {
        !matches!(
            token,
            InlineToken::BoxEnd { .. }
                | InlineToken::HyphenationPoint { .. }
                | InlineToken::Space {
                    collapsible: true,
                    ..
                }
        )
    });
    let start = last_visible.map_or(0, |index| index + 1);
    let mut index = 0;
    line.retain(|token| {
        let keep = index < start
            || !matches!(
                token,
                InlineToken::Space {
                    collapsible: true,
                    ..
                }
            );
        index += 1;
        keep
    });
}

/// Trims leading and trailing collapsible spaces from a line before it is committed
/// (CSS Text 3 §4.1: collapsible spaces at the start/end of a line are removed).
fn finalize_line(mut line: Vec<InlineToken>) -> Vec<InlineToken> {
    // Drain the leading run once instead of removing from the front one by one: a long
    // run of soft hyphens (or collapsible spaces) is a single zero-width line, and the
    // repeated `remove(0)` was quadratic.
    let leading = line
        .iter()
        .take_while(|token| {
            matches!(
                token,
                InlineToken::Space {
                    collapsible: true,
                    ..
                } | InlineToken::HyphenationPoint { .. }
            )
        })
        .count();
    line.drain(..leading);
    trim_trailing_collapsible_spaces(&mut line);
    line
}

/// Finds the last soft wrap opportunity in `current` that leaves visible content on
/// the line, returning `(token index, kind, style)`.
/// Finds the last soft wrap opportunity in `current` that leaves visible content on
/// the line, returning `(token index, kind)`.
///
/// `first_visible` is the index of the first token that produces visible content
/// (word or atomic box), maintained incrementally by [`break_into_lines`], so the
/// visible-content check is O(1) instead of rescanning the line prefix.
fn last_soft_wrap(
    current: &std::collections::VecDeque<InlineToken>,
    first_visible: Option<usize>,
) -> Option<(usize, SoftWrapKind)> {
    for index in (0..current.len()).rev() {
        if !first_visible.is_some_and(|first| first < index) {
            // No visible content before this index; continue only while there could
            // still be an earlier visible token.
            first_visible?;
            continue;
        }
        match &current[index] {
            InlineToken::HyphenationPoint { .. } => {
                return Some((index, SoftWrapKind::AtSoftHyphen));
            }
            InlineToken::Space {
                allow_wrap: true, ..
            } => {
                return Some((index, SoftWrapKind::AtSpace));
            }
            _ => {}
        }
    }
    None
}

/// Returns the index of the first visible token in `line`.
fn first_visible_token(line: &std::collections::VecDeque<InlineToken>) -> Option<usize> {
    line.iter().position(|token| match token {
        InlineToken::Word { start, end, .. } => end > start,
        InlineToken::Atomic { .. } => true,
        _ => false,
    })
}

/// Splits the token stream into lines at soft wrap opportunities (CSS 2.2 §16.6.1).
///
/// A line break is only inserted where the content provides a break opportunity:
/// collapsible/preserved spaces, explicit hyphens and soft hyphens. Word fragments
/// coming from adjacent inline boxes (`<span>a</span><span>b</span>`) provide no
/// opportunity by themselves, so they stay in the same line box even if that line
/// overflows (audit finding F-09).
///
/// Soft hyphens are invisible until a line actually breaks at them; when that happens
/// a rendered `-` is appended to the line and the next line continues after the
/// hyphenation point (audit finding F-15).
///
/// The implementation is streaming: tokens are moved into the current line and, when a
/// break happens, the line prefix is drained (no per-token clones). The current width
/// and the first visible token index are maintained incrementally, so the whole pass
/// is linear in the number of tokens.
pub(crate) fn break_into_lines(
    tokens: Vec<InlineToken>,
    text_pool: &mut String,
    styles: &[Arc<ComputedStyle>],
    container_width: f32,
    measurer: &dyn TextMeasurer,
) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::with_capacity(tokens.len() / 16 + 1);
    if tokens.is_empty() {
        return lines;
    }

    let mut current: std::collections::VecDeque<InlineToken> =
        std::collections::VecDeque::with_capacity(32);
    let mut current_width = 0.0_f32;
    let mut first_visible: Option<usize> = None;

    for token in tokens {
        // A forced break always ends the current line, even if it overflows.
        if matches!(token, InlineToken::ForcedBreak) {
            let line_tokens = finalize_line(std::mem::take(&mut current).into());
            lines.push(Line {
                tokens: line_tokens,
                justified: false,
            });
            current_width = 0.0;
            first_visible = None;
            continue;
        }

        let visible = match &token {
            InlineToken::Word { start, end, .. } => end > start,
            InlineToken::Atomic { .. } => true,
            _ => false,
        };
        if visible && first_visible.is_none() {
            first_visible = Some(current.len());
        }
        current_width += token_advance(&token, styles, container_width);
        current.push_back(token);

        // Wrap greedily at the last valid soft wrap opportunity when the line overflows.
        while current_width > container_width {
            let Some((break_index, kind)) = last_soft_wrap(&current, first_visible) else {
                break;
            };
            let mut line_tokens: Vec<InlineToken> = Vec::with_capacity(break_index + 1);
            let mut removed_width = 0.0_f32;
            for _ in 0..break_index {
                if let Some(removed) = current.pop_front() {
                    removed_width += token_advance(&removed, styles, container_width);
                    line_tokens.push(removed);
                }
            }
            let Some(opportunity) = current.pop_front() else {
                break;
            };
            removed_width += token_advance(&opportunity, styles, container_width);
            if kind == SoftWrapKind::AtSoftHyphen {
                // Materialize the soft hyphen as a rendered '-' at the end of the line.
                if let InlineToken::HyphenationPoint { style } = opportunity {
                    let start = text_pool.len();
                    text_pool.push('-');
                    let hyphen_style = &styles[style as usize];
                    let width = shaped_width(measurer, &text_pool[start..], hyphen_style);
                    let (ascent, descent) =
                        line_box_extents(&metrics_styled(measurer, hyphen_style));
                    line_tokens.push(InlineToken::Word {
                        start,
                        end: text_pool.len(),
                        width,
                        style,
                        ascent,
                        descent,
                    });
                }
            } else if let InlineToken::Space {
                text,
                width,
                style,
                ascent,
                descent,
                collapsible,
                allow_wrap,
            } = opportunity
            {
                line_tokens.push(InlineToken::Space {
                    text,
                    width,
                    style,
                    ascent,
                    descent,
                    collapsible,
                    allow_wrap,
                });
            }
            current_width = (current_width - removed_width).max(0.0);
            let line_tokens = finalize_line(line_tokens);
            lines.push(Line {
                tokens: line_tokens,
                justified: true,
            });
            // Update the first visible token index after removing `break_index + 1`
            // tokens from the front of `current`.
            let removed_count = break_index + 1;
            first_visible = match first_visible {
                Some(index) if index >= removed_count => Some(index - removed_count),
                _ => first_visible_token(&current),
            };
        }
    }

    // Final line of the paragraph: never justified (CSS 2.2 §16.2). A final line that
    // collapses to nothing (e.g. a text node made only of spaces) generates no line box
    // (CSS 2.2 §9.4.2).
    if !current.is_empty() {
        let line_tokens = finalize_line(current.into());
        if !line_tokens.is_empty() {
            lines.push(Line {
                tokens: line_tokens,
                justified: false,
            });
        }
    }

    lines
}
