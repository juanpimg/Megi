//! # CSS `background-image` geometry: layers, tiling, sizing, positioning
//!
//! Pure geometry helpers used by the painter to rasterize `background-image`
//! layers according to CSS Backgrounds Level 3 §2.3–2.7:
//!
//! - **`background-size`**: `auto` (intrinsic size), `cover` / `contain`
//!   (aspect-ratio preserving) and explicit lengths/percentages resolved against
//!   the **background positioning area** selected by `background-origin`.
//! - **`background-position`**: keyword/percentage/length offsets per axis
//!   (ignored on an axis using `space`/`round`, as the spec requires).
//! - **`background-repeat`**: `repeat`, `repeat-x`, `repeat-y`, `no-repeat`,
//!   plus real `space` (tiles distributed with equal interior gaps) and `round`
//!   (tile scaled so a whole number of copies fits) semantics.
//! - **`background-origin` / `background-clip`**: the positioning area and the
//!   painting clip box (`border-box`, `padding-box`, `content-box`). `text` is
//!   parsed by the CSS crate but unsupported here: the painter falls back to the
//!   border box and documents it.
//! - **`background-attachment: fixed`**: the layer is positioned against the
//!   render viewport (this engine has no scrolling yet).
//!
//! [`background_tiles_with_origin`] is the pure, deterministic entry point:
//! given the positioning area, the clip area, an intrinsic image size and the
//! three computed properties it returns the (unclipped) placement rectangles of
//! every tile intersecting the clip area, in row-major order. The painter clips
//! the whole layer with a clip item, so partially visible tiles are safely drawn.
//!
//! Tiling is capped at [`MAX_BACKGROUND_TILES`] rects (deterministically
//! truncated) to keep hostile `background-size: 0.01px; repeat` pages bounded.
//! Multiple layers are painted bottom-to-top by the caller (CSS lists the top
//! layer first). `background-clip: text`, multiple positioning-area edge offsets
//! (`background-position: right 10px bottom 20px`), 3D/attachment `local`,
//! gradients and images per layer beyond the render-provided URL map are out of
//! scope for this phase.

use css::{BackgroundAttachment, BackgroundClip, BackgroundOrigin, BackgroundPosition};
use css::{BackgroundRepeat, BackgroundRepeatAxis, BackgroundSize, Length};
use layout::Rect;

/// Hard cap on the number of tile rectangles generated for one background layer.
pub const MAX_BACKGROUND_TILES: usize = 4096;

/// Resolves the box used as the **positioning area** for a layer.
pub fn background_origin_rect(
    origin: BackgroundOrigin,
    border_box: Rect,
    padding_box: Rect,
    content_box: Rect,
) -> Rect {
    match origin {
        BackgroundOrigin::BorderBox => border_box,
        BackgroundOrigin::PaddingBox => padding_box,
        BackgroundOrigin::ContentBox => content_box,
    }
}

/// Resolves the box used as the **painting clip** for a layer.
///
/// `background-clip: text` is not implemented by the software painter; it falls
/// back to the border box (documented Phase 7 limitation).
pub fn background_clip_rect(
    clip: BackgroundClip,
    border_box: Rect,
    padding_box: Rect,
    content_box: Rect,
) -> Rect {
    match clip {
        BackgroundClip::BorderBox | BackgroundClip::Text => border_box,
        BackgroundClip::PaddingBox => padding_box,
        BackgroundClip::ContentBox => content_box,
    }
}

/// Resolves the drawn size of one background tile, in CSS px, against the
/// positioning area `positioning_area`.
///
/// `cover`/`contain` preserve the intrinsic aspect ratio; explicit lengths and
/// percentages are resolved against the corresponding axis (an `auto` axis keeps
/// the intrinsic ratio).
pub fn background_tile_size(
    positioning_area: Rect,
    image_size: (u32, u32),
    size: BackgroundSize,
) -> (f32, f32) {
    let intrinsic_width = image_size.0 as f32;
    let intrinsic_height = image_size.1 as f32;
    if intrinsic_width <= 0.0 || intrinsic_height <= 0.0 {
        return (0.0, 0.0);
    }
    let area_width = positioning_area.width.max(0.0);
    let area_height = positioning_area.height.max(0.0);
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

/// Resolves one `background-position` component to an offset from the
/// positioning-area origin. Percentages place the tile proportionally to the
/// leftover space.
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

/// Plans one axis: returns the adjusted tile length and the tile origins whose
/// rects intersect `clip` (CSS Backgrounds 3 §2.3).
#[allow(clippy::too_many_arguments)]
fn plan_axis(
    positioning_origin: f32,
    positioning_length: f32,
    clip_origin: f32,
    clip_length: f32,
    position: Length,
    tile: f32,
    repeat: BackgroundRepeatAxis,
    limit: usize,
) -> (f32, Vec<f32>) {
    let mut tile = tile;
    let clip_end = clip_origin + clip_length;
    let intersects_clip = |origin: f32, tile: f32| origin < clip_end && origin + tile > clip_origin;
    let mut origins = Vec::new();

    match repeat {
        BackgroundRepeatAxis::Repeat => {
            let base = positioning_origin + resolve_position(position, positioning_length, tile);
            if tile > 0.0 {
                let start = ((clip_origin - base - tile) / tile).floor() as i64 - 1;
                let end = ((clip_end - base) / tile).ceil() as i64 + 1;
                let count = ((end - start).max(0) as usize).min(limit);
                for step in 0..count as i64 {
                    let origin = base + (start + step) as f32 * tile;
                    if intersects_clip(origin, tile) {
                        origins.push(origin);
                    }
                }
            } else if intersects_clip(base, tile) {
                origins.push(base);
            }
        }
        BackgroundRepeatAxis::NoRepeat => {
            let base = positioning_origin + resolve_position(position, positioning_length, tile);
            if intersects_clip(base, tile) {
                origins.push(base);
            }
        }
        BackgroundRepeatAxis::Round => {
            // Scale the tile so a whole number of copies fits the positioning area.
            if tile > 0.0 && positioning_length > 0.0 {
                let count = (positioning_length / tile).round().max(1.0);
                tile = positioning_length / count;
                for index in 0..count as usize {
                    if origins.len() >= limit {
                        break;
                    }
                    let origin = positioning_origin + index as f32 * tile;
                    if intersects_clip(origin, tile) {
                        origins.push(origin);
                    }
                }
            } else if intersects_clip(positioning_origin, tile) {
                origins.push(positioning_origin);
            }
        }
        BackgroundRepeatAxis::Space => {
            // Distribute whole tiles with equal interior gaps; a single tile is
            // centred and a too-large tile behaves like `no-repeat` (CSS Backgrounds 3 §2.3).
            if tile > 0.0 && positioning_length >= tile {
                let count = (positioning_length / tile).floor().max(1.0);
                let count = count as usize;
                if count == 1 {
                    let origin = positioning_origin + (positioning_length - tile) / 2.0;
                    if intersects_clip(origin, tile) {
                        origins.push(origin);
                    }
                } else {
                    let gap = (positioning_length - count as f32 * tile) / (count - 1) as f32;
                    for index in 0..count {
                        if origins.len() >= limit {
                            break;
                        }
                        let origin = positioning_origin + index as f32 * (tile + gap);
                        if intersects_clip(origin, tile) {
                            origins.push(origin);
                        }
                    }
                }
            } else if intersects_clip(positioning_origin, tile) {
                origins.push(positioning_origin);
            }
        }
    }
    if origins.len() > limit {
        origins.truncate(limit);
    }
    (tile, origins)
}

/// Computes the placement rectangles of a single `background-image` layer.
///
/// `positioning_area` is selected by `background-origin` (or the viewport for
/// `background-attachment: fixed`); `clip_area` by `background-clip`. Returned
/// rects are **not clipped**: the painter pushes a clip to `clip_area` around the
/// whole layer. Only tiles intersecting `clip_area` are returned, row-major, and
/// the result is capped to [`MAX_BACKGROUND_TILES`].
pub fn background_tiles_with_origin(
    positioning_area: Rect,
    clip_area: Rect,
    image_size: (u32, u32),
    repeat: BackgroundRepeat,
    size: BackgroundSize,
    position: BackgroundPosition,
) -> Vec<Rect> {
    if positioning_area.width <= 0.0
        || positioning_area.height <= 0.0
        || clip_area.width <= 0.0
        || clip_area.height <= 0.0
    {
        return Vec::new();
    }
    let (base_width, base_height) = background_tile_size(positioning_area, image_size, size);
    if base_width <= 0.0 || base_height <= 0.0 {
        return Vec::new();
    }
    let (tile_width, x_origins) = plan_axis(
        positioning_area.x,
        positioning_area.width,
        clip_area.x,
        clip_area.width,
        position.x,
        base_width,
        repeat.x,
        MAX_BACKGROUND_TILES,
    );
    let remaining = MAX_BACKGROUND_TILES / x_origins.len().max(1);
    let (tile_height, y_origins) = plan_axis(
        positioning_area.y,
        positioning_area.height,
        clip_area.y,
        clip_area.height,
        position.y,
        base_height,
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
            if overlaps(tile, clip_area) {
                tiles.push(tile);
            }
        }
    }
    tiles
}

/// Backwards-compatible single-box variant of [`background_tiles_with_origin`]:
/// the positioning area and the clip are both `box` (the legacy painter path and
/// the pure-function tests).
pub fn background_tiles(
    box_rect: Rect,
    image_size: (u32, u32),
    repeat: BackgroundRepeat,
    size: BackgroundSize,
    position: BackgroundPosition,
) -> Vec<Rect> {
    background_tiles_with_origin(box_rect, box_rect, image_size, repeat, size, position)
}

/// Whether the layer's `background-attachment` positions it against the viewport.
pub fn uses_viewport_positioning(attachment: BackgroundAttachment) -> bool {
    attachment == BackgroundAttachment::Fixed
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

    #[test]
    fn test_space_distributes_equal_gaps() {
        // 10px tile in a 45px area: 4 tiles, 3 gaps of (45-40)/3.
        let tiles = background_tiles(
            padding(45.0, 10.0),
            (10, 10),
            BackgroundRepeat::parse("space").unwrap(),
            BackgroundSize::Auto,
            BackgroundPosition::ZERO,
        );
        assert_eq!(tiles.len(), 4);
        assert_eq!(tiles[0].x, 0.0);
        assert_eq!(tiles[1].x, 10.0 + 5.0 / 3.0);
        assert!((tiles[3].x + tiles[3].width - 45.0).abs() < 1e-4);
    }

    #[test]
    fn test_round_scales_tile_to_fit() {
        // 10px tile in a 45px area => round(4.5) = 5 tiles of 9px.
        let tiles = background_tiles(
            padding(45.0, 10.0),
            (10, 10),
            BackgroundRepeat::parse("round").unwrap(),
            BackgroundSize::Auto,
            BackgroundPosition::ZERO,
        );
        assert_eq!(tiles.len(), 5);
        assert!((tiles[0].width - 9.0).abs() < 1e-4);
        assert!((tiles[4].x + tiles[4].width - 45.0).abs() < 1e-4);
    }

    #[test]
    fn test_origin_box_selects_positioning_area() {
        let border = Rect::new(0.0, 0.0, 100.0, 50.0);
        let padding_box = Rect::new(5.0, 5.0, 90.0, 40.0);
        let content = Rect::new(10.0, 10.0, 80.0, 30.0);
        assert_eq!(
            background_origin_rect(BackgroundOrigin::ContentBox, border, padding_box, content),
            content
        );
        assert_eq!(
            background_clip_rect(BackgroundClip::PaddingBox, border, padding_box, content),
            padding_box
        );
        assert_eq!(
            background_clip_rect(BackgroundClip::Text, border, padding_box, content),
            border,
            "background-clip: text falls back to the border box"
        );
    }
}
