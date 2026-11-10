//! Benchmark registry: every measured workload lives here.
//!
//! Fixtures are generated and parsed in [`register`], *before* the timed loop;
//! only the operation under test runs inside the measurement window. The
//! division mirrors `docs/PERFORMANCE.md` and the row names are a stable API
//! for regression tracking.

use std::hint::black_box;

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

/// Registers every benchmark into `suite`, applying `opts.scale` to the sizes.
pub fn register(suite: &mut Suite, opts: &Options) {
    register_html(suite, opts.scale);
    register_css(suite, opts.scale);
    register_layout_and_paint(suite, opts.scale);
    register_feature_e2e(suite, opts.scale);
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
/// - `layout/sticky-text-1mb`: **pending** (registered as SKIP): `position:
///   sticky` parses but layout treats it like `relative` (no scrolling yet) and
///   `text-transform` parses but layout/paint do not apply it. Enable the row
///   once both land.
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

    // Sticky + text-transform are parsed but not applied yet; keep the gap visible.
    suite.skip(
        "layout/sticky-text-1mb",
        "pending: `position: sticky` parses but layout treats it like `relative`          (no scrolling) and `text-transform` parses but layout/paint do not apply it",
    );
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
