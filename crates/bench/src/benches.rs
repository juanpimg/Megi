//! Benchmark registry: every measured workload lives here.
//!
//! Fixtures are generated and parsed in [`register`], *before* the timed loop;
//! only the operation under test runs inside the measurement window. The
//! division mirrors `docs/PERFORMANCE.md` and the row names are a stable API
//! for regression tracking.

use std::hint::black_box;
use std::sync::Arc;

use dom::{Document, NodeId};
use html::ParseOptions;
use layout::Rect;

use crate::cli::Options;
use crate::generators;
use crate::runner::{BenchEntry, Suite};

/// Parse limits used to measure the *algorithmic* cost of extreme inputs.
///
/// The production defaults (`max_depth: 4096`, `max_attributes_per_element:
/// 4096`) intentionally truncate hostile input; measuring them would only show
/// the cost of the guard, not of the parser. These benchmarks therefore lift
/// every limit so the reported time is the parser's real work on the full
/// input. `html/nested-div-64k-default-limits` keeps the defaults on purpose
/// and documents the anti-DoS behaviour.
const GENEROUS_PARSE: ParseOptions = ParseOptions {
    max_depth: usize::MAX,
    max_nodes: usize::MAX,
    max_attributes_per_element: usize::MAX,
    max_input_bytes: usize::MAX,
};

const VIEWPORT_WIDTH: u32 = 800;
const VIEWPORT_HEIGHT: u32 = 600;

/// Committed 800×600 PNG fixture from `crates/image` (13 947 B, RGBA8).
static PNG_800X600: &[u8] = include_bytes!("../../image/tests/fixtures/png/large_800x600.png");
/// Deterministic baseline JPEG (800×600, quality 85, ~75 KiB). See
/// `crates/bench/fixtures/README.md` for the exact generation recipe.
static JPEG_800X600: &[u8] = include_bytes!("../fixtures/jpeg-800x600.jpg");
/// Deterministic indexed GIF (800×600, single frame, ~82 KiB); the decoder only
/// materialises the first frame.
static GIF_800X600: &[u8] = include_bytes!("../fixtures/gif-800x600.gif");
/// Real libwebp lossless photo (64×48, predictor/colour transforms) generated
/// with Pillow; committed in `crates/image` and pixel-verified against PIL.
static WEBP_PHOTO: &[u8] = include_bytes!("../../image/tests/fixtures/webp/photo_transforms.webp");
/// Real libwebp lossless alpha image (40×32, straight alpha).
static WEBP_ALPHA: &[u8] = include_bytes!("../../image/tests/fixtures/webp/alpha_real.webp");
/// Real libwebp lossless palette image (32×24, colour indexing).
static WEBP_PALETTE: &[u8] = include_bytes!("../../image/tests/fixtures/webp/palette_real.webp");
/// Real libwebp lossy VP8 keyframe (1024×768, PIL q75, 40 552 B) committed in
/// `crates/image`; decoded by the in-tree VP8 decoder (from-scratch port).
static WEBP_LOSSY_1024: &[u8] =
    include_bytes!("../../image/tests/fixtures/webp/photo_1024x768_lossy.webp");
/// Real libwebp lossy VP8 keyframe (64×48, PIL q75).
static WEBP_LOSSY_Q75: &[u8] =
    include_bytes!("../../image/tests/fixtures/webp/photo_lossy_q75.webp");
/// Real libwebp lossy VP8 keyframe (64×48, PIL q90).
static WEBP_LOSSY_Q90: &[u8] =
    include_bytes!("../../image/tests/fixtures/webp/photo_lossy_q90.webp");

/// Registers every benchmark into `suite`, applying `opts.scale` to the sizes.
pub fn register(suite: &mut Suite, opts: &Options) {
    register_html(suite, opts.scale);
    register_css(suite, opts.scale);
    register_layout_and_paint(suite, opts.scale);
    register_feature_e2e(suite, opts.scale);
    register_layer_effects(suite);
    register_fixture_effect_rows(suite);
    register_effect_fixture_pipelines(suite);
    register_backdrop_stage_rows(suite);
    register_image(suite);
    register_render_pipeline(suite);
    register_png_rows(suite);
    #[cfg(feature = "fonts")]
    register_fonts(suite, opts.scale);
}

/// HTML tokenizer + tree construction workloads.
fn register_html(suite: &mut Suite, scale: f64) {
    // (a) Realistic ~1 MiB page (headings, paragraphs, inline formatting, lists, tables).
    let target_bytes = generators::scale(generators::REALISTIC_TARGET_BYTES, scale);
    let realistic = generators::realistic_document(target_bytes);
    let size_mib = realistic.html.len() as f64 / (1024.0 * 1024.0);
    {
        let input = realistic.html.clone();
        let detail = format!(
            "{size_mib:.2} MiB, {} secciones, {} reglas CSS",
            realistic.sections, realistic.style_rules
        );
        suite.add(
            BenchEntry::new("html/realistic-1mb", "html", detail, move || {
                black_box(html::parse_document(&input, false));
            })
            .with_units(size_mib, "MiB"),
        );
    }

    // (b) Deeply nested `<div>` (the classic quadratic-tree-builder trigger).
    for (base_depth, name) in [
        (generators::NESTED_DEPTHS[0], "html/nested-div-10k"),
        (generators::NESTED_DEPTHS[1], "html/nested-div-32k"),
        (generators::NESTED_DEPTHS[2], "html/nested-div-64k"),
    ] {
        let depth = generators::scale(base_depth, scale);
        let input = generators::nested_divs(depth);
        let mib = input.len() as f64 / (1024.0 * 1024.0);
        let entry = BenchEntry::new(
            name,
            "html",
            format!("{depth} divs anidados, {mib:.2} MiB"),
            move || {
                black_box(html::parse_document_with_options(
                    &input,
                    false,
                    &GENEROUS_PARSE,
                ));
            },
        )
        .with_units(mib, "MiB");
        // 32k/64k are the expensive probes: keep their iteration count low.
        suite.add(if base_depth >= 32_000 {
            entry.heavy()
        } else {
            entry
        });
    }

    // (b') Same input with production anti-DoS limits, for the guard's cost.
    {
        let depth = generators::scale(generators::NESTED_DEPTHS[2], scale);
        let input = generators::nested_divs(depth);
        let probe = html::parse_document_with_options(&input, false, &html::ParseOptions::DEFAULT);
        let nodes = probe.document.len();
        let dropped = probe.limit_violations.depth_exceeded;
        let detail = format!(
            "{depth} divs con límites por defecto: profundidad máx. 4096, \
             {dropped} start tags descartados, {nodes} nodos finales"
        );
        suite.add(BenchEntry::new(
            "html/nested-div-64k-default-limits",
            "html",
            detail,
            move || {
                black_box(html::parse_document(&input, false));
            },
        ));
    }

    // (c) Massive stray `</p>` end tags.
    {
        let count = generators::scale(generators::END_P_TAGS, scale);
        let input = generators::massive_end_p(count);
        let mib = input.len() as f64 / (1024.0 * 1024.0);
        suite.add(
            BenchEntry::new(
                "html/massive-end-p",
                "html",
                format!("{count} </p> consecutivos, {mib:.2} MiB"),
                move || {
                    black_box(html::parse_document(&input, false));
                },
            )
            .with_units(count as f64, "tags"),
        );
    }

    // (d) Massive formatting elements: every close runs the adoption agency.
    {
        let count = generators::scale(generators::FORMATTING_BLOCKS, scale);
        let input = generators::massive_formatting(count);
        let mib = input.len() as f64 / (1024.0 * 1024.0);
        suite.add(
            BenchEntry::new(
                "html/massive-formatting",
                "html",
                format!("{count} bloques <b><i><u> mal anidados, {mib:.2} MiB"),
                move || {
                    black_box(html::parse_document(&input, false));
                },
            )
            .with_units(count as f64, "blocks"),
        );
    }

    // (e) One element carrying 40k attributes.
    {
        let count = generators::scale(generators::ATTRIBUTE_COUNT, scale);
        let input = generators::tag_with_attributes(count);
        let mib = input.len() as f64 / (1024.0 * 1024.0);
        suite.add(
            BenchEntry::new(
                "html/tag-40k-attrs",
                "html",
                format!("{count} atributos en un solo elemento, {mib:.2} MiB"),
                move || {
                    black_box(html::parse_document_with_options(
                        &input,
                        false,
                        &GENEROUS_PARSE,
                    ));
                },
            )
            .with_units(count as f64, "attributes")
            .heavy(),
        );
    }

    // (f) Pretty-printed document (5k indented blocks).
    {
        let blocks = generators::scale(generators::PRETTY_BLOCKS, scale);
        let input = generators::pretty_printed(blocks);
        let mib = input.len() as f64 / (1024.0 * 1024.0);
        suite.add(
            BenchEntry::new(
                "html/pretty-5k-blocks",
                "html",
                format!("{blocks} bloques indentados, {mib:.2} MiB"),
                move || {
                    black_box(html::parse_document(&input, false));
                },
            )
            .with_units(blocks as f64, "blocks"),
        );
    }
}

/// CSS parsing, cascade/style resolution and selector matching.
fn register_css(suite: &mut Suite, scale: f64) {
    // Stylesheet parsing.
    {
        let rules = generators::scale(generators::CSS_RULES, scale);
        let sheet = generators::stylesheet_rules(rules);
        let mib = sheet.len() as f64 / (1024.0 * 1024.0);
        suite.add(
            BenchEntry::new(
                "css/parse-10k-rules",
                "css",
                format!("{rules} reglas, {mib:.2} MiB"),
                move || {
                    black_box(css::parse_stylesheet(&sheet));
                },
            )
            .with_units(mib, "MiB"),
        );
    }

    // Cascade + computed style resolution over 2k and 10k element documents.
    for (base_nodes, name) in [
        (generators::STYLE_NODES[0], "css/resolve-styles-2k-nodes"),
        (generators::STYLE_NODES[1], "css/resolve-styles-10k-nodes"),
    ] {
        let target = generators::scale(base_nodes, scale);
        let markup = generators::element_document(target);
        let document = html::parse_document(&markup, false);
        let elements = count_elements(&document);
        let author_css = generators::element_author_css();
        let rule_count = author_css.matches('{').count();
        suite.add(
            BenchEntry::new(
                name,
                "css",
                format!("{elements} elementos, {rule_count} reglas de autor"),
                move || {
                    black_box(css::resolve_document_styles(&document, Some(&author_css)));
                },
            )
            .with_units(elements as f64, "elements"),
        );
    }

    // Selector matching: simple compounds and descendant combinators.
    let markup = generators::element_document(generators::scale(generators::SELECTOR_NODES, scale));
    {
        let document = html::parse_document(&markup, false);
        let elements = collect_elements(&document);
        let selectors = css::parse_selectors("div.item, span.label, .c0, #e42");
        let selector_count = selectors.selectors.len().max(1);
        let element_count = elements.len();
        let matches = (element_count * selector_count) as f64;
        suite.add(
            BenchEntry::new(
                "css/selector-match-simple",
                "css",
                format!("{element_count} elementos × {selector_count} selectores simples"),
                move || {
                    let mut hits = 0usize;
                    for &element in &elements {
                        for selector in &selectors.selectors {
                            if selector.matches(&document, element) {
                                hits += 1;
                            }
                        }
                    }
                    black_box(hits);
                },
            )
            .with_units(matches, "matches"),
        );
    }
    {
        let document = html::parse_document(&markup, false);
        let elements = collect_elements(&document);
        let selectors = css::parse_selectors(
            "div#app div.item span.label, body div span, div.item.c0, .app .label",
        );
        let selector_count = selectors.selectors.len().max(1);
        let element_count = elements.len();
        let matches = (element_count * selector_count) as f64;
        suite.add(
            BenchEntry::new(
                "css/selector-match-descendant",
                "css",
                format!("{element_count} elementos × {selector_count} selectores con combinadores"),
                move || {
                    let mut hits = 0usize;
                    for &element in &elements {
                        for selector in &selectors.selectors {
                            if selector.matches(&document, element) {
                                hits += 1;
                            }
                        }
                    }
                    black_box(hits);
                },
            )
            .with_units(matches, "matches"),
        );
    }
}

/// Layout and painting workloads share one realistic fixture: the document is
/// parsed, styled and laid out once, and each benchmark re-runs only its stage.
fn register_layout_and_paint(suite: &mut Suite, scale: f64) {
    let target_bytes = generators::scale(generators::REALISTIC_TARGET_BYTES, scale);
    let realistic = generators::realistic_document(target_bytes);
    let html_source = realistic.html;
    let size_mib = html_source.len() as f64 / (1024.0 * 1024.0);
    let document = html::parse_document(&html_source, false);
    let styles = paint::resolve_embedded_styles(&document);
    let styled_elements = styles.len();
    let viewport = Rect::new(0.0, 0.0, VIEWPORT_WIDTH as f32, VIEWPORT_HEIGHT as f32);

    let Some(layout_root) = layout::layout_document(&document, &styles, viewport) else {
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
    let display_list_raster = paint::build_display_list(&layout_root, viewport);
    let display_list_png = paint::build_display_list(&layout_root, viewport);
    let display_items = display_list_raster.items.len();
    let png_bytes = {
        let pixmap =
            paint::rasterize_display_list(&display_list_png, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
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
                black_box(layout::layout_document(&document, &styles, viewport));
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
            black_box(paint::build_display_list(&layout_root, viewport));
        },
    ));

    // Software rasterization at 800x600 (no PNG encoding).
    suite.add(
        BenchEntry::new(
            "paint/rasterize-800x600",
            "paint",
            format!("{display_items} items → pixmap {VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT}"),
            move || {
                black_box(paint::rasterize_display_list(
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
                let pixmap = paint::rasterize_display_list(
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
                let document = html::parse_document(&html_source, false);
                let styles = paint::resolve_embedded_styles(&document);
                if let Some(root) = layout::layout_document(&document, &styles, viewport) {
                    let display_list = paint::build_display_list(&root, viewport);
                    let pixmap = paint::rasterize_display_list(
                        &display_list,
                        VIEWPORT_WIDTH,
                        VIEWPORT_HEIGHT,
                    );
                    black_box(pixmap.to_png());
                }
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
fn register_feature_e2e(suite: &mut Suite, scale: f64) {
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
        let document = html::parse_document(&markup, false);
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
                    black_box(css::resolve_document_styles(&document, Some(&author_css)));
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
        let document = html::parse_document(&markup, false);
        let styles = paint::resolve_embedded_styles(&document);
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
                    black_box(layout::layout_document(&document, &styles, viewport));
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
                        "{what}: {items} items de display list, PNG {png_bytes} B,                          parse+styles+layout+dl+raster+PNG"
                    ),
                    move || {
                        let document = html::parse_document(&html_source, false);
                        let styles = paint::resolve_embedded_styles(&document);
                        if let Some(root) = layout::layout_document(&document, &styles, viewport) {
                            let display_list = paint::build_display_list(&root, viewport);
                            let pixmap = paint::rasterize_display_list(
                                &display_list,
                                VIEWPORT_WIDTH,
                                VIEWPORT_HEIGHT,
                            );
                            black_box(pixmap.to_png());
                        }
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
    let document = html::parse_document(html_source, false);
    let styles = paint::resolve_embedded_styles(&document);
    let root = layout::layout_document(&document, &styles, viewport)?;
    let display_list = paint::build_display_list(&root, viewport);
    let pixmap = paint::rasterize_display_list(&display_list, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    Some((display_list.items.len(), pixmap.to_png().len()))
}

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
fn register_layer_effects(suite: &mut Suite) {
    let viewport = Rect::new(0.0, 0.0, VIEWPORT_WIDTH as f32, VIEWPORT_HEIGHT as f32);

    // (a) Filter / backdrop-filter raster cost on a synthetic layer stack.
    {
        let mut items: Vec<paint::DisplayItem> = Vec::new();
        items.push(paint::DisplayItem::SolidColor {
            rect: viewport,
            color: css::Color::WHITE,
        });
        for index in 0..24 {
            let cell = Rect::new(
                (index % 6) as f32 * 130.0 + 8.0,
                (index / 6) as f32 * 140.0 + 8.0,
                120.0,
                130.0,
            );
            items.push(paint::DisplayItem::SolidColor {
                rect: cell,
                color: css::Color::new((index * 9) as u8, 90, 180, 255),
            });
            let color_filters = vec![
                css::FilterFunction::Brightness(0.85),
                css::FilterFunction::Grayscale(0.2),
            ];
            let spatial_filters = vec![
                css::FilterFunction::Blur(2.0),
                css::FilterFunction::Brightness(1.05),
            ];
            items.push(paint::DisplayItem::BackdropFilter {
                rect: cell,
                filters: if index % 2 == 0 {
                    color_filters
                } else {
                    spatial_filters
                },
            });
            items.push(paint::DisplayItem::EffectGroup {
                rect: Rect::new(
                    cell.x + 4.0,
                    cell.y + 4.0,
                    cell.width - 8.0,
                    cell.height - 8.0,
                ),
                filters: vec![
                    css::FilterFunction::Brightness(0.9),
                    css::FilterFunction::Contrast(1.1),
                    css::FilterFunction::Saturate(1.2),
                ],
                blend: css::MixBlendMode::Normal,
                items: vec![paint::DisplayItem::SolidColor {
                    rect: cell,
                    color: css::Color::new(240, 120, 40, 200),
                }],
            });
        }
        let display_list = paint::DisplayList { items };
        suite.add(BenchEntry::new(
            "paint/layer-effects-800x600",
            "paint",
            "24 backdrop-filter + 24 filter groups → pixmap".to_string(),
            move || {
                black_box(paint::rasterize_display_list(
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
        let html_node = document.create_element(dom::Namespace::Html, "html", vec![]);
        document.append_child(document.root(), html_node);
        let mut root_style = css::ComputedStyle::initial();
        root_style.display = css::Display::Block;
        root_style.background_color = css::Color::WHITE;
        let mut root =
            layout::LayoutBox::new(layout::BoxType::BlockNode(html_node), Some(root_style));
        root.dimensions.content = viewport;
        let image = Arc::new(
            image::RgbaImage::new(16, 10, vec![120u8; 16 * 10 * 4]).expect("RGBA fixture"),
        );
        let mut images = paint::ImageStore::new();
        let fits = [
            css::ObjectFit::Contain,
            css::ObjectFit::Cover,
            css::ObjectFit::None,
            css::ObjectFit::ScaleDown,
        ];
        for index in 0..128u32 {
            let node = document.create_element(dom::Namespace::Html, "img", vec![]);
            document.append_child(html_node, node);
            images.insert(node, Arc::clone(&image));
            let mut style = css::ComputedStyle::initial();
            style.display = css::Display::Block;
            style.object_fit = fits[index as usize % fits.len()];
            let mut child = layout::LayoutBox::new(layout::BoxType::Replaced(node), Some(style));
            child.dimensions = layout::Dimensions {
                content: Rect::new(
                    (index % 16) as f32 * 50.0,
                    (index / 16) as f32 * 72.0,
                    40.0,
                    30.0,
                ),
                padding: layout::EdgeSizes::ZERO,
                border: layout::EdgeSizes::ZERO,
                margin: layout::EdgeSizes::ZERO,
            };
            root.children.push(child);
        }
        let backgrounds = paint::BackgroundImages::new();
        suite.add(BenchEntry::new(
            "paint/object-fit-800x600",
            "paint",
            "128 replaced images (contain/cover/none/scale-down) → display list + pixmap"
                .to_string(),
            move || {
                let display_list = paint::build_display_list_with_images_and_backgrounds(
                    &root,
                    viewport,
                    &images,
                    &backgrounds,
                );
                black_box(paint::rasterize_display_list(
                    &display_list,
                    VIEWPORT_WIDTH,
                    VIEWPORT_HEIGHT,
                ));
            },
        ));
    }

    // (c) Isolated save-png components (encode / write / both).
    {
        let mut pattern =
            paint::Pixmap::new_with_background(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, css::Color::WHITE);
        for y in (0..VIEWPORT_HEIGHT).step_by(16) {
            for x in (0..VIEWPORT_WIDTH).step_by(16) {
                pattern.fill_rect(
                    Rect::new(x as f32, y as f32, 8.0, 8.0),
                    css::Color::new(30, 60, 120, 255),
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
        let mut pattern = paint::Pixmap::new_with_background(width, height, css::Color::WHITE);
        for y in 0..height {
            let color = if y < 58 {
                css::Color::new(0x20, 0x21, 0x24, 255)
            } else {
                css::Color::new(0xF1, 0xF3, 0xF4, 255)
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
fn register_fixture_effect_rows(suite: &mut Suite) {
    fn paint_tail(html_source: &'static str, viewport: Rect) -> impl Fn() {
        let document = html::parse_document(html_source, false);
        let styles = paint::resolve_embedded_styles(&document);
        let root = layout::layout_document(&document, &styles, viewport);
        let images = paint::ImageStore::new();
        let backgrounds = paint::BackgroundImages::new();
        move || {
            if let Some(root) = &root {
                let display_list = paint::build_display_list_with_images_and_backgrounds(
                    root,
                    viewport,
                    &images,
                    &backgrounds,
                );
                black_box(paint::rasterize_display_list(
                    &display_list,
                    viewport.width as u32,
                    viewport.height as u32,
                ));
            }
        }
    }
    let filter_fixture: &'static str =
        include_str!("../../../tests/fixtures/render/filter_effects.html");
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
        include_str!("../../../tests/fixtures/render/backdrop_filter.html");
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

/// End-to-end pipeline rows for the layer-effect fixtures (R173): parse →
/// styles → layout → display list → raster → PNG, with the committed pattern
/// asset available so the backdrop fixture mirrors the render output. The
/// `*-no-effect` rows strip the CSS declarations to expose the feature cost.
fn register_effect_fixture_pipelines(suite: &mut Suite) {
    static PATTERN_PNG: &[u8] = include_bytes!("../../../tests/fixtures/render/assets/pattern.png");

    fn pipeline_row(html_source: &'static str, width: u32, height: u32) -> impl Fn() {
        let document = html::parse_document(html_source, false);
        let styles = paint::resolve_embedded_styles(&document);
        let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
        let root = layout::layout_document(&document, &styles, viewport);
        let mut backgrounds = paint::BackgroundImages::new();
        if let Ok(pattern) = image::decode(PATTERN_PNG) {
            backgrounds.insert("assets/pattern.png".to_string(), Arc::new(pattern));
        }
        let images = paint::ImageStore::new();
        move || {
            if let Some(root) = &root {
                let display_list = paint::build_display_list_with_images_and_backgrounds(
                    root,
                    viewport,
                    &images,
                    &backgrounds,
                );
                let pixmap = paint::rasterize_display_list(&display_list, width, height);
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
        include_str!("../../../tests/fixtures/render/filter_effects.html");
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
        include_str!("../../../tests/fixtures/render/backdrop_filter.html");
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
fn register_backdrop_stage_rows(suite: &mut Suite) {
    fn stages(
        html_source: &'static str,
        width: u32,
        height: u32,
    ) -> (paint::DisplayList, paint::Pixmap) {
        static PATTERN_PNG: &[u8] =
            include_bytes!("../../../tests/fixtures/render/assets/pattern.png");
        let document = html::parse_document(html_source, false);
        let styles = paint::resolve_embedded_styles(&document);
        let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
        let root = layout::layout_document(&document, &styles, viewport).expect("layout");
        let mut backgrounds = paint::BackgroundImages::new();
        if let Ok(pattern) = image::decode(PATTERN_PNG) {
            backgrounds.insert("assets/pattern.png".to_string(), Arc::new(pattern));
        }
        let images = paint::ImageStore::new();
        let display_list = paint::build_display_list_with_images_and_backgrounds(
            &root,
            viewport,
            &images,
            &backgrounds,
        );
        let pixmap = paint::rasterize_display_list(&display_list, width, height);
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
    let fixture: &'static str = include_str!("../../../tests/fixtures/render/backdrop_filter.html");
    let bare = strip(fixture, "backdrop-filter");
    let (display_list, pixmap) = stages(fixture, 300, 260);
    let (display_list_bare, pixmap_bare) = stages(bare, 300, 260);
    suite.add(BenchEntry::new(
        "paint/backdrop-fixture-raster",
        "paint",
        "backdrop_filter.html display list → pixmap (no PNG)".to_string(),
        move || {
            black_box(paint::rasterize_display_list(&display_list, 300, 260));
        },
    ));
    suite.add(BenchEntry::new(
        "paint/backdrop-fixture-raster-no-effect",
        "paint",
        "same fixture without backdrop-filter → pixmap (no PNG)".to_string(),
        move || {
            black_box(paint::rasterize_display_list(&display_list_bare, 300, 260));
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

/// Image decoding workloads (rounds 22/25/31): PNG/JPEG/GIF 800×600, real WebP
/// VP8L lossless fixtures (transforms/alpha/palette) and WebP VP8 **lossy**
/// keyframes (1024×768 + 64×48 q75/q90) → RGBA8.
///
/// Real pages spend a meaningful part of their time decoding images, which the
/// legacy parse/style/layout/paint rows never exercised. These rows use
/// committed fixtures (PNG/WebP from `crates/image/tests/fixtures`, JPEG/GIF from
/// `crates/bench/fixtures`, see the README there) loaded with `include_bytes!`,
/// so the measurement is deterministic and offline. Each row decodes the whole
/// image with the format defaults and drops the resulting RGBA buffer.
///
/// The `image/decode-webp-lossy*` rows exercise the in-tree VP8 keyframe decoder
/// (boolean entropy decoder, intra prediction, IDCT/WHT, loop filter and
/// YUV420→RGB); `image/decode-webp-lossy` is the 1024×768 heavy row, while the
/// 64×48 q75/q90 rows keep the default iteration count for a stable median of
/// the small-image path. Their determinism is asserted by the unit test below.
///
/// Reference medians (release, 5 iterations / heavy cap 3, idle host) are in
/// `docs/PERFORMANCE.md`; treat a reproducible >15 % regression as a finding.
fn register_image(suite: &mut Suite) {
    // (name, fixture, label, heavy). Heavy rows get fewer timed iterations; the tiny
    // 64×48 lossy rows keep the default iteration count so their median is stable.
    for (name, bytes, label, heavy) in [
        ("image/decode-png-800x600", PNG_800X600, "PNG", true),
        ("image/decode-jpeg-800x600", JPEG_800X600, "JPEG", true),
        ("image/decode-gif", GIF_800X600, "GIF", true),
        (
            "image/decode-webp-lossless",
            WEBP_PHOTO,
            "WebP VP8L (transforms)",
            true,
        ),
        (
            "image/decode-webp-alpha",
            WEBP_ALPHA,
            "WebP VP8L (alpha)",
            true,
        ),
        (
            "image/decode-webp-palette",
            WEBP_PALETTE,
            "WebP VP8L (palette)",
            true,
        ),
        (
            "image/decode-webp-lossy",
            WEBP_LOSSY_1024,
            "WebP VP8 lossy (1024×768)",
            true,
        ),
        (
            "image/decode-webp-lossy-q75",
            WEBP_LOSSY_Q75,
            "WebP VP8 lossy (q75)",
            false,
        ),
        (
            "image/decode-webp-lossy-q90",
            WEBP_LOSSY_Q90,
            "WebP VP8 lossy (q90)",
            false,
        ),
    ] {
        match image::decode(bytes) {
            Ok(probe) => {
                let pixels = (probe.width as f64) * (probe.height as f64) / 1_000_000.0;
                let detail = format!(
                    "{label} {}x{} ({} B) → RGBA8, decode completo",
                    probe.width,
                    probe.height,
                    bytes.len()
                );
                let entry = BenchEntry::new(name, "image", detail, move || {
                    black_box(image::decode(bytes).expect("committed image fixture decodes"));
                })
                .with_units(pixels, "MPix");
                suite.add(if heavy { entry.heavy() } else { entry });
            }
            Err(error) => suite.skip(name, format!("fixture de imagen inválido: {error}")),
        }
    }

    // Scaled-decode companion row: integer box downscale (800×600 → 400×300) while
    // unfiltering, so the scaled path cannot silently regress against the full decode.
    let scaled_label = "PNG 800x600 → box downscale ×2 (400x300), decode escalado";
    suite.add(
        BenchEntry::new(
            "image/decode-png-scaled-x2",
            "image",
            scaled_label.to_string(),
            move || {
                black_box(
                    image::decode_scaled(PNG_800X600, 400)
                        .expect("committed PNG fixture decodes scaled"),
                );
            },
        )
        .with_units(0.12, "MPix")
        .heavy(),
    );
}

/// Registers the **render crate** end-to-end row (`render/html-to-png`).
///
/// `pipeline/html-to-png` predates the `render` crate and composes html/css/layout/paint
/// manually, so it never exercises the real browser pipeline (loader, resource scanning,
/// style collection, scripts/images scans, render options and the final PNG encode). This
/// row feeds a deterministic data URL through `render::render_url` to measure that path.
fn register_render_pipeline(suite: &mut Suite) {
    fn percent_encode_data_url(html: &str) -> String {
        let mut out = String::with_capacity(html.len() + html.len() / 2 + 16);
        out.push_str("data:text/html,");
        for byte in html.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                out.push(byte as char);
            } else {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
        out
    }

    let mut html = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>\
         html,body{margin:0;padding:0;background:#fff}\
         .card{display:inline-block;width:80px;height:24px;margin:2px;background:#ddd}\
         .card:nth-child(3n){background:#9cf}\
         .card:nth-child(4n-1){background:#fc9}\
         .label{color:#123;font:12px sans-serif}\
         </style></head><body>",
    );
    for index in 0..240 {
        html.push_str("<div class=\"card\"><span class=\"label\">item ");
        html.push_str(&index.to_string());
        html.push_str("</span></div>");
    }
    html.push_str("</body></html>");
    let url = percent_encode_data_url(&html);
    let options = render::RenderOptions {
        viewport: render::Viewport::new(800, 600).expect("render viewport"),
        ..render::RenderOptions::default()
    };
    let detail = format!(
        "render crate end-to-end: data URL ({} B HTML) → DOM/styles/layout/paint → PNG",
        html.len()
    );
    suite.add(
        BenchEntry::new("render/html-to-png", "render", detail, move || {
            black_box(render::render_url(&url, &options).expect("render benchmark fixture"));
        })
        .with_units(0.096, "MPix")
        .heavy(),
    );
}

/// Counts element nodes in `document` (top-down pre-order).
fn count_elements(document: &Document) -> usize {
    collect_elements(document).len()
}

/// Collects every element [`NodeId`] in `document`.
fn collect_elements(document: &Document) -> Vec<NodeId> {
    let mut elements = Vec::new();
    let mut stack: Vec<NodeId> = Vec::new();
    let mut child = document.node(document.root()).first_child;
    while let Some(node_id) = child {
        stack.push(node_id);
        child = document.node(node_id).next_sibling;
    }
    while let Some(node_id) = stack.pop() {
        if document.node(node_id).as_element().is_some() {
            elements.push(node_id);
        }
        let mut child = document.node(node_id).first_child;
        while let Some(child_id) = child {
            stack.push(child_id);
            child = document.node(child_id).next_sibling;
        }
    }
    elements
}

/// Optional font benchmarks (`--features fonts`).
///
/// They load the DejaVu fixture that ships with `crates/fonts`; when the
/// fixture is absent the benchmarks are reported as skipped instead of failing
/// the suite (the `fonts` crate is still being migrated during the sprint).
#[cfg(feature = "fonts")]
fn register_fonts(suite: &mut Suite, scale: f64) {
    use fonts::{FontDatabase, FontStyle};

    let root = crate::runner::workspace_root();
    let candidates = [
        root.join("crates/fonts/assets/DejaVuSans.ttf"),
        root.join("crates/fonts/tests/fixtures/DejaVuSans.ttf"),
    ];
    let Some(path) = candidates.iter().find(|path| path.is_file()) else {
        suite.skip(
            "fonts/measure-1k-words",
            "DejaVuSans.ttf no encontrado (fixture de crates/fonts ausente)",
        );
        suite.skip("fonts/shape-1k-words", "DejaVuSans.ttf no encontrado");
        return;
    };
    let Ok(bytes) = std::fs::read(path) else {
        suite.skip("fonts/measure-1k-words", "no se pudo leer DejaVuSans.ttf");
        suite.skip("fonts/shape-1k-words", "no se pudo leer DejaVuSans.ttf");
        return;
    };
    let mut database = FontDatabase::new();
    if let Err(error) = database.load_font_data(bytes) {
        let reason = format!("font fixture inválido: {error}");
        suite.skip("fonts/measure-1k-words", reason.clone());
        suite.skip("fonts/shape-1k-words", reason);
        return;
    }
    let Ok(measure_font) = database.font(&["DejaVu Sans"], 400, FontStyle::Normal) else {
        let reason = "DejaVu Sans no resuelve".to_string();
        suite.skip("fonts/measure-1k-words", reason.clone());
        suite.skip("fonts/shape-1k-words", reason);
        return;
    };
    let Ok(shape_font) = database.font(&["DejaVu Sans"], 400, FontStyle::Normal) else {
        let reason = "DejaVu Sans no resuelve".to_string();
        suite.skip("fonts/measure-1k-words", reason.clone());
        suite.skip("fonts/shape-1k-words", reason);
        return;
    };
    let words = generators::words(generators::scale(generators::FONT_WORDS, scale));
    let word_count = words.len();
    let measure_words = words.clone();
    suite.add(
        BenchEntry::new(
            "fonts/measure-1k-words",
            "fonts",
            format!("{word_count} palabras, DejaVu Sans 16px"),
            move || {
                let mut total = 0.0f32;
                for word in &measure_words {
                    if let Ok(width) = measure_font.measure_text(16.0, word) {
                        total += width;
                    }
                }
                black_box(total);
            },
        )
        .with_units(word_count as f64, "words"),
    );
    suite.add(
        BenchEntry::new(
            "fonts/shape-1k-words",
            "fonts",
            format!("{word_count} palabras, DejaVu Sans 16px"),
            move || {
                let mut glyphs = 0usize;
                for word in &words {
                    if let Ok(run) = shape_font.shape(16.0, word) {
                        glyphs += run.glyphs.len();
                    }
                }
                black_box(glyphs);
            },
        )
        .with_units(word_count as f64, "words"),
    );
}

/// Deterministic RGBA pattern for the PNG-phase benchmarks.
///
/// `flat` mimics a UI page (repeating rows/bands, long runs); the other mode is a
/// photo-like gradient plus deterministic LCG noise, which stresses the deflate match search.
/// `alpha` varies the alpha channel (straight RGBA) for the alpha-encode row.
fn png_pattern(width: u32, height: u32, photo: bool, alpha: bool) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    let mut state = 0x9E37_79B9u32;
    for y in 0..height {
        for x in 0..width {
            let (r, g, b) = if photo {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (state >> 24) as u8;
                let gradient = ((x * 255 / width.max(1)) + (y * 127 / height.max(1))) as u8;
                (
                    gradient.wrapping_add(noise / 4),
                    gradient.rotate_left(3).wrapping_add(noise / 8),
                    gradient.rotate_left(6).wrapping_add(noise / 16),
                )
            } else if y < height / 8 {
                (0x20, 0x21, 0x24)
            } else if y < height / 4 {
                (0xF1, 0xF3, 0xF4)
            } else if (x / 40 + y / 24) % 3 == 0 {
                (0xE8, 0xF0, 0xFE)
            } else {
                (0xFF, 0xFF, 0xFF)
            };
            let a = if alpha { (state >> 8) as u8 } else { 0xFF };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    rgba
}

/// Registers the isolated PNG encode-phase rows (`filter`, `deflate`, full `encode`).
///
/// The filter/deflate rows let the round report separate the two dominant phases; the
/// deflate row captures the already-filtered scanlines so only `zlib_compress` runs inside
/// the measurement window. Rows cover the two render viewports (800×600, 1024×768) with a
/// flat UI pattern and the full encoder also covers a photo-like alpha pattern.
fn register_png_rows(suite: &mut Suite) {
    for (index, width, height) in [(0usize, 800u32, 600u32), (1, 1024, 768)] {
        let (filter_name, deflate_name, encode_name) = if index == 0 {
            (
                "paint/png-filter-800x600",
                "paint/png-deflate-800x600",
                "paint/png-encode-800x600",
            )
        } else {
            (
                "paint/png-filter-1024x768",
                "paint/png-deflate-1024x768",
                "paint/png-encode-1024x768",
            )
        };
        let rgba = Arc::new(png_pattern(width, height, false, false));
        let filtered = Arc::new(paint::filter_scanlines(width, height, &rgba));
        let filter_rgba = Arc::clone(&rgba);
        suite.add(
            BenchEntry::new(
                filter_name,
                "paint",
                format!("filter_scanlines {width}x{height} (flat UI)"),
                move || {
                    black_box(paint::filter_scanlines(width, height, &filter_rgba));
                },
            )
            .heavy(),
        );
        let deflate_filtered = Arc::clone(&filtered);
        suite.add(
            BenchEntry::new(
                deflate_name,
                "paint",
                format!("zlib_compress {width}x{height} (pre-filtered flat UI)"),
                move || {
                    black_box(paint::zlib_compress(&deflate_filtered));
                },
            )
            .heavy(),
        );
        let encode_rgba = Arc::clone(&rgba);
        suite.add(
            BenchEntry::new(
                encode_name,
                "paint",
                format!("encode_rgba {width}x{height} (flat UI)"),
                move || {
                    black_box(paint::encode_rgba(width, height, &encode_rgba));
                },
            )
            .heavy(),
        );
        let alpha = Arc::new(png_pattern(width, height, true, true));
        let alpha_name = if index == 0 {
            "paint/png-encode-800x600-alpha"
        } else {
            "paint/png-encode-1024x768-alpha"
        };
        suite.add(
            BenchEntry::new(
                alpha_name,
                "paint",
                format!("encode_rgba {width}x{height} (photo alpha)"),
                move || {
                    black_box(paint::encode_rgba(width, height, &alpha));
                },
            )
            .heavy(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_image_fixtures_decode_to_rgba() {
        for (label, bytes, width, height) in [
            ("png", PNG_800X600, 800, 600),
            ("jpeg", JPEG_800X600, 800, 600),
            ("gif", GIF_800X600, 800, 600),
            ("webp-photo", WEBP_PHOTO, 64, 48),
            ("webp-alpha", WEBP_ALPHA, 40, 32),
            ("webp-palette", WEBP_PALETTE, 32, 24),
            ("webp-lossy-1024", WEBP_LOSSY_1024, 1024, 768),
            ("webp-lossy-q75", WEBP_LOSSY_Q75, 64, 48),
            ("webp-lossy-q90", WEBP_LOSSY_Q90, 64, 48),
        ] {
            let decoded = image::decode(bytes).expect("committed fixture decodes");
            assert_eq!((decoded.width, decoded.height), (width, height), "{label}");
            assert_eq!(decoded.data.len(), (width * height * 4) as usize, "{label}");
        }
    }

    /// The VP8 lossy bench rows must measure a deterministic decode: two decodes of
    /// the same fixture produce identical RGBA bytes (and opaque alpha).
    #[test]
    fn webp_lossy_bench_fixtures_decode_deterministically() {
        for (label, bytes) in [
            ("webp-lossy-1024", WEBP_LOSSY_1024),
            ("webp-lossy-q75", WEBP_LOSSY_Q75),
            ("webp-lossy-q90", WEBP_LOSSY_Q90),
        ] {
            let first = image::decode(bytes).expect("first decode");
            let second = image::decode(bytes).expect("second decode");
            assert_eq!(
                (first.width, first.height),
                (second.width, second.height),
                "{label}"
            );
            assert_eq!(
                first.data, second.data,
                "{label}: decode must be deterministic"
            );
            assert!(
                first
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| pixel[3] == 255),
                "{label}: VP8 lossy has no alpha, pixels must be opaque"
            );
        }
    }
}
