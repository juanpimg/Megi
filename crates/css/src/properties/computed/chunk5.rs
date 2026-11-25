//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

impl ComputedStyle {
    /// CSS Masking L1 §3.1: `mask-image` confines the painted area of a box to the
    /// mask's alpha channel. Megi does not rasterise masks yet; the mask resource is
    /// used as a plain background image instead, while the background colour stays
    /// transparent so a masked icon paints its glyph and not a solid colour square
    /// (Wikipedia Vector 2022 / Codex icons declare both properties together).
    pub(crate) fn mask_image_url(entries: &[(&str, &Declaration)]) -> Option<String> {
        let declaration = find_declaration(entries, "mask-image")
            .or_else(|| find_declaration(entries, "-webkit-mask-image"))?;
        let value = declaration.raw_value.trim();
        if value.is_empty() || value.eq_ignore_ascii_case("none") {
            return None;
        }
        parse_url_component(value)
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
    pub(crate) fn compute_background_layers(
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
        // Fallback for unsupported masking: paint the mask resource itself when no
        // author background image exists, so mask-only icons remain visible.
        let mut image_values = image_values;
        if image_values.is_empty()
            && let Some(url) = Self::mask_image_url(entries)
        {
            image_values.push((Some(url), None));
        }
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

        // CSS Masking 1 §6.1: a single `mask-image` applies to the whole box,
        // independent of the background layer list. It lives on each layer so
        // `ComputedStyle`'s literal (chunk3.rs) stays unchanged.
        let mask_image = super::super::mask::compute_mask_image(entries);
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
                mask_image: mask_image.clone(),
            });
        }
        (layers, explicit)
    }

    /// Extracts the background color from `background-color` or the `background` shorthand.
    ///
    /// The `background` shorthand accepts a `<color>` in any position (CSS Backgrounds 3 §3:
    /// e.g. `background: #eee`, `background: url(bg.png) no-repeat red`); a missing or
    /// unsupported color component computes to `transparent`.
    pub(crate) fn compute_background_color(
        entries: &[(&str, &Declaration)],
        current_color: Color,
    ) -> Color {
        // CSS Masking L1 §3.1: the colour is only visible through the mask. Without
        // mask rasterisation, painting it unmasked fills the whole icon box with a
        // solid square (Wikipedia Vector/CDX icons declare mask + colour together).
        if Self::mask_image_url(entries).is_some() {
            return Color::TRANSPARENT;
        }
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
    pub(crate) fn compute_borders(
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
