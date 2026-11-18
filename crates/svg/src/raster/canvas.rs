//! Submodule of [`super`]: RGBA canvas, coverage compositing and patterns.

use super::*;

/// RGBA canvas with a reusable coverage buffer.
pub(crate) struct Canvas {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) pixels: Vec<u8>,
    pub(crate) coverage: Vec<f32>,
}

impl Canvas {
    pub(crate) fn clear_coverage(&mut self) {
        self.coverage.fill(0.0);
    }

    /// Composites the current coverage buffer with a resolved paint.
    pub(crate) fn composite_paint(
        &mut self,
        paint: &PreparedPaint<'_>,
        opacity: f32,
        context: &PaintContext<'_>,
    ) -> Result<(), SvgError> {
        match paint {
            PreparedPaint::Solid(color) => {
                let alpha = (f32::from(color.a) / 255.0) * opacity.clamp(0.0, 1.0);
                self.composite(*color, alpha);
                Ok(())
            }
            PreparedPaint::Gradient { spec, inverse } => {
                self.composite_gradient(spec, *inverse, opacity.clamp(0.0, 1.0));
                Ok(())
            }
            PreparedPaint::Pattern { spec } => self.composite_pattern(spec, opacity, context),
        }
    }

    /// Tiles a pattern over the current coverage (clipped by it), rendering the pattern
    /// content once per tile. Tile count is capped by [`MAX_PATTERN_TILES`].
    fn composite_pattern(
        &mut self,
        spec: &PatternSpec,
        opacity: f32,
        context: &PaintContext<'_>,
    ) -> Result<(), SvgError> {
        if context.depth >= MAX_PATTERN_DEPTH || !spec.usable() {
            return Ok(());
        }
        let (px, py, width, height) = spec.tile_box(context.bbox);
        if width.abs() < MIN_PATTERN_TILE || height.abs() < MIN_PATTERN_TILE {
            return Ok(());
        }
        let content = spec.content_transform(context.bbox);
        // Pattern space → device pixels (patternTransform applies to the whole pattern).
        let base = context
            .viewport
            .then(context.shape_transform)
            .then(spec.transform);
        let Some(inverse) = base.inverse() else {
            return Ok(());
        };
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for (x, y) in [
            (0.0, 0.0),
            (self.width as f32, 0.0),
            (0.0, self.height as f32),
            (self.width as f32, self.height as f32),
        ] {
            let (px_space, py_space) = inverse.apply(x, y);
            if !px_space.is_finite() || !py_space.is_finite() {
                return Ok(());
            }
            min_x = min_x.min(px_space);
            min_y = min_y.min(py_space);
            max_x = max_x.max(px_space);
            max_y = max_y.max(py_space);
        }
        // Compute the tile span in f64: a huge but finite `patternTransform` can exceed
        // i64, and clamping the absolute index would silently move (or drop) a valid
        // pattern whose phase is beyond the tile budget. Reject only over-budget spans.
        // Compute the span in f64: a huge but finite `patternTransform` can exceed i64, and
        // clamping the absolute index (or adding 1 to a saturated cast) would either panic
        // or silently move a valid pattern whose phase is beyond the tile budget. Only an
        // over-budget *span* is rejected.
        let i0f = ((min_x - px) / width).floor();
        let i1f = ((max_x - px) / width).ceil();
        let j0f = ((min_y - py) / height).floor();
        let j1f = ((max_y - py) / height).ceil();
        let columns = i1f - i0f + 3.0;
        let rows = j1f - j0f + 3.0;
        if !columns.is_finite() || !rows.is_finite() || columns * rows > MAX_PATTERN_TILES as f32 {
            return Err(SvgError::Limit(
                "pattern tile budget exceeded (too many repetitions)".to_owned(),
            ));
        }
        if columns <= 0.0 || rows <= 0.0 {
            return Ok(());
        }
        // Charge the whole tile grid against the *global* budget before doing any
        // work (#439). The per-paint cap above is still useful as a fast reject,
        // but only this shared counter bounds tiles^depth across nested patterns.
        let tiles = (columns.ceil() as u64).saturating_mul(rows.ceil() as u64);
        let mut left = context.budget.get();
        if tiles > left.tiles {
            return Err(SvgError::Limit(
                "global pattern tile budget exceeded".to_owned(),
            ));
        }
        left.tiles -= tiles;
        context.budget.set(left);
        let i0 = (i0f as i64).saturating_sub(1);
        let i1 = (i1f as i64).saturating_add(1);
        let j0 = (j0f as i64).saturating_sub(1);
        let j1 = (j1f as i64).saturating_add(1);
        for column in i0..=i1 {
            for row in j0..=j1 {
                let origin =
                    Transform::translate(px + column as f32 * width, py + row as f32 * height);
                let content_to_device = base.then(origin).then(content);
                // Tile quad in device pixels for clipping/canvas sizing.
                let quad = base.then(origin);
                let mut tile_min_x = f32::INFINITY;
                let mut tile_min_y = f32::INFINITY;
                let mut tile_max_x = f32::NEG_INFINITY;
                let mut tile_max_y = f32::NEG_INFINITY;
                for (x, y) in [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)] {
                    let (dx, dy) = quad.apply(x, y);
                    if !dx.is_finite() || !dy.is_finite() {
                        continue;
                    }
                    tile_min_x = tile_min_x.min(dx);
                    tile_min_y = tile_min_y.min(dy);
                    tile_max_x = tile_max_x.max(dx);
                    tile_max_y = tile_max_y.max(dy);
                }
                if !tile_min_x.is_finite() || !tile_min_y.is_finite() {
                    continue;
                }
                let x0 = (tile_min_x.floor() as i64).max(0);
                let y0 = (tile_min_y.floor() as i64).max(0);
                let x1 = (tile_max_x.ceil() as i64).min(self.width as i64);
                let y1 = (tile_max_y.ceil() as i64).min(self.height as i64);
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                let tile_width = (x1 - x0) as usize;
                let tile_height = (y1 - y0) as usize;
                let tile_pixels = (tile_width as u64).saturating_mul(tile_height as u64);
                let mut left = context.budget.get();
                if tile_pixels > left.pixels {
                    return Err(SvgError::Limit(
                        "global pattern tile pixel budget exceeded".to_owned(),
                    ));
                }
                left.pixels -= tile_pixels;
                context.budget.set(left);
                let mut tile = Canvas::new(tile_width, tile_height);
                let local =
                    Transform::translate(-(x0 as f32), -(y0 as f32)).then(content_to_device);
                for shape in &spec.shapes {
                    let shape_context = PaintContext {
                        gradients: context.gradients,
                        patterns: context.patterns,
                        shape_transform: shape.transform,
                        bbox: shape.bbox,
                        viewport: local,
                        depth: context.depth + 1,
                        budget: context.budget,
                    };
                    draw_shape(&mut tile, shape, &shape_context)?;
                }
                // Composite the tile only where the main shape coverage is positive.
                for ty in 0..tile_height {
                    for tx in 0..tile_width {
                        let main_x = x0 as usize + tx;
                        let main_y = y0 as usize + ty;
                        let main_index = main_y * self.width + main_x;
                        let coverage = self.coverage[main_index].clamp(0.0, 1.0);
                        if coverage <= 0.0 {
                            continue;
                        }
                        let tile_index = (ty * tile_width + tx) * 4;
                        let alpha = f32::from(tile.pixels[tile_index + 3]) / 255.0;
                        if alpha <= 0.0 {
                            continue;
                        }
                        let color = Color {
                            r: tile.pixels[tile_index],
                            g: tile.pixels[tile_index + 1],
                            b: tile.pixels[tile_index + 2],
                            a: 255,
                        };
                        composite_pixel(
                            &mut self.pixels,
                            main_index * 4,
                            color,
                            alpha * coverage * opacity.clamp(0.0, 1.0),
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn new(width: usize, height: usize) -> Self {
        let pixel_count = width.saturating_mul(height);
        Self {
            width,
            height,
            pixels: vec![0u8; pixel_count * 4],
            coverage: vec![0.0f32; pixel_count],
        }
    }

    /// Composites another canvas's straight-RGBA pixels over this one (source-over).
    pub(crate) fn composite_layer(&mut self, layer: &Canvas) {
        for (index, pixel) in layer.pixels.as_chunks::<4>().0.iter().enumerate() {
            let alpha = f32::from(pixel[3]) / 255.0;
            if alpha <= 0.0 {
                continue;
            }
            let color = Color {
                r: pixel[0],
                g: pixel[1],
                b: pixel[2],
                a: 255,
            };
            composite_pixel(&mut self.pixels, index * 4, color, alpha);
        }
    }

    /// Adds a glyph coverage mask at the given device position (max coverage).
    /// Adds a glyph mask rotated by `angle` radians around its baseline origin `(x, y)`.
    pub(crate) fn add_mask_rotated(
        &mut self,
        x: f32,
        y: f32,
        mask: &megi_fonts::GlyphMask,
        angle: f32,
    ) {
        if angle.abs() < 1e-4 {
            self.add_mask(x, y, mask);
            return;
        }
        let (sin, cos) = angle.sin_cos();
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let canvas_width = self.width as f32;
        let canvas_height = self.height as f32;
        let margin = 2.0 * mask.width.max(mask.height) as f32 + 2.0;
        let rounded_x = x.round();
        let rounded_y = y.round();
        if rounded_x < -margin
            || rounded_y < -margin
            || rounded_x > canvas_width + margin
            || rounded_y > canvas_height + margin
        {
            return;
        }
        let origin_x = rounded_x as i64;
        let origin_y = rounded_y as i64;
        for row in 0..mask.height {
            for column in 0..mask.width {
                let value = mask.data[(row * mask.width + column) as usize];
                if value == 0 {
                    continue;
                }
                let local_x = column as f32 + mask.left;
                let local_y = row as f32 + mask.top;
                let dx = local_x * cos - local_y * sin;
                let dy = local_x * sin + local_y * cos;
                let px = origin_x.saturating_add(dx.round() as i64);
                let py = origin_y.saturating_add(dy.round() as i64);
                if px < 0 || py < 0 || px >= self.width as i64 || py >= self.height as i64 {
                    continue;
                }
                let index = py as usize * self.width + px as usize;
                let alpha = f32::from(value) / 255.0;
                if alpha > self.coverage[index] {
                    self.coverage[index] = alpha;
                }
            }
        }
    }

    pub(crate) fn add_mask(&mut self, x: f32, y: f32, mask: &megi_fonts::GlyphMask) {
        // A far-off-canvas glyph origin (e.g. `x='1e38'`) saturated the i64 cast and then
        // overflowed when adding the glyph offsets (debug panic / release divergence).
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let canvas_width = self.width as f32;
        let canvas_height = self.height as f32;
        let rounded_x = x.round();
        let rounded_y = y.round();
        if rounded_x + mask.width as f32 <= 0.0
            || rounded_y + mask.height as f32 <= 0.0
            || rounded_x >= canvas_width
            || rounded_y >= canvas_height
        {
            return;
        }
        let origin_x = rounded_x as i64;
        let origin_y = rounded_y as i64;
        for row in 0..mask.height {
            let py = origin_y + i64::from(row);
            if py < 0 || py >= self.height as i64 {
                continue;
            }
            for column in 0..mask.width {
                let px = origin_x + i64::from(column);
                if px < 0 || px >= self.width as i64 {
                    continue;
                }
                let value = mask.data[(row * mask.width + column) as usize];
                if value == 0 {
                    continue;
                }
                let index = py as usize * self.width + px as usize;
                let alpha = f32::from(value) / 255.0;
                if alpha > self.coverage[index] {
                    self.coverage[index] = alpha;
                }
            }
        }
    }

    /// Dilates the coverage buffer by `radius` pixels (approximate text stroke).
    pub(crate) fn dilate_coverage(&mut self, radius: i32) {
        if radius <= 0 || self.width == 0 || self.height == 0 {
            return;
        }
        let source = self.coverage.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let index = y * self.width + x;
                if source[index] >= 1.0 {
                    continue;
                }
                let mut best = source[index];
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        let sx = (x as i64).saturating_add(i64::from(dx));
                        let sy = (y as i64).saturating_add(i64::from(dy));
                        if sx < 0 || sy < 0 || sx >= self.width as i64 || sy >= self.height as i64 {
                            continue;
                        }
                        best = best.max(source[sy as usize * self.width + sx as usize]);
                    }
                }
                self.coverage[index] = best;
            }
        }
    }

    /// Composites a gradient: evaluates the gradient at each covered pixel centre
    /// (deterministic, no sampling).
    fn composite_gradient(
        &mut self,
        spec: &GradientSpec,
        inverse: crate::transform::Transform,
        opacity: f32,
    ) {
        let width = self.width;
        let pixels = &mut self.pixels;
        for (index, coverage) in self.coverage.iter().enumerate() {
            let coverage = coverage.clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let x = (index % width) as f32 + 0.5;
            let y = (index / width) as f32 + 0.5;
            let (tx, ty) = inverse.apply(x, y);
            if !tx.is_finite() || !ty.is_finite() {
                continue;
            }
            let color = spec.evaluate(spec.parameter(tx, ty));
            let src_a = coverage * opacity * (f32::from(color.a) / 255.0);
            if src_a <= 0.0 {
                continue;
            }
            composite_pixel(pixels, index * 4, color, src_a);
        }
    }

    /// Composites the current coverage buffer with `color` (source-over, straight alpha).
    pub(crate) fn composite(&mut self, color: Color, alpha: f32) {
        let pixels = &mut self.pixels;
        for (index, coverage) in self.coverage.iter().enumerate() {
            let src_a = coverage.clamp(0.0, 1.0) * alpha;
            if src_a <= 0.0 {
                continue;
            }
            composite_pixel(pixels, index * 4, color, src_a);
        }
    }
}
