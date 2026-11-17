#![no_main]
//! Fuzz target: WHATWG HTML tokenizer + tree builder + canonical serializer.
//!
//! The parser is the main attack surface of the engine (it consumes third-party markup), so
//! this target drives **both** entry points with their *default* [`html::ParseOptions`]:
//!
//! - `html::parse_document` (full document, scripting disabled);
//! - `html::parse_fragment` with a context element derived from the fuzzer input, covering
//!   HTML contexts as well as SVG/MathML foreign content.
//!
//! The canonical serializer is then exercised on both results, because it is the other
//! recursive tree walk in the crate (regression guard for the F1/F6 audit findings).
//! Outputs are intentionally discarded: the target looks for panics, aborts, stack
//! overflows, UB under ASan and timeouts, not for semantic mismatches.

use dom::Namespace;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The HTML parser takes UTF-8; decode lossily so every fuzzer mutation is accepted and
    // lone-surrogate/replacement-character handling is exercised too.
    let text = String::from_utf8_lossy(data);

    let scripting = data.first().is_some_and(|b| b & 1 != 0);
    let mut options = html::ParseOptions::default();
    if let Some(&b) = data.get(1) {
        options.max_depth = (b as usize % 64).max(4);
        options.max_nodes = ((b as usize % 8) + 1) * 4096;
        options.max_attributes_per_element = (b as usize % 16) + 1;
        options.max_input_bytes = (b as usize % 4 + 1) * 256 * 1024;
    }
    let document = html::parse_document_with_options(&text, scripting, &options);
    let _ = html::serialize_dom_for_test(&document.document);
    let _ = html::serialize_html(&document.document);

    // Fragment parsing with a context derived from the first byte(s): this covers the
    // insertion-mode reset and foreign-content paths that full documents rarely reach.
    const CONTEXTS: &[(&str, Namespace)] = &[
        ("div", Namespace::Html),
        ("body", Namespace::Html),
        ("td", Namespace::Html),
        ("tr", Namespace::Html),
        ("table", Namespace::Html),
        ("select", Namespace::Html),
        ("option", Namespace::Html),
        ("template", Namespace::Html),
        ("title", Namespace::Html),
        ("textarea", Namespace::Html),
        ("script", Namespace::Html),
        ("style", Namespace::Html),
        ("plaintext", Namespace::Html),
        ("noscript", Namespace::Html),
        ("svg", Namespace::Svg),
        ("math", Namespace::MathMl),
        ("annotation-xml", Namespace::MathMl),
        ("foreignObject", Namespace::Svg),
    ];
    let selector = data
        .iter()
        .fold(0usize, |acc, byte| acc.wrapping_mul(31).wrapping_add(*byte as usize));
    let (context_name, context_namespace) = CONTEXTS[selector % CONTEXTS.len()];
    let fragment = html::parse_fragment_with_options(
        &text,
        context_namespace,
        context_name,
        false,
        &options,
    );
    let _ = html::serialize_dom_for_test(&fragment.document);
});
