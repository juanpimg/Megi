//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

impl ComputedStyle {
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

        let text_align = TextAlign::resolve_inherited(
            direction,
            find_declaration(&entries, "text-align").map(|decl| decl.raw_value.as_str()),
            parent.map(|parent| (parent.direction, parent.text_align)),
        );

        let (line_height, line_height_factor) =
            if let Some(decl) = find_declaration(&entries, "line-height") {
                match Self::parse_line_height_factor(&decl.raw_value) {
                    Some(factor) if (factor * font_size).is_finite() => {
                        (Some(factor * font_size), Some(factor))
                    }
                    Some(_) => (None, None),
                    None => (
                        Self::parse_line_height(&decl.raw_value, font_size, root_font_size),
                        None,
                    ),
                }
            } else {
                // Inherited: the px value stays and the factor (`<number>` or `normal` = 1.2)
                // is propagated so layout re-resolves it per element (CSS 2.2 §10.8.1).
                (
                    parent.and_then(|p| p.line_height),
                    parent.and_then(|p| p.line_height_factor),
                )
            };

        let text_overflow = find_declaration(&entries, "text-overflow")
            .and_then(|d| TextOverflow::parse(&d.raw_value))
            .unwrap_or_else(|| parent.map_or(TextOverflow::Clip, |p| p.text_overflow));

        let white_space = find_declaration(&entries, "white-space")
            .and_then(|d| WhiteSpace::parse(&d.raw_value))
            .unwrap_or_else(|| parent.map_or(WhiteSpace::Normal, |p| p.white_space));
        let writing_mode = find_declaration(&entries, "writing-mode")
            .and_then(|d| WritingMode::parse(&d.raw_value))
            .unwrap_or_else(|| parent.map_or(WritingMode::HorizontalTb, |p| p.writing_mode));
        // CSS Multi-column §5 (#1932): positive integer.
        let column_count = find_declaration(&entries, "column-count")
            .and_then(|d| d.raw_value.trim().parse::<u32>().ok().filter(|n| *n > 0))
            .unwrap_or_else(|| parent.map_or(1, |p| p.column_count));
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
        // CSS `transform-origin` (#1934). Not inherited: absent/malformed values keep
        // the initial `50% 50%` rather than copying the parent.
        let transform_origin = find_declaration(&entries, "transform-origin")
            .and_then(|d| {
                crate::transform_origin::TransformOrigin::parse(
                    &d.raw_value,
                    font_size,
                    root_font_size,
                )
            })
            .unwrap_or(crate::transform_origin::TransformOrigin::INITIAL);

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
            max_width_from_ua: false, // set by the cascade, which knows the declaration origin
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
            column_count,
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
            text_overflow,
            direction,
            unicode_bidi,
            white_space,
            writing_mode,
            vertical_align,
            text_decoration,
            list_style_type,
            list_style_image,
            visibility,
            opacity,
            transform,
            transform_origin,
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
}
