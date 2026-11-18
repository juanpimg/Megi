//! Layout/paint and feature end-to-end workloads.

use super::*;

/// Layout and painting workloads share one realistic fixture: the document is
/// parsed, styled and laid out once, and each benchmark re-runs only its stage.
pub(super) fn register_layout_and_paint(suite: &mut Suite, scale: f64) {
    let target_bytes = generators::scale(generators::REALISTIC_TARGET_BYTES, scale);
    let realistic = generators::realistic_document(target_bytes);
    let html_source = realistic.html;
    let size_mib = html_source.len() as f64 / (1024.0 * 1024.0);
    let document = megi_html::parse_document(&html_source, false);
    let styles = megi_paint::resolve_embedded_styles(&document);
    let styled_elements = styles.len();
    let viewport = Rect::new(0.0, 0.0, VIEWPORT_WIDTH as f32, VIEWPORT_HEIGHT as f32);

    let Some(layout_root) = megi_layout::layout_document(&document, &styles, viewport) else {
        suite.skip("layout/realistic-1mb", "layout_document devolvió None");
        for name in [
            "paint/build-display-list",
            "paint/rasterize-800x600",
            "paint/render-to-png-800x600",
            "pipeline/html-to-png-800x600",
        ] {
            suite.skip(name, "layout_document devolvió None");
        }
        return;
    };

    // Two independent display lists so each paint benchmark can own one.
    let display_list_raster = megi_paint::build_display_list(&layout_root, viewport);
    let display_list_png = megi_paint::build_display_list(&layout_root, viewport);
    let display_items = display_list_raster.items.len();
    let png_bytes = {
        let pixmap =
            megi_paint::rasterize_display_list(&display_list_png, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
        pixmap.to_png().len()
    };
    let png_mib = png_bytes as f64 / (1024.0 * 1024.0);

    // Layout (fixture pre-parsed and pre-styled outside the timed window).
    suite.add(
        BenchEntry::new(
            "layout/realistic-1mb",
            "layout",
            format!(
                "{styled_elements} elementos con estilo, {size_mib:.2} MiB, viewport \
                 {VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT}"
            ),
            move || {
                black_box(megi_layout::layout_document(&document, &styles, viewport));
            },
        )
        .with_units(styled_elements as f64, "elements"),
    );

    // Display list generation from the pre-built layout tree.
    suite.add(BenchEntry::new(
        "paint/build-display-list",
        "paint",
        format!("{display_items} items de display list"),
        move || {
            black_box(megi_paint::build_display_list(&layout_root, viewport));
        },
    ));

    // Software rasterization at 800x600 (no PNG encoding).
    suite.add(
        BenchEntry::new(
            "paint/rasterize-800x600",
            "paint",
            format!("{display_items} items → pixmap {VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT}"),
            move || {
                black_box(megi_paint::rasterize_display_list(
                    &display_list_raster,
                    VIEWPORT_WIDTH,
                    VIEWPORT_HEIGHT,
                ));
            },
        )
        .with_units(
            (VIEWPORT_WIDTH as f64 * VIEWPORT_HEIGHT as f64) / 1_000_000.0,
            "MPix",
        )
        .heavy(),
    );

    // Rasterization + PNG encoding (the end-to-end paint tail).
    suite.add(
        BenchEntry::new(
            "paint/render-to-png-800x600",
            "paint",
            format!("rasterize + to_png → {png_bytes} bytes ({png_mib:.2} MiB)"),
            move || {
                let pixmap = megi_paint::rasterize_display_list(
                    &display_list_png,
                    VIEWPORT_WIDTH,
                    VIEWPORT_HEIGHT,
                );
                black_box(pixmap.to_png());
            },
        )
        .with_units(png_mib, "MiB")
        .heavy(),
    );

    // Full HTML → PNG pipeline (what a browser tab actually does in Phase 4).
    suite.add(
        BenchEntry::new(
            "pipeline/html-to-png-800x600",
            "pipeline",
            format!("{size_mib:.2} MiB: parse + styles + layout + display list + raster + PNG"),
            move || {
                let document = megi_html::parse_document(&html_source, false);
                let styles = megi_paint::resolve_embedded_styles(&document);
                let root = megi_layout::layout_document(&document, &styles, viewport)
                    .expect("fixture must lay out");
                let display_list = megi_paint::build_display_list(&root, viewport);
                let pixmap = megi_paint::rasterize_display_list(
                    &display_list,
                    viewport.width as u32,
                    viewport.height as u32,
                );
                black_box(pixmap.to_png());
            },
        )
        .heavy(),
    );
}

/// End-to-end rows for the features added in rounds 18–19 (gradients, grid,
/// custom properties/`calc()`, clipping) plus the pending sticky/text row.
///
/// These rows complement the legacy matrix: `docs/PERFORMANCE.md` keeps its
/// budgets, while these fixtures make a future feature regression visible in
/// the standard suite. Reference medians (r20, 800×600, release build, 5
/// iterations / heavy cap 3; see the round report for the exact host load):
/// treat a reproducible >15 % increase over that reference as a regression and
/// re-measure on an idle host before filing it.
///
/// - `pipeline/gradients-800x600`: ~60 boxes with linear/radial gradients
///   (stops, angles, alpha) through parse → styles → layout → display list →
///   raster → PNG.
/// - `pipeline/grid-800x600`: three grids (3×3 `1fr`, `repeat(auto-fill,
///   minmax())`, explicit spans) with text cells.
/// - `css/vars-calc-10k`: ~10k elements resolved with `:root` custom
///   properties, `var()` in colors/sizes/margins and simple `calc()` lengths.
/// - `paint/clipping-800x600`: 48 rounded `overflow:hidden` boxes and gradient
///   `background-clip:text` headings through display list + raster + PNG.
/// - `layout/sticky-text-1mb`: ~1 MiB document with `position: sticky` headers
///   and cards plus `text-transform: uppercase/lowercase/capitalize`
///   paragraphs; measures `layout_document` with styles resolved outside the
///   loop (round 20 added the sticky model, text-transform and mixed `calc()`).
pub(super) fn register_feature_e2e(suite: &mut Suite, scale: f64) {
    let viewport = Rect::new(0.0, 0.0, VIEWPORT_WIDTH as f32, VIEWPORT_HEIGHT as f32);

    // Gradients pipeline.
    let gradient_boxes = generators::scale(generators::GRADIENT_BOXES, scale);
    let gradients_html = generators::gradients_page(gradient_boxes);
    add_feature_pipeline(
        suite,
        "pipeline/gradients-800x600",
        format!("{gradient_boxes} cajas con linear/radial gradients"),
        gradients_html,
        viewport,
    );

    // Grid pipeline.
    let grid_html = generators::grid_page();
    add_feature_pipeline(
        suite,
        "pipeline/grid-800x600",
        format!(
            "3 grids (3x3 fr, auto-fill minmax, spans), {} celdas con texto",
            generators::GRID_CELLS
        ),
        grid_html,
        viewport,
    );

    // Custom properties + calc in the cascade.
    {
        let nodes = generators::scale(generators::VARS_CALC_NODES, scale);
        let markup = generators::element_document(nodes);
        let document = megi_html::parse_document(&markup, false);
        let elements = count_elements(&document);
        let author_css = generators::vars_calc_css();
        let vars = author_css.matches("var(--").count();
        let calcs = author_css.matches("calc(").count();
        suite.add(
            BenchEntry::new(
                "css/vars-calc-10k",
                "css",
                format!("{elements} elementos, {vars} var(), {calcs} calc()"),
                move || {
                    black_box(megi_css::resolve_document_styles(
                        &document,
                        Some(&author_css),
                    ));
                },
            )
            .with_units(elements as f64, "elements"),
        );
    }

    // Clipping paint tail (display list + raster + PNG).
    let clip_boxes = generators::scale(generators::CLIPPING_BOXES, scale);
    let clipping_html = generators::clipping_page(clip_boxes);
    let clip_detail = format!("{clip_boxes} cajas radius+overflow, clip:text");
    add_feature_pipeline(
        suite,
        "paint/clipping-800x600",
        clip_detail,
        clipping_html,
        viewport,
    );

    // Sticky positioning + text-transform layout (round 20 model).
    {
        let target_bytes = generators::scale(generators::STICKY_TEXT_TARGET_BYTES, scale);
        let markup = generators::sticky_text_page(target_bytes);
        let document = megi_html::parse_document(&markup, false);
        let styles = megi_paint::resolve_embedded_styles(&document);
        let styled_elements = styles.len();
        let mib = markup.len() as f64 / (1024.0 * 1024.0);
        suite.add(
            BenchEntry::new(
                "layout/sticky-text-1mb",
                "layout",
                format!(
                    "{styled_elements} elementos con estilo, {mib:.2} MiB, sticky headers + \
                     text-transform, viewport {VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT}"
                ),
                move || {
                    black_box(megi_layout::layout_document(&document, &styles, viewport));
                },
            )
            .with_units(styled_elements as f64, "elements"),
        );
    }
}

/// Owns a generated HTML page and registers a heavy `pipeline/...` row that
/// parses, styles, lays out, builds the display list, rasterizes and encodes
/// PNG (the same end-to-end tail as `pipeline/html-to-png-800x600`). Fixture
/// construction and the one-shot probe for the row detail happen before the
/// timed loop.
fn add_feature_pipeline(
    suite: &mut Suite,
    name: &'static str,
    what: String,
    html_source: String,
    viewport: Rect,
) {
    match probe_pipeline(&html_source, viewport) {
        Some((items, png_bytes)) => {
            suite.add(
                BenchEntry::new(
                    name,
                    "pipeline",
                    format!(
                        "{what}: {items} items de display list, PNG {png_bytes} B - parse+styles+layout+dl+raster+PNG"
                    ),
                    move || {
                        let document = megi_html::parse_document(&html_source, false);
                        let styles = megi_paint::resolve_embedded_styles(&document);
                        let root = megi_layout::layout_document(&document, &styles, viewport)
                            .expect("fixture must lay out");
                        let display_list = megi_paint::build_display_list(&root, viewport);
                        let pixmap = megi_paint::rasterize_display_list(
                            &display_list,
                            viewport.width as u32,
                            viewport.height as u32,
                        );
                        black_box(pixmap.to_png());
                    },
                )
                .heavy(),
            );
        }
        None => suite.skip(name, "layout_document devolvió None"),
    }
}

/// Runs the pipeline once outside the timed loop to validate the fixture and
/// report its display-list/PNG sizes. Returns `(items, png_bytes)`.
fn probe_pipeline(html_source: &str, viewport: Rect) -> Option<(usize, usize)> {
    let document = megi_html::parse_document(html_source, false);
    let styles = megi_paint::resolve_embedded_styles(&document);
    let root = megi_layout::layout_document(&document, &styles, viewport)?;
    let display_list = megi_paint::build_display_list(&root, viewport);
    let pixmap = megi_paint::rasterize_display_list(&display_list, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    Some((display_list.items.len(), pixmap.to_png().len()))
}
