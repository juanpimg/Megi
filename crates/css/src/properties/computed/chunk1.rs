//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Cached value fingerprint used by layout's `StyleCache` to pick an interning bucket.
///
/// This is a **pure cache hint**, not a style value: `PartialEq` always returns `true` so it
/// never participates in [`ComputedStyle`] value equality. Layout only uses it to select a
/// bucket and still verifies exact `ComputedStyle` equality before sharing an `Arc`, so a
/// stale or colliding fingerprint can never change which style a box gets — it only changes
/// how many candidates the interning has to compare.
///
/// `0` means "not computed" (hand-built styles such as [`ComputedStyle::initial`] or test
/// fixtures); layout then falls back to computing the fingerprint on demand.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StyleFingerprint(pub(crate) u64);

impl StyleFingerprint {
    /// Stores `fingerprint`, remapping `0` to `1` so it cannot be confused with "unset".
    pub(crate) fn new(fingerprint: u64) -> Self {
        Self(if fingerprint == 0 { 1 } else { fingerprint })
    }
}

impl PartialEq for StyleFingerprint {
    #[inline(always)]
    fn eq(&self, _other: &Self) -> bool {
        // Deliberately excluded from `ComputedStyle` equality (see the type docs).
        true
    }
}

/// Complete computed style for an element in the DOM tree.
#[derive(Clone, Debug, PartialEq)]
pub struct ComputedStyle {
    // Display and Positioning
    pub display: Display,
    pub position: Position,
    pub float: Float,
    pub clear: Clear,
    pub box_sizing: BoxSizing,
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
    pub z_index: Option<i32>,
    pub overflow: Overflow,

    // Dimensions
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub max_width: Length,
    pub min_height: Length,
    pub max_height: Length,

    // Box Model Edges
    pub margin: RectEdges<Length>,
    pub padding: RectEdges<Length>,
    pub border_width: RectEdges<f32>,
    pub border_color: RectEdges<Color>,
    pub border_style: RectEdges<BorderStyle>,

    // Tables (CSS 2.1 §17); none of these properties are inherited.
    /// `border-spacing` as `(horizontal, vertical)` CSS pixels; initial `(0.0, 0.0)`.
    pub border_spacing: (f32, f32),
    pub border_collapse: BorderCollapse,
    pub table_layout: TableLayout,
    pub caption_side: CaptionSide,
    pub empty_cells: EmptyCells,

    // Colors & Background
    pub color: Color,
    pub background_color: Color,
    /// `background-image` URL as authored (outer quotes removed) for the first plain
    /// `url(...)` layer; `None` for `none`, gradients and other unsupported image types.
    ///
    /// Contract: `render` resolves this URL against the document base URL and caches the
    /// decoded bitmap; `paint` stretches it to the element's padding box. The shorthand
    /// (`background: <color> url(...)`) populates both color and image. `background-repeat`,
    /// `-size`, `-position`, `-clip` and multiple layers are out of scope.
    pub background_image: Option<String>,
    /// `background-repeat` (initial `repeat`; CSS Backgrounds 3 §2.3).
    pub background_repeat: BackgroundRepeat,
    /// `background-size` (initial `auto`; CSS Backgrounds 3 §2.4).
    pub background_size: BackgroundSize,
    /// `background-position` (initial `0% 0%`; CSS Backgrounds 3 §2.5).
    pub background_position: BackgroundPosition,
    /// `true` when `background-repeat`, `background-size` or `background-position`
    /// (or the `background` shorthand) were declared for this element.
    ///
    /// `paint` uses this to keep the legacy "stretch the bitmap to the padding
    /// box" behaviour for documents that only set `background-image` (all Phase
    /// 4/5/6 golden fixtures), while applying full CSS tiling as soon as the new
    /// background properties are authored. See [`crate::ComputedStyle::background_image`].
    pub background_geometry_explicit: bool,
    /// Computed `background-image` layers (CSS Backgrounds 3 §3.1).
    ///
    /// Stored **top layer first**, exactly as authored
    /// (`background-image: url(top), url(bottom)`), so the painter paints them in
    /// reverse order (bottom-to-top). Empty when only the legacy scalar fields
    /// are used; [`ComputedStyle::background_image`] and the scalar
    /// repeat/size/position fields mirror the first layer for compatibility with
    /// the `render` image loader and the Phase 4/5/6 golden fixtures.
    pub background_layers: Vec<BackgroundLayer>,
    /// Computed `border-radius` for all four corners (initial: zero).
    pub border_radius: BorderRadii,
    /// Computed `box-shadow` layers, top-first (initial: empty).
    pub box_shadows: Vec<BoxShadow>,
    /// Computed `text-shadow` layers, top-first (inherited; initial: empty).
    pub text_shadows: Vec<TextShadow>,
    /// Computed `filter` function list (initial: empty, i.e. `none`). Not inherited.
    pub filter: Vec<FilterFunction>,
    /// Computed `mix-blend-mode` (initial: `normal`). Not inherited.
    pub mix_blend_mode: MixBlendMode,
    /// Computed `backdrop-filter` function list (initial: empty). Not inherited.
    pub backdrop_filter: Vec<FilterFunction>,
    /// Computed `aspect-ratio` (`None` = `auto`, the initial value). Not inherited.
    pub aspect_ratio: Option<AspectRatio>,
    /// Computed `container-type` (initial `normal`). Not inherited. Container queries use
    /// this to find the nearest query container in [`crate::container`].
    pub container_type: ContainerType,
    /// Computed `container-name` list (initial empty). Not inherited.
    pub container_names: Vec<String>,
    /// Computed `object-fit` for replaced elements (initial: `fill`). Not inherited.
    pub object_fit: ObjectFit,
    /// Computed `object-position` (initial: `50% 50%`). Not inherited.
    pub object_position: BackgroundPosition,

    // Flexbox (CSS Flexbox 1); none of these properties are inherited.
    pub flex_direction: crate::flex::FlexDirection,
    pub flex_wrap: crate::flex::FlexWrap,
    pub justify_content: crate::flex::JustifyContent,
    pub align_items: crate::flex::AlignItems,
    pub align_self: crate::flex::AlignSelf,
    /// `row-gap` (initial `0`).
    pub row_gap: Length,
    /// `column-gap` (initial `0`).
    pub column_gap: Length,
    /// `flex-grow` (initial `0`).
    pub flex_grow: f32,
    /// `flex-shrink` (initial `1`).
    pub flex_shrink: f32,
    /// `flex-basis` (initial `auto`).
    pub flex_basis: Length,
    /// `true` when the element was authored as `display: inline-flex`.
    ///
    /// The computed `display` stays `InlineBlock` (so the exhaustive `Display`
    /// match in other crates keeps compiling without a new variant); layout treats
    /// the box as an inline-level flex container when this flag is set.
    pub is_inline_flex: bool,

    // CSS Grid MVP (none of these properties are inherited).
    /// `grid-template-columns` tracks (empty = `none`).
    pub grid_template_columns: crate::grid::GridTemplate,
    /// `grid-template-rows` tracks (empty = `none`).
    pub grid_template_rows: crate::grid::GridTemplate,
    /// `grid-auto-rows` tracks for implicit rows (empty = `auto`).
    pub grid_auto_rows: crate::grid::GridTemplate,
    /// `justify-items` (initial `stretch`).
    pub justify_items: crate::flex::AlignItems,
    /// `justify-self` (initial `auto`).
    pub justify_self: crate::flex::AlignSelf,
    /// `true` when authored as `display: inline-grid`.
    pub is_inline_grid: bool,

    // CSS Grid explicit placement and item order.
    /// `grid-column-start` (`grid-column` shorthand/longhand).
    pub grid_column_start: crate::grid::GridLine,
    /// `grid-column-end`.
    pub grid_column_end: crate::grid::GridLine,
    /// `grid-row-start` (`grid-row` shorthand/longhand).
    pub grid_row_start: crate::grid::GridLine,
    /// `grid-row-end`.
    pub grid_row_end: crate::grid::GridLine,
    /// `order` for grid/flex items (initial `0`).
    pub order: i32,
    /// `grid-auto-flow` packing mode (initial `row`).
    pub grid_auto_flow: crate::grid::GridAutoFlow,
    /// `grid-template-areas` (names and `.` cells; invalid templates are ignored).
    pub grid_template_areas: Option<crate::grid::GridTemplateAreas>,
    /// `grid-area: <name>` area placement (explicit lines override it).
    pub grid_area: Option<String>,
    /// `justify-content` for grid track distribution (initial `stretch`).
    pub grid_justify_content: crate::grid::GridContentDistribution,
    /// `align-content` for grid track distribution (initial `stretch`).
    pub grid_align_content: crate::grid::GridContentDistribution,
    /// Mixed `calc()` side values for `<length-percentage>` properties.
    pub calc_lengths: CalcLengths,

    // Typography
    pub font_size: f32,
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub font_family: String,
    /// Custom properties (`--x`) computed/inherited for this element, as raw token values.
    ///
    /// Lazy copy-on-write: `None` when neither this element nor its ancestors declare any
    /// custom property (so style resolution does not allocate a map per element), otherwise
    /// an [`std::sync::Arc`] shared with descendants and cloned only when this element
    /// declares its own `--*` declarations. See [`crate::custom`].
    pub custom_properties: Option<std::sync::Arc<std::collections::HashMap<String, String>>>,
    pub line_height: Option<f32>,
    /// Unitless `<number>` factor when `line-height` was declared as a number (`1.5`).
    ///
    /// `line_height` keeps the computed px value for backwards compatibility; layout uses this
    /// factor to re-resolve the used value as `factor * font-size` for every descendant, as
    /// required by CSS 2.2 §10.8.1 when a numeric `line-height` is inherited.
    pub line_height_factor: Option<f32>,
    pub text_align: TextAlign,
    /// CSS `text-overflow` (#1929): how clipped inline content signals truncation.
    pub text_overflow: TextOverflow,
    pub direction: Direction,
    pub unicode_bidi: UnicodeBidi,
    pub white_space: WhiteSpace,
    pub vertical_align: VerticalAlign,
    pub text_decoration: TextDecoration,
    pub list_style_type: ListStyleType,
    /// `list-style-image` URL as authored (outer quotes removed) for a plain `url(...)`; `None`
    /// for `none`/gradients. `render` resolves the URL; `paint` draws it as the list marker.
    /// Not inherited; populated by the `list-style` shorthand too.
    pub list_style_image: Option<String>,

    // Visibility
    pub visibility: Visibility,
    pub opacity: f32,
    /// CSS `transform` (2D, #1539). `None` means `none`/no transform. Not inherited.
    pub transform: Option<crate::transform::Transform>,
    /// CSS `transform-origin` (#1934), relative to the border box. Not inherited.
    pub transform_origin: crate::transform_origin::TransformOrigin,

    // Typography: text transformation and spacing (CSS Text 3). All inherited.
    /// `text-transform` (initial `none`); applied by layout while shaping text.
    pub text_transform: TextTransform,
    /// `word-spacing` (initial `normal` = `0px`), resolved by layout in the IFC.
    pub word_spacing: Length,
    /// `letter-spacing` (initial `normal` = `0px`), resolved by layout in the IFC.
    pub letter_spacing: Length,

    // Generated content (`::before` / `::after`, #1540). Not inherited.
    /// Computed `content` value (`normal`, `none`, or generated items).
    pub content: Content,
    /// `counter-reset` pairs `(name, value)`.
    pub counter_reset: Vec<(String, i32)>,
    /// `counter-increment` pairs `(name, delta)`.
    pub counter_increment: Vec<(String, i32)>,

    // CSS Animations 1 / CSS Transitions 1: longhand lists resolved into one entry per
    // animation name / transitioned property. Not inherited.
    /// Animations declared on this element (initial: empty).
    pub animations: Vec<crate::animation::AnimationSpec>,
    /// Transitions declared on this element (initial: empty).
    pub transitions: Vec<crate::animation::TransitionSpec>,
    /// `true` when `width: fit-content` was authored; layout uses shrink-to-fit sizing.
    pub width_fit_content: bool,
    /// `true` when `height: fit-content` was authored; layout uses intrinsic height.
    pub height_fit_content: bool,
    /// Resolved `::backdrop` fill color for top-layer modal elements (`dialog:modal::backdrop`).
    pub backdrop_color: Option<Color>,
    /// Cached value fingerprint for layout's style interning (see [`StyleFingerprint`]).
    ///
    /// Excluded from `PartialEq`; `0` means "not computed" (hand-built styles), in which
    /// case layout falls back to hashing the style value itself.
    pub(crate) style_fingerprint: StyleFingerprint,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self::initial()
    }
}

/// Linear lookup over the cascaded declarations of one element.
///
/// The cascaded map is tiny (a handful of longhands per element) but `ComputedStyle::compute`
/// performs a few dozen lookups; scanning a flat slice of the map entries avoids one SipHash
/// per lookup on the style-resolution hot path.
pub(crate) fn find_declaration<'a>(
    entries: &[(&'a str, &'a Declaration)],
    name: &str,
) -> Option<&'a Declaration> {
    entries
        .iter()
        .find_map(|(key, declaration)| (*key == name).then_some(*declaration))
}
