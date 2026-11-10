//! # CSS `background-image` geometry: tiling, sizing and positioning
//!
//! Pure geometry helpers used by the painter to rasterize `background-image`
//! layers according to CSS Backgrounds Level 3 §2.3–2.5:
//!
//! - **`background-size`**: `auto` (intrinsic size), `cover` / `contain`
//!   (aspect-ratio preserving), or explicit lengths/percentages resolved against
//!   the **padding box** (the default `background-origin: padding-box`).
//! - **`background-position`**: keyword/percentage/length offsets per axis.
//! - **`background-repeat`**: `repeat`, `repeat-x`, `repeat-y` and `no-repeat`
//!   are fully implemented. `space` and `round` are parsed by the CSS crate but
//!   currently painted with plain `repeat` geometry (documented limitation of
//!   this phase).
//!
//! [`background_tiles`] is a pure, deterministic function: given a padding box,
//! an intrinsic image size and the three computed properties it returns the
//! (unclipped) placement rectangles of every tile that intersects the padding
//! box, in row-major order. The painter clips the whole layer to the padding box
//! with a clip item, so partially visible tiles are safely drawn and the
//! rasterizer's clip stack removes the overdraw.
//!
//! Tiling is capped at [`MAX_BACKGROUND_TILES`] rects (deterministically
//! truncated) to keep hostile `background-size: 0.01px; repeat` pages bounded.
//! `background-origin`/`-clip`, multiple layers, `background-attachment: fixed`
//! and gradients are out of scope.

use css::{BackgroundPosition, BackgroundRepeat, BackgroundRepeatAxis, BackgroundSize, Length};
use layout::Rect;

/// Hard cap on the number of tile rectangles generated for one background layer.
pub const MAX_BACKGROUND_TILES: usize = 4096;

/// Resolves the drawn size of one background tile, in CSS px.
///
/// `cover`/`contain` preserve the intrinsic aspect ratio and are computed
/// against the padding box; explicit lengths and percentages are resolved
/// against the corresponding axis (an `auto` axis keeps the intrinsic ratio).
pub fn background_tile_size(
    padding_box: Rect,
    image_size: (u32, u32),
    size: BackgroundSize,
) -> (f32, f32) {
    let intrinsic_width = image_size.0 as f32;
    let intrinsic_height = image_size.1 as f32;
    if intrinsic_width <= 0.0 || intrinsic_height <= 0.0 {
        return (0.0, 0.0);
    }
    let area_width = padding_box.width.max(0.0);
    let area_height = padding_box.height.max(0.0);
    let (width, height) = match size {
        BackgroundSize::Auto => (intrinsic_width, intrinsic_height),
        BackgroundSize::Cover => {
            if area_width <= 0.0 || area_height <= 0.0 {
                (intrinsic_width, intrinsic_height)
            } else {
                let scale = (area_width / intrinsic_width).max(area_height / intrinsic_height);
                (intrinsic_width * scale, intrinsic_height * scale)
            }
        }
        BackgroundSize::Contain => {
            if area_width <= 0.0 || area_height <= 0.0 {
                (intrinsic_width, intrinsic_height)
            } else {
                let scale = (area_width / intrinsic_width).min(area_height / intrinsic_height);
                (intrinsic_width * scale, intrinsic_height * scale)
            }
        }
        BackgroundSize::Explicit(width, height) => {
            let resolved_width = resolve_axis_length(width, area_width);
            let resolved_height = resolve_axis_length(height, area_height);
            match (resolved_width, resolved_height) {
                (Some(w), Some(h)) => (w, h),
                (Some(w), None) => (w, w * intrinsic_height / intrinsic_width),
                (None, Some(h)) => (h * intrinsic_width / intrinsic_height, h),
                (None, None) => (intrinsic_width, intrinsic_height),
            }
        }
    };
    (
        if width.is_finite() {
            width.max(0.0)
        } else {
            0.0
        },
        if height.is_finite() {
            height.max(0.0)
        } else {
            0.0
        },
    )
}

fn resolve_axis_length(length: Length, area: f32) -> Option<f32> {
    match length {
        Length::Px(px) => Some(px.max(0.0)),
        Length::Percent(pct) => Some((area * pct / 100.0).max(0.0)),
        Length::Auto => None,
    }
}

/// Resolves one `background-position` component to an offset from the padding
/// box origin. Percentages place the tile proportionally to the leftover space.
fn resolve_position(value: Length, area: f32, tile: f32) -> f32 {
    let offset = match value {
        Length::Px(px) => px,
        Length::Percent(pct) => (area - tile) * (pct / 100.0),
        Length::Auto => 0.0,
    };
    if offset.is_finite() { offset } else { 0.0 }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

/// Origins along one axis for a tile of length `tile` positioned at `base`.
///
/// `padding_origin`/`padding_length` describe the clipping area on that axis.
/// Returns at most `limit` origins (deterministic truncation).
fn axis_origins(
    padding_origin: f32,
    padding_length: f32,
    base: f32,
    tile: f32,
    repeat: BackgroundRepeatAxis,
    limit: usize,
) -> Vec<f32> {
    let padding_end = padding_origin + padding_length;
    let intersects = |origin: f32| origin < padding_end && origin + tile > padding_origin;
    let mut origins = Vec::new();
    match repeat {
        BackgroundRepeatAxis::NoRepeat => {
            if intersects(base) {
                origins.push(base);
            }
        }
        // `space`/`round` are parsed but painted as plain `repeat` in this phase.
        BackgroundRepeatAxis::Repeat
        | BackgroundRepeatAxis::Space
        | BackgroundRepeatAxis::Round => {
            if tile <= 0.0 || !tile.is_finite() || !padding_length.is_finite() {
                if intersects(base) {
                    origins.push(base);
                }
                return origins;
            }
            // k such that base + k*tile intersects [padding_origin, padding_end).
            let start = ((padding_origin - base - tile) / tile).floor() as i64 - 1;
            let end = ((padding_end - base) / tile).ceil() as i64 + 1;
            let span = (end - start).max(0) as usize;
            let count = span.min(limit);
            for step in 0..count as i64 {
                let origin = base + (start + step) as f32 * tile;
                if intersects(origin) {
                    origins.push(origin);
                }
            }
        }
    }
    if origins.len() > limit {
        origins.truncate(limit);
    }
    origins
}

/// Computes the placement rectangles of a single `background-image` layer.
///
/// The returned rects are **not clipped**: the painter pushes a clip to the
/// padding box around the whole layer. Only tiles intersecting `padding_box`
/// are returned, in row-major order. The result is capped to
/// [`MAX_BACKGROUND_TILES`].
pub fn background_tiles(
    padding_box: Rect,
    image_size: (u32, u32),
    repeat: BackgroundRepeat,
    size: BackgroundSize,
    position: BackgroundPosition,
) -> Vec<Rect> {
    if padding_box.width <= 0.0 || padding_box.height <= 0.0 {
        return Vec::new();
    }
    let (tile_width, tile_height) = background_tile_size(padding_box, image_size, size);
    if tile_width <= 0.0 || tile_height <= 0.0 {
        return Vec::new();
    }
    let base_x = padding_box.x + resolve_position(position.x, padding_box.width, tile_width);
    let base_y = padding_box.y + resolve_position(position.y, padding_box.height, tile_height);

    let x_origins = axis_origins(
        padding_box.x,
        padding_box.width,
        base_x,
        tile_width,
        repeat.x,
        MAX_BACKGROUND_TILES,
    );
    let remaining = MAX_BACKGROUND_TILES / x_origins.len().max(1);
    let y_origins = axis_origins(
        padding_box.y,
        padding_box.height,
        base_y,
        tile_height,
        repeat.y,
        remaining.max(1),
    );

    let mut tiles = Vec::with_capacity(x_origins.len().saturating_mul(y_origins.len()));
    for &y in &y_origins {
        for &x in &x_origins {
            if tiles.len() >= MAX_BACKGROUND_TILES {
                return tiles;
            }
            let tile = Rect::new(x, y, tile_width, tile_height);
            if overlaps(tile, padding_box) {
                tiles.push(tile);
            }
        }
    }
    tiles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padding(width: f32, height: f32) -> Rect {
        Rect::new(0.0, 0.0, width, height)
    }

    #[test]
    fn test_no_repeat_centered() {
        let tiles = background_tiles(
            padding(100.0, 50.0),
            (16, 16),
            BackgroundRepeat::NO_REPEAT,
            BackgroundSize::Auto,
            BackgroundPosition::parse("center", 16.0, 16.0).unwrap(),
        );
        assert_eq!(tiles, vec![Rect::new(42.0, 17.0, 16.0, 16.0)]);
    }

    #[test]
    fn test_cover_and_contain_preserve_aspect_ratio() {
        let cover = background_tiles(
            padding(100.0, 50.0),
            (20, 20),
            BackgroundRepeat::NO_REPEAT,
            BackgroundSize::Cover,
            BackgroundPosition::parse("center", 16.0, 16.0).unwrap(),
        );
        assert_eq!(cover, vec![Rect::new(0.0, -25.0, 100.0, 100.0)]);
        let contain = background_tiles(
            padding(100.0, 50.0),
            (20, 20),
            BackgroundRepeat::NO_REPEAT,
            BackgroundSize::Contain,
            BackgroundPosition::parse("center", 16.0, 16.0).unwrap(),
        );
        assert_eq!(contain, vec![Rect::new(25.0, 0.0, 50.0, 50.0)]);
    }

    #[test]
    fn test_repeat_axis_counts() {
        let both = background_tiles(
            padding(12.0, 12.0),
            (4, 4),
            BackgroundRepeat::REPEAT,
            BackgroundSize::Auto,
            BackgroundPosition::ZERO,
        );
        assert_eq!(both.len(), 9);
        let x_only = background_tiles(
            padding(12.0, 12.0),
            (4, 4),
            BackgroundRepeat::parse("repeat-x").unwrap(),
            BackgroundSize::Auto,
            BackgroundPosition::ZERO,
        );
        assert_eq!(x_only.len(), 3);
        assert!(x_only.iter().all(|tile| tile.y == 0.0));
    }

    #[test]
    fn test_explicit_sizes_and_positions() {
        let half = background_tiles(
            padding(100.0, 50.0),
            (20, 20),
            BackgroundRepeat::NO_REPEAT,
            BackgroundSize::parse("50%", 16.0, 16.0).unwrap(),
            BackgroundPosition::parse("50%", 16.0, 16.0).unwrap(),
        );
        // 50% width/height resolves to 50x25; 50% position centers it.
        assert_eq!(half, vec![Rect::new(25.0, 12.5, 50.0, 25.0)]);
        let px = background_tiles(
            padding(100.0, 50.0),
            (20, 20),
            BackgroundRepeat::NO_REPEAT,
            BackgroundSize::parse("100px 10px", 16.0, 16.0).unwrap(),
            BackgroundPosition::parse("10px 0", 16.0, 16.0).unwrap(),
        );
        assert_eq!(px, vec![Rect::new(10.0, 0.0, 100.0, 10.0)]);
    }
}
