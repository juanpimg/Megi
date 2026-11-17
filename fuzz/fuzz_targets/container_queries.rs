#![no_main]
//! Fuzz target: CSS **container queries** (size + style) and cascade layers.
//!
//! Drives the CSS parser with `@container`/`@layer` rules and the parsed query evaluator:
//!
//! - `css::parse_stylesheet`, `css::parse_list_of_declarations`, `css::tokenize`;
//! - `css::container::parse_query` on the input and on its lines (multi-line seeds);
//! - `css::container::evaluate` against **input-derived** container sizes, including the
//!   degenerate values a real page can produce: `0`, negative, `NaN`, `+∞` and values far
//!   above any viewport. Earlier revisions used five fixed pairs (#710), so the
//!   comparison/threshold arithmetic never saw those;
//! - style queries with a `ComputedStyle` carrying input-derived custom properties
//!   (`--name: value`), because `@container style(--x)` resolves against them.
//!
//! Everything is bounded by the CSS parser's own depth/count caps; no network, filesystem or
//! DOM is required. Panics, aborts, ASan/UBSan findings and timeouts are the signals.

use std::collections::HashMap;
use std::sync::Arc;

use libfuzzer_sys::fuzz_target;

/// Container sizes worth probing on every query: the unit range plus the degenerate cases
/// (`NaN`, `±∞`, negative, zero, huge) that broke threshold comparisons before.
fn candidate_sizes(data: &[u8]) -> Vec<css::container::ContainerSize> {
    let mut sizes = Vec::with_capacity(16);
    let at = |index: usize| -> u32 { u32::from(data.get(index).copied().unwrap_or(0)) };
    // Input-derived finite values (0..=4096 in quarter steps) for both axes.
    sizes.push(css::container::ContainerSize {
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
        sizes.push(css::container::ContainerSize {
            inline_size: Some(value),
            block_size: Some(value),
        });
    }
    // Mixed presence: inline only, block only, neither.
    sizes.push(css::container::ContainerSize {
        inline_size: Some(at(2) as f32),
        block_size: None,
    });
    sizes.push(css::container::ContainerSize {
        inline_size: None,
        block_size: Some(at(3) as f32),
    });
    sizes.push(css::container::ContainerSize {
        inline_size: None,
        block_size: None,
    });
    sizes
}

/// A style with two input-derived custom properties, so `@container style(--x: …)` and
/// `style(--x)` have something to resolve against.
fn container_style(data: &[u8]) -> css::ComputedStyle {
    let mut style = css::ComputedStyle::default();
    let at = |index: usize| -> u8 { data.get(index).copied().unwrap_or(0) };
    let mut properties = HashMap::new();
    properties.insert("--fuzz-a".to_string(), at(4).to_string());
    properties.insert(
        "--fuzz-b".to_string(),
        match at(5) % 4 {
            0 => "0".to_string(),
            1 => "1px".to_string(),
            2 => "auto".to_string(),
            _ => String::new(),
        },
    );
    style.custom_properties = Some(Arc::new(properties));
    style
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    let stylesheet = css::parse_stylesheet(&text);
    let _ = stylesheet.rules.len();
    let declarations = css::parse_list_of_declarations(&text);
    let _ = declarations.len();
    let tokens = css::tokenize(&text);
    let _ = tokens.len();

    let sizes = candidate_sizes(data);
    let style = container_style(data);

    let mut queries = Vec::new();
    if let Some(query) = css::container::parse_query(&text) {
        queries.push(query);
    }
    // Line-shaped preludes: CSS seeds are multi-line, so this reaches many more grammars.
    for line in text.lines().take(16) {
        if line.len() > 256 {
            continue;
        }
        if let Some(query) = css::container::parse_query(line) {
            queries.push(query);
        }
    }

    for query in &queries {
        for size in &sizes {
            let _ = css::container::evaluate(query, size, &style);
        }
    }
});
