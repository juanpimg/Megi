//! # WP-6.4 — parser-inserted script execution hook
//!
//! WHATWG HTML §13.2.6 runs a *parser-inserted* `<script>` as soon as its end tag is
//! tokenized: the script element is already in the tree, and `document.write` output is
//! inserted into the parser's input stream at the current position (§13.2.6.4.7 "the rules
//! for the end tag"), i.e. the written HTML is tokenized immediately after the `</script>`
//! token instead of at the end of the document.
//!
//! The parser itself must not depend on a JavaScript engine, so the behaviour is exposed
//! through the [`ScriptExecutor`] hook: hosts (closed-loop harness, renderer) install an
//! executor with [`crate::TreeBuilder::with_script_executor`] or use
//! [`crate::parse_document_with_options_and_executor`]. Without an executor the parser is
//! byte-for-byte the pre-WP-6.4 parser (zero-cost path: no allocations, no extra branches on
//! tokenization other than a single empty check per token).
//!
//! Documented cases handled by the hook:
//! * inline parser-inserted scripts run at their end tag (or at EOF for an unterminated
//!   script), in document order, with the partially-built DOM visible to them;
//! * `document.write`/`writeln` output is re-tokenized at the script's position, including
//!   nested `<script>` elements written that way (`scripted_webkit01`);
//! * scripts with a `src` attribute are *not* executed by the parser: fetching and
//!   execution ordering for external scripts is a host concern (the renderer handles it
//!   post-parse), so the hook is skipped for them;
//! * `defer`/`async` do not apply to inline parser-inserted scripts, so they are irrelevant
//!   to this hook (external deferred scripts are handled by the host);
//! * executor failures are non-fatal: they are recorded in
//!   [`crate::ParseOutcome::script_errors`] and parsing continues.

use dom::Document;

/// Result of executing one parser-inserted `<script>` element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptExecution {
    /// HTML written through `document.write`/`document.writeln`, in call order.
    ///
    /// The parser inserts it into the tokenizer's input stream immediately after the
    /// current `</script>` token, so it is re-tokenized at the script's insertion point.
    pub written_html: String,
    /// Non-fatal execution error (JavaScript exception, budget exhaustion, ...).
    ///
    /// Parsing always continues: the error is surfaced through
    /// [`crate::ParseOutcome::script_errors`].
    pub error: Option<String>,
}

/// Host-provided JavaScript executor used by the tree builder for parser-inserted scripts.
///
/// Implementations run `script_text` against `document` (the parser's live DOM) and return
/// the HTML written with `document.write` so the parser can re-tokenize it in place.
pub trait ScriptExecutor {
    /// Executes one parser-inserted script.
    fn execute(
        &mut self,
        document: &mut Document,
        script_text: &str,
        source_name: &str,
    ) -> ScriptExecution;
}

#[cfg(test)]
mod parser_script_executor_tests {
    use dom::{Attribute, Document, Namespace, NodeId};

    use super::{ScriptExecution, ScriptExecutor};
    use crate::{ParseOptions, ParseOutcome, serialize_dom_for_test};

    /// Test executor: records every script and returns canned `document.write` output,
    /// optionally mutating the parser's document (like real page scripts do).
    #[derive(Default)]
    struct RecordingExecutor {
        seen: Vec<String>,
        written: Vec<String>,
        mutate_id: Option<(&'static str, &'static str)>,
        detach_document_element: bool,
        error: Option<String>,
    }

    impl ScriptExecutor for RecordingExecutor {
        fn execute(
            &mut self,
            document: &mut Document,
            script_text: &str,
            _source_name: &str,
        ) -> ScriptExecution {
            self.seen.push(script_text.to_owned());
            if self.detach_document_element {
                detach_html_and_move_first_table_to_root(document);
                return ScriptExecution::default();
            }
            if let Some((old_id, new_id)) = self.mutate_id
                && let Some(node) = document.get_element_by_id(old_id)
                && let Some(element) = document.node_mut(node).as_element_mut()
            {
                element.set_attribute(Attribute::new("id", new_id), true);
            }
            let index = self.seen.len().saturating_sub(1);
            ScriptExecution {
                written_html: self.written.get(index).cloned().unwrap_or_default(),
                error: self.error.clone(),
            }
        }
    }

    /// Simulates `document.documentElement.remove(); document.appendChild(table)`.
    fn detach_html_and_move_first_table_to_root(document: &mut Document) {
        let root = document.root();
        let html = document
            .children(root)
            .find(|&id| child_is_html(document, id));
        let table = document
            .get_elements_by_tag_name("table")
            .into_iter()
            .next();
        if let Some(html) = html {
            document.remove_child(root, html).expect("html is attached");
        }
        if let Some(table) = table {
            assert!(
                document.append_child(root, table),
                "append the detached table to the document"
            );
        }
    }

    fn child_is_html(document: &Document, id: NodeId) -> bool {
        document.node(id).as_element().is_some_and(|element| {
            element.namespace == Namespace::Html && element.local_name == "html"
        })
    }

    fn parse_with(input: &str, executor: RecordingExecutor) -> ParseOutcome {
        crate::tree_builder::parse_document_with_options_and_executor(
            input,
            true,
            &ParseOptions::default(),
            Some(Box::new(executor)),
        )
    }

    #[test]
    fn test_parser_inserted_script_runs_and_written_html_is_retokenized() {
        let mut executor = RecordingExecutor {
            written: vec!["<b>written</b>".to_owned()],
            ..RecordingExecutor::default()
        };
        let outcome = parse_with(
            "<body>1<script>document.write('<b>written</b>')</script>3</body>",
            std::mem::take(&mut executor),
        );
        assert_eq!(executor.seen, Vec::<String>::new());
        assert_eq!(
            serialize_dom_for_test(&outcome.document),
            "| <html>\n|   <head>\n|   <body>\n|     \"1\"\n|     <script>\n|       \"document.write('<b>written</b>')\"\n|     <b>\n|       \"written\"\n|     \"3\""
        );
        assert!(outcome.script_errors.is_empty());
    }

    #[test]
    fn test_nested_scripts_written_with_document_write_execute_in_order() {
        let executor = RecordingExecutor {
            written: vec![
                "<script>inner</script>".to_owned(),
                "<i>nested</i>".to_owned(),
            ],
            ..RecordingExecutor::default()
        };
        let outcome = parse_with("<body><script>outer</script></body>", executor);
        let dump = serialize_dom_for_test(&outcome.document);
        assert!(dump.contains("<script>"), "{dump}");
        assert!(dump.contains("\"inner\""), "{dump}");
        assert!(dump.contains("<i>"), "{dump}");
        assert!(dump.contains("\"nested\""), "{dump}");
        assert!(outcome.script_errors.is_empty());
    }

    #[test]
    fn test_script_mutates_dom_while_parsing() {
        let executor = RecordingExecutor {
            mutate_id: Some(("before", "after")),
            ..RecordingExecutor::default()
        };
        let outcome = parse_with(
            "<body><p id=\"before\"><script>mutate</script>tail</p></body>",
            executor,
        );
        let dump = serialize_dom_for_test(&outcome.document);
        assert!(dump.contains("id=\"after\""), "{dump}");
        assert!(!dump.contains("id=\"before\""), "{dump}");
        assert!(dump.contains("\"tail\""), "{dump}");
    }

    #[test]
    fn test_no_executor_means_no_script_execution() {
        let outcome = crate::tree_builder::parse_document_with_options_and_executor(
            "<body><script>never()</script></body>",
            true,
            &ParseOptions::default(),
            None,
        );
        let dump = serialize_dom_for_test(&outcome.document);
        assert!(dump.contains("\"never()\""), "{dump}");
        assert!(outcome.script_errors.is_empty());
    }

    #[test]
    fn test_executor_errors_are_non_fatal() {
        let executor = RecordingExecutor {
            error: Some("boom".to_owned()),
            ..RecordingExecutor::default()
        };
        let outcome = parse_with("<body><script>x</script><p>after</p></body>", executor);
        assert_eq!(outcome.script_errors, vec!["boom".to_owned()]);
        let dump = serialize_dom_for_test(&outcome.document);
        assert!(dump.contains("\"after\""), "{dump}");
    }

    #[test]
    fn test_external_script_is_not_executed() {
        let executor = RecordingExecutor::default();
        let outcome = parse_with(
            "<body><script src=\"page.js\">ignored()</script><p>after</p></body>",
            executor,
        );
        assert!(outcome.script_errors.is_empty());
        let dump = serialize_dom_for_test(&outcome.document);
        assert!(dump.contains("\"ignored()\""), "{dump}");
        assert!(dump.contains("\"after\""), "{dump}");
    }

    #[test]
    fn test_detached_document_element_is_not_resurrected() {
        let executor = RecordingExecutor {
            detach_document_element: true,
            ..RecordingExecutor::default()
        };
        let outcome = parse_with(
            "<table><tbody><tr><script>detach</script></tbody></table><b>lost</b>",
            executor,
        );
        let dump = serialize_dom_for_test(&outcome.document);
        assert_eq!(
            dump, "| <table>\n|   <tbody>\n|     <tr>\n|       <script>\n|         \"detach\"",
            "detached documentElement must not be re-synthesized and orphan tokens are dropped"
        );
    }
}
