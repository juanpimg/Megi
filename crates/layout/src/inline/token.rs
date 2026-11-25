//! Shared inline token type (CSS 2.2 §9.4.2).
//!
//! Defined below the token collectors so both `tokens` and `tokenize_text`
//! depend on this leaf module instead of on each other.
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
