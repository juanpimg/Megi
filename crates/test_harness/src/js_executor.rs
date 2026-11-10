//! # WP-6.4 script executor for the closed-loop tree-construction harness
//!
//! The html parser exposes a [`ScriptExecutor`] hook for **parser-inserted** scripts.
//! This module implements it with `crates/js`: the script runs against the parser's
//! document through [`JsRuntime::eval_on_document`], and a small JS prelude/epilogue
//! adapts the DOM for parser-time execution without any changes to the JS crate:
//!
//! * `document.write`/`document.writeln` are redirected into a buffer. The written HTML
//!   is returned to the parser, which re-tokenizes it at the script's insertion point
//!   (including nested parser-inserted scripts, as in `scripted_webkit01`).
//! * `documentElement.remove()` and `document.appendChild(node)` are polyfilled on the
//!   live document (the crate's DOM bridge does not expose them yet); the document node
//!   handle is captured lazily via `documentElement.parentNode`. This is what
//!   `scripted_foster01` needs.
//! * The epilogue stores the captured writes (and any thrown error) in marker elements
//!   that the Rust side extracts and removes from the document with
//!   [`dom::Document::remove_subtree`], so the serialized tree matches the fixtures.
//!
//! Each script runs in a fresh realm (`eval_on_document` semantics): html5lib's
//! `scripted_*` cases are self-contained scripts, so no cross-script globals are needed.

use dom::Document;
use html::{ScriptExecution, ScriptExecutor};
use js::{Budget, JsRuntime};

/// Marker element id used to hand the captured `document.write` output back to Rust.
const WRITES_MARKER_ID: &str = "__megi_parser_writes__";
/// Marker element id used to hand a non-fatal script error back to Rust.
const ERROR_MARKER_ID: &str = "__megi_parser_error__";

/// JS prelude: parser-time DOM adapters (see the module docs).
const PRELUDE: &str = r#"
(function () {
  globalThis.__megi_writes = [];
  var write = function (value) { globalThis.__megi_writes.push(String(value)); };
  try { document.write = write; } catch (e) { globalThis.__megi_write_error = String(e); }
  try { document.writeln = function (value) { write(value); write("\n"); }; } catch (e) {}
  var element = document.documentElement;
  var document_node = element ? element.parentNode : null;
  if (element && typeof element.remove !== "function") {
    element.remove = function () {
      var parent = this.parentNode;
      if (parent) { parent.removeChild(this); }
    };
    try {
      Object.defineProperty(document, "documentElement", {
        configurable: true,
        get: function () { return element; }
      });
    } catch (e) { globalThis.__megi_document_element_error = String(e); }
  }
  try {
    document.appendChild = function (node) { return document_node.appendChild(node); };
  } catch (e) { globalThis.__megi_append_child_error = String(e); }
})();
"#;

/// JS epilogue: publishes writes/errors in marker nodes the Rust side can read.
const EPILOGUE: &str = r#"
(function () {
  var element = document.documentElement;
  if (!element) { return; }
  if (globalThis.__megi_writes && globalThis.__megi_writes.length > 0) {
    var marker = document.createElement("div");
    marker.id = "__megi_parser_writes__";
    marker.textContent = globalThis.__megi_writes.join("");
    element.appendChild(marker);
  }
  if (globalThis.__megi_error) {
    var error_marker = document.createElement("div");
    error_marker.id = "__megi_parser_error__";
    error_marker.textContent = String(globalThis.__megi_error);
    element.appendChild(error_marker);
  }
})();
"#;

/// Executes parser-inserted scripts for the conformance harness using Boa.
#[derive(Debug, Default)]
pub struct JsScriptExecutor {
    budget: Budget,
    executed: usize,
}

impl JsScriptExecutor {
    /// Creates a fresh executor with the conservative [`Budget::SMALL`] anti-DoS budget.
    pub fn new() -> Self {
        Self {
            budget: Budget::SMALL,
            executed: 0,
        }
    }

    /// Number of scripts executed so far (diagnostics only).
    pub const fn executed(&self) -> usize {
        self.executed
    }

    /// Extracts and removes the text of a marker element with the given id.
    fn take_marker(document: &mut Document, id: &str) -> Option<String> {
        let node = document.get_element_by_id(id)?;
        let text = document.text_content(node);
        // `remove_node` re-parents children; `remove_subtree` frees the marker and its
        // text child without leaving stray text behind.
        document.remove_subtree(node);
        Some(text)
    }

    fn run(
        document: &mut Document,
        script: &str,
        source: &str,
        budget: &Budget,
    ) -> ScriptExecution {
        let wrapped = format!(
            "{PRELUDE}\ntry {{\n{script}\n}} catch (e) {{ globalThis.__megi_error = String(e); }}\n{EPILOGUE}"
        );
        let outcome = JsRuntime::eval_on_document(document, &wrapped, source, budget);
        let error = match &outcome {
            Ok(()) => Self::take_marker(document, ERROR_MARKER_ID),
            Err(other) => Some(other.to_string()),
        };
        let written_html = Self::take_marker(document, WRITES_MARKER_ID).unwrap_or_default();
        ScriptExecution {
            written_html,
            error,
        }
    }
}

impl ScriptExecutor for JsScriptExecutor {
    fn execute(
        &mut self,
        document: &mut Document,
        script_text: &str,
        source_name: &str,
    ) -> ScriptExecution {
        self.executed += 1;
        Self::run(document, script_text, source_name, &self.budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_write_is_captured_and_markers_removed() {
        let mut doc = html::parse_document("<html><body></body></html>", false);
        let mut executor = JsScriptExecutor::new();
        let outcome = executor.execute(&mut doc, "document.write('<p>hola</p>')", "<script>");
        assert_eq!(outcome.written_html, "<p>hola</p>");
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        assert!(doc.get_element_by_id(WRITES_MARKER_ID).is_none());
        assert_eq!(
            html::serialize_dom_for_test(&doc),
            "| <html>\n|   <head>\n|   <body>"
        );
    }

    #[test]
    fn test_dom_mutation_and_remove_polyfill() {
        let mut doc = html::parse_document(
            "<html><body>hello<table><tbody><tr></tr></tbody></table></body></html>",
            false,
        );
        let mut executor = JsScriptExecutor::new();
        let outcome = executor.execute(
            &mut doc,
            "var t = document.querySelector('table'); document.documentElement.remove(); document.appendChild(t);",
            "<script>",
        );
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        assert_eq!(
            html::serialize_dom_for_test(&doc),
            "| <table>\n|   <tbody>\n|     <tr>"
        );
    }

    #[test]
    fn test_script_errors_are_non_fatal() {
        let mut doc = html::parse_document("<html><body></body></html>", false);
        let mut executor = JsScriptExecutor::new();
        let outcome = executor.execute(&mut doc, "throw new Error('boom')", "<script>");
        assert!(outcome.written_html.is_empty());
        assert!(outcome.error.is_some());
        assert!(doc.get_element_by_id(ERROR_MARKER_ID).is_none());
    }
}
