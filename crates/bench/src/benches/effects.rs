//! Layer-effect and fixture-driven effect workloads.

use super::*;

/// Round 26 paint-layer benchmarks: `filter`/`backdrop-filter`/`object-fit`
/// raster and build costs, plus the isolated save-png components.
///
/// These rows isolate the new layer effects from parse/layout/PNG so a
/// regression/optimisation is visible in the standard suite:
/// - `paint/layer-effects-800x600`: 24 backdrop-filter + 24 filter groups over
///   a checker background, rasterized to a pixmap (no PNG encode).
/// - `paint/object-fit-800x600`: display list + raster of 128 replaced images
///   with contain/cover/none/scale-down.
/// - `shell/save-png-encode-800x600`: `Pixmap::to_png()` of an 800×600 pattern.
/// - `shell/save-png-write-800x600`: writing those PNG bytes to disk.
/// - `shell/save-png-total-800x600`: encode + write (isolated `--save-png`).
pub(super) fn register_layer_effects(suite: &mut Suite) {
    let viewport = Rect::new(0.0, 0.0, VIEWPORT_WIDTH as f32, VIEWPORT_HEIGHT as f32);

    // (a) Filter / backdrop-filter raster cost on a synthetic layer stack.
    {
        let mut items: Vec<megi_paint::DisplayItem> = Vec::new();
        items.push(megi_paint::DisplayItem::SolidColor {
            rect: viewport,
            color: megi_css::Color::WHITE,
        });
        for index in 0..24 {
            let cell = Rect::new(
                (index % 6) as f32 * 130.0 + 8.0,
                (index / 6) as f32 * 140.0 + 8.0,
                120.0,
                130.0,
            );
            items.push(megi_paint::DisplayItem::SolidColor {
                rect: cell,
                color: megi_css::Color::new((index * 9) as u8, 90, 180, 255),
            });
            let color_filters = vec![
                megi_css::FilterFunction::Brightness(0.85),
                megi_css::FilterFunction::Grayscale(0.2),
            ];
            let spatial_filters = vec![
                megi_css::FilterFunction::Blur(2.0),
                megi_css::FilterFunction::Brightness(1.05),
            ];
            items.push(megi_paint::DisplayItem::BackdropFilter {
                rect: cell,
                filters: if index % 2 == 0 {
                    color_filters
                } else {
                    spatial_filters
                },
            });
            items.push(megi_paint::DisplayItem::EffectGroup {
                rect: Rect::new(
                    cell.x + 4.0,
                    cell.y + 4.0,
                    cell.width - 8.0,
                    cell.height - 8.0,
                ),
                filters: vec![
                    megi_css::FilterFunction::Brightness(0.9),
                    megi_css::FilterFunction::Contrast(1.1),
                    megi_css::FilterFunction::Saturate(1.2),
                ],
                blend: megi_css::MixBlendMode::Normal,
                items: vec![megi_paint::DisplayItem::SolidColor {
                    rect: cell,
                    color: megi_css::Color::new(240, 120, 40, 200),
                }],
            });
        }
        let display_list = megi_paint::DisplayList { items };
        suite.add(BenchEntry::new(
            "paint/layer-effects-800x600",
            "paint",
            "24 backdrop-filter + 24 filter groups → pixmap".to_string(),
            move || {
                black_box(megi_paint::rasterize_display_list(
                    &display_list,
                    VIEWPORT_WIDTH,
                    VIEWPORT_HEIGHT,
                ));
            },
        ));
    }

    // (b) object-fit: display list + raster of replaced images.
    {
        let mut document = Document::new();
        let html_node = document.create_element(megi_core::Namespace::Html, "html", vec![]);
        document.append_child(document.root(), html_node);
        let mut root_style = megi_css::ComputedStyle::initial();
        root_style.display = megi_css::Display::Block;
        root_style.background_color = megi_css::Color::WHITE;
        let mut root = megi_layout::LayoutBox::new(
            megi_layout::BoxType::BlockNode(html_node),
            Some(root_style),
        );
        root.dimensions.content = viewport;
        let image = Arc::new(
            megi_image::RgbaImage::new(16, 10, vec![120u8; 16 * 10 * 4]).expect("RGBA fixture"),
        );
        let mut images = megi_paint::ImageStore::new();
        let fits = [
            megi_css::ObjectFit::Contain,
            megi_css::ObjectFit::Cover,
            megi_css::ObjectFit::None,
            megi_css::ObjectFit::ScaleDown,
        ];
        for index in 0..128u32 {
            let node = document.create_element(megi_core::Namespace::Html, "img", vec![]);
            document.append_child(html_node, node);
            images.insert(node, Arc::clone(&image));
            let mut style = megi_css::ComputedStyle::initial();
            style.display = megi_css::Display::Block;
            style.object_fit = fits[index as usize % fits.len()];
            let mut child =
                megi_layout::LayoutBox::new(megi_layout::BoxType::Replaced(node), Some(style));
            child.dimensions = megi_layout::Dimensions {
                content: Rect::new(
                    (index % 16) as f32 * 50.0,
                    (index / 16) as f32 * 72.0,
                    40.0,
                    30.0,
                ),
                padding: megi_layout::EdgeSizes::ZERO,
                border: megi_layout::EdgeSizes::ZERO,
                margin: megi_layout::EdgeSizes::ZERO,
            };
            root.children.push(child);
        }
        let backgrounds = megi_paint::BackgroundImages::new();
        suite.add(BenchEntry::new(
            "paint/object-fit-800x600",
            "paint",
            "128 replaced images (contain/cover/none/scale-down) → display list + pixmap"
                .to_string(),
            move || {
                let display_list = megi_paint::build_display_list_with_images_and_backgrounds(
                    &root,
                    viewport,
                    &images,
                    &backgrounds,
                );
                black_box(megi_paint::rasterize_display_list(
                    &display_list,
                    VIEWPORT_WIDTH,
                    VIEWPORT_HEIGHT,
                ));
            },
        ));
    }

    // (c) Isolated save-png components (encode / write / both).
    {
        let mut pattern = megi_paint::Pixmap::new_with_background(
            VIEWPORT_WIDTH,
            VIEWPORT_HEIGHT,
            megi_css::Color::WHITE,
        );
        for y in (0..VIEWPORT_HEIGHT).step_by(16) {
            for x in (0..VIEWPORT_WIDTH).step_by(16) {
                pattern.fill_rect(
                    Rect::new(x as f32, y as f32, 8.0, 8.0),
                    megi_css::Color::new(30, 60, 120, 255),
                );
            }
        }
        let png_bytes = pattern.to_png();
        let pattern = Arc::new(pattern);
        let output_path = std::env::temp_dir().join("megi-bench-save-png-800x600.png");
        let pattern_for_encode = Arc::clone(&pattern);
        suite.add(BenchEntry::new(
            "shell/save-png-encode-800x600",
            "shell",
            format!("Pixmap::to_png 800x600 ({} B)", png_bytes.len()),
            move || {
                black_box(pattern_for_encode.to_png());
            },
        ));
        let bytes_for_write = png_bytes.clone();
        let path_for_write = output_path.clone();
        suite.add(BenchEntry::new(
            "shell/save-png-write-800x600",
            "shell",
            format!("fs::write {} B", bytes_for_write.len()),
            move || {
                std::fs::write(&path_for_write, &bytes_for_write).expect("write png");
                black_box(bytes_for_write.len());
            },
        ));
        let path_for_total = output_path;
        suite.add(BenchEntry::new(
            "shell/save-png-total-800x600",
            "shell",
            "encode + write (isolated --save-png cost)".to_string(),
            move || {
                let bytes = pattern.to_png();
                std::fs::write(&path_for_total, &bytes).expect("write png");
                black_box(bytes.len());
            },
        ));
    }

    // (d) Chrome screenshot sizes: the final PNG encode is the floor of
    // `--chrome` (the page pixmap is already rasterized; only the top band is
    // painted, then the full PNG is re-encoded). R196 baseline: +16,91 ms total
    // chrome overhead; these rows document the encoder share at both viewports.
    for (width, height) in [(640u32, 400u32), (1024, 768)] {
        let mut pattern =
            megi_paint::Pixmap::new_with_background(width, height, megi_css::Color::WHITE);
        for y in 0..height {
            let color = if y < 58 {
                megi_css::Color::new(0x20, 0x21, 0x24, 255)
            } else {
                megi_css::Color::new(0xF1, 0xF3, 0xF4, 255)
            };
            pattern.fill_rect(Rect::new(0.0, y as f32, width as f32, 1.0), color);
        }
        suite.add(BenchEntry::new(
            if width == 640 {
                "shell/chrome-png-640x400"
            } else {
                "shell/chrome-png-1024x768"
            },
            "shell",
            format!("Pixmap::to_png chrome-sized {width}x{height} (final --chrome encode floor)"),
            move || {
                black_box(pattern.to_png());
            },
        ));
    }
}

/// Fixture-based layer-effect rows (R166): full feature vs. the same fixture
/// with its `filter`/`backdrop-filter` declarations stripped, measuring only the
/// paint tail (display list + raster, no PNG encode). The delta isolates the
/// per-fixture layer-effect cost so an A/B between paint binaries is meaningful.
pub(super) fn register_fixture_effect_rows(suite: &mut Suite) {
    fn paint_tail(html_source: &'static str, viewport: Rect) -> impl Fn() {
        let document = megi_html::parse_document(html_source, false);
        let styles = megi_paint::resolve_embedded_styles(&document);
        let root = megi_layout::layout_document(&document, &styles, viewport);
        let images = megi_paint::ImageStore::new();
        let backgrounds = megi_paint::BackgroundImages::new();
        move || {
            if let Some(root) = &root {
                let display_list = megi_paint::build_display_list_with_images_and_backgrounds(
                    root,
                    viewport,
                    &images,
                    &backgrounds,
                );
                black_box(megi_paint::rasterize_display_list(
                    &display_list,
                    viewport.width as u32,
                    viewport.height as u32,
                ));
            }
        }
    }
    let filter_fixture: &'static str =
        include_str!("../../../../tests/fixtures/render/filter_effects.html");
    let filter_stripped: String = filter_fixture
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !(line.starts_with("filter:") || line.starts_with("-webkit-filter:"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let filter_stripped: &'static str = Box::leak(filter_stripped.into_boxed_str());
    let filter_viewport = Rect::new(0.0, 0.0, 300.0, 260.0);
    suite.add(BenchEntry::new(
        "paint/filter-fixture",
        "paint",
        "filter_effects.html (11 filtered boxes) → display list + pixmap".to_string(),
        paint_tail(filter_fixture, filter_viewport),
    ));
    suite.add(BenchEntry::new(
        "paint/filter-fixture-no-effect",
        "paint",
        "same fixture without filter declarations → display list + pixmap".to_string(),
        paint_tail(filter_stripped, filter_viewport),
    ));

    let backdrop_fixture: &'static str =
        include_str!("../../../../tests/fixtures/render/backdrop_filter.html");
    let backdrop_stripped: String = backdrop_fixture
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !(line.starts_with("backdrop-filter:") || line.starts_with("-webkit-backdrop-filter:"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let backdrop_stripped: &'static str = Box::leak(backdrop_stripped.into_boxed_str());
    let backdrop_viewport = Rect::new(0.0, 0.0, 300.0, 240.0);
    suite.add(BenchEntry::new(
        "paint/backdrop-fixture",
        "paint",
        "backdrop_filter.html (blur + dim overlays) → display list + pixmap".to_string(),
        paint_tail(backdrop_fixture, backdrop_viewport),
    ));
    suite.add(BenchEntry::new(
        "paint/backdrop-fixture-no-effect",
        "paint",
        "same fixture without backdrop-filter declarations → display list + pixmap".to_string(),
        paint_tail(backdrop_stripped, backdrop_viewport),
    ));
}
