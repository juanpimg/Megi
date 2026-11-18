//! Text measurement, font metrics and text transforms (CSS Text 3 §2).
use super::*;

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
    crate::calc::length(length, None, font_size)
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
pub(crate) fn shaped_width(measurer: &dyn TextMeasurer, text: &str, style: &ComputedStyle) -> f32 {
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
pub(crate) fn metrics_styled(measurer: &dyn TextMeasurer, style: &ComputedStyle) -> FontMetrics {
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
