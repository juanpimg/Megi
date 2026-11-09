//! # Inline Formatting Context (IFC) Layout Engine
//!
//! Normative implementation of W3C CSS 2.1 §9.4.2 (Inline formatting contexts),
//! §10.8 (Line height calculations, strut, and vertical-align), and
//! CSS Text Module Level 3 (§3–§5: white-space processing and line breaking).

use css::{ComputedStyle, Length, TextAlign, VerticalAlign, WhiteSpace};
use dom::NodeId;

use crate::box_tree::{BoxType, LayoutBox};
use crate::geometry::Rect;

/// Typographic font metrics for text measurement and baseline alignment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub font_size: f32,
    pub ascent: f32,
    pub descent: f32,
    pub x_height: f32,
    pub line_height: f32,
}

impl FontMetrics {
    /// Constructs font metrics based on font size and optional computed line height.
    ///
    /// Standard typographic ratios:
    /// - `ascent = 0.8 * font_size`
    /// - `descent = 0.2 * font_size`
    /// - `x_height = 0.5 * font_size`
    /// - default `line_height = 1.2 * font_size`
    pub fn new(font_size: f32, computed_line_height: Option<f32>) -> Self {
        let ascent = 0.8 * font_size;
        let descent = 0.2 * font_size;
        let x_height = 0.5 * font_size;
        let line_height = computed_line_height.unwrap_or(1.2 * font_size);
        Self {
            font_size,
            ascent,
            descent,
            x_height,
            line_height,
        }
    }
}

/// Abstract contract for measuring text advance and querying typographic metrics.
pub trait TextMeasurer {
    /// Measures the horizontal advance width of a text string in pixels.
    fn measure_text(&self, text: &str, font_family: &str, font_size: f32) -> f32;

    /// Computes font metrics for a font family, size, and optional computed line height.
    fn font_metrics(
        &self,
        font_family: &str,
        font_size: f32,
        computed_line_height: Option<f32>,
    ) -> FontMetrics;
}

/// Default deterministic text measurer implementing standard CSS typographic ratios.
///
/// Guarantees 100% cross-platform repeatability in tests:
/// - Monospace fonts: `0.6 * font_size` advance per character.
/// - Proportional fonts: `0.5 * font_size` advance per character.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultTextMeasurer;

impl TextMeasurer for DefaultTextMeasurer {
    fn measure_text(&self, text: &str, font_family: &str, font_size: f32) -> f32 {
        let is_monospace = font_family.eq_ignore_ascii_case("monospace")
            || font_family.eq_ignore_ascii_case("courier")
            || font_family.eq_ignore_ascii_case("consolas")
            || font_family.eq_ignore_ascii_case("ui-monospace");

        let advance = if is_monospace {
            0.6 * font_size
        } else {
            0.5 * font_size
        };

        text.chars().count() as f32 * advance
    }

    fn font_metrics(
        &self,
        _font_family: &str,
        font_size: f32,
        computed_line_height: Option<f32>,
    ) -> FontMetrics {
        FontMetrics::new(font_size, computed_line_height)
    }
}

/// An atomic item in the linearized inline content stream.
#[derive(Clone, Debug)]
enum InlineToken {
    /// Start of an inline container element (`<span>`).
    BoxStart {
        node_id: NodeId,
        style: ComputedStyle,
    },
    /// End of an inline container element.
    BoxEnd {
        node_id: NodeId,
        style: ComputedStyle,
    },
    /// An unbroken word run.
    Word {
        text: String,
        width: f32,
        style: ComputedStyle,
    },
    /// A whitespace run (collapsible or preserved).
    Space {
        text: String,
        width: f32,
        style: ComputedStyle,
        collapsible: bool,
        allow_wrap: bool,
    },
    /// A forced line break (`\n` in pre/pre-wrap/pre-line or `<br>`).
    ForcedBreak { style: ComputedStyle },
}

/// Resolves a CSS [`Length`] into pixels relative to containing block width.
fn resolve_length(len: Length, cb_width: f32) -> f32 {
    match len {
        Length::Px(px) => px,
        Length::Percent(pct) => cb_width * (pct / 100.0),
        Length::Auto => 0.0,
    }
}

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
    let container_width = layout_box.dimensions.content.width;
    let container_x = layout_box.dimensions.content.x;
    let container_y = layout_box.dimensions.content.y;
    let container_style = layout_box
        .style
        .clone()
        .unwrap_or_else(ComputedStyle::initial);

    // 1. Flatten children into a sequential stream of inline tokens
    let mut tokens = Vec::new();
    collect_inline_tokens(&layout_box.children, measurer, &mut tokens);

    if tokens.is_empty() {
        layout_box.children.clear();
        return;
    }

    // 2. Break tokens into lines using greedy line breaking
    let lines = break_into_lines(tokens, container_width);

    if lines.is_empty() {
        layout_box.children.clear();
        return;
    }

    // 3. Strut metrics for the containing block container per CSS 2.1 §10.8
    let strut_metrics = measurer.font_metrics(
        &container_style.font_family,
        container_style.font_size,
        container_style.line_height,
    );
    let strut_leading = strut_metrics.line_height - (strut_metrics.ascent + strut_metrics.descent);
    let strut_ascent = strut_metrics.ascent + strut_leading / 2.0;
    let strut_descent = strut_metrics.descent + strut_leading / 2.0;

    // 4. Construct LineBox layout boxes and resolve baseline / vertical-align
    let mut line_boxes = Vec::new();
    let mut current_line_y = container_y;

    // Track active inline boxes across line breaks for box splitting
    let mut persistent_active_boxes: Vec<ActiveBoxInfo> = Vec::new();

    for line_tokens in lines {
        let ctx = LineContext {
            container_x,
            line_y: current_line_y,
            container_width,
            container_style: &container_style,
            strut_ascent,
            strut_descent,
        };
        let (line_box, new_active) =
            format_line(line_tokens, &ctx, measurer, persistent_active_boxes);

        current_line_y += line_box.dimensions.content.height;
        persistent_active_boxes = new_active;
        line_boxes.push(line_box);
    }

    layout_box.children = line_boxes;
}

/// Recursively flattens inline boxes into [`InlineToken`]s.
fn collect_inline_tokens(
    boxes: &[LayoutBox],
    measurer: &dyn TextMeasurer,
    tokens: &mut Vec<InlineToken>,
) {
    for child in boxes {
        match &child.box_type {
            BoxType::InlineNode(node_id) => {
                let style = child.style.clone().unwrap_or_else(ComputedStyle::initial);
                tokens.push(InlineToken::BoxStart {
                    node_id: *node_id,
                    style: style.clone(),
                });
                collect_inline_tokens(&child.children, measurer, tokens);
                tokens.push(InlineToken::BoxEnd {
                    node_id: *node_id,
                    style,
                });
            }
            BoxType::TextNode(txt) => {
                let style = child.style.clone().unwrap_or_else(ComputedStyle::initial);
                tokenize_text(txt, &style, measurer, tokens);
            }
            // `<br>` forces a line break (CSS 2.2 §16.6.1). It has no width and no edges.
            BoxType::LineBreak => {
                tokens.push(InlineToken::ForcedBreak {
                    style: ComputedStyle::initial(),
                });
            }
            _ => {}
        }
    }
}

/// Tokenizes text into words, spaces, and forced breaks according to `white_space`.
fn tokenize_text(
    txt: &str,
    style: &ComputedStyle,
    measurer: &dyn TextMeasurer,
    tokens: &mut Vec<InlineToken>,
) {
    match style.white_space {
        WhiteSpace::Normal | WhiteSpace::Nowrap => {
            let allow_wrap = style.white_space == WhiteSpace::Normal;
            let mut word_buf = String::new();
            let mut saw_space = false;

            for ch in txt.chars() {
                if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
                    if !word_buf.is_empty() {
                        let w =
                            measurer.measure_text(&word_buf, &style.font_family, style.font_size);
                        tokens.push(InlineToken::Word {
                            text: std::mem::take(&mut word_buf),
                            width: w,
                            style: style.clone(),
                        });
                    }
                    saw_space = true;
                } else {
                    if saw_space {
                        let space_str = " ".to_string();
                        let sw =
                            measurer.measure_text(&space_str, &style.font_family, style.font_size);
                        tokens.push(InlineToken::Space {
                            text: space_str,
                            width: sw,
                            style: style.clone(),
                            collapsible: true,
                            allow_wrap,
                        });
                        saw_space = false;
                    }
                    word_buf.push(ch);
                    if allow_wrap && (ch == '-' || ch == '\u{00AD}') {
                        let w =
                            measurer.measure_text(&word_buf, &style.font_family, style.font_size);
                        tokens.push(InlineToken::Word {
                            text: std::mem::take(&mut word_buf),
                            width: w,
                            style: style.clone(),
                        });
                        tokens.push(InlineToken::Space {
                            text: String::new(),
                            width: 0.0,
                            style: style.clone(),
                            collapsible: false,
                            allow_wrap: true,
                        });
                    }
                }
            }

            if !word_buf.is_empty() {
                let w = measurer.measure_text(&word_buf, &style.font_family, style.font_size);
                tokens.push(InlineToken::Word {
                    text: word_buf,
                    width: w,
                    style: style.clone(),
                });
            }
            if saw_space {
                let space_str = " ".to_string();
                let sw = measurer.measure_text(&space_str, &style.font_family, style.font_size);
                tokens.push(InlineToken::Space {
                    text: space_str,
                    width: sw,
                    style: style.clone(),
                    collapsible: true,
                    allow_wrap,
                });
            }
        }
        WhiteSpace::Pre => {
            let lines: Vec<&str> = txt.split('\n').collect();
            for (idx, line) in lines.iter().enumerate() {
                if idx > 0 {
                    tokens.push(InlineToken::ForcedBreak {
                        style: style.clone(),
                    });
                }
                if !line.is_empty() {
                    let w = measurer.measure_text(line, &style.font_family, style.font_size);
                    tokens.push(InlineToken::Word {
                        text: line.to_string(),
                        width: w,
                        style: style.clone(),
                    });
                }
            }
        }
        WhiteSpace::PreWrap => {
            let lines: Vec<&str> = txt.split('\n').collect();
            for (idx, line) in lines.iter().enumerate() {
                if idx > 0 {
                    tokens.push(InlineToken::ForcedBreak {
                        style: style.clone(),
                    });
                }
                let mut word_buf = String::new();
                for ch in line.chars() {
                    if ch == ' ' || ch == '\t' {
                        if !word_buf.is_empty() {
                            let w = measurer.measure_text(
                                &word_buf,
                                &style.font_family,
                                style.font_size,
                            );
                            tokens.push(InlineToken::Word {
                                text: std::mem::take(&mut word_buf),
                                width: w,
                                style: style.clone(),
                            });
                        }
                        let space_str = ch.to_string();
                        let sw =
                            measurer.measure_text(&space_str, &style.font_family, style.font_size);
                        tokens.push(InlineToken::Space {
                            text: space_str,
                            width: sw,
                            style: style.clone(),
                            collapsible: false,
                            allow_wrap: true,
                        });
                    } else {
                        word_buf.push(ch);
                        if ch == '-' || ch == '\u{00AD}' {
                            let w = measurer.measure_text(
                                &word_buf,
                                &style.font_family,
                                style.font_size,
                            );
                            tokens.push(InlineToken::Word {
                                text: std::mem::take(&mut word_buf),
                                width: w,
                                style: style.clone(),
                            });
                            tokens.push(InlineToken::Space {
                                text: String::new(),
                                width: 0.0,
                                style: style.clone(),
                                collapsible: false,
                                allow_wrap: true,
                            });
                        }
                    }
                }
                if !word_buf.is_empty() {
                    let w = measurer.measure_text(&word_buf, &style.font_family, style.font_size);
                    tokens.push(InlineToken::Word {
                        text: word_buf,
                        width: w,
                        style: style.clone(),
                    });
                }
            }
        }
        WhiteSpace::PreLine => {
            let lines: Vec<&str> = txt.split('\n').collect();
            for (idx, line) in lines.iter().enumerate() {
                if idx > 0 {
                    tokens.push(InlineToken::ForcedBreak {
                        style: style.clone(),
                    });
                }
                let mut word_buf = String::new();
                let mut saw_space = false;
                for ch in line.chars() {
                    if ch == ' ' || ch == '\t' || ch == '\r' {
                        if !word_buf.is_empty() {
                            let w = measurer.measure_text(
                                &word_buf,
                                &style.font_family,
                                style.font_size,
                            );
                            tokens.push(InlineToken::Word {
                                text: std::mem::take(&mut word_buf),
                                width: w,
                                style: style.clone(),
                            });
                        }
                        saw_space = true;
                    } else {
                        if saw_space {
                            let space_str = " ".to_string();
                            let sw = measurer.measure_text(
                                &space_str,
                                &style.font_family,
                                style.font_size,
                            );
                            tokens.push(InlineToken::Space {
                                text: space_str,
                                width: sw,
                                style: style.clone(),
                                collapsible: true,
                                allow_wrap: true,
                            });
                            saw_space = false;
                        }
                        word_buf.push(ch);
                        if ch == '-' || ch == '\u{00AD}' {
                            let w = measurer.measure_text(
                                &word_buf,
                                &style.font_family,
                                style.font_size,
                            );
                            tokens.push(InlineToken::Word {
                                text: std::mem::take(&mut word_buf),
                                width: w,
                                style: style.clone(),
                            });
                            tokens.push(InlineToken::Space {
                                text: String::new(),
                                width: 0.0,
                                style: style.clone(),
                                collapsible: false,
                                allow_wrap: true,
                            });
                        }
                    }
                }
                if !word_buf.is_empty() {
                    let w = measurer.measure_text(&word_buf, &style.font_family, style.font_size);
                    tokens.push(InlineToken::Word {
                        text: word_buf,
                        width: w,
                        style: style.clone(),
                    });
                }
                if saw_space {
                    let space_str = " ".to_string();
                    let sw = measurer.measure_text(&space_str, &style.font_family, style.font_size);
                    tokens.push(InlineToken::Space {
                        text: space_str,
                        width: sw,
                        style: style.clone(),
                        collapsible: true,
                        allow_wrap: true,
                    });
                }
            }
        }
    }
}

/// Splits inline tokens into lines using greedy wrapping at soft wrap opportunities.
fn break_into_lines(tokens: Vec<InlineToken>, container_width: f32) -> Vec<Vec<InlineToken>> {
    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut current_line_width = 0.0;

    for token in tokens {
        match token {
            InlineToken::ForcedBreak { style } => {
                // Trim trailing collapsible spaces before ending line
                trim_trailing_collapsible_spaces(&mut current_line, &mut current_line_width);
                lines.push(std::mem::take(&mut current_line));
                current_line_width = 0.0;
                let _ = style;
            }
            InlineToken::BoxStart { node_id, style } => {
                let left_edge = resolve_length(style.margin.left, container_width)
                    + style.border_width.left
                    + resolve_length(style.padding.left, container_width);
                current_line_width += left_edge;
                current_line.push(InlineToken::BoxStart { node_id, style });
            }
            InlineToken::BoxEnd { node_id, style } => {
                let right_edge = resolve_length(style.margin.right, container_width)
                    + style.border_width.right
                    + resolve_length(style.padding.right, container_width);
                current_line_width += right_edge;
                current_line.push(InlineToken::BoxEnd { node_id, style });
            }
            InlineToken::Space {
                text,
                width,
                style,
                collapsible,
                allow_wrap,
            } => {
                // Trim leading collapsible spaces on line
                if collapsible && !line_has_visible_content(&current_line) {
                    continue;
                }
                current_line_width += width;
                current_line.push(InlineToken::Space {
                    text,
                    width,
                    style,
                    collapsible,
                    allow_wrap,
                });
            }
            InlineToken::Word { text, width, style } => {
                let can_wrap =
                    style.white_space != WhiteSpace::Nowrap && style.white_space != WhiteSpace::Pre;
                let line_has_content = line_has_visible_content(&current_line);
                let fits = current_line_width + width <= container_width;

                if fits || !line_has_content || !can_wrap {
                    // Fits on current line, or unbreakable first word, or nowrap
                    current_line_width += width;
                    current_line.push(InlineToken::Word { text, width, style });
                } else {
                    // Wrap to new line: trim trailing spaces from current line
                    trim_trailing_collapsible_spaces(&mut current_line, &mut current_line_width);
                    lines.push(std::mem::take(&mut current_line));
                    current_line_width = width;
                    current_line.push(InlineToken::Word { text, width, style });
                }
            }
        }
    }

    if !current_line.is_empty() {
        trim_trailing_collapsible_spaces(&mut current_line, &mut current_line_width);
        lines.push(current_line);
    }

    lines
}

fn line_has_visible_content(line: &[InlineToken]) -> bool {
    line.iter().any(|t| matches!(t, InlineToken::Word { .. }))
}

fn trim_trailing_collapsible_spaces(line: &mut Vec<InlineToken>, current_width: &mut f32) {
    let mut i = line.len();
    while i > 0 {
        match &line[i - 1] {
            InlineToken::BoxEnd { .. } => {
                i -= 1;
            }
            InlineToken::Space {
                collapsible: true,
                width,
                ..
            } => {
                *current_width -= *width;
                line.remove(i - 1);
                i -= 1;
            }
            _ => break,
        }
    }
}

/// Metadata for an active inline box that may span across line boundaries.
#[derive(Clone, Debug)]
struct ActiveBoxInfo {
    node_id: NodeId,
    style: ComputedStyle,
    is_first_fragment: bool,
}

/// Context parameters for formatting a single line in an IFC.
struct LineContext<'a> {
    container_x: f32,
    line_y: f32,
    container_width: f32,
    container_style: &'a ComputedStyle,
    strut_ascent: f32,
    strut_descent: f32,
}

/// Formats a single line into a [`BoxType::LineBox`] [`LayoutBox`].
fn format_line(
    tokens: Vec<InlineToken>,
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
    } = *ctx;
    // 1. Calculate line content width for text-align
    let mut line_content_width = 0.0;
    for token in &tokens {
        match token {
            InlineToken::Word { width, .. } | InlineToken::Space { width, .. } => {
                line_content_width += *width;
            }
            InlineToken::BoxStart { style, .. } => {
                line_content_width += resolve_length(style.margin.left, container_width)
                    + style.border_width.left
                    + resolve_length(style.padding.left, container_width);
            }
            InlineToken::BoxEnd { style, .. } => {
                line_content_width += resolve_length(style.padding.right, container_width)
                    + style.border_width.right
                    + resolve_length(style.margin.right, container_width);
            }
            InlineToken::ForcedBreak { .. } => {}
        }
    }

    // 2. Horizontal text-align offset
    let remaining_width = (container_width - line_content_width).max(0.0);
    let align_offset = match container_style.text_align {
        TextAlign::Left | TextAlign::Justify => 0.0,
        TextAlign::Right => remaining_width,
        TextAlign::Center => remaining_width / 2.0,
    };

    // 3. Baseline and line box height calculation (CSS 2.1 §10.8)
    let mut max_ascent = strut_ascent;
    let mut max_descent = strut_descent;

    for token in &tokens {
        // `vertical-align` applies to inline boxes, not to the text runs they contain:
        // text tokens always contribute their baseline metrics, while `BoxStart` tokens
        // contribute the shifted metrics of the inline box itself (CSS 2.2 §10.8.1).
        let (style, align_applies) = match token {
            InlineToken::BoxStart { style, .. } => (style, true),
            InlineToken::Word { style, .. } | InlineToken::Space { style, .. } => (style, false),
            _ => continue,
        };

        let metrics = measurer.font_metrics(&style.font_family, style.font_size, style.line_height);
        let leading = metrics.line_height - (metrics.ascent + metrics.descent);
        let half_leading = leading / 2.0;
        let mut a_prime = metrics.ascent + half_leading;
        let mut d_prime = metrics.descent + half_leading;

        match if align_applies {
            style.vertical_align
        } else {
            VerticalAlign::Baseline
        } {
            VerticalAlign::Baseline => {}
            VerticalAlign::Sub => {
                let shift = 0.20 * container_style.font_size;
                a_prime -= shift;
                d_prime += shift;
            }
            VerticalAlign::Super => {
                let shift = 0.33 * container_style.font_size;
                a_prime += shift;
                d_prime -= shift;
            }
            VerticalAlign::Length(px) => {
                a_prime += px;
                d_prime -= px;
            }
            VerticalAlign::Middle => {
                let midpoint = 0.25 * container_style.font_size;
                let box_h = a_prime + d_prime;
                a_prime = midpoint + box_h / 2.0;
                d_prime = box_h / 2.0 - midpoint;
            }
            VerticalAlign::TextTop => {
                let box_h = a_prime + d_prime;
                a_prime = strut_ascent;
                d_prime = box_h - strut_ascent;
            }
            VerticalAlign::TextBottom => {
                let box_h = a_prime + d_prime;
                d_prime = strut_descent;
                a_prime = box_h - strut_descent;
            }
            VerticalAlign::Top | VerticalAlign::Bottom => {}
        }

        max_ascent = max_ascent.max(a_prime);
        max_descent = max_descent.max(d_prime);
    }

    let line_height = max_ascent + max_descent;
    let baseline_y_in_line = max_ascent;

    // 4. Fragment assembly using active boxes stack
    struct ActiveBoxFrame {
        node_id: NodeId,
        style: ComputedStyle,
        is_first_fragment: bool,
        start_x: f32,
        children: Vec<LayoutBox>,
    }

    let mut active_stack: Vec<ActiveBoxFrame> = Vec::new();
    let mut line_children = Vec::new();
    let mut curr_x = container_x + align_offset;

    // Reopen boxes that were split from previous lines
    for prev in incoming_active_boxes {
        active_stack.push(ActiveBoxFrame {
            node_id: prev.node_id,
            style: prev.style,
            is_first_fragment: prev.is_first_fragment,
            start_x: curr_x,
            children: Vec::new(),
        });
    }

    for token in tokens {
        match token {
            InlineToken::BoxStart { node_id, style } => {
                let left_margin = resolve_length(style.margin.left, container_width);
                let left_border = style.border_width.left;
                let left_padding = resolve_length(style.padding.left, container_width);
                let left_edges = left_margin + left_border + left_padding;
                let start_x = curr_x;
                curr_x += left_edges;

                active_stack.push(ActiveBoxFrame {
                    node_id,
                    style,
                    is_first_fragment: true,
                    start_x,
                    children: Vec::new(),
                });
            }
            InlineToken::BoxEnd { node_id, .. } => {
                if let Some(pos) = active_stack.iter().rposition(|b| b.node_id == node_id) {
                    let frame = active_stack.remove(pos);
                    let right_padding = resolve_length(frame.style.padding.right, container_width);
                    let right_border = frame.style.border_width.right;
                    let right_margin = resolve_length(frame.style.margin.right, container_width);
                    let right_edges = right_padding + right_border + right_margin;

                    let left_margin = if frame.is_first_fragment {
                        resolve_length(frame.style.margin.left, container_width)
                    } else {
                        0.0
                    };
                    let left_border = if frame.is_first_fragment {
                        frame.style.border_width.left
                    } else {
                        0.0
                    };
                    let left_padding = if frame.is_first_fragment {
                        resolve_length(frame.style.padding.left, container_width)
                    } else {
                        0.0
                    };

                    let box_w = (curr_x
                        - (frame.start_x + left_margin + left_border + left_padding))
                        .max(0.0);
                    curr_x += right_edges;

                    let mut inline_box = LayoutBox::new(
                        BoxType::InlineNode(frame.node_id),
                        Some(frame.style.clone()),
                    );
                    inline_box.dimensions.margin.left = left_margin;
                    inline_box.dimensions.margin.right = right_margin;
                    inline_box.dimensions.border.left = left_border;
                    inline_box.dimensions.border.right = right_border;
                    inline_box.dimensions.padding.left = left_padding;
                    inline_box.dimensions.padding.right = right_padding;

                    inline_box.dimensions.content.x =
                        frame.start_x + left_margin + left_border + left_padding;
                    inline_box.dimensions.content.y = line_y;
                    inline_box.dimensions.content.width = box_w;
                    inline_box.dimensions.content.height = line_height;
                    inline_box.children = frame.children;

                    if let Some(parent_frame) = active_stack.last_mut() {
                        parent_frame.children.push(inline_box);
                    } else {
                        line_children.push(inline_box);
                    }
                }
            }
            InlineToken::Word { text, width, style }
            | InlineToken::Space {
                text, width, style, ..
            } => {
                let metrics =
                    measurer.font_metrics(&style.font_family, style.font_size, style.line_height);
                let leading = metrics.line_height - (metrics.ascent + metrics.descent);
                let half_leading = leading / 2.0;
                let mut a_prime = metrics.ascent + half_leading;
                let d_prime = metrics.descent + half_leading;
                let box_h = a_prime + d_prime;

                // `vertical-align` belongs to the innermost inline box containing this
                // text run, not to the text formatting artifact itself.
                let vertical_align = active_stack
                    .last()
                    .map_or(VerticalAlign::Baseline, |frame| frame.style.vertical_align);

                match vertical_align {
                    VerticalAlign::Baseline => {}
                    VerticalAlign::Sub => {
                        let shift = 0.20 * container_style.font_size;
                        a_prime -= shift;
                    }
                    VerticalAlign::Super => {
                        let shift = 0.33 * container_style.font_size;
                        a_prime += shift;
                    }
                    VerticalAlign::Length(px) => {
                        a_prime += px;
                    }
                    VerticalAlign::Middle => {
                        let x_height = 0.5 * container_style.font_size;
                        a_prime = x_height / 2.0 + box_h / 2.0;
                    }
                    VerticalAlign::TextTop => {
                        a_prime = strut_ascent;
                    }
                    VerticalAlign::TextBottom => {
                        a_prime = box_h - strut_descent;
                    }
                    VerticalAlign::Top => {
                        a_prime = max_ascent;
                    }
                    VerticalAlign::Bottom => {
                        a_prime = box_h - max_descent;
                    }
                }

                let target_list = if let Some(active) = active_stack.last_mut() {
                    &mut active.children
                } else {
                    &mut line_children
                };

                if let Some(last) = target_list.last_mut()
                    && let BoxType::TextNode(ref mut last_text) = last.box_type
                    && last.style.as_ref() == Some(&style)
                {
                    last_text.push_str(&text);
                    last.dimensions.content.width += width;
                    curr_x += width;
                    continue;
                }

                let y_in_line =
                    (baseline_y_in_line - a_prime).clamp(0.0, (line_height - box_h).max(0.0));
                let mut text_box =
                    LayoutBox::new(BoxType::TextNode(text.clone()), Some(style.clone()));
                text_box.dimensions.content.x = curr_x;
                text_box.dimensions.content.y = line_y + y_in_line;
                text_box.dimensions.content.width = width;
                text_box.dimensions.content.height = box_h;

                curr_x += width;
                target_list.push(text_box);
            }
            InlineToken::ForcedBreak { .. } => {}
        }
    }

    // 5. Handle any inline boxes that remain open at line break (split across lines)
    let mut outgoing_active = Vec::new();
    while let Some(frame) = active_stack.pop() {
        let left_margin = if frame.is_first_fragment {
            resolve_length(frame.style.margin.left, container_width)
        } else {
            0.0
        };
        let left_border = if frame.is_first_fragment {
            frame.style.border_width.left
        } else {
            0.0
        };
        let left_padding = if frame.is_first_fragment {
            resolve_length(frame.style.padding.left, container_width)
        } else {
            0.0
        };

        // Interior split: right edges are 0.0 per CSS 2.1 §9.4.2
        let box_w = (curr_x - (frame.start_x + left_margin + left_border + left_padding)).max(0.0);

        let mut inline_box = LayoutBox::new(
            BoxType::InlineNode(frame.node_id),
            Some(frame.style.clone()),
        );
        inline_box.dimensions.margin.left = left_margin;
        inline_box.dimensions.margin.right = 0.0;
        inline_box.dimensions.border.left = left_border;
        inline_box.dimensions.border.right = 0.0;
        inline_box.dimensions.padding.left = left_padding;
        inline_box.dimensions.padding.right = 0.0;

        inline_box.dimensions.content.x = frame.start_x + left_margin + left_border + left_padding;
        inline_box.dimensions.content.y = line_y;
        inline_box.dimensions.content.width = box_w;
        inline_box.dimensions.content.height = line_height;
        inline_box.children = frame.children;

        outgoing_active.push(ActiveBoxInfo {
            node_id: frame.node_id,
            style: frame.style,
            is_first_fragment: false,
        });

        if let Some(parent) = active_stack.last_mut() {
            parent.children.push(inline_box);
        } else {
            line_children.push(inline_box);
        }
    }
    outgoing_active.reverse();

    // 6. Assemble the LineBox
    //
    // Line boxes are pure formatting artifacts: they must not carry a painting style
    // (no background, border or padding), otherwise paint duplicates the container's
    // decoration once per line (audit finding F-02).
    let mut line_box = LayoutBox::new(BoxType::LineBox, None);
    line_box.dimensions.content = Rect::new(container_x, line_y, container_width, line_height);
    line_box.children = line_children;

    (line_box, outgoing_active)
}
