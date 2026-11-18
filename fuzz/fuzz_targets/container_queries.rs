#![no_main]
//! Fuzz target: CSS **container size queries** and cascade layers.
//!
//! Drives the CSS parser with `@container`/`@layer` rules and the parsed query evaluator:
//!
//! - `megi_css::parse_stylesheet`, `megi_css::parse_list_of_declarations`, `megi_css::tokenize`;
//! - `megi_css::container::parse_query` on the input and on its lines (multi-line seeds);
//! - `megi_css::container::evaluate` against **input-derived** container sizes, including the
//!   degenerate values a real page can produce: `0`, negative, `NaN`, `+∞` and values far
//!   above any viewport. Earlier revisions used five fixed pairs (#710), so the
//!   comparison/threshold arithmetic never saw those.
//!
//! Style/container scroll-state queries are not implemented in `megi_css::container`, so this
//! target exercises the size-query surface only.
//!
//! Everything is bounded by the CSS parser's own depth/count caps; no network, filesystem or
//! DOM is required. Panics, aborts, ASan/UBSan findings and timeouts are the signals.

use libfuzzer_sys::fuzz_target;

/// Container sizes worth probing on every query: the unit range plus the degenerate cases
/// (`NaN`, `±∞`, negative, zero, huge) that broke threshold comparisons before.
fn candidate_sizes(data: &[u8]) -> Vec<megi_css::container::ContainerSize> {
    let mut sizes = Vec::with_capacity(16);
    let at = |index: usize| -> u32 { u32::from(data.get(index).copied().unwrap_or(0)) };
    // Input-derived finite values (0..=4096 in quarter steps) for both axes.
    sizes.push(megi_css::container::ContainerSize {
        inline_size: Some(at(0) as f32 * 16.0),
        block_size: Some(at(1) as f32 * 16.0),
    });
    // Degenerate values no layout would produce but every comparison must survive.
    for value in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -1.0,
        0.0,
        f32::MIN_POSITIVE,
        f32::MAX,
    ] {
        sizes.push(megi_css::container::ContainerSize {
            inline_size: Some(value),
            block_size: Some(value),
        });
    }
    // Mixed presence: inline only, block only, neither.
    sizes.push(megi_css::container::ContainerSize {
        inline_size: Some(at(2) as f32),
        block_size: None,
    });
    sizes.push(megi_css::container::ContainerSize {
        inline_size: None,
        block_size: Some(at(3) as f32),
    });
    sizes.push(megi_css::container::ContainerSize {
        inline_size: None,
        block_size: None,
    });
    sizes
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    let stylesheet = megi_css::parse_stylesheet(&text);
    let _ = stylesheet.rules.len();
    let declarations = megi_css::parse_list_of_declarations(&text);
    let _ = declarations.len();
    let tokens = megi_css::tokenize(&text);
    let _ = tokens.len();

    let sizes = candidate_sizes(data);
    let style = megi_css::ComputedStyle::default();

    let mut queries = Vec::new();
    if let Some(query) = megi_css::container::parse_query(&text) {
        queries.push(query);
    }
    // Line-shaped preludes: CSS seeds are multi-line, so this reaches many more grammars.
    for line in text.lines().take(16) {
        if line.len() > 256 {
            continue;
        }
        if let Some(query) = megi_css::container::parse_query(line) {
            queries.push(query);
        }
    }

    for query in &queries {
        for size in &sizes {
            let _ = megi_css::container::evaluate(query, size, &style);
        }
    }
});
