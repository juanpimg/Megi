//! Canvas path and text primitives (arc/fill, glyph boxes).

use super::*;

impl CanvasSurface {
    /// Starts a new path (Canvas 2D §4.2, `beginPath`).
    pub fn begin_path(&mut self) {
        self.arcs.clear();
    }

    /// Appends a circle/arc to the current path. Only the circular disc is
    /// rasterized by `fill()` for now; start/end angles are accepted and
    /// ignored by this deterministic subset (#1959).
    pub fn arc(&mut self, x: f64, y: f64, radius: f64, start: f64, end: f64) {
        if !x.is_finite()
            || !y.is_finite()
            || !radius.is_finite()
            || !start.is_finite()
            || !end.is_finite()
            || radius <= 0.0
        {
            return;
        }
        if self.arcs.len() >= MAX_CANVAS_PATH_ITEMS {
            return;
        }
        self.arcs.push((x, y, radius));
    }

    /// Draws a deterministic glyph-box approximation of `text` (#1961). The `js`
    /// crate has no font rasterizer dependency (DAG), so each character is a
    /// small filled box; this keeps real-site canvas scripts running.
    pub fn fill_text(&mut self, x: f64, y: f64, text: &str) {
        for (index, _) in text.chars().enumerate() {
            let left = x + index as f64 * 8.0;
            self.fill_rect(left, y - 8.0, 6.0, 8.0);
        }
    }

    /// Approximate text width: 8 px per code point (#1961).
    pub fn measure_text(text: &str) -> f64 {
        text.chars().count() as f64 * 8.0
    }

    /// Fills the current path with `fill_style` (Canvas 2D `fill`).
    pub fn fill_path(&mut self) {
        let arcs = std::mem::take(&mut self.arcs);
        if self.width == 0 || self.height == 0 {
            return;
        }
        let max_x = self.width as i64 - 1;
        let max_y = self.height as i64 - 1;
        let color = self.fill_style;
        for (cx, cy, radius) in arcs {
            if !cx.is_finite() || !cy.is_finite() || !radius.is_finite() || radius <= 0.0 {
                continue;
            }
            // #2154: HTML Canvas 2D — clip fill bounding box to the backing surface.
            let x0 = ((cx - radius).floor() as i64).max(0);
            let x1 = ((cx + radius).ceil() as i64).min(max_x);
            let y0 = ((cy - radius).floor() as i64).max(0);
            let y1 = ((cy + radius).ceil() as i64).min(max_y);
            let rr = radius * radius;
            for py in y0..=y1 {
                for px in x0..=x1 {
                    let dx = px as f64 - cx;
                    let dy = py as f64 - cy;
                    if dx * dx + dy * dy <= rr {
                        self.blend(px, py, color);
                    }
                }
            }
        }
    }
}
