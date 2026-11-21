//! Public document and fragment parsing entry points.

use crate::scripting::ScriptExecutor;
use crate::tokenizer::{Tokenizer, TokenizerConfig};
use megi_core::{Document, Namespace};

use super::*;

/// Parses a complete HTML document string into a [`megi_core::Document`] (§13.2) using the
/// default anti-DoS [`ParseOptions`].
///
/// The signature is unchanged for callers that only need the tree; use
/// [`parse_document_with_options`] when the limit outcomes must be inspected.
pub fn parse_document(input: &str, scripting_enabled: bool) -> Document {
    parse_document_with_options(input, scripting_enabled, &ParseOptions::default()).document
}

/// Parses a complete HTML document string with explicit anti-DoS [`ParseOptions`].
///
/// The parser is a pure function of `(truncated input, options)`: when a hard limit is
/// exceeded the offending tokens are ignored and counted in
/// [`ParseOutcome::limit_violations`], so hostile input can never make this function
/// panic, abort, recurse without bound, or tokenize more than `max_input_bytes`.
pub fn parse_document_with_options(
    input: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
) -> ParseOutcome {
    parse_document_with_options_and_executor(input, scripting_enabled, options, None)
}

/// Parses a complete HTML document with a host-provided **parser-inserted script executor**
/// (WP-6.4). See [`crate::scripting`] for the documented semantics.
///
/// `script_executor = None` behaves exactly like [`parse_document_with_options`] (the
/// zero-cost path: no script is executed and the tokenizer is untouched).
pub fn parse_document_with_options_and_executor(
    input: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
    script_executor: Option<Box<dyn crate::scripting::ScriptExecutor>>,
) -> ParseOutcome {
    let (input, input_truncated) = truncate_input(input, options.max_input_bytes);
    let mut tok = Tokenizer::from_str_input(input, TokenizerConfig::default());
    let mut tb = TreeBuilder::with_options(scripting_enabled, *options);
    tb.limit_violations.input_truncated = input_truncated;
    tb.script_executor = script_executor;

    drive_tokenizer(&mut tok, &mut tb);

    // Ensure `<html>`, `<head>`, and `<body>` exist if parsing ended before `InBody`.
    // If a parser-inserted script detached the document element during the parse, the
    // synthetic skeleton must NOT be re-created (`scripted_foster01`): the fixture expects
    // the post-script document (e.g. a lone detached table at the document root).
    let root = tb.doc.root();
    let has_html = tb.doc.children(root).any(|id| tb.node_in_html(id, "html"));
    if !tb.html_element_seen
        && !has_html
        && tb.mode != InsertionMode::InFrameset
        && tb.mode != InsertionMode::AfterFrameset
    {
        let html_id = tb.doc.create_element(Namespace::Html, "html", vec![]);
        let head_id = tb.doc.create_element(Namespace::Html, "head", vec![]);
        let body_id = tb.doc.create_element(Namespace::Html, "body", vec![]);
        tb.doc.append_child(root, html_id);
        tb.doc.append_child(html_id, head_id);
        tb.doc.append_child(html_id, body_id);
    } else if let Some(html_id) = tb
        .doc
        .children(root)
        .find(|&id| tb.node_in_html(id, "html"))
    {
        let has_head = tb
            .doc
            .children(html_id)
            .any(|id| tb.node_in_html(id, "head"));
        let has_body_or_frameset = tb
            .doc
            .children(html_id)
            .any(|id| tb.node_in_html(id, "body") || tb.node_in_html(id, "frameset"));
        if !has_head {
            let head_id = tb.doc.create_element(Namespace::Html, "head", vec![]);
            tb.doc.append_child(html_id, head_id);
        }
        if !has_body_or_frameset {
            let body_id = tb.doc.create_element(Namespace::Html, "body", vec![]);
            tb.doc.append_child(html_id, body_id);
        }
    }

    tb.update_selectedcontent_elements();
    let TreeBuilder {
        doc,
        limit_violations,
        script_errors,
        parse_errors,
        ..
    } = tb;
    ParseOutcome {
        document: doc,
        limit_violations,
        script_errors,
        parse_errors,
    }
}

/// Convenience wrapper for [`parse_document_with_options_and_executor`] using the default
/// anti-DoS [`ParseOptions`].
pub fn parse_document_with_script_executor(
    input: &str,
    scripting_enabled: bool,
    executor: Box<dyn ScriptExecutor>,
) -> ParseOutcome {
    parse_document_with_options_and_executor(
        input,
        scripting_enabled,
        &ParseOptions::default(),
        Some(executor),
    )
}

/// Parses an HTML fragment in the context of `(context_namespace, context_local_name)` (§13.4)
/// and returns a [`megi_core::Document`] whose root contains the resulting fragment children,
/// using the default anti-DoS [`ParseOptions`].
pub fn parse_fragment(
    input: &str,
    context_namespace: Namespace,
    context_local_name: &str,
    scripting_enabled: bool,
) -> Document {
    parse_fragment_with_options(
        input,
        context_namespace,
        context_local_name,
        scripting_enabled,
        &ParseOptions::default(),
    )
    .document
}

/// Parses an HTML fragment with explicit anti-DoS [`ParseOptions`]; see
/// [`parse_fragment`] and [`parse_document_with_options`].
pub fn parse_fragment_with_options(
    input: &str,
    context_namespace: Namespace,
    context_local_name: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
) -> ParseOutcome {
    let tb = TreeBuilder::with_options(scripting_enabled, *options);
    let tb = run_fragment_parser(
        tb,
        input,
        context_namespace,
        context_local_name,
        scripting_enabled,
        options,
    );
    let TreeBuilder {
        doc,
        limit_violations,
        script_errors,
        parse_errors,
        ..
    } = tb;
    ParseOutcome {
        document: doc,
        limit_violations,
        script_errors,
        parse_errors,
    }
}

/// Parses an HTML fragment **into an existing [`Document`]**, appending the resulting
/// children to `doc.root()` (WHATWG HTML §13.4 fragment parsing).
///
/// This is the in-place building block used by [`crate::set_inner_html`],
/// [`crate::set_outer_html`] and [`crate::document_write`]: it avoids creating a temporary
/// document and copying nodes between arenas (the input is cloned into a temporary arena so a
/// parser panic cannot erase the caller's DOM, #1454). Existing document content is left untouched;
/// the new fragment children are appended as the last children of the document root (the
/// caller usually moves them under the target element). Returns the
/// [`LimitViolations`] hit while parsing the fragment.
pub fn parse_fragment_into(
    doc: &mut Document,
    input: &str,
    context_namespace: Namespace,
    context_local_name: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
) -> LimitViolations {
    // Parse into a clone: `doc` itself is never moved out, so a panic inside the parser
    // leaves the caller's DOM completely intact (#1454).
    let existing = doc.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let tb = TreeBuilder::with_document(scripting_enabled, *options, existing);
        let tb = run_fragment_parser(
            tb,
            input,
            context_namespace,
            context_local_name,
            scripting_enabled,
            options,
        );
        let TreeBuilder {
            doc: parsed,
            limit_violations,
            ..
        } = tb;
        (parsed, limit_violations)
    }));
    match result {
        Ok((parsed, limit_violations)) => {
            *doc = parsed;
            limit_violations
        }
        Err(_) => {
            // `doc` was never taken, so it still holds the original DOM (#1454).
            LimitViolations::default()
        }
    }
}
