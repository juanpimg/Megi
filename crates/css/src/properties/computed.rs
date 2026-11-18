//! `ComputedStyle` and CSS cascade computation (extracted from `properties`).

use std::collections::HashMap;

use crate::rule::Declaration;

use super::calc::*;
use super::parse::*;
use super::values::*;

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
struct StyleFingerprint(u64);

impl StyleFingerprint {
    /// Stores `fingerprint`, remapping `0` to `1` so it cannot be confused with "unset".
    fn new(fingerprint: u64) -> Self {
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
    style_fingerprint: StyleFingerprint,
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

impl ComputedStyle {
    /// Returns the initial computed style before any CSS rules are applied.
    pub fn initial() -> Self {
        Self {
            display: Display::Inline,
            position: Position::Static,
            float: Float::None,
            clear: Clear::None,
            box_sizing: BoxSizing::ContentBox,
            top: Length::Auto,
            right: Length::Auto,
            bottom: Length::Auto,
            left: Length::Auto,
            z_index: None,
            overflow: Overflow::Visible,

            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Auto,
            max_width: Length::Auto,
            min_height: Length::Auto,
            max_height: Length::Auto,

            margin: RectEdges::all(Length::ZERO),
            padding: RectEdges::all(Length::ZERO),
            border_width: RectEdges::all(0.0),
            border_color: RectEdges::all(Color::BLACK),
            border_style: RectEdges::all(BorderStyle::None),

            border_spacing: (0.0, 0.0),
            border_collapse: BorderCollapse::Separate,
            table_layout: TableLayout::Auto,
            caption_side: CaptionSide::Top,
            empty_cells: EmptyCells::Show,

            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            background_image: None,
            background_repeat: BackgroundRepeat::REPEAT,
            background_size: BackgroundSize::Auto,
            background_position: BackgroundPosition::ZERO,
            background_geometry_explicit: false,
            background_layers: Vec::new(),
            border_radius: BorderRadii::default(),
            box_shadows: Vec::new(),
            text_shadows: Vec::new(),
            filter: Vec::new(),
            mix_blend_mode: MixBlendMode::Normal,
            backdrop_filter: Vec::new(),
            aspect_ratio: None,
            container_type: ContainerType::Normal,
            container_names: Vec::new(),
            object_fit: ObjectFit::Fill,
            object_position: BackgroundPosition {
                x: Length::Percent(50.0),
                y: Length::Percent(50.0),
            },

            flex_direction: crate::flex::FlexDirection::Row,
            flex_wrap: crate::flex::FlexWrap::NoWrap,
            justify_content: crate::flex::JustifyContent::FlexStart,
            align_items: crate::flex::AlignItems::Stretch,
            align_self: crate::flex::AlignSelf::Auto,
            row_gap: Length::ZERO,
            column_gap: Length::ZERO,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            is_inline_flex: false,

            grid_template_columns: crate::grid::GridTemplate::default(),
            grid_template_rows: crate::grid::GridTemplate::default(),
            grid_auto_rows: crate::grid::GridTemplate::default(),
            justify_items: crate::flex::AlignItems::Stretch,
            justify_self: crate::flex::AlignSelf::Auto,
            is_inline_grid: false,

            grid_column_start: crate::grid::GridLine::Auto,
            grid_column_end: crate::grid::GridLine::Auto,
            grid_row_start: crate::grid::GridLine::Auto,
            grid_row_end: crate::grid::GridLine::Auto,
            order: 0,
            grid_auto_flow: crate::grid::GridAutoFlow::Row,
            grid_template_areas: None,
            grid_area: None,
            grid_justify_content: crate::grid::GridContentDistribution::Stretch,
            grid_align_content: crate::grid::GridContentDistribution::Stretch,
            calc_lengths: CalcLengths::default(),

            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: "sans-serif".to_string(),
            custom_properties: None,
            line_height: None,
            line_height_factor: None,
            text_align: TextAlign::Left,
            direction: Direction::Ltr,
            unicode_bidi: UnicodeBidi::Normal,
            white_space: WhiteSpace::Normal,
            vertical_align: VerticalAlign::Baseline,
            text_decoration: TextDecoration::NONE,
            list_style_type: ListStyleType::Disc,
            list_style_image: None,

            visibility: Visibility::Visible,
            opacity: 1.0,
            transform: None,

            text_transform: TextTransform::None,
            word_spacing: Length::Px(0.0),
            letter_spacing: Length::Px(0.0),
            content: Content::Normal,
            counter_reset: Vec::new(),
            counter_increment: Vec::new(),
            animations: Vec::new(),
            transitions: Vec::new(),
            width_fit_content: false,
            height_fit_content: false,
            backdrop_color: None,
            style_fingerprint: StyleFingerprint::default(),
        }
    }

    /// Sets `backdrop_color` and refreshes the cached style fingerprint.
    pub fn set_backdrop_color(&mut self, color: Option<Color>) {
        self.backdrop_color = color;
        self.style_fingerprint = StyleFingerprint::new(self.compute_fingerprint());
    }

    /// Returns the cached value fingerprint used by layout's style interning.
    ///
    /// `0` means "not computed" (hand-built styles); layout then calls
    /// [`ComputedStyle::compute_fingerprint`]. The value is only a bucket selector:
    /// interning verifies exact equality before sharing, so collisions are safe.
    #[inline]
    pub fn fingerprint(&self) -> u64 {
        self.style_fingerprint.0
    }

    /// Computes the cheap value fingerprint that layout's style interning uses as a bucket
    /// key. [`ComputedStyle::compute`] caches it in the returned style, so the layout tree
    /// builder never has to re-hash the (large) style struct per element.
    #[inline]
    pub fn compute_fingerprint(&self) -> u64 {
        fn mix(hash: &mut u64, value: u64) {
            *hash ^= value;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        fn mix_len(hash: &mut u64, length: Length) {
            match length {
                Length::Px(px) => {
                    mix(hash, 1);
                    mix(hash, px.to_bits() as u64);
                }
                Length::Percent(percent) => {
                    mix(hash, 2);
                    mix(hash, percent.to_bits() as u64);
                }
                Length::Auto => mix(hash, 3),
            }
        }
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        mix(&mut hash, self.display as u64);
        mix(&mut hash, self.position as u64);
        mix(&mut hash, self.float as u64);
        mix(&mut hash, self.clear as u64);
        mix(&mut hash, self.box_sizing as u64);
        mix(&mut hash, self.overflow as u64);
        mix(&mut hash, self.visibility as u64);
        mix(&mut hash, self.direction as u64);
        mix(&mut hash, self.unicode_bidi as u64);
        mix_len(&mut hash, self.width);
        mix_len(&mut hash, self.height);
        mix_len(&mut hash, self.min_width);
        mix_len(&mut hash, self.max_width);
        mix_len(&mut hash, self.min_height);
        mix_len(&mut hash, self.max_height);
        for edge in [
            self.margin.top,
            self.margin.right,
            self.margin.bottom,
            self.margin.left,
            self.padding.top,
            self.padding.right,
            self.padding.bottom,
            self.padding.left,
        ] {
            mix_len(&mut hash, edge);
        }
        for width in [
            self.border_width.top,
            self.border_width.right,
            self.border_width.bottom,
            self.border_width.left,
        ] {
            mix(&mut hash, width.to_bits() as u64);
        }
        mix(&mut hash, self.font_size.to_bits() as u64);
        mix(&mut hash, u64::from(self.font_weight));
        mix(&mut hash, self.font_style as u64);
        mix(&mut hash, self.font_family.len() as u64);
        mix(
            &mut hash,
            self.line_height.map_or(0, |height| height.to_bits() as u64),
        );
        mix(&mut hash, self.opacity.to_bits() as u64);
        mix(
            &mut hash,
            u64::from(self.color.r) | (u64::from(self.color.g) << 8),
        );
        mix(
            &mut hash,
            u64::from(self.color.b) | (u64::from(self.color.a) << 8),
        );
        mix(
            &mut hash,
            u64::from(self.background_color.r) | (u64::from(self.background_color.g) << 8),
        );
        mix(
            &mut hash,
            u64::from(self.background_color.b) | (u64::from(self.background_color.a) << 8),
        );
        mix(&mut hash, self.text_align as u64);
        mix(&mut hash, self.white_space as u64);
        mix(&mut hash, self.text_transform as u64);
        mix_len(&mut hash, self.letter_spacing);
        mix_len(&mut hash, self.word_spacing);
        mix(&mut hash, self.calc_lengths.width.is_some() as u64);
        mix(&mut hash, self.is_inline_flex as u64);
        mix(&mut hash, self.is_inline_grid as u64);

        fn timing_tag(timing: &crate::animation::TimingFunction) -> u64 {
            use crate::animation::TimingFunction;
            match timing {
                TimingFunction::Linear => 1,
                TimingFunction::Ease => 2,
                TimingFunction::EaseIn => 3,
                TimingFunction::EaseOut => 4,
                TimingFunction::EaseInOut => 5,
                TimingFunction::CubicBezier(x1, y1, x2, y2) => {
                    6 ^ ((x1.to_bits() as u64) << 1)
                        ^ ((y1.to_bits() as u64) << 17)
                        ^ ((x2.to_bits() as u64) << 33)
                        ^ (y2.to_bits() as u64)
                }
                TimingFunction::Steps(count, position) => {
                    7 ^ (u64::from(*count) << 32) ^ (*position as u64)
                }
            }
        }

        for animation in &self.animations {
            for byte in animation.name.as_bytes() {
                mix(&mut hash, u64::from(*byte));
            }
            mix(&mut hash, animation.duration_ms.to_bits() as u64);
            mix(&mut hash, animation.delay_ms.to_bits() as u64);
            mix(&mut hash, timing_tag(&animation.timing));
            mix(&mut hash, animation.direction as u64);
            mix(&mut hash, animation.fill_mode as u64);
            mix(&mut hash, animation.play_state as u64);
            match animation.iteration_count {
                crate::animation::IterationCount::Number(count) => {
                    mix(&mut hash, 8);
                    mix(&mut hash, count.to_bits() as u64);
                }
                crate::animation::IterationCount::Infinite => mix(&mut hash, 9),
            }
        }
        for transition in &self.transitions {
            match &transition.property {
                crate::animation::TransitionProperty::All => mix(&mut hash, 10),
                crate::animation::TransitionProperty::None => mix(&mut hash, 11),
                crate::animation::TransitionProperty::Property(name) => {
                    mix(&mut hash, 12);
                    for byte in name.as_bytes() {
                        mix(&mut hash, u64::from(*byte));
                    }
                }
            }
            mix(&mut hash, transition.duration_ms.to_bits() as u64);
            mix(&mut hash, transition.delay_ms.to_bits() as u64);
            mix(&mut hash, timing_tag(&transition.timing));
        }
        mix(&mut hash, self.width_fit_content as u64);
        mix(&mut hash, self.height_fit_content as u64);
        if let Some(c) = self.backdrop_color {
            mix(&mut hash, 1);
            mix(&mut hash, u64::from(c.r) | (u64::from(c.g) << 8));
            mix(&mut hash, u64::from(c.b) | (u64::from(c.a) << 8));
        }
        match &self.content {
            Content::Normal => mix(&mut hash, 13),
            Content::None => mix(&mut hash, 14),
            Content::Items(items) => {
                mix(&mut hash, 15);
                mix(&mut hash, items.len() as u64);
                for item in items {
                    match item {
                        ContentItem::String(s) => {
                            mix(&mut hash, 16);
                            for &b in s.as_bytes() {
                                mix(&mut hash, u64::from(b));
                            }
                        }
                        ContentItem::Attr(name) => {
                            mix(&mut hash, 17);
                            for &b in name.as_bytes() {
                                mix(&mut hash, u64::from(b));
                            }
                        }
                        ContentItem::Counter(name) => {
                            mix(&mut hash, 18);
                            for &b in name.as_bytes() {
                                mix(&mut hash, u64::from(b));
                            }
                        }
                        ContentItem::Quote(open) => mix(&mut hash, if *open { 19 } else { 20 }),
                        ContentItem::Url(u) => {
                            mix(&mut hash, 21);
                            for &b in u.as_bytes() {
                                mix(&mut hash, u64::from(b));
                            }
                        }
                    }
                }
            }
        }
        mix(&mut hash, self.counter_reset.len() as u64);
        mix(&mut hash, self.counter_increment.len() as u64);
        hash
    }

    /// Computes values given the cascaded declarations for an element and its parent's style.
    pub fn compute(
        cascaded: &HashMap<String, &Declaration>,
        parent: Option<&ComputedStyle>,
        root_style: Option<&ComputedStyle>,
    ) -> Self {
        // Custom properties are lazy: documents without `--*`/`var()` (the vast
        // majority) skip the resolver entirely, borrow the cascaded declarations and share
        // the parent's `Arc<HashMap>` as-is. This keeps style resolution allocation-free in
        // the common case.
        let inherited_custom = parent.and_then(|style| style.custom_properties.clone());
        let needs_resolution = crate::custom::needs_resolution(cascaded);
        let mut resolved_declarations: Vec<(String, Declaration)> = Vec::new();
        let custom_properties = if needs_resolution {
            let (resolved, custom) =
                crate::custom::resolve_declarations(cascaded, inherited_custom.as_ref());
            resolved_declarations = resolved;
            custom
        } else {
            inherited_custom
        };
        let entries: Vec<(&str, &Declaration)> = if needs_resolution {
            resolved_declarations
                .iter()
                .map(|(name, declaration)| (name.as_str(), declaration))
                .collect()
        } else {
            cascaded
                .iter()
                .map(|(name, declaration)| (name.as_str(), *declaration))
                .collect()
        };
        let root_font_size = root_style.map_or(16.0, |r| r.font_size);
        let parent_font_size = parent.map_or(16.0, |p| p.font_size);

        // 1. Compute font-size first because em units depend on it
        let font_size = if let Some(decl) = find_declaration(&entries, "font-size") {
            Self::compute_font_size(&decl.raw_value, parent_font_size, root_font_size)
        } else {
            parent_font_size
        };

        // 2. Inherited properties
        let color = if let Some(decl) = find_declaration(&entries, "color") {
            Color::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(Color::BLACK, |p| p.color))
        } else {
            parent.map_or(Color::BLACK, |p| p.color)
        };

        let font_weight = if let Some(decl) = find_declaration(&entries, "font-weight") {
            Self::parse_font_weight(&decl.raw_value)
        } else {
            parent.map_or(400, |p| p.font_weight)
        };

        let font_style = if let Some(decl) = find_declaration(&entries, "font-style") {
            FontStyle::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(FontStyle::Normal, |p| p.font_style))
        } else {
            parent.map_or(FontStyle::Normal, |p| p.font_style)
        };

        let font_family = if let Some(decl) = find_declaration(&entries, "font-family") {
            decl.raw_value.clone()
        } else {
            parent.map_or_else(|| "sans-serif".to_string(), |p| p.font_family.clone())
        };

        let direction = if let Some(decl) = find_declaration(&entries, "direction") {
            Direction::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(Direction::Ltr, |p| p.direction))
        } else {
            parent.map_or(Direction::Ltr, |p| p.direction)
        };

        let unicode_bidi = if let Some(decl) = find_declaration(&entries, "unicode-bidi") {
            UnicodeBidi::parse(&decl.raw_value).unwrap_or(UnicodeBidi::Normal)
        } else {
            UnicodeBidi::Normal
        };

        let default_align = match direction {
            Direction::Ltr => TextAlign::Left,
            Direction::Rtl => TextAlign::Right,
        };
        let text_align = if let Some(decl) = find_declaration(&entries, "text-align") {
            TextAlign::parse_for_direction(&decl.raw_value, direction).unwrap_or_else(|| {
                parent.map_or(default_align, |p| {
                    if p.direction != direction
                        && ((p.direction == Direction::Ltr && p.text_align == TextAlign::Left)
                            || (p.direction == Direction::Rtl && p.text_align == TextAlign::Right))
                    {
                        default_align
                    } else {
                        p.text_align
                    }
                })
            })
        } else {
            parent.map_or(default_align, |p| {
                if p.direction != direction
                    && ((p.direction == Direction::Ltr && p.text_align == TextAlign::Left)
                        || (p.direction == Direction::Rtl && p.text_align == TextAlign::Right))
                {
                    default_align
                } else {
                    p.text_align
                }
            })
        };

        let (line_height, line_height_factor) =
            if let Some(decl) = find_declaration(&entries, "line-height") {
                match Self::parse_line_height_factor(&decl.raw_value) {
                    Some(factor) => (Some(factor * font_size), Some(factor)),
                    None => (Self::parse_line_height(&decl.raw_value, font_size), None),
                }
            } else {
                // Inherited: the px value is kept as before and the numeric factor (if any) is
                // propagated so layout can re-resolve `factor * font-size` per descendant.
                (
                    parent.and_then(|p| p.line_height),
                    parent.and_then(|p| p.line_height_factor),
                )
            };

        let white_space = if let Some(decl) = find_declaration(&entries, "white-space") {
            WhiteSpace::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(WhiteSpace::Normal, |p| p.white_space))
        } else {
            parent.map_or(WhiteSpace::Normal, |p| p.white_space)
        };

        let text_transform = if let Some(decl) = find_declaration(&entries, "text-transform") {
            TextTransform::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(TextTransform::None, |p| p.text_transform))
        } else {
            parent.map_or(TextTransform::None, |p| p.text_transform)
        };

        let inherited_spacing = |property: &str, parent_value: Option<Length>| {
            if let Some(decl) = find_declaration(&entries, property) {
                let value = decl.raw_value.trim();
                if value.eq_ignore_ascii_case("normal") {
                    Length::Px(0.0)
                } else {
                    match Length::parse(value, font_size, root_font_size) {
                        // Computed values of spacing lengths are absolute: resolve a
                        // percentage against the element font-size (CSS Text 3).
                        Some(Length::Percent(pct)) => Length::Px(font_size * pct / 100.0),
                        Some(length) => length,
                        None => parent_value.unwrap_or(Length::Px(0.0)),
                    }
                }
            } else {
                parent_value.unwrap_or(Length::Px(0.0))
            }
        };
        let word_spacing = inherited_spacing("word-spacing", parent.map(|p| p.word_spacing));
        let letter_spacing = inherited_spacing("letter-spacing", parent.map(|p| p.letter_spacing));

        let visibility = find_declaration(&entries, "visibility")
            .and_then(|d| Visibility::parse(&d.raw_value))
            .unwrap_or_else(|| parent.map_or(Visibility::Visible, |p| p.visibility));

        let list_style_type = if let Some(decl) = find_declaration(&entries, "list-style-type") {
            ListStyleType::parse(&decl.raw_value)
                .unwrap_or_else(|| parent.map_or(ListStyleType::Disc, |p| p.list_style_type))
        } else if let Some(decl) = find_declaration(&entries, "list-style") {
            split_components(&decl.raw_value)
                .iter()
                .find_map(|component| ListStyleType::parse(component))
                .unwrap_or_else(|| parent.map_or(ListStyleType::Disc, |p| p.list_style_type))
        } else {
            parent.map_or(ListStyleType::Disc, |p| p.list_style_type)
        };

        // 3. Non-inherited properties
        let vertical_align = find_declaration(&entries, "vertical-align")
            .and_then(|d| VerticalAlign::parse(&d.raw_value))
            .unwrap_or(VerticalAlign::Baseline);

        let text_decoration = find_declaration(&entries, "text-decoration-line")
            .or_else(|| find_declaration(&entries, "text-decoration"))
            .and_then(|d| TextDecoration::parse(&d.raw_value))
            .unwrap_or(TextDecoration::NONE);

        let display = find_declaration(&entries, "display")
            .and_then(|d| Display::parse(&d.raw_value))
            .unwrap_or(Display::Inline);

        let position = find_declaration(&entries, "position")
            .and_then(|d| Position::parse(&d.raw_value))
            .unwrap_or(Position::Static);

        let float = find_declaration(&entries, "float")
            .and_then(|d| Float::parse(&d.raw_value))
            .unwrap_or(Float::None);

        let clear = find_declaration(&entries, "clear")
            .and_then(|d| Clear::parse(&d.raw_value))
            .unwrap_or(Clear::None);

        let box_sizing = find_declaration(&entries, "box-sizing")
            .and_then(|d| BoxSizing::parse(&d.raw_value))
            .unwrap_or(BoxSizing::ContentBox);

        // Tables (CSS 2.1 §17): none of these are inherited.
        let border_spacing = find_declaration(&entries, "border-spacing")
            .and_then(|d| parse_border_spacing(&d.raw_value, font_size, root_font_size))
            .unwrap_or((0.0, 0.0));
        let border_collapse = find_declaration(&entries, "border-collapse")
            .and_then(|d| BorderCollapse::parse(&d.raw_value))
            .unwrap_or(BorderCollapse::Separate);
        let table_layout = find_declaration(&entries, "table-layout")
            .and_then(|d| TableLayout::parse(&d.raw_value))
            .unwrap_or(TableLayout::Auto);
        let caption_side = find_declaration(&entries, "caption-side")
            .and_then(|d| CaptionSide::parse(&d.raw_value))
            .unwrap_or(CaptionSide::Top);
        let empty_cells = find_declaration(&entries, "empty-cells")
            .and_then(|d| EmptyCells::parse(&d.raw_value))
            .unwrap_or(EmptyCells::Show);
        let computed_flex = crate::flex::compute_flex(&entries, font_size, root_font_size);
        let is_inline_flex = find_declaration(&entries, "display").is_some_and(|declaration| {
            declaration
                .raw_value
                .trim()
                .eq_ignore_ascii_case("inline-flex")
        });
        let computed_grid = crate::grid::compute_grid(&entries, font_size, root_font_size);
        let calc_lengths = Self::compute_calc_lengths(&entries, font_size, root_font_size);
        // `inline-grid` has no dedicated `Display` variant (keeps exhaustive matches
        // in other crates stable): it computes as an inline block plus a flag, like
        // `inline-flex`. The layout crate dispatches the grid algorithm from the flag.
        let display = if computed_grid.is_inline_grid {
            Display::InlineBlock
        } else {
            display
        };

        let background_color = Self::compute_background_color(&entries, color);
        // Images are not inherited; `none`, gradients and invalid values compute to `None`.
        // The scalar fields mirror the first (top-most) background layer so the
        // existing single-image render loader keeps working unchanged.
        let (background_layers, background_geometry_explicit) =
            Self::compute_background_layers(&entries, font_size, root_font_size, color);
        let (filter, mix_blend_mode, backdrop_filter) =
            Self::compute_effects(&entries, font_size, root_font_size, color);
        let (aspect_ratio, object_fit, object_position) =
            Self::compute_replaced_properties(&entries, font_size, root_font_size);
        let container_type = find_declaration(&entries, "container-type")
            .and_then(|decl| ContainerType::parse(&decl.raw_value))
            .unwrap_or(ContainerType::Normal);
        let container_names = find_declaration(&entries, "container-name")
            .map(|decl| {
                decl.raw_value
                    .split_ascii_whitespace()
                    .filter(|name| !name.eq_ignore_ascii_case("none"))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let inherited_text_shadows = parent
            .map(|style| style.text_shadows.as_slice())
            .unwrap_or_default();
        let (border_radius, box_shadows, text_shadows) = Self::compute_decorations(
            &entries,
            font_size,
            root_font_size,
            color,
            inherited_text_shadows,
        );
        let first_layer = background_layers.first().cloned().unwrap_or_default();
        let background_image = first_layer.image.clone();
        let (background_repeat, background_size, background_position) =
            (first_layer.repeat, first_layer.size, first_layer.position);
        let list_style_image = find_declaration(&entries, "list-style-image")
            .and_then(|d| parse_image_value(&d.raw_value));

        let width_decl = find_declaration(&entries, "width");
        let width_fit_content =
            width_decl.is_some_and(|d| d.raw_value.trim().eq_ignore_ascii_case("fit-content"));
        let width = width_decl
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let height_decl = find_declaration(&entries, "height");
        let height_fit_content =
            height_decl.is_some_and(|d| d.raw_value.trim().eq_ignore_ascii_case("fit-content"));
        let height = height_decl
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let min_width = find_declaration(&entries, "min-width")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let max_width = find_declaration(&entries, "max-width")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let min_height = find_declaration(&entries, "min-height")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let max_height = find_declaration(&entries, "max-height")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let top = find_declaration(&entries, "top")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let right = find_declaration(&entries, "right")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let bottom = find_declaration(&entries, "bottom")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let left = find_declaration(&entries, "left")
            .and_then(|d| mixed_length_fallback(&d.raw_value, font_size, root_font_size))
            .unwrap_or(Length::Auto);

        let z_index = find_declaration(&entries, "z-index")
            .and_then(|d| d.raw_value.trim().parse::<i32>().ok());

        // `overflow` is a shorthand for `overflow-x`/`overflow-y`; only the block-start
        // (first) value is stored until layout learns to clip on both axes independently.
        let overflow = find_declaration(&entries, "overflow")
            .and_then(|d| {
                split_components(&d.raw_value)
                    .first()
                    .and_then(|component| Overflow::parse(component))
            })
            .unwrap_or(Overflow::Visible);

        // #1670: `NaN.clamp(0,1)` stays NaN, which would poison paint/alpha blending. Reject
        // all non-finite values before clamping.
        let opacity = find_declaration(&entries, "opacity")
            .and_then(|d| d.raw_value.trim().parse::<f32>().ok())
            .filter(|val| val.is_finite())
            .map(|val| val.clamp(0.0, 1.0))
            .unwrap_or(1.0);

        // CSS `transform` (2D subset, #1539). Not inherited: an element with no declaration
        // carries no transform even when an ancestor has one.
        let transform = find_declaration(&entries, "transform")
            .and_then(|d| crate::transform::Transform::parse(&d.raw_value));

        // CSS Animations 1 / Transitions 1 longhand lists (not inherited).
        let animations = crate::animation::parse_animations(&entries);
        let transitions = crate::animation::parse_transitions(&entries);

        // Generated content (`::before`/`::after`, #1540). Not inherited.
        let content = find_declaration(&entries, "content")
            .and_then(|d| Content::parse(&d.raw_value))
            .unwrap_or(Content::Normal);
        let counter_reset = find_declaration(&entries, "counter-reset")
            .map(|d| parse_counter_list(&d.raw_value, 0))
            .unwrap_or_default();
        let counter_increment = find_declaration(&entries, "counter-increment")
            .map(|d| parse_counter_list(&d.raw_value, 1))
            .unwrap_or_default();

        // Margins & Paddings
        let margin = Self::compute_edges(&entries, &MARGIN_PROPERTIES, font_size, root_font_size);
        let padding = Self::compute_edges(&entries, &PADDING_PROPERTIES, font_size, root_font_size);

        // Borders
        let (border_width, border_color, border_style) =
            Self::compute_borders(&entries, font_size, root_font_size, color);

        let mut style = Self {
            display,
            position,
            float,
            clear,
            box_sizing,
            top,
            right,
            bottom,
            left,
            z_index,
            overflow,
            width,
            height,
            min_width,
            max_width,
            min_height,
            max_height,
            margin,
            padding,
            border_width,
            border_color,
            border_style,
            border_spacing,
            border_collapse,
            table_layout,
            caption_side,
            empty_cells,
            color,
            background_color,
            background_image,
            background_repeat,
            background_size,
            background_position,
            background_geometry_explicit,
            background_layers,
            border_radius,
            box_shadows,
            text_shadows,
            filter,
            mix_blend_mode,
            backdrop_filter,
            aspect_ratio,
            container_type,
            container_names,
            object_fit,
            object_position,

            flex_direction: computed_flex.direction,
            flex_wrap: computed_flex.wrap,
            justify_content: computed_flex.justify_content,
            align_items: computed_flex.align_items,
            align_self: computed_flex.align_self,
            row_gap: computed_flex.row_gap,
            column_gap: computed_flex.column_gap,
            flex_grow: computed_flex.flex_grow,
            flex_shrink: computed_flex.flex_shrink,
            flex_basis: computed_flex.flex_basis,
            is_inline_flex,

            grid_template_columns: computed_grid.template_columns,
            grid_template_rows: computed_grid.template_rows,
            grid_auto_rows: computed_grid.auto_rows,
            justify_items: computed_grid.justify_items,
            justify_self: computed_grid.justify_self,
            is_inline_grid: computed_grid.is_inline_grid,

            grid_column_start: computed_grid.column_start,
            grid_column_end: computed_grid.column_end,
            grid_row_start: computed_grid.row_start,
            grid_row_end: computed_grid.row_end,
            order: computed_grid.order,
            grid_auto_flow: computed_grid.auto_flow,
            grid_template_areas: computed_grid.template_areas,
            grid_area: computed_grid.grid_area,
            grid_justify_content: computed_grid.justify_content,
            grid_align_content: computed_grid.align_content,
            calc_lengths,
            font_size,
            font_weight,
            font_style,
            font_family,
            custom_properties,
            line_height,
            line_height_factor,
            text_align,
            direction,
            unicode_bidi,
            white_space,
            vertical_align,
            text_decoration,
            list_style_type,
            list_style_image,
            visibility,
            opacity,
            transform,
            text_transform,
            word_spacing,
            letter_spacing,
            content,
            counter_reset,
            counter_increment,
            animations,
            transitions,
            width_fit_content,
            height_fit_content,
            backdrop_color: None,
            style_fingerprint: StyleFingerprint::default(),
        };
        style.style_fingerprint = StyleFingerprint::new(style.compute_fingerprint());
        style
    }

    /// Inherits properties from a parent style according to CSS cascading rules.
    pub fn inherit_from(&mut self, parent: &ComputedStyle) {
        self.color = parent.color;
        self.font_size = parent.font_size;
        self.font_weight = parent.font_weight;
        self.font_style = parent.font_style;
        self.font_family = parent.font_family.clone();
        self.line_height = parent.line_height;
        self.line_height_factor = parent.line_height_factor;
        self.text_align = parent.text_align;
        self.white_space = parent.white_space;
        self.text_transform = parent.text_transform;
        self.word_spacing = parent.word_spacing;
        self.letter_spacing = parent.letter_spacing;
        self.visibility = parent.visibility;
        self.list_style_type = parent.list_style_type;
        // `text-decoration` is *not* inherited (CSS Text Decoration 3 §2): decorations
        // propagate visually through descendant inline boxes but do not affect their
        // computed style, and layout/paint can walk the ancestor chain when needed.
        self.text_decoration = TextDecoration::NONE;
        // `text-shadow` is inherited (CSS Text Decoration 3 §3); `box-shadow` and
        // `border-radius` are not.
        self.text_shadows = parent.text_shadows.clone();
    }

    fn compute_font_size(val: &str, parent_size: f32, root_size: f32) -> f32 {
        let val = val.trim().to_ascii_lowercase();
        match val.as_str() {
            "xx-small" => 9.0,
            "x-small" => 10.0,
            "small" => 13.0,
            "medium" => 16.0,
            "large" => 18.0,
            "x-large" => 24.0,
            "xx-large" => 32.0,
            "smaller" => parent_size / 1.2,
            "larger" => parent_size * 1.2,
            _ => {
                if let Some(Length::Px(px)) = Length::parse(&val, parent_size, root_size) {
                    px
                } else if let Some(Length::Percent(pct)) =
                    Length::parse(&val, parent_size, root_size)
                {
                    (pct / 100.0) * parent_size
                } else {
                    parent_size
                }
            }
        }
    }

    fn parse_font_weight(val: &str) -> u16 {
        match val.trim().to_ascii_lowercase().as_str() {
            "normal" => 400,
            "bold" => 700,
            "bolder" => 700,
            "lighter" => 300,
            // CSS Fonts 4: numeric font-weight must be in 1..=1000 (#1676).
            other => other
                .parse::<u16>()
                .ok()
                .filter(|weight| (1..=1000).contains(weight))
                .unwrap_or(400),
        }
    }

    /// Returns the unitless `<number>` factor of a `line-height` declaration, if any.
    fn parse_line_height_factor(val: &str) -> Option<f32> {
        let val = val.trim();
        if val.eq_ignore_ascii_case("normal") || val.ends_with('%') {
            return None;
        }
        // #1671: reject NaN/infinity and negative numbers, not just `>= 0`.
        val.parse::<f32>()
            .ok()
            .filter(|factor| factor.is_finite() && *factor >= 0.0)
    }

    fn parse_line_height(val: &str, font_size: f32) -> Option<f32> {
        let val = val.trim().to_ascii_lowercase();
        if val == "normal" {
            return Some(font_size * 1.2);
        }
        // #1671: a numeric line-height must be finite and non-negative; otherwise CSS falls
        // through to Length::parse, and an invalid value yields None (initial used).
        if let Ok(factor) = val.parse::<f32>()
            && factor.is_finite()
            && factor >= 0.0
        {
            return Some(factor * font_size);
        }
        match Length::parse(&val, font_size, font_size) {
            Some(Length::Px(px)) => Some(px),
            // CSS 2.2 §10.8.1: percentage line-height computes against the element font-size.
            Some(Length::Percent(pct)) => Some(font_size * pct / 100.0),
            _ => None,
        }
    }

    fn compute_edges(
        entries: &[(&str, &Declaration)],
        names: &EdgeProperties,
        font_size: f32,
        root_font_size: f32,
    ) -> RectEdges<Length> {
        let mut edges = RectEdges::all(Length::ZERO);

        // 1. Shorthand (e.g. `margin: 8px` or `margin: 10px 20px`). Shorthands are normally
        //    expanded before the cascade; this keeps direct `compute` callers working.
        if let Some(decl) = find_declaration(entries, names.shorthand) {
            let mut parsed = [Length::ZERO; 4];
            let mut count = 0usize;
            for component in decl.raw_value.split_ascii_whitespace() {
                let Some(length) = Length::parse(component, font_size, root_font_size) else {
                    break;
                };
                if count < parsed.len() {
                    parsed[count] = length;
                }
                count += 1;
            }
            match count {
                1 => edges = RectEdges::all(parsed[0]),
                2 => {
                    edges.top = parsed[0];
                    edges.bottom = parsed[0];
                    edges.right = parsed[1];
                    edges.left = parsed[1];
                }
                3 => {
                    edges.top = parsed[0];
                    edges.right = parsed[1];
                    edges.left = parsed[1];
                    edges.bottom = parsed[2];
                }
                4 => {
                    edges.top = parsed[0];
                    edges.right = parsed[1];
                    edges.bottom = parsed[2];
                    edges.left = parsed[3];
                }
                _ => {}
            }
        }

        // 2. Logical block/inline properties (e.g. `margin-block: 1em`).
        if let Some(decl) = find_declaration(entries, names.block)
            && let Some(length) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.top = length;
            edges.bottom = length;
        }
        if let Some(decl) = find_declaration(entries, names.inline)
            && let Some(length) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.left = length;
            edges.right = length;
        }

        // 3. Specific longhands (e.g. `margin-top`), via a static name table.
        let mut sides = [edges.top, edges.right, edges.bottom, edges.left];
        for (index, name) in names.longhands.iter().enumerate() {
            if let Some(decl) = find_declaration(entries, name)
                && let Some(length) = Length::parse(&decl.raw_value, font_size, root_font_size)
            {
                sides[index] = length;
            }
        }

        RectEdges {
            top: sides[0],
            right: sides[1],
            bottom: sides[2],
            left: sides[3],
        }
    }

    /// Computes `filter` (with the `-webkit-filter` alias) and `mix-blend-mode`.
    ///
    /// Neither property is inherited (Filter Effects 1 §2 / Compositing 1 §5).
    /// Invalid values fall back to the initial values (`none`, `normal`).
    fn compute_effects(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
    ) -> (Vec<FilterFunction>, MixBlendMode, Vec<FilterFunction>) {
        // One linear scan over the cascaded declarations instead of five
        // `find_declaration` lookups per element (R173).
        let mut filter_decl = None;
        let mut webkit_filter_decl = None;
        let mut backdrop_decl = None;
        let mut webkit_backdrop_decl = None;
        let mut mix_decl = None;
        for (name, declaration) in entries {
            match *name {
                "filter" => filter_decl = Some(*declaration),
                "-webkit-filter" => webkit_filter_decl = Some(*declaration),
                "backdrop-filter" => backdrop_decl = Some(*declaration),
                "-webkit-backdrop-filter" => webkit_backdrop_decl = Some(*declaration),
                "mix-blend-mode" => mix_decl = Some(*declaration),
                _ => {}
            }
        }
        let parse_list = |declaration: Option<&Declaration>| {
            declaration
                .map(|decl| {
                    parse_filter_list(&decl.raw_value, font_size, root_font_size, current_color)
                })
                .unwrap_or_default()
        };
        let filter = parse_list(filter_decl.or(webkit_filter_decl));
        let backdrop_filter = {
            let parsed = parse_list(backdrop_decl);
            if parsed.is_empty() {
                parse_list(webkit_backdrop_decl)
            } else {
                parsed
            }
        };
        let mix_blend_mode = mix_decl
            .and_then(|decl| MixBlendMode::parse(&decl.raw_value))
            .unwrap_or_default();
        (filter, mix_blend_mode, backdrop_filter)
    }

    /// Computes `aspect-ratio`, `object-fit` and `object-position` (CSS Box Sizing
    /// 4 §5 / CSS Images 3 §5.5). None of them is inherited.
    fn compute_replaced_properties(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
    ) -> (Option<AspectRatio>, ObjectFit, BackgroundPosition) {
        let aspect_ratio = find_declaration(entries, "aspect-ratio")
            .and_then(|decl| parse_aspect_ratio(&decl.raw_value));
        let object_fit = find_declaration(entries, "object-fit")
            .and_then(|decl| ObjectFit::parse(&decl.raw_value))
            .unwrap_or_default();
        let object_position = find_declaration(entries, "object-position")
            .and_then(|decl| BackgroundPosition::parse(&decl.raw_value, font_size, root_font_size))
            .unwrap_or(BackgroundPosition {
                x: Length::Percent(50.0),
                y: Length::Percent(50.0),
            });
        (aspect_ratio, object_fit, object_position)
    }

    /// Computes `border-radius`, `box-shadow` and `text-shadow` (CSS Backgrounds 3
    /// §5 and CSS Text Decoration 3 §3).
    ///
    /// Shorthands are expanded before the cascade; longhands take precedence over
    /// the `border-radius` shorthand for direct [`ComputedStyle::compute`] callers.
    /// Invalid components fall back to the initial value (`0`, `none`). The lists
    /// are capped at [`MAX_BOX_SHADOWS`]/[`MAX_TEXT_SHADOWS`] layers (documented
    /// paint limits) and blur radii are clamped at paint time.
    fn compute_decorations(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
        inherited_text_shadows: &[TextShadow],
    ) -> (BorderRadii, Vec<BoxShadow>, Vec<TextShadow>) {
        let shorthand_radius = find_declaration(entries, "border-radius").and_then(|decl| {
            parse_border_radius_shorthand(&decl.raw_value, font_size, root_font_size)
        });
        let mut radii = shorthand_radius.unwrap_or_default();
        for (corner, names) in [
            (&mut radii.top_left, ["border-top-left-radius"]),
            (&mut radii.top_right, ["border-top-right-radius"]),
            (&mut radii.bottom_right, ["border-bottom-right-radius"]),
            (&mut radii.bottom_left, ["border-bottom-left-radius"]),
        ] {
            if let Some(decl) = find_declaration(entries, names[0])
                && let Some(radius) = Radius::parse(&decl.raw_value, font_size, root_font_size)
            {
                *corner = radius;
            }
        }
        let box_shadows = find_declaration(entries, "box-shadow")
            .and_then(|decl| {
                parse_box_shadows(&decl.raw_value, font_size, root_font_size, current_color)
            })
            .unwrap_or_default();
        let text_shadows = find_declaration(entries, "text-shadow")
            .and_then(|decl| {
                parse_text_shadows(&decl.raw_value, font_size, root_font_size, current_color)
            })
            .unwrap_or_else(|| inherited_text_shadows.to_vec());
        (radii, box_shadows, text_shadows)
    }

    /// Computes the `background-image` layers (CSS Backgrounds 3 §3).
    ///
    /// Returns the layers **top-first** and whether any background geometry was
    /// authored (repeat/size/position/origin/clip/attachment longhands, the
    /// `background` shorthand, or more than one image layer). Documents that only
    /// declare a single `background-image` keep
    /// [`background_geometry_explicit`](Self::background_geometry_explicit) `false`
    /// so the painter keeps the legacy stretch behaviour of the existing goldens.
    ///
    /// The number of layers is determined by the `background-image` list; other
    /// longhand lists are repeated cyclically (CSS Backgrounds 3 §3.1).
    fn compute_background_layers(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
    ) -> (Vec<BackgroundLayer>, bool) {
        let image_decl = find_declaration(entries, "background-image");
        let repeat_decl = find_declaration(entries, "background-repeat");
        let size_decl = find_declaration(entries, "background-size");
        let position_decl = find_declaration(entries, "background-position");
        let origin_decl = find_declaration(entries, "background-origin");
        let clip_decl = find_declaration(entries, "background-clip")
            .or_else(|| find_declaration(entries, "-webkit-background-clip"));
        let attachment_decl = find_declaration(entries, "background-attachment");
        let shorthand = find_declaration(entries, "background");

        // Each comma-separated image may be a plain `url()` or a gradient.
        let image_values: Vec<(Option<String>, Option<std::sync::Arc<Gradient>>)> = image_decl
            .map(|decl| {
                split_top_level_commas(&decl.raw_value)
                    .iter()
                    .map(|component| {
                        parse_background_image_component(
                            component,
                            font_size,
                            root_font_size,
                            current_color,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        const MAX_BACKGROUND_LAYERS: usize = 32;
        let image_values: Vec<_> = image_values
            .into_iter()
            .take(MAX_BACKGROUND_LAYERS)
            .collect();
        let layers_count = image_values.len().max(1);
        // The `background` shorthand is expanded before the cascade; this fallback
        // keeps direct `ComputedStyle::compute` callers working (single layer).
        let shorthand_tokens =
            shorthand.and_then(|decl| parse_background_shorthand_tokens(&decl.raw_value));

        let explicit = repeat_decl.is_some()
            || size_decl.is_some()
            || position_decl.is_some()
            || origin_decl.is_some()
            || clip_decl.is_some()
            || find_declaration(entries, "-webkit-background-clip").is_some()
            || attachment_decl.is_some()
            || shorthand.is_some()
            || layers_count > 1
            || image_values.iter().any(|(_, gradient)| gradient.is_some());

        let repeats = cycle_property_list(
            repeat_decl,
            BackgroundRepeat::parse,
            layers_count,
            BackgroundRepeat::REPEAT,
        );
        let sizes = cycle_property_list(
            size_decl,
            |value| BackgroundSize::parse(value, font_size, root_font_size),
            layers_count,
            BackgroundSize::Auto,
        );
        let positions = cycle_property_list(
            position_decl,
            |value| BackgroundPosition::parse(value, font_size, root_font_size),
            layers_count,
            BackgroundPosition::ZERO,
        );
        let origins = cycle_property_list(
            origin_decl,
            BackgroundOrigin::parse,
            layers_count,
            BackgroundOrigin::PaddingBox,
        );
        let clips = cycle_property_list(
            clip_decl,
            BackgroundClip::parse,
            layers_count,
            BackgroundClip::BorderBox,
        );
        let attachments = cycle_property_list(
            attachment_decl,
            BackgroundAttachment::parse,
            layers_count,
            BackgroundAttachment::Scroll,
        );

        let mut layers = Vec::with_capacity(layers_count);
        let mut gradient_count = 0usize;
        for index in 0..layers_count {
            let (image, mut gradient) = image_values
                .get(index.min(image_values.len().saturating_sub(1)))
                .cloned()
                .unwrap_or((None, None));
            // Cap the number of gradient layers per computed style.
            if gradient.is_some() {
                gradient_count += 1;
                if gradient_count > MAX_GRADIENTS {
                    gradient = None;
                }
            }
            let image = image.or_else(|| {
                shorthand_tokens
                    .as_ref()
                    .and_then(|tokens| tokens.image.clone())
                    .and_then(|value| parse_image_value(&value))
            });
            layers.push(BackgroundLayer {
                image,
                gradient,
                repeat: repeats[index],
                size: sizes[index],
                position: positions[index],
                origin: origins[index],
                clip: clips[index],
                attachment: attachments[index],
            });
        }
        (layers, explicit)
    }

    /// Extracts the background color from `background-color` or the `background` shorthand.
    ///
    /// The `background` shorthand accepts a `<color>` in any position (CSS Backgrounds 3 §3:
    /// e.g. `background: #eee`, `background: url(bg.png) no-repeat red`); a missing or
    /// unsupported color component computes to `transparent`.
    fn compute_background_color(entries: &[(&str, &Declaration)], current_color: Color) -> Color {
        if let Some(decl) = find_declaration(entries, "background-color")
            && let Some(color) = parse_color_component(&decl.raw_value, current_color)
        {
            return color;
        }
        if let Some(decl) = find_declaration(entries, "background")
            && let Some(color) = split_components(&decl.raw_value)
                .iter()
                .find_map(|component| parse_color_component(component, current_color))
        {
            return color;
        }
        Color::TRANSPARENT
    }

    /// Computes the four border widths, colors and styles per CSS 2.2 §8.5.
    ///
    /// Supported syntax, in cascade-precedence order (highest last):
    ///
    /// 1. `border: <width> || <style> || <color>` (any order).
    /// 2. `border-width` / `border-style` / `border-color` with one to four values,
    ///    expanded in top/right/bottom/left order.
    /// 3. `border-{top,right,bottom,left}: <width> || <style> || <color>` (any order).
    /// 4. `border-{side}-width` / `border-{side}-style` / `border-{side}-color` longhands.
    ///
    /// Shorthands are normally expanded before the cascade (see [`crate::shorthand`]); the
    /// shorthand handling here keeps direct [`ComputedStyle::compute`] callers working.
    ///
    /// The initial border width is `medium` (3 px); finally, CSS 2.2 §8.5.3 requires a computed
    /// width of zero when the computed style of that side is `none` or `hidden`, which
    /// suppresses the phantom borders previously painted for `border-style: none` elements.
    fn compute_borders(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
    ) -> (RectEdges<f32>, RectEdges<Color>, RectEdges<BorderStyle>) {
        /// CSS 2.2 §8.5.1 initial `border-width` value (`medium`).
        const INITIAL_BORDER_WIDTH: f32 = 3.0;
        let mut widths = [INITIAL_BORDER_WIDTH; 4];
        let mut styles = [BorderStyle::None; 4];
        let mut colors = [current_color; 4];

        // 1. The four-sided `border` shorthand.
        if let Some(decl) = find_declaration(entries, "border")
            && let Some(spec) =
                parse_border_shorthand(&decl.raw_value, font_size, root_font_size, current_color)
        {
            if let Some(width) = spec.width {
                widths = [width; 4];
            }
            if let Some(style) = spec.style {
                styles = [style; 4];
            }
            if let Some(color) = spec.color {
                colors = [color; 4];
            }
        }

        // 2. One-to-four-value `border-width` / `border-style` / `border-color` shorthands.
        if let Some(decl) = find_declaration(entries, "border-width")
            && let Some(edges) =
                parse_border_width_edges(&decl.raw_value, font_size, root_font_size)
        {
            widths = [edges.top, edges.right, edges.bottom, edges.left];
        }
        if let Some(decl) = find_declaration(entries, "border-style")
            && let Some(edges) = parse_border_style_edges(&decl.raw_value)
        {
            styles = [edges.top, edges.right, edges.bottom, edges.left];
        }
        if let Some(decl) = find_declaration(entries, "border-color")
            && let Some(edges) = parse_border_color_edges(&decl.raw_value, current_color)
        {
            colors = [edges.top, edges.right, edges.bottom, edges.left];
        }

        // 3. Per-side shorthands and longhands (the latter win, as they are more specific).
        for (index, side) in BORDER_SIDE_PROPERTIES.iter().enumerate() {
            if let Some(decl) = find_declaration(entries, side.shorthand)
                && let Some(spec) = parse_border_shorthand(
                    &decl.raw_value,
                    font_size,
                    root_font_size,
                    current_color,
                )
            {
                if let Some(width) = spec.width {
                    widths[index] = width;
                }
                if let Some(style) = spec.style {
                    styles[index] = style;
                }
                if let Some(color) = spec.color {
                    colors[index] = color;
                }
            }
            if let Some(decl) = find_declaration(entries, side.width)
                && let Some(width) = parse_border_width(&decl.raw_value, font_size, root_font_size)
            {
                widths[index] = width;
            }
            if let Some(decl) = find_declaration(entries, side.style)
                && let Some(style) = BorderStyle::parse(&decl.raw_value)
            {
                styles[index] = style;
            }
            if let Some(decl) = find_declaration(entries, side.color)
                && let Some(color) = parse_color_component(&decl.raw_value, current_color)
            {
                colors[index] = color;
            }
        }

        // 4. `border-style: none | hidden` => computed border width is zero (CSS 2.2 §8.5.3).
        let width_or_zero = |style: BorderStyle, width: f32| match style {
            BorderStyle::None | BorderStyle::Hidden => 0.0,
            _ => width,
        };
        let widths = RectEdges {
            top: width_or_zero(styles[0], widths[0]),
            right: width_or_zero(styles[1], widths[1]),
            bottom: width_or_zero(styles[2], widths[2]),
            left: width_or_zero(styles[3], widths[3]),
        };

        (
            widths,
            RectEdges {
                top: colors[0],
                right: colors[1],
                bottom: colors[2],
                left: colors[3],
            },
            RectEdges {
                top: styles[0],
                right: styles[1],
                bottom: styles[2],
                left: styles[3],
            },
        )
    }
}
