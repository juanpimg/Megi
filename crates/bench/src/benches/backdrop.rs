//! Effect-fixture, backdrop and PNG-region encoder workloads.

use super::*;

/// End-to-end pipeline rows for the layer-effect fixtures (R173): parse →
/// styles → layout → display list → raster → PNG, with the committed pattern
/// asset available so the backdrop fixture mirrors the render output. The
/// `*-no-effect` rows strip the CSS declarations to expose the feature cost.
pub(super) fn register_effect_fixture_pipelines(suite: &mut Suite) {
    static PATTERN_PNG: &[u8] =
        include_bytes!("../../../../tests/fixtures/render/assets/pattern.png");

    fn pipeline_row(html_source: &'static str, width: u32, height: u32) -> impl Fn() {
        let document = megi_html::parse_document(html_source, false);
        let styles = megi_paint::resolve_embedded_styles(&document);
        let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
        let root = megi_layout::layout_document(&document, &styles, viewport);
        let mut backgrounds = megi_paint::BackgroundImages::new();
        if let Ok(pattern) = megi_image::decode(PATTERN_PNG) {
            backgrounds.insert("assets/pattern.png".to_string(), Arc::new(pattern));
        }
        let images = megi_paint::ImageStore::new();
        move || {
            if let Some(root) = &root {
                let display_list = megi_paint::build_display_list_with_images_and_backgrounds(
                    root,
                    viewport,
                    &images,
                    &backgrounds,
                );
                let pixmap = megi_paint::rasterize_display_list(&display_list, width, height);
                black_box(pixmap.to_png());
            }
        }
    }

    fn without_property(fixture: &'static str, properties: &[&str]) -> &'static str {
        let mut text = fixture.to_string();
        for property in properties {
            let needle = format!("{property}:");
            text = text
                .lines()
                .filter(|line| !line.trim_start().starts_with(&needle))
                .collect::<Vec<_>>()
                .join("\n");
        }
        Box::leak(text.into_boxed_str())
    }

    let filter_fixture: &'static str =
        include_str!("../../../../tests/fixtures/render/filter_effects.html");
    let filter_bare = without_property(filter_fixture, &["filter", "-webkit-filter"]);
    suite.add(BenchEntry::new(
        "pipeline/filter-fixture-300x260",
        "pipeline",
        "filter_effects.html → parse/styles/layout/display-list/raster/PNG".to_string(),
        pipeline_row(filter_fixture, 300, 260),
    ));
    suite.add(BenchEntry::new(
        "pipeline/filter-fixture-300x260-no-effect",
        "pipeline",
        "same fixture without filter declarations".to_string(),
        pipeline_row(filter_bare, 300, 260),
    ));

    let backdrop_fixture: &'static str =
        include_str!("../../../../tests/fixtures/render/backdrop_filter.html");
    let backdrop_bare = without_property(
        backdrop_fixture,
        &["backdrop-filter", "-webkit-backdrop-filter"],
    );
    suite.add(BenchEntry::new(
        "pipeline/backdrop-fixture-300x260",
        "pipeline",
        "backdrop_filter.html (pattern asset) → full pipeline + PNG".to_string(),
        pipeline_row(backdrop_fixture, 300, 260),
    ));
    suite.add(BenchEntry::new(
        "pipeline/backdrop-fixture-300x260-no-effect",
        "pipeline",
        "same fixture without backdrop-filter declarations".to_string(),
        pipeline_row(backdrop_bare, 300, 260),
    ));
}

/// Sub-stage rows for backdrop-filter (R181): raster of the display list vs
/// PNG encoding of the resulting pixmap, full fixture and no-effect variant.
/// This splits the r27 E2E overhead (+0,55 ms) between the two stages.
pub(super) fn register_backdrop_stage_rows(suite: &mut Suite) {
    fn stages(
        html_source: &'static str,
        width: u32,
        height: u32,
    ) -> (megi_paint::DisplayList, megi_paint::Pixmap) {
        static PATTERN_PNG: &[u8] =
            include_bytes!("../../../../tests/fixtures/render/assets/pattern.png");
        let document = megi_html::parse_document(html_source, false);
        let styles = megi_paint::resolve_embedded_styles(&document);
        let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
        let root = megi_layout::layout_document(&document, &styles, viewport).expect("layout");
        let mut backgrounds = megi_paint::BackgroundImages::new();
        if let Ok(pattern) = megi_image::decode(PATTERN_PNG) {
            backgrounds.insert("assets/pattern.png".to_string(), Arc::new(pattern));
        }
        let images = megi_paint::ImageStore::new();
        let display_list = megi_paint::build_display_list_with_images_and_backgrounds(
            &root,
            viewport,
            &images,
            &backgrounds,
        );
        let pixmap = megi_paint::rasterize_display_list(&display_list, width, height);
        (display_list, pixmap)
    }
    fn strip(html_source: &'static str, property: &str) -> &'static str {
        let needle = format!("{property}:");
        let stripped = html_source
            .lines()
            .filter(|line| !line.trim_start().starts_with(&needle))
            .collect::<Vec<_>>()
            .join("\n");
        Box::leak(stripped.into_boxed_str())
    }
    let fixture: &'static str =
        include_str!("../../../../tests/fixtures/render/backdrop_filter.html");
    let bare = strip(fixture, "backdrop-filter");
    let (display_list, pixmap) = stages(fixture, 300, 260);
    let (display_list_bare, pixmap_bare) = stages(bare, 300, 260);
    suite.add(BenchEntry::new(
        "paint/backdrop-fixture-raster",
        "paint",
        "backdrop_filter.html display list → pixmap (no PNG)".to_string(),
        move || {
            black_box(megi_paint::rasterize_display_list(&display_list, 300, 260));
        },
    ));
    suite.add(BenchEntry::new(
        "paint/backdrop-fixture-raster-no-effect",
        "paint",
        "same fixture without backdrop-filter → pixmap (no PNG)".to_string(),
        move || {
            black_box(megi_paint::rasterize_display_list(
                &display_list_bare,
                300,
                260,
            ));
        },
    ));
    suite.add(BenchEntry::new(
        "paint/backdrop-fixture-png",
        "paint",
        "backdrop_filter.html pixmap → PNG encode".to_string(),
        move || {
            black_box(pixmap.to_png());
        },
    ));
    suite.add(BenchEntry::new(
        "paint/backdrop-fixture-png-no-effect",
        "paint",
        "same fixture without backdrop-filter → PNG encode".to_string(),
        move || {
            black_box(pixmap_bare.to_png());
        },
    ));
}

/// R231: synthetic backdrop-blur raster rows (blur-only and blur+color ops).
///
/// The display list paints a checker background, then `BackdropFilter` items over
/// cells; the `no-effect` row paints the same background without the backdrop
/// filters so the delta isolates blur/color cost.
pub(super) fn register_backdrop_blur_rows(suite: &mut Suite) {
    let width = 800u32;
    let height = 600u32;
    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    fn checker() -> Vec<megi_paint::DisplayItem> {
        let mut items = vec![megi_paint::DisplayItem::SolidColor {
            rect: Rect::new(0.0, 0.0, 800.0, 600.0),
            color: megi_css::Color::WHITE,
        }];
        for y in 0..20 {
            for x in 0..27 {
                let color = if (x + y) % 2 == 0 {
                    megi_css::Color::new(220, 40, 60, 255)
                } else {
                    megi_css::Color::new(30, 90, 200, 255)
                };
                items.push(megi_paint::DisplayItem::SolidColor {
                    rect: Rect::new((x * 30) as f32, (y * 30) as f32, 30.0, 30.0),
                    color,
                });
            }
        }
        items
    }
    fn with_filters(filters: Vec<megi_css::FilterFunction>) -> Vec<megi_paint::DisplayItem> {
        let mut items = checker();
        for y in 0..4 {
            for x in 0..3 {
                items.push(megi_paint::DisplayItem::BackdropFilter {
                    rect: Rect::new(
                        40.0 + (x * 230) as f32,
                        30.0 + (y * 140) as f32,
                        180.0,
                        110.0,
                    ),
                    filters: filters.clone(),
                });
            }
        }
        items
    }
    for (name, filters) in [
        (
            "paint/backdrop-blur-800x600",
            vec![megi_css::FilterFunction::Blur(4.0)],
        ),
        (
            "paint/backdrop-blur-color-800x600",
            vec![
                megi_css::FilterFunction::Blur(4.0),
                megi_css::FilterFunction::Brightness(0.9),
                megi_css::FilterFunction::Grayscale(0.2),
            ],
        ),
    ] {
        let list = megi_paint::DisplayList {
            items: with_filters(filters),
        };
        suite.add(
            BenchEntry::new(
                name,
                "paint",
                "12 backdrop-filter overlays over a checker → pixmap".to_string(),
                move || {
                    black_box(megi_paint::rasterize_display_list(&list, width, height));
                },
            )
            .heavy(),
        );
    }
    let no_effect = megi_paint::DisplayList { items: checker() };
    suite.add(
        BenchEntry::new(
            "paint/backdrop-blur-no-effect-800x600",
            "paint",
            "same checker background without backdrop filters".to_string(),
            move || {
                black_box(megi_paint::rasterize_display_list(
                    &no_effect, width, height,
                ));
            },
        )
        .heavy(),
    );
    let _ = viewport;
}

/// R216: full PNG encode vs row-cached region encode at 10/25/50/100 % dirty rows.
///
/// The cache re-filters only changed rows (+ the row below each change, whose PNG
/// filter depends on its predecessor); the DEFLATE stream still covers the whole
/// image, so output bytes are identical to `Pixmap::to_png()`. Each pair mutates the
/// same rows and colors, making the A/B fair at both report sizes.
pub(super) fn register_png_region_encoders(suite: &mut Suite) {
    fn fixture(width: u32, height: u32) -> megi_paint::Pixmap {
        let mut pixmap = megi_paint::Pixmap::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let r = ((x * 7 + y * 3) % 256) as u8;
                let g = ((x * 2 + y * 11) % 256) as u8;
                let b = ((x * 5 + y * 5) % 256) as u8;
                pixmap.set_pixel(x, y, megi_css::Color::new(r, g, b, 255));
            }
        }
        pixmap
    }

    fn fill_band(pixmap: &mut megi_paint::Pixmap, rows: u32, color: megi_css::Color) {
        let stride = pixmap.width as usize;
        let end = (rows as usize * stride).min(pixmap.pixels.len());
        pixmap.pixels[..end].fill(color);
    }

    for (width, height) in [(800u32, 600u32), (1024, 768)] {
        for pct in [10u32, 25, 50, 100] {
            let dirty_rows = ((height as u64 * u64::from(pct)) / 100) as u32;
            let dirty = megi_layout::Rect::new(0.0, 0.0, width as f32, dirty_rows as f32);
            let full_name: &'static str = match (width, pct) {
                (800, 10) => "paint/png-encode-full-800x600-10pct",
                (800, 25) => "paint/png-encode-full-800x600-25pct",
                (800, 50) => "paint/png-encode-full-800x600-50pct",
                (800, _) => "paint/png-encode-full-800x600-100pct",
                (_, 10) => "paint/png-encode-full-1024x768-10pct",
                (_, 25) => "paint/png-encode-full-1024x768-25pct",
                (_, 50) => "paint/png-encode-full-1024x768-50pct",
                (_, _) => "paint/png-encode-full-1024x768-100pct",
            };
            let region_name: &'static str = match (width, pct) {
                (800, 10) => "paint/png-encode-regions-10pct",
                (800, 25) => "paint/png-encode-regions-25pct",
                (800, 50) => "paint/png-encode-regions-50pct",
                (800, _) => "paint/png-encode-regions-100pct",
                (_, 10) => "paint/png-encode-regions-1024x768-10pct",
                (_, 25) => "paint/png-encode-regions-1024x768-25pct",
                (_, 50) => "paint/png-encode-regions-1024x768-50pct",
                (_, _) => "paint/png-encode-regions-1024x768-100pct",
            };

            let mut full_pixmap = fixture(width, height);
            let mut full_alternate = false;
            suite.add(
                BenchEntry::new(
                    full_name,
                    "paint",
                    format!("full Pixmap::to_png {width}x{height}, {pct}% rows redrawn"),
                    move || {
                        full_alternate = !full_alternate;
                        let color = if full_alternate {
                            megi_css::Color::new(11, 22, 33, 255)
                        } else {
                            megi_css::Color::new(44, 55, 66, 255)
                        };
                        fill_band(&mut full_pixmap, dirty_rows, color);
                        black_box(full_pixmap.to_png());
                    },
                )
                .heavy(),
            );

            let mut pixmap = fixture(width, height);
            let mut encoder = megi_paint::PngRegionEncoder::new();
            let _ = encoder.encode_regions(&pixmap, &[dirty]);
            let mut alternate = false;
            suite.add(
                BenchEntry::new(
                    region_name,
                    "paint",
                    format!("row-cached to_png {width}x{height}, {pct}% dirty rows"),
                    move || {
                        alternate = !alternate;
                        let color = if alternate {
                            megi_css::Color::new(11, 22, 33, 255)
                        } else {
                            megi_css::Color::new(44, 55, 66, 255)
                        };
                        fill_band(&mut pixmap, dirty_rows, color);
                        black_box(encoder.encode_regions(&pixmap, &[dirty]));
                    },
                )
                .heavy(),
            );
        }
    }
}
