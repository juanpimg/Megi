//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

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
            max_width_from_ua: false,
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
            column_count: 1,
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
            text_overflow: TextOverflow::Clip,
            direction: Direction::Ltr,
            unicode_bidi: UnicodeBidi::Normal,
            white_space: WhiteSpace::Normal,
            writing_mode: WritingMode::HorizontalTb,
            vertical_align: VerticalAlign::Baseline,
            text_decoration: TextDecoration::NONE,
            list_style_type: ListStyleType::Disc,
            list_style_image: None,

            visibility: Visibility::Visible,
            opacity: 1.0,
            transform: None,
            transform_origin: crate::transform_origin::TransformOrigin::INITIAL,

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

    /// Recomputes and caches the layout-interning style fingerprint after mutating fields.
    pub fn refresh_fingerprint(&mut self) {
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
        mix(&mut hash, self.text_overflow as u64);
        mix(&mut hash, self.white_space as u64);
        mix(&mut hash, self.writing_mode as u64);
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
}
