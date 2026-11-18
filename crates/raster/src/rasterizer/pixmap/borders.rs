//! Antialiased CSS border bands, dashes and mitred corners.

#[allow(unused_imports)]
use super::*;

impl Pixmap {
    /// Draws a rectangular border with one [`BorderSide`] per CSS side
    /// (order: top, right, bottom, left).
    ///
    /// The four bands and the four mitred corner squares partition the border
    /// ring exactly (no double blending). `solid`, `dashed`, `dotted`,
    /// `double` and `none`/`hidden` are implemented; the 3D styles
    /// (`groove`/`ridge`/`inset`/`outset`) fall back to `solid`.
    ///
    /// CSS 2.2 §8.5.2: if the sum of two opposing border widths exceeds the
    /// border box dimension, both are reduced proportionally.
    pub fn draw_border(&mut self, rect: Rect, sides: [BorderSide; 4]) {
        if rect.width <= 0.0
            || rect.height <= 0.0
            || !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.width.is_finite()
            || !rect.height.is_finite()
        {
            return;
        }

        let mut widths = [
            sides[0].width.max(0.0),
            sides[1].width.max(0.0),
            sides[2].width.max(0.0),
            sides[3].width.max(0.0),
        ];
        let vertical = widths[0] + widths[2];
        if vertical > rect.height {
            let scale = rect.height / vertical;
            widths[0] *= scale;
            widths[2] *= scale;
        }
        let horizontal = widths[1] + widths[3];
        if horizontal > rect.width {
            let scale = rect.width / horizontal;
            widths[1] *= scale;
            widths[3] *= scale;
        }
        let mut sides = sides;
        for (side, width) in sides.iter_mut().zip(widths) {
            side.width = width;
        }
        let [top, right, bottom, left] = sides;
        if ![top, right, bottom, left]
            .iter()
            .any(|side| side.is_visible())
        {
            return;
        }

        let x0 = rect.x;
        let y0 = rect.y;
        let x1 = rect.x + rect.width;
        let y1 = rect.y + rect.height;

        // Bands, excluding the corner squares.
        let inner_left = x0 + left.width;
        let inner_right = x1 - right.width;
        let inner_top = y0 + top.width;
        let inner_bottom = y1 - bottom.width;
        if inner_right > inner_left {
            if top.width > 0.0 {
                self.draw_band_horizontal(
                    inner_left,
                    inner_right,
                    y0,
                    top.width,
                    top.style,
                    top.color,
                    x0,
                );
            }
            if bottom.width > 0.0 {
                self.draw_band_horizontal(
                    inner_left,
                    inner_right,
                    inner_bottom,
                    bottom.width,
                    bottom.style,
                    bottom.color,
                    x0,
                );
            }
        }
        if inner_bottom > inner_top {
            if left.width > 0.0 {
                self.draw_band_vertical(
                    inner_top,
                    inner_bottom,
                    x0,
                    left.width,
                    left.style,
                    left.color,
                    y0,
                );
            }
            if right.width > 0.0 {
                self.draw_band_vertical(
                    inner_top,
                    inner_bottom,
                    inner_right,
                    right.width,
                    right.style,
                    right.color,
                    y0,
                );
            }
        }

        // Mitred corners: the adjacent sides partition the corner square along
        // the outer→inner diagonal.
        if top.width > 0.0 && left.width > 0.0 {
            self.draw_corner(
                Rect::new(x0, y0, left.width, top.width),
                top.color,
                left.color,
                Corner::TopLeft,
            );
        }
        if top.width > 0.0 && right.width > 0.0 {
            self.draw_corner(
                Rect::new(inner_right, y0, right.width, top.width),
                top.color,
                right.color,
                Corner::TopRight,
            );
        }
        if bottom.width > 0.0 && left.width > 0.0 {
            self.draw_corner(
                Rect::new(x0, inner_bottom, left.width, bottom.width),
                bottom.color,
                left.color,
                Corner::BottomLeft,
            );
        }
        if bottom.width > 0.0 && right.width > 0.0 {
            self.draw_corner(
                Rect::new(inner_right, inner_bottom, right.width, bottom.width),
                bottom.color,
                right.color,
                Corner::BottomRight,
            );
        }
    }

    /// Paints one horizontal border band (top/bottom), applying its style
    /// pattern with the phase anchored at `x_origin`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_band_horizontal(
        &mut self,
        x_start: f32,
        x_end: f32,
        y: f32,
        thickness: f32,
        style: megi_display::BorderStyle,
        color: Color,
        x_origin: f32,
    ) {
        match style {
            megi_display::BorderStyle::Dashed => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(x_start, x_end, unit * 3.0, unit * 3.0, x_origin)
                {
                    self.fill_rect(Rect::new(start, y, end - start, thickness), color);
                }
            }
            megi_display::BorderStyle::Dotted => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(x_start, x_end, unit, unit * 2.0, x_origin) {
                    self.fill_rect(Rect::new(start, y, end - start, thickness), color);
                }
            }
            megi_display::BorderStyle::Double => {
                let line = thickness / 3.0;
                self.fill_rect(Rect::new(x_start, y, x_end - x_start, line), color);
                self.fill_rect(
                    Rect::new(x_start, y + line * 2.0, x_end - x_start, line),
                    color,
                );
            }
            _ => self.fill_rect(Rect::new(x_start, y, x_end - x_start, thickness), color),
        }
    }

    /// Paints one vertical border band (left/right), applying its style
    /// pattern with the phase anchored at `y_origin`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_band_vertical(
        &mut self,
        y_start: f32,
        y_end: f32,
        x: f32,
        thickness: f32,
        style: megi_display::BorderStyle,
        color: Color,
        y_origin: f32,
    ) {
        match style {
            megi_display::BorderStyle::Dashed => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(y_start, y_end, unit * 3.0, unit * 3.0, y_origin)
                {
                    self.fill_rect(Rect::new(x, start, thickness, end - start), color);
                }
            }
            megi_display::BorderStyle::Dotted => {
                let unit = thickness.max(1.0);
                for (start, end) in dash_segments(y_start, y_end, unit, unit * 2.0, y_origin) {
                    self.fill_rect(Rect::new(x, start, thickness, end - start), color);
                }
            }
            megi_display::BorderStyle::Double => {
                let line = thickness / 3.0;
                self.fill_rect(Rect::new(x, y_start, line, y_end - y_start), color);
                self.fill_rect(
                    Rect::new(x + line * 2.0, y_start, line, y_end - y_start),
                    color,
                );
            }
            _ => self.fill_rect(Rect::new(x, y_start, thickness, y_end - y_start), color),
        }
    }

    /// Paints one mitred corner square, splitting it between the two adjacent
    /// sides along the outer→inner diagonal with antialiased coverage.
    pub(crate) fn draw_corner(
        &mut self,
        rect: Rect,
        color_a: Color,
        color_b: Color,
        corner: Corner,
    ) {
        let Some(clip) = self.clip_rect() else {
            return;
        };
        let cx0 = rect.x.max(clip.x).max(0.0);
        let cy0 = rect.y.max(clip.y).max(0.0);
        let cx1 = (rect.x + rect.width)
            .min(clip.x + clip.width)
            .min(self.width as f32);
        let cy1 = (rect.y + rect.height)
            .min(clip.y + clip.height)
            .min(self.height as f32);
        if cx0 >= cx1 || cy0 >= cy1 {
            return;
        }

        let ix0 = cx0.floor() as u32;
        let iy0 = cy0.floor() as u32;
        let ix1 = (cx1.ceil() as i64).clamp(0, i64::from(self.width)) as u32;
        let iy1 = (cy1.ceil() as i64).clamp(0, i64::from(self.height)) as u32;
        let stride = self.width as usize;

        for y in iy0..iy1 {
            let py = y as f32 + 0.5;
            let coverage_y =
                f32::from(overlap_coverage(cy0.max(y as f32), cy1.min(y as f32 + 1.0))) / 256.0;
            for x in ix0..ix1 {
                let px = x as f32 + 0.5;
                let coverage_x =
                    f32::from(overlap_coverage(cx0.max(x as f32), cx1.min(x as f32 + 1.0))) / 256.0;
                let coverage = coverage_x * coverage_y;
                if coverage <= 0.0 {
                    continue;
                }
                let fraction = corner.fraction(px, py, rect);
                if fraction <= 0.0 {
                    let alpha = (f32::from(color_b.a) * coverage).round() as u8;
                    if alpha > 0 {
                        self.pixels[y as usize * stride + x as usize] = blend_over(
                            self.pixels[y as usize * stride + x as usize],
                            with_alpha(color_b, u32::from(alpha)),
                        );
                    }
                } else if fraction >= 1.0 {
                    let alpha = (f32::from(color_a.a) * coverage).round() as u8;
                    if alpha > 0 {
                        self.pixels[y as usize * stride + x as usize] = blend_over(
                            self.pixels[y as usize * stride + x as usize],
                            with_alpha(color_a, u32::from(alpha)),
                        );
                    }
                } else {
                    // Partitioned coverage: mix the two premultiplied sources,
                    // then composite the resulting straight color over the dst.
                    let weight_a = f64::from(color_a.a) * f64::from(fraction) * f64::from(coverage);
                    let weight_b =
                        f64::from(color_b.a) * f64::from(1.0 - fraction) * f64::from(coverage);
                    let total = weight_a + weight_b;
                    if total <= 0.0 {
                        continue;
                    }
                    let r = ((f64::from(color_a.r) * weight_a + f64::from(color_b.r) * weight_b)
                        / total)
                        .round() as u8;
                    let g = ((f64::from(color_a.g) * weight_a + f64::from(color_b.g) * weight_b)
                        / total)
                        .round() as u8;
                    let b = ((f64::from(color_a.b) * weight_a + f64::from(color_b.b) * weight_b)
                        / total)
                        .round() as u8;
                    let alpha = total.round().clamp(0.0, 255.0) as u8;
                    let src = Color::new(r, g, b, alpha);
                    self.pixels[y as usize * stride + x as usize] =
                        blend_over(self.pixels[y as usize * stride + x as usize], src);
                }
            }
        }
    }
}
