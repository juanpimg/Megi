//! HTML tokenization and CSS parsing workloads.

use super::*;

/// HTML tokenizer + tree construction workloads.
pub(super) fn register_html(suite: &mut Suite, scale: f64) {
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
                black_box(megi_html::parse_document(&input, false));
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
                black_box(megi_html::parse_document_with_options(
                    &input,
                    false,
                    &GENEROUS_PARSE,
                ));
            },
        )
        .with_units(mib, "MiB");
        // 32k/64k are the expensive probes: keep their iteration count low.
        // Classify on the scaled depth so --scale changes the sample regime (#1208).
        suite.add(if depth >= 32_000 {
            entry.heavy()
        } else {
            entry
        });
    }

    // (b') Same input with production anti-DoS limits, for the guard's cost.
    {
        let depth = generators::scale(generators::NESTED_DEPTHS[2], scale);
        let input = generators::nested_divs(depth);
        let probe = megi_html::parse_document_with_options(
            &input,
            false,
            &megi_html::ParseOptions::DEFAULT,
        );
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
                black_box(megi_html::parse_document(&input, false));
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
                    black_box(megi_html::parse_document(&input, false));
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
                    black_box(megi_html::parse_document(&input, false));
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
                    black_box(megi_html::parse_document_with_options(
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
                    black_box(megi_html::parse_document(&input, false));
                },
            )
            .with_units(blocks as f64, "blocks"),
        );
    }
}

/// CSS parsing, cascade/style resolution and selector matching.
pub(super) fn register_css(suite: &mut Suite, scale: f64) {
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
                    black_box(megi_css::parse_stylesheet(&sheet));
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
        let document = megi_html::parse_document(&markup, false);
        let elements = count_elements(&document);
        let author_css = generators::element_author_css();
        let rule_count = author_css.matches('{').count();
        suite.add(
            BenchEntry::new(
                name,
                "css",
                format!("{elements} elementos, {rule_count} reglas de autor"),
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

    // Selector matching: simple compounds and descendant combinators.
    let markup = generators::element_document(generators::scale(generators::SELECTOR_NODES, scale));
    {
        let document = megi_html::parse_document(&markup, false);
        let elements = collect_elements(&document);
        let selectors = megi_css::parse_selectors("div.item, span.label, .c0, #e42");
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
        let document = megi_html::parse_document(&markup, false);
        let elements = collect_elements(&document);
        let selectors = megi_css::parse_selectors(
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
