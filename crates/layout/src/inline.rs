//! # Inline Formatting Context (IFC) Layout Engine
//!
//! Normative implementation of W3C CSS 2.1 §9.4.2 (Inline formatting contexts),
//! §10.8 (Line height calculations, strut, and vertical-align), and
//! CSS Text Module Level 3 (§3–§5: white-space processing and line breaking).

use std::borrow::Cow;
use std::sync::Arc;

use css::{ComputedStyle, Length, TextAlign, TextTransform, VerticalAlign, WhiteSpace};
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

    /// Weight- and style-aware variant of [`TextMeasurer::measure_text`].
    ///
    /// The default implementation ignores the weight/style and forwards to
    /// [`TextMeasurer::measure_text`], so existing measurers keep compiling and
    /// behaving exactly as before. The IFC uses this variant internally so that
    /// real font backends can honour `font-weight`/`font-style`.
    fn measure_text_styled(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        _font_weight: u16,
        _font_italic: bool,
    ) -> f32 {
        self.measure_text(text, font_family, font_size)
    }

    /// Weight- and style-aware variant of [`TextMeasurer::font_metrics`].
    ///
    /// Defaults to [`TextMeasurer::font_metrics`] (weight/style ignored).
    fn font_metrics_styled(
        &self,
        font_family: &str,
        font_size: f32,
        computed_line_height: Option<f32>,
        _font_weight: u16,
        _font_italic: bool,
    ) -> FontMetrics {
        self.font_metrics(font_family, font_size, computed_line_height)
    }
}

/// Measures `text` with the run's weight and style from a computed style.
///
/// Exposed to the block layout algorithm for intrinsic (shrink-to-fit) width
/// calculations of floats and absolutely positioned boxes.
#[inline]
pub(crate) fn measure_text_width(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &ComputedStyle,
) -> f32 {
    let transformed = transform_text(text, style.text_transform);
    shaped_width(measurer, &transformed, style)
}

/// Measures `text` with the run's weight and style from a computed style.
#[inline]
fn measure_styled(measurer: &dyn TextMeasurer, text: &str, style: &ComputedStyle) -> f32 {
    measurer.measure_text_styled(
        text,
        &style.font_family,
        style.font_size,
        style.font_weight,
        crate::text::style_is_italic(style),
    )
}

/// Resolves a `word-spacing`/`letter-spacing` computed length to CSS pixels.
///
/// Percentages resolve against the element `font-size` (a Megi extension: CSS only allows
/// lengths, but the parser stores a [`Length`]); `auto`/`normal` are zero.
#[inline]
fn spacing_px(length: Length, font_size: f32) -> f32 {
    match length {
        Length::Px(px) => px,
        Length::Percent(pct) => font_size * pct / 100.0,
        Length::Auto => 0.0,
    }
}

/// Applies the computed `text-transform` to a text run (CSS Text 3 §2.1).
///
/// Returns a borrowed slice for `none` so the default path does not allocate. `uppercase`/
/// `lowercase` use the Unicode simple mappings (which can expand, e.g. `ß` -> `SS`);
/// `capitalize` uppercases the first alphabetic character of each whitespace-delimited word
/// (browsers also split on hyphens/apostrophes; documented simplification).
pub(crate) fn transform_text(text: &str, transform: TextTransform) -> Cow<'_, str> {
    match transform {
        TextTransform::None => Cow::Borrowed(text),
        TextTransform::Uppercase => {
            let mut out = String::with_capacity(text.len());
            for ch in text.chars() {
                out.extend(ch.to_uppercase());
            }
            Cow::Owned(out)
        }
        TextTransform::Lowercase => {
            let mut out = String::with_capacity(text.len());
            for ch in text.chars() {
                out.extend(ch.to_lowercase());
            }
            Cow::Owned(out)
        }
        TextTransform::Capitalize => {
            let mut out = String::with_capacity(text.len());
            let mut at_word_start = true;
            for ch in text.chars() {
                if ch.is_whitespace() {
                    out.push(ch);
                    at_word_start = true;
                } else if at_word_start && ch.is_alphabetic() {
                    out.extend(ch.to_uppercase());
                    at_word_start = false;
                } else {
                    out.push(ch);
                }
            }
            Cow::Owned(out)
        }
    }
}

/// Measures `text` including the computed `letter-spacing`/`word-spacing`.
///
/// `letter-spacing` adds one spacing unit per character and `word-spacing` one per space
/// character (`U+0020`/tab) in the run; this matches how the IFC tokens carry explicit
/// space tokens.
#[inline]
fn shaped_width(measurer: &dyn TextMeasurer, text: &str, style: &ComputedStyle) -> f32 {
    let base = measure_styled(measurer, text, style);
    // Fast path: no spacing overrides (the common case) skips the character scan.
    let letter_spacing = spacing_px(style.letter_spacing, style.font_size);
    let word_spacing = spacing_px(style.word_spacing, style.font_size);
    if letter_spacing == 0.0 && word_spacing == 0.0 {
        return base;
    }
    let mut characters = 0u32;
    let mut spaces = 0u32;
    for ch in text.chars() {
        characters += 1;
        if matches!(ch, ' ' | '\t') {
            spaces += 1;
        }
    }
    base + letter_spacing * characters as f32 + word_spacing * spaces as f32
}

/// Resolves real font metrics for a computed style (weight/style aware).
///
/// The used `line-height` is resolved here per CSS 2.2 §10.8.1:
///
/// 1. `line-height: <number>` is a *factor* that is inherited as a number and
///    recomputed against each element's own `font-size`
///    ([`ComputedStyle::line_height_factor`]); this is why a child with a larger
///    font gets a proportionally larger line box (audit finding F-12);
/// 2. otherwise the computed px value (`<length>`, `<percentage>` already resolved
///    against the declaring element, or the `normal` fallback provided by the CSS
///    crate) applies;
/// 3. when no line-height is available the measurer's natural line height
///    (`ascent + descent + line_gap` for real fonts, `1.2 * font_size` for
///    [`DefaultTextMeasurer`]) is used.
#[inline]
fn metrics_styled(measurer: &dyn TextMeasurer, style: &ComputedStyle) -> FontMetrics {
    let computed_line_height = style
        .line_height_factor
        .map(|factor| factor * style.font_size)
        .or(style.line_height);
    measurer.font_metrics_styled(
        &style.font_family,
        style.font_size,
        computed_line_height,
        style.font_weight,
        crate::text::style_is_italic(style),
    )
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
///
/// Style handles are stored as ids into the per-IFC style table instead of
/// `Arc<ComputedStyle>` clones: one text node or inline element contributes a
/// single entry, so tokenization performs one atomic refcount bump per node
/// instead of one per word/space token. The table lives for the whole IFC pass.
#[derive(Clone, Debug)]
enum InlineToken {
    /// Start of an inline container element (`<span>`).
    BoxStart { node_id: NodeId, style: u32 },
    /// End of an inline container element.
    BoxEnd { node_id: NodeId, style: u32 },
    /// An unbroken word run.
    ///
    /// The characters live in the per-IFC text pool (`[start, end)` byte range),
    /// which avoids one `String` allocation per word on text-heavy pages.
    Word {
        start: usize,
        end: usize,
        width: f32,
        style: u32,
        /// Half-leading line-box ascent/descent for the style (computed once per
        /// text node instead of once per word).
        ascent: f32,
        descent: f32,
    },
    /// A whitespace run (collapsible or preserved).
    ///
    /// Stored as a single `char` (`'\0'` means "no rendered character": the break
    /// opportunity created after an explicit hyphen) to avoid one `String`
    /// allocation per space token on text-heavy pages.
    Space {
        text: char,
        width: f32,
        style: u32,
        ascent: f32,
        descent: f32,
        collapsible: bool,
        allow_wrap: bool,
    },
    /// A soft hyphen (U+00AD): zero-width break opportunity. When the line is not
    /// broken here the character is invisible; when the line ends here the layout
    /// appends a rendered `-` to the line (see [`break_into_lines`]).
    HyphenationPoint { style: u32 },
    /// A forced line break (`\n` in pre/pre-wrap/pre-line or `<br>`).
    ForcedBreak,
    /// An atomic inline-level box: `display: inline-block` or a replaced element
    /// (CSS 2.2 §9.2.1.1, §10.3.9). It participates in the line as a single unit.
    Atomic {
        layout_box: Box<LayoutBox>,
        /// Margin box width (line advance).
        width: f32,
        /// Margin box height.
        height: f32,
        /// Distance from the margin box top to the alignment baseline.
        baseline: f32,
        vertical_align: VerticalAlign,
    },
}

/// Pushes a style handle into the per-IFC table and returns its id.
fn push_style(styles: &mut Vec<Arc<ComputedStyle>>, style: Arc<ComputedStyle>) -> u32 {
    let id = styles.len() as u32;
    styles.push(style);
    id
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
    let pos =
        crate::block::PositioningContext::from_containing_block(layout_box.dimensions.content);
    layout_inline_formatting_context_with_measurer_and_pos(layout_box, measurer, pos);
}

/// IFC layout with the inherited positioning context (needed to lay out atomic
/// inline-level boxes and their absolutely positioned descendants).
pub(crate) fn layout_inline_formatting_context_with_measurer_and_pos(
    layout_box: &mut LayoutBox,
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
    for child in &layout_box.children {
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
    collect_inline_tokens(
        &mut inline_children,
        measurer,
        &mut text_pool,
        &mut styles,
        &mut tokens,
        container_width,
        pos,
    );

    if tokens.is_empty() {
        layout_box.children.clear();
        layout_box.style = Some(container_style);
        return;
    }

    // 2. Break tokens into lines at valid soft wrap opportunities.
    let lines = break_into_lines(tokens, &mut text_pool, &styles, container_width, measurer);

    if lines.is_empty() {
        layout_box.children.clear();
        layout_box.style = Some(container_style);
        return;
    }

    // 3. Strut metrics for the containing block container per CSS 2.2 §10.8
    let strut_metrics = metrics_styled(measurer, &container_style);
    let strut_leading = strut_metrics.line_height - (strut_metrics.ascent + strut_metrics.descent);
    let strut_ascent = strut_metrics.ascent + strut_leading / 2.0;
    let strut_descent = strut_metrics.descent + strut_leading / 2.0;

    // 4. Construct LineBox layout boxes and resolve baseline / vertical-align
    let mut line_boxes = Vec::with_capacity(lines.len());
    let mut current_line_y = container_y;

    // Track active inline boxes across line breaks for box splitting
    let mut persistent_active_boxes: Vec<ActiveBoxInfo> = Vec::new();

    for line in lines {
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
    layout_box.style = Some(container_style);
}

/// Recursively flattens inline boxes into [`InlineToken`]s.
///
/// Atomic inline-level boxes (`inline-block`, replaced elements) are laid out eagerly
/// at a provisional origin and emitted as [`InlineToken::Atomic`]: the line breaker
/// treats them as unbreakable units with a known margin box and baseline.
fn collect_inline_tokens(
    boxes: &mut [LayoutBox],
    measurer: &dyn TextMeasurer,
    text_pool: &mut String,
    styles: &mut Vec<Arc<ComputedStyle>>,
    tokens: &mut Vec<InlineToken>,
    container_width: f32,
    pos: crate::block::PositioningContext,
) {
    for child in boxes.iter_mut() {
        match &child.box_type {
            BoxType::InlineNode(node_id) => {
                let node_id = *node_id;
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
                });
                collect_inline_tokens(
                    &mut child.children,
                    measurer,
                    text_pool,
                    styles,
                    tokens,
                    container_width,
                    pos,
                );
                tokens.push(InlineToken::BoxEnd {
                    node_id,
                    style: style_id,
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
            BoxType::InlineBlock(_) | BoxType::Replaced(_) | BoxType::InlineTable(_) => {
                let atomic = crate::block::layout_atomic_inline(
                    std::mem::replace(child, LayoutBox::new(BoxType::LineBreak, None)),
                    container_width,
                    None,
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

/// Flushes the pending word (`[word_start, pool.len())`) into an
/// [`InlineToken::Word`] backed by the per-IFC text pool.
fn flush_word(
    pool: &str,
    word_start: usize,
    style: &ComputedStyle,
    style_id: u32,
    line_metrics: (f32, f32),
    measurer: &dyn TextMeasurer,
    tokens: &mut Vec<InlineToken>,
) {
    let (ascent, descent) = line_metrics;
    if word_start >= pool.len() {
        return;
    }
    let width = shaped_width(measurer, &pool[word_start..], style);
    tokens.push(InlineToken::Word {
        start: word_start,
        end: pool.len(),
        width,
        style: style_id,
        ascent,
        descent,
    });
}

/// Pushes a collapsible space token (CSS Text 3 §4.1 white-space collapsing).
fn push_collapsible_space(
    style_id: u32,
    width: f32,
    ascent: f32,
    descent: f32,
    allow_wrap: bool,
    tokens: &mut Vec<InlineToken>,
) {
    tokens.push(InlineToken::Space {
        text: ' ',
        width,
        style: style_id,
        ascent,
        descent,
        collapsible: true,
        allow_wrap,
    });
}

/// Tokenizes a text node into words, spaces, break opportunities and forced breaks
/// following the element's `white-space` processing model (CSS Text 3 §4).
///
/// ## Soft hyphens (U+00AD)
///
/// A soft hyphen is a formatting character: it is invisible while the line is not
/// broken there, and only produces a rendered `-` when the line ends at it. It is
/// modeled as a zero-width [`InlineToken::HyphenationPoint`] break opportunity; the
/// rendered hyphen is only materialized when a line actually breaks at that point
/// (see [`break_into_lines`]).
fn tokenize_text(
    txt: &str,
    style: &ComputedStyle,
    style_id: u32,
    measurer: &dyn TextMeasurer,
    pool: &mut String,
    tokens: &mut Vec<InlineToken>,
) {
    let white_space = style.white_space;
    // CSS `text-transform` is applied to the rendered text while shaping; the DOM tree is
    // untouched (documented) and all line breaking/measurement below works on the
    // transformed run stored in the per-IFC pool.
    let transformed = transform_text(txt, style.text_transform);
    let txt: &str = &transformed;
    let (line_ascent, line_descent) = line_box_extents(&metrics_styled(measurer, style));

    // `pre`: no wrapping, no space collapsing. Each newline-separated run is preserved
    // verbatim (spaces included) and soft hyphens stay invisible.
    if white_space == WhiteSpace::Pre {
        for (index, line) in txt.split('\n').enumerate() {
            if index > 0 {
                tokens.push(InlineToken::ForcedBreak);
            }
            let start = pool.len();
            for ch in line.chars() {
                if ch != '\u{00AD}' {
                    pool.push(ch);
                }
            }
            if start < pool.len() {
                let width = shaped_width(measurer, &pool[start..], style);
                tokens.push(InlineToken::Word {
                    start,
                    end: pool.len(),
                    width,
                    style: style_id,
                    ascent: line_ascent,
                    descent: line_descent,
                });
            }
        }
        return;
    }

    // normal | nowrap | pre-wrap | pre-line
    let collapses_spaces = white_space != WhiteSpace::PreWrap;
    let break_on_newline = matches!(white_space, WhiteSpace::PreWrap | WhiteSpace::PreLine);
    let allow_wrap = white_space != WhiteSpace::Nowrap;

    let mut word_start = pool.len();
    let mut pending_collapsible_space = false;
    let mut collapsible_space_width: Option<f32> = None;

    for ch in txt.chars() {
        match ch {
            // Preserved newline: forced line break.
            '\n' if break_on_newline => {
                flush_word(
                    pool,
                    word_start,
                    style,
                    style_id,
                    (line_ascent, line_descent),
                    measurer,
                    tokens,
                );
                word_start = pool.len();
                pending_collapsible_space = false;
                tokens.push(InlineToken::ForcedBreak);
            }
            // Collapsible (`normal`/`nowrap`/`pre-line`) or preserved (`pre-wrap`) space.
            ' ' | '\t' | '\r' | '\n' => {
                flush_word(
                    pool,
                    word_start,
                    style,
                    style_id,
                    (line_ascent, line_descent),
                    measurer,
                    tokens,
                );
                word_start = pool.len();
                if collapses_spaces {
                    pending_collapsible_space = true;
                } else {
                    let mut buf = [0u8; 4];
                    let text: &str = ch.encode_utf8(&mut buf);
                    let width = shaped_width(measurer, text, style);
                    tokens.push(InlineToken::Space {
                        text: ch,
                        width,
                        style: style_id,
                        ascent: line_ascent,
                        descent: line_descent,
                        collapsible: false,
                        allow_wrap,
                    });
                }
            }
            // Soft hyphen: invisible break opportunity (nothing is rendered unless the
            // line is broken here).
            '\u{00AD}' => {
                if allow_wrap {
                    flush_word(
                        pool,
                        word_start,
                        style,
                        style_id,
                        (line_ascent, line_descent),
                        measurer,
                        tokens,
                    );
                    word_start = pool.len();
                    if pending_collapsible_space {
                        let width = *collapsible_space_width
                            .get_or_insert_with(|| shaped_width(measurer, " ", style));
                        push_collapsible_space(
                            style_id,
                            width,
                            line_ascent,
                            line_descent,
                            allow_wrap,
                            tokens,
                        );
                        pending_collapsible_space = false;
                    }
                    tokens.push(InlineToken::HyphenationPoint { style: style_id });
                }
            }
            _ => {
                if pending_collapsible_space {
                    let width = *collapsible_space_width
                        .get_or_insert_with(|| shaped_width(measurer, " ", style));
                    push_collapsible_space(
                        style_id,
                        width,
                        line_ascent,
                        line_descent,
                        allow_wrap,
                        tokens,
                    );
                    pending_collapsible_space = false;
                }
                pool.push(ch);
                // An explicit hyphen is a soft wrap opportunity *after* the hyphen
                // (the hyphen itself stays visible on both sides of a break).
                if ch == '-' && allow_wrap {
                    flush_word(
                        pool,
                        word_start,
                        style,
                        style_id,
                        (line_ascent, line_descent),
                        measurer,
                        tokens,
                    );
                    word_start = pool.len();
                    tokens.push(InlineToken::Space {
                        text: '\0',
                        width: 0.0,
                        style: style_id,
                        ascent: line_ascent,
                        descent: line_descent,
                        collapsible: false,
                        allow_wrap: true,
                    });
                }
            }
        }
    }

    flush_word(
        pool,
        word_start,
        style,
        style_id,
        (line_ascent, line_descent),
        measurer,
        tokens,
    );
    if pending_collapsible_space {
        let width =
            *collapsible_space_width.get_or_insert_with(|| shaped_width(measurer, " ", style));
        push_collapsible_space(
            style_id,
            width,
            line_ascent,
            line_descent,
            allow_wrap,
            tokens,
        );
    }
}

/// A formatted line produced by [`break_into_lines`].
struct Line {
    /// Tokens that belong to this line (inline box fragments are split later by
    /// [`format_line`]).
    tokens: Vec<InlineToken>,
    /// `true` when the line ended at a soft wrap opportunity (and is therefore a
    /// candidate for `text-align: justify`; the last line and lines before a forced
    /// break are never justified, CSS 2.2 §16.2).
    justified: bool,
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
fn token_advance(token: &InlineToken, styles: &[Arc<ComputedStyle>], container_width: f32) -> f32 {
    match token {
        InlineToken::Word { width, .. }
        | InlineToken::Space { width, .. }
        | InlineToken::Atomic { width, .. } => *width,
        InlineToken::BoxStart { style, .. } => {
            let style = &styles[*style as usize];
            resolve_length(style.margin.left, container_width)
                + style.border_width.left
                + resolve_length(style.padding.left, container_width)
        }
        InlineToken::BoxEnd { style, .. } => {
            let style = &styles[*style as usize];
            resolve_length(style.margin.right, container_width)
                + style.border_width.right
                + resolve_length(style.padding.right, container_width)
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
    current: &[InlineToken],
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
fn first_visible_token(line: &[InlineToken]) -> Option<usize> {
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
fn break_into_lines(
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

    let mut current: Vec<InlineToken> = Vec::with_capacity(32);
    let mut current_width = 0.0_f32;
    let mut first_visible: Option<usize> = None;

    for token in tokens {
        // A forced break always ends the current line, even if it overflows.
        if matches!(token, InlineToken::ForcedBreak) {
            let line_tokens = finalize_line(std::mem::take(&mut current));
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
        current.push(token);

        // Wrap greedily at the last valid soft wrap opportunity when the line overflows.
        while current_width > container_width {
            let Some((break_index, kind)) = last_soft_wrap(&current, first_visible) else {
                break;
            };
            let mut line_tokens: Vec<InlineToken> = Vec::with_capacity(break_index + 1);
            let mut removed_width = 0.0_f32;
            for removed in current.drain(..break_index) {
                removed_width += token_advance(&removed, styles, container_width);
                line_tokens.push(removed);
            }
            let opportunity = current.remove(0);
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
        let line_tokens = finalize_line(current);
        if !line_tokens.is_empty() {
            lines.push(Line {
                tokens: line_tokens,
                justified: false,
            });
        }
    }

    lines
}

/// Metadata for an active inline box that may span across line boundaries.
#[derive(Clone, Debug)]
struct ActiveBoxInfo {
    node_id: NodeId,
    style: Arc<ComputedStyle>,
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
    /// `true` when this line must be justified (soft-wrapped line of a
    /// `text-align: justify` paragraph; the last line and lines before forced breaks
    /// are never justified, CSS 2.2 §16.2).
    justify: bool,
}

/// Half-leading box ascent/descent of a font's line box (CSS 2.2 §10.8.1).
fn line_box_extents(metrics: &FontMetrics) -> (f32, f32) {
    let leading = metrics.line_height - (metrics.ascent + metrics.descent);
    (
        metrics.ascent + leading / 2.0,
        metrics.descent + leading / 2.0,
    )
}

/// Applies the `vertical-align` shift to an `(ascent, descent)` pair relative to the
/// line box baseline (CSS 2.2 §10.8.1).
///
/// `top`/`bottom` do not shift the box relative to the baseline: those boxes are
/// anchored to the line box edges once the line height is known (aligned subtree).
fn align_extents(
    vertical_align: VerticalAlign,
    ascent: f32,
    descent: f32,
    container_font_size: f32,
    strut_ascent: f32,
    strut_descent: f32,
) -> (f32, f32) {
    match vertical_align {
        VerticalAlign::Baseline | VerticalAlign::Top | VerticalAlign::Bottom => (ascent, descent),
        VerticalAlign::Sub => {
            let shift = 0.20 * container_font_size;
            (ascent - shift, descent + shift)
        }
        VerticalAlign::Super => {
            let shift = 0.33 * container_font_size;
            (ascent + shift, descent - shift)
        }
        VerticalAlign::Length(px) => (ascent + px, descent - px),
        VerticalAlign::Middle => {
            let midpoint = 0.25 * container_font_size;
            let height = ascent + descent;
            (midpoint + height / 2.0, height / 2.0 - midpoint)
        }
        VerticalAlign::TextTop => (strut_ascent, ascent + descent - strut_ascent),
        VerticalAlign::TextBottom => (ascent + descent - strut_descent, strut_descent),
    }
}

/// Anchor of a `vertical-align: top|bottom` inline box inside the line box.
///
/// `(top, baseline, height)` are absolute layout coordinates: the top of the aligned
/// subtree, the absolute y of its baseline, and its height (ascent + descent).
fn anchor_for_style(
    style: &ComputedStyle,
    measurer: &dyn TextMeasurer,
    line_y: f32,
    line_height: f32,
) -> Option<(f32, f32, f32)> {
    let is_top = matches!(style.vertical_align, VerticalAlign::Top);
    let is_bottom = matches!(style.vertical_align, VerticalAlign::Bottom);
    if !is_top && !is_bottom {
        return None;
    }
    let metrics = metrics_styled(measurer, style);
    let (ascent, descent) = line_box_extents(&metrics);
    let height = ascent + descent;
    let top = if is_top {
        line_y
    } else {
        line_y + line_height - height
    };
    Some((top, top + ascent, height))
}

/// An inline box fragment being assembled while formatting a line.
struct ActiveBoxFrame {
    node_id: NodeId,
    style: Arc<ComputedStyle>,
    is_first_fragment: bool,
    start_x: f32,
    children: Vec<LayoutBox>,
    /// `(subtree top, subtree baseline, subtree height)` for `top`/`bottom` aligned
    /// inline boxes; `None` for every other box.
    anchor: Option<(f32, f32, f32)>,
}

/// Fast path for a line made only of text from one text node (no inline boxes,
/// atomic boxes, vertical alignment or justification).
///
/// The generic path performs two passes over the tokens, tracks aligned subtrees and
/// merges adjacent fragments; for plain text lines (the overwhelming majority on
/// text-heavy pages) the result is a line box with exactly one merged text fragment.
/// Building that fragment directly keeps the output tree and geometry identical while
/// skipping the per-token bookkeeping.
fn try_format_simple_text_line(
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

/// Formats a single line into a [`BoxType::LineBox`] [`LayoutBox`].
fn format_line(
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
            start_x: curr_x,
            children: Vec::new(),
            anchor,
        });
    }

    for (index, token) in tokens.into_iter().enumerate() {
        match token {
            InlineToken::BoxStart { node_id, style } => {
                let style_arc = styles[style as usize].clone();
                let style = style_arc.as_ref();
                let left_margin = resolve_length(style.margin.left, container_width);
                let left_border = style.border_width.left;
                let left_padding = resolve_length(style.padding.left, container_width);
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
                    start_x,
                    children: Vec::new(),
                    anchor,
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

                    let mut inline_box = LayoutBox::with_shared_style(
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

        // Interior split: right edges are 0.0 per CSS 2.2 §9.4.2
        let box_w = (curr_x - (frame.start_x + left_margin + left_border + left_padding)).max(0.0);

        let mut inline_box = LayoutBox::with_shared_style(
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
        inline_box.dimensions.content.y = frame.anchor.map_or(line_y, |(top, _, _)| top);
        inline_box.dimensions.content.width = box_w;
        inline_box.dimensions.content.height =
            frame.anchor.map_or(line_height, |(_, _, height)| height);
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

/// Positions a text run on the line and appends it to the innermost active box (or to
/// the line itself), merging with the previous fragment when possible.
#[allow(clippy::too_many_arguments)]
fn push_text_fragment(
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
        last_text.push_str(text);
        last.dimensions.content.width += width;
        *curr_x += width;
        return;
    }

    let mut text_box = LayoutBox::with_shared_style(
        BoxType::TextNode(text.to_owned()),
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
