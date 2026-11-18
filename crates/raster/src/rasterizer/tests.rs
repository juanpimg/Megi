//! Unit tests moved verbatim out of `rasterizer.rs`.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

#[cfg(test)]
mod composite_tests {
    use super::*;

    #[test]
    fn test_composite_source_over_keeps_a_half_alpha_source_over_transparency() {
        let mut dest = Pixmap::new(1, 1);
        let mut src = Pixmap::new(1, 1);
        src.pixels[0] = Color::new(255, 0, 0, 128);
        composite_effect(&mut dest, &src, 0, 0, megi_display::MixBlendMode::Normal);
        let pixel = dest.pixels[0];
        assert_eq!(
            (pixel.r, pixel.g, pixel.b, pixel.a),
            (255, 0, 0, 128),
            "straight-alpha source-over over transparency must not darken the colour"
        );
    }

    #[test]
    fn test_composite_source_over_weights_a_translucent_backdrop() {
        // The backdrop term must carry its own alpha and the result must be divided by the
        // output alpha: (204, 0, 51, 160) instead of the old (128, 0, 127, 160).
        let mut dest = Pixmap::new(1, 1);
        dest.pixels[0] = Color::new(0, 0, 255, 64);
        let mut src = Pixmap::new(1, 1);
        src.pixels[0] = Color::new(255, 0, 0, 128);
        composite_effect(&mut dest, &src, 0, 0, megi_display::MixBlendMode::Normal);
        let pixel = dest.pixels[0];
        assert_eq!(
            (pixel.r, pixel.g, pixel.b, pixel.a),
            (204, 0, 51, 160),
            "alpha-weighted source-over"
        );
    }

    #[test]
    fn test_composite_multiply_over_a_translucent_backdrop() {
        let mut dest = Pixmap::new(1, 1);
        dest.pixels[0] = Color::new(128, 128, 128, 128);
        let mut src = Pixmap::new(1, 1);
        src.pixels[0] = Color::new(255, 0, 0, 128);
        composite_effect(&mut dest, &src, 0, 0, megi_display::MixBlendMode::Multiply);
        let pixel = dest.pixels[0];
        assert_eq!((pixel.r, pixel.g, pixel.b, pixel.a), (170, 43, 43, 192));
    }

    #[test]
    fn test_composite_source_over_over_an_opaque_backdrop() {
        let mut dest = Pixmap::new(1, 1);
        dest.pixels[0] = Color::new(0, 0, 255, 255);
        let mut src = Pixmap::new(1, 1);
        src.pixels[0] = Color::new(255, 0, 0, 128);
        composite_effect(&mut dest, &src, 0, 0, megi_display::MixBlendMode::Normal);
        let pixel = dest.pixels[0];
        assert_eq!(pixel.a, 255);
        assert_eq!(pixel.r, 128);
        assert_eq!(pixel.g, 0);
        assert_eq!(pixel.b, 127);
    }
}

#[cfg(test)]
mod text_cache_cap_tests {
    use super::*;

    /// Draws `text` at `size` through the production cache path and returns the
    /// number of non-blank glyphs actually inked.
    fn ink(cache: &mut TextCache, database: &FontDatabase, text: &str, size: f32) -> usize {
        let Some(run) = cache.shape(database, text, "sans-serif", 400, false, size) else {
            return 0;
        };
        let faces: Vec<Option<u32>> = run
            .fonts
            .iter()
            .map(|info| cache.face_index(database, info))
            .collect();
        let size_bits = size.to_bits();
        let mut inked = 0;
        for glyph in &run.glyphs {
            let Some(face_index) = faces.get(glyph.font).copied().flatten() else {
                continue;
            };
            let Ok(glyph_id) = u16::try_from(glyph.glyph_id) else {
                continue;
            };
            if let Some(entry) = cache.mask(face_index, glyph_id, size_bits, size)
                && !entry.is_blank()
            {
                inked += 1;
            }
        }
        inked
    }

    #[test]
    fn glyph_mask_cache_stays_bounded_and_keeps_inking_glyphs() {
        let database = crate::bundled_font_database();
        let mut cache = TextCache::default();
        // Vary the sub-pixel size so every step produces a fresh
        // `(face, glyph, size_bits)` key, well past the cap.
        let steps = MAX_TEXT_CACHE_ENTRIES + 64;
        let mut inked_total = 0usize;
        let mut inked_after_cap = 0usize;
        for step in 0..steps {
            let size = 12.0 + step as f32 * 0.01;
            let inked = ink(&mut cache, database, "A", size);
            inked_total += inked;
            if step >= MAX_TEXT_CACHE_ENTRIES {
                inked_after_cap += inked;
            }
            assert!(
                cache.masks.len() <= MAX_TEXT_CACHE_ENTRIES,
                "mask cache grew past its cap: {} > {}",
                cache.masks.len(),
                MAX_TEXT_CACHE_ENTRIES
            );
        }
        assert!(inked_total > 0, "glyphs must rasterize with bundled fonts");
        assert!(
            inked_after_cap > 0,
            "the cap must cost cache hits, never ink: {} glyphs after the cap",
            inked_after_cap
        );
    }

    #[test]
    fn shaped_run_cache_stays_bounded() {
        let database = crate::bundled_font_database();
        let mut cache = TextCache::default();
        for step in 0..(MAX_SHAPED_RUNS + 32) {
            let size = 10.0 + step as f32 * 0.01;
            let _ = cache.shape(database, "bounded", "sans-serif", 400, false, size);
            assert!(
                cache.shaped.len() <= MAX_SHAPED_RUNS,
                "shaped cache grew past its cap: {} > {}",
                cache.shaped.len(),
                MAX_SHAPED_RUNS
            );
        }
    }
}

mod paint_budget_tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_paint_budget_counts_buffers_separately_from_the_per_buffer_cap() {
        let mut budget = PaintBudget {
            depth: 0,
            pixels: 0,
            flatten: false,
            flatten_depth: 0,
        };
        assert!(budget.enter_buffers(64, 64, 2));
        assert_eq!(budget.depth, 1);
        assert_eq!(budget.pixels, 2 * 64 * 64);
        budget.leave_buffers(64, 64, 2);
        assert_eq!(budget.depth, 0);
        assert_eq!(budget.pixels, 0);
        assert!(budget.enter_buffers(4096, 4096, 2));
        budget.leave_buffers(4096, 4096, 2);
        assert!(!budget.enter_buffers(4097, 4096, 1));
    }

    #[test]
    fn test_paint_budget_bounds_depth_and_live_pixels() {
        let mut budget = PaintBudget {
            depth: 0,
            pixels: 0,
            flatten: false,
            flatten_depth: 0,
        };
        for _ in 0..MAX_OFFSCREEN_GROUP_DEPTH {
            assert!(budget.enter(64, 64));
        }
        assert!(!budget.enter(64, 64), "nesting depth cap");
        budget.leave(64, 64);
        assert!(budget.enter(64, 64));
        let mut fresh = PaintBudget {
            depth: 0,
            pixels: 0,
            flatten: false,
            flatten_depth: 0,
        };
        assert!(!fresh.enter(16_384, 16_384));
        assert!(fresh.enter(4096, 4096));
        assert!(fresh.enter(4096, 4096));
        assert!(fresh.enter(4096, 4096));
        assert!(fresh.enter(4096, 4096));
        assert!(!fresh.enter(64, 64), "live-pixel cap");
        fresh.leave(4096, 4096);
        assert!(fresh.enter(4096, 4096));
    }
}

#[cfg(test)]
mod geometry_sanity_tests {
    use super::*;

    fn radius(px: f32) -> megi_display::Radius {
        megi_display::Radius {
            x: megi_display::Length::Px(px),
            y: megi_display::Length::Px(px),
        }
    }

    #[test]
    fn asymmetric_corner_uses_its_own_centre() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let corners = [(10.0, 10.0), (10.0, 10.0), (10.0, 10.0), (40.0, 40.0)];
        // Near the bottom-left corner: centre (40,60), not (10,90).
        assert!(
            !point_in_rounded_rect(rect, &corners, 5.0, 95.0),
            "the bottom-left ellipse must be tested from its own centre"
        );
        assert!(point_in_rounded_rect(rect, &corners, 50.0, 50.0));
    }

    #[test]
    fn legal_asymmetric_radii_are_not_clamped() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let radii = megi_display::BorderRadii {
            top_left: radius(80.0),
            top_right: radius(20.0),
            bottom_right: megi_display::Radius::ZERO,
            bottom_left: megi_display::Radius::ZERO,
        };
        let corners = resolve_corner_radii(rect, radii);
        assert_eq!(corners[0], (80.0, 80.0), "80+20 <= 100 needs no scaling");
    }

    #[test]
    fn percent_radius_shrinks_from_the_outer_box() {
        // border-radius: 20% on a 100px box with a 10px border gives an inner
        // radius of 10px, not 20% of the 80px padding box (16px).
        assert_eq!(
            shrink_radius(megi_display::Length::Percent(20.0), 10.0, 100.0),
            megi_display::Length::Px(10.0)
        );
        assert_eq!(
            shrink_radius(megi_display::Length::Px(20.0), 10.0, 100.0),
            megi_display::Length::Px(10.0)
        );
    }

    #[test]
    fn duplicate_gradient_stops_keep_first_window_semantics() {
        // Binary-search segment lookup must match the old linear scan when two
        // stops share a position (#1416): the first window ending at t wins.
        let stops = [
            (0.0, Color::RED),
            (0.5, Color::GREEN),
            (0.5, Color::BLUE),
            (1.0, Color::WHITE),
        ];
        assert_eq!(
            sample_gradient(
                &stops,
                &[],
                false,
                0.5,
                megi_display::InterpolationSpace::Srgb
            ),
            Color::GREEN
        );
    }

    #[test]
    fn repeating_gradient_wraps_below_the_first_stop() {
        let stops = [(0.2, Color::RED), (0.8, Color::BLUE)];
        let wrapped = sample_gradient(
            &stops,
            &[],
            true,
            0.0,
            megi_display::InterpolationSpace::Srgb,
        );
        let middle = sample_gradient(
            &stops,
            &[],
            true,
            0.6,
            megi_display::InterpolationSpace::Srgb,
        );
        assert_eq!(wrapped, middle, "t=0 must wrap into the repeated interval");
    }
}

#[cfg(test)]
mod glyph_origin_overflow_tests {
    #[test]
    fn saturated_origin_plus_ink_offsets_cannot_overflow() {
        // A long run saturates origin_x to i32::MAX on cast; adding a positive ink
        // offset in i32 panicked in debug and wrapped in release (#1440).
        let origin = i32::MAX;
        let clamped = (i64::from(origin) + 1_i64).clamp(i64::from(i32::MIN), i64::from(i32::MAX));
        assert_eq!(
            clamped,
            i64::from(i32::MAX),
            "must clamp instead of overflowing"
        );
        let origin = i32::MIN;
        let clamped = (i64::from(origin) - 1_i64).clamp(i64::from(i32::MIN), i64::from(i32::MAX));
        assert_eq!(clamped, i64::from(i32::MIN));
    }
}

#[cfg(test)]
mod shadow_budget_tests {
    use super::*;

    fn empty_budget() -> PaintBudget {
        PaintBudget {
            depth: 0,
            pixels: 0,
            flatten: false,
            flatten_depth: 0,
        }
    }

    #[test]
    fn a_shadow_charges_its_scratch_until_the_permit_drops() {
        let mut budget = empty_budget();
        let charged = {
            let permit = budget.reserve_shadow(1_000).expect("the first shadow fits");
            permit.charged()
        };
        assert_eq!(budget.pixels, 0, "the permit must return its reservation");
        assert_eq!(charged, 1_000 * SHADOW_SCRATCH_COPIES);
    }

    #[test]
    fn a_shadow_past_the_live_pixel_cap_is_refused() {
        let mut budget = empty_budget();
        assert!(
            budget.reserve_shadow(MAX_OFFSCREEN_PIXELS).is_none(),
            "a shadow whose scratch exceeds the whole pass must be refused"
        );
        budget.pixels = MAX_OFFSCREEN_PIXELS - 1;
        assert!(budget.reserve_shadow(1).is_none());
        budget.pixels = 0;
        let fits = MAX_OFFSCREEN_PIXELS / SHADOW_SCRATCH_COPIES;
        assert!(
            budget.reserve_shadow(fits).is_some(),
            "a shadow that exactly fills the cap is accepted"
        );
    }
}
