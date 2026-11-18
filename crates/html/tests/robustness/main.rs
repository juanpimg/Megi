//! Robustness regression tests for the core-engine audit (`F1`, `F3`, `F10`-style API
//! hardening) and the anti-DoS parsing limits.
//!
//! The historical failure mode being guarded here is a **process abort** (SIGABRT from a
//! Rust stack overflow), not a recoverable panic, so the deep-input cases run inside a
//! worker thread with a deliberately small stack: if the implementation regresses to
//! recursion, the `join()` below fails the test instead of silently passing on the main
//! thread's 8 MiB stack.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use megi_html::{
    ParseErrorCode, ParseOptions, Token, Tokenizer, TokenizerConfig, parse_document,
    parse_document_with_options, parse_fragment_with_options,
};

mod deep_tree;
mod parse_limits;
mod scripting;
mod tokenizer;

/// Budget multiplier from `MEGI_PERF_BUDGET_SCALE` (default `1.0`), shared with
/// `crates/html/tests/perf.rs` so CI-loaded runs can scale wall-clock budgets.
fn budget_scale() -> f64 {
    parse_budget_scale(std::env::var("MEGI_PERF_BUDGET_SCALE").ok().as_deref())
}

/// Parses the scale: finite and positive, capped at 1000; anything else means `1.0`.
fn parse_budget_scale(raw: Option<&str>) -> f64 {
    raw.and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .map_or(1.0, |scale| scale.min(1000.0))
}

fn scaled_budget(base: Duration) -> Duration {
    base.mul_f64(budget_scale())
}

const SMALL_STACK: usize = 512 * 1024;

fn in_thread_with_stack<T: Send + 'static>(
    stack_size: usize,
    task: impl FnOnce() -> T + Send + 'static,
) -> T {
    std::thread::Builder::new()
        .stack_size(stack_size)
        .spawn(task)
        .expect("worker thread must spawn")
        .join()
        .expect("worker thread must not panic/abort")
}

fn find_element_by_name(input: &str, local_name: &str) -> bool {
    let doc = parse_document(input, false);
    let mut stack = vec![doc.root()];
    while let Some(current) = stack.pop() {
        for child in doc.children(current) {
            if let Some(el) = doc.node(child).as_element()
                && el.local_name == local_name
            {
                return true;
            }
            stack.push(child);
        }
    }
    false
}

fn collect_text_bytes(
    doc: &megi_core::Document,
    node: megi_core::NodeId,
    total: &mut usize,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    if let megi_core::NodeKind::Text(text) = &doc.node(node).kind {
        *total += text.len();
    }
    let mut child = doc.node(node).first_child;
    while let Some(id) = child {
        collect_text_bytes(doc, id, total, depth + 1);
        child = doc.node(id).next_sibling;
    }
}

/// Text of the first `<p>` in a parsed document (`text_content` on the root does not descend).
fn paragraph_text(document: &megi_core::Document) -> String {
    document
        .get_elements_by_tag_name("p")
        .first()
        .map(|node| document.text_content(*node))
        .unwrap_or_default()
}
