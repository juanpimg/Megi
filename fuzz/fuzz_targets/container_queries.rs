#![no_main]
//! Fuzz target: CSS **container queries** (R228) + cascade layers parsing.
//!
//! Drives the CSS parser with `@container`/`@layer` rules and the parsed query evaluator:
//!
//! - `css::parse_stylesheet` (at-rules, declarations, nested blocks);
//! - `css::parse_list_of_declarations` (inline style blocks);
//! - `css::tokenize` (raw tokenizer);
//! - `css::container::parse_query` on the input (prelude-shaped seeds and arbitrary
//!   bytes) and `css::container::evaluate` against several fixed container sizes so the
//!   condition/resolution code runs without a layout tree.
//!
//! Everything is bounded by the CSS parser's own depth/count caps; no network, no
//! filesystem and no DOM is required.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    let stylesheet = css::parse_stylesheet(&text);
    let _ = stylesheet.rules.len();
    let declarations = css::parse_list_of_declarations(&text);
    let _ = declarations.len();
    let tokens = css::tokenize(&text);
    let _ = tokens.len();

    if let Some(query) = css::container::parse_query(&text) {
        let style = css::ComputedStyle::default();
        for (inline, block) in [
            (Some(320.0f32), Some(200.0f32)),
            (Some(1024.0), None),
            (None, Some(768.0)),
            (None, None),
            (Some(0.0), Some(0.0)),
        ] {
            let size = css::container::ContainerSize {
                inline_size: inline,
                block_size: block,
            };
            let _ = css::container::evaluate(&query, &size, &style);
        }
        // Also exercise a few query strings derived from the input (line splits) so
        // multi-line CSS seeds feed more preludes.
        for line in text.lines().take(16) {
            if line.len() > 256 {
                continue;
            }
            if let Some(query) = css::container::parse_query(line) {
                let size = css::container::ContainerSize {
                    inline_size: Some(480.0),
                    block_size: Some(320.0),
                };
                let _ = css::container::evaluate(&query, &size, &style);
            }
        }
    }
});
