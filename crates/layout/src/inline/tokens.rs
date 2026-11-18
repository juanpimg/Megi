//! Inline token stream, style table and text-node tokenization.
use super::*;

/// An atomic item in the linearized inline content stream.
///
/// Style handles are stored as ids into the per-IFC style table instead of
/// `Arc<ComputedStyle>` clones: one text node or inline element contributes a
/// single entry, so tokenization performs one atomic refcount bump per node
/// instead of one per word/space token. The table lives for the whole IFC pass.
#[derive(Clone, Debug)]
pub(crate) enum InlineToken {
    /// Start of an inline container element (`<span>`).
    BoxStart {
        node_id: NodeId,
        style: u32,
        is_pseudo: bool,
    },
    /// End of an inline container element.
    BoxEnd {
        node_id: NodeId,
        style: u32,
        is_pseudo: bool,
    },
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
) {
    for child in boxes.iter_mut() {
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

    // Helper: CJK ideographs and kana are break opportunities per UAX #14.
    fn is_cjk_break(ch: char) -> bool {
        matches!(ch as u32,
            0x2E80..=0x2EFF |  // CJK Radicals Supplement
            0x2F00..=0x2FDF |  // Kangxi Radicals
            0x3000..=0x303F |  // CJK Symbols and Punctuation
            0x3040..=0x309F |  // Hiragana
            0x30A0..=0x30FF |  // Katakana
            0x3100..=0x312F |  // Bopomofo
            0x3130..=0x318F |  // Hangul Compatibility Jamo
            0x3190..=0x319F |  // Kanbun
            0x31A0..=0x31BF |  // Bopomofo Extended
            0x31C0..=0x31EF |  // CJK Strokes
            0x31F0..=0x31FF |  // Katakana Phonetic Extensions
            0x3200..=0x32FF |  // Enclosed CJK Letters and Months
            0x3300..=0x33FF |  // CJK Compatibility
            0x3400..=0x4DBF |  // CJK Unified Ideographs Extension A
            0x4E00..=0x9FFF |  // CJK Unified Ideographs
            0xA000..=0xA4CF |  // Yi Syllables
            0xAC00..=0xD7AF |  // Hangul Syllables
            0xF900..=0xFAFF |  // CJK Compatibility Ideographs
            0xFE30..=0xFE4F |  // CJK Compatibility Forms
            0xFF00..=0xFFEF |  // Halfwidth and Fullwidth Forms
            0x20000..=0x2A6DF |  // CJK Unified Ideographs Extension B
            0x2A700..=0x2B73F |  // CJK Unified Ideographs Extension C
            0x2B740..=0x2B81F |  // CJK Unified Ideographs Extension D
            0x2B820..=0x2CEAF |  // CJK Unified Ideographs Extension E
            0x2CEB0..=0x2EBEF   // CJK Unified Ideographs Extension F
        )
    }

    for ch in txt.chars() {
        // Bidi formatting controls are not rendered; their direction was applied when
        // the paragraph was reordered, so strip them before shaping (#1448).
        if crate::bidi::is_bidi_control(ch) {
            continue;
        }
        // CJK characters are break opportunities: flush the current word first.
        if is_cjk_break(ch) && word_start < pool.len() {
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
        }
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
            // After a CJK character, flush the word so the next character can start
            // a new line (UAX #14 ideographic break opportunity).
            _ if is_cjk_break(ch) => {
                pool.push(ch);
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
