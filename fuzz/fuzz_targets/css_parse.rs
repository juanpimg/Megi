#![no_main]
//! Fuzz target: CSS Syntax Level 3 tokenizer + stylesheet parser + declaration lists.
//!
//! Drives every public CSS entry point that consumes untrusted text:
//!
//! - `css::parse_stylesheet` (full rule list, at-rules, nested blocks);
//! - `css::parse_list_of_declarations` (declaration blocks, e.g. `style="..."`);
//! - `css::tokenize` (raw tokenizer, including escapes and URL tokens).
//!
//! The CSS parser is recursive-descent over component values and was the cause of the
//! audit finding F5 (stack overflow on deeply nested blocks); the depth caps that fixed it
//! are exercised whenever libFuzzer generates nested `{`, `(`, `[` and `@media` tokens.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    let stylesheet = css::parse_stylesheet(&text);
    let _ = stylesheet.rules.len();

    let declarations = css::parse_list_of_declarations(&text);
    let _ = declarations.len();

    let tokens = css::tokenize(&text);
    let _ = tokens.len();
});
