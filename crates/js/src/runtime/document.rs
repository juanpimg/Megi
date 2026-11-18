use super::*;

// DOM document bindings (split verbatim from the former monolithic document.rs).
mod parse;
mod query;

pub(crate) use parse::*;
pub(crate) use query::*;

#[cfg(test)]
mod document_write_budget_tests {
    use super::*;

    #[test]
    pub(crate) fn document_write_stops_at_the_cumulative_budget() {
        let mut runtime = JsRuntime::new().expect("runtime");
        host_cell(&mut runtime.context)
            .expect("host installed")
            .max_document_write_bytes = 4 * 1024;
        runtime
            .eval(
                "var s = 'A'.repeat(1024);
                 for (var i = 0; i < 6; i++) { document.write(s); }",
                "write_budget.js",
            )
            .expect("document.write loop");
        let console = runtime.take_console();
        let document = runtime.take_document();
        let body = document.body().expect("body");
        assert_eq!(
            document.text_content(body).len(),
            4 * 1024,
            "only the writes that fit the budget are parsed into the document"
        );
        assert!(
            console.iter().any(|entry| {
                entry.level == "warn" && entry.message.contains("document.write ignored")
            }),
            "the exceeded budget must be reported: {console:?}"
        );
    }

    #[test]
    pub(crate) fn document_write_reports_parser_limit_violations() {
        let mut runtime = JsRuntime::new().expect("runtime");
        host_cell(&mut runtime.context)
            .expect("host installed")
            .max_document_write_nodes = 64;
        runtime
            .eval(
                "document.write('<i></i>'.repeat(100));",
                "nodes_violation.js",
            )
            .expect("write");
        let console = runtime.take_console();
        assert!(
            console.iter().any(|entry| {
                entry.level == "warn"
                    && entry.message.contains("parse limit violation")
                    && entry.message.contains("node(s)")
            }),
            "parser truncation must be visible to script: {console:?}"
        );
    }
}

#[cfg(test)]
mod document_write_node_budget_tests {
    use super::*;

    #[test]
    pub(crate) fn document_write_node_budget_is_enforced_and_warned_once() {
        let mut runtime = JsRuntime::new().expect("runtime");
        // The parser must use the host's node cap, not the 1M default: 100 elements with a
        // 64-node budget leave ~62 nodes.
        host_cell(&mut runtime.context)
            .expect("host installed")
            .max_document_write_nodes = 64;
        runtime
            .eval("document.write('<i></i>'.repeat(100));", "nodes.js")
            .expect("first write");
        let after_first = runtime.document().len();
        assert!(
            after_first <= 64,
            "the write parser stops at the host node cap: {after_first} nodes"
        );
        // Host-side rejection: a cap below the current size rejects every write; the
        // warning must appear exactly once.
        host_cell(&mut runtime.context)
            .expect("host installed")
            .max_document_write_nodes = 1;
        runtime
            .eval(
                "for (var j = 0; j < 5; j++) { document.write('<b></b>'); }",
                "nodes.js",
            )
            .expect("rejected writes");
        let console = runtime.take_console();
        let warnings = console
            .iter()
            .filter(|entry| entry.message.contains("document.write ignored"))
            .count();
        assert_eq!(warnings, 1, "the rejection is reported once: {console:?}");
        let document = runtime.take_document();
        assert_eq!(
            document.len(),
            after_first,
            "rejected writes add nothing to the document"
        );
    }
}

#[cfg(test)]
mod markup_parse_budget_tests {
    use super::*;

    #[test]
    pub(crate) fn innerhtml_is_charged_against_the_cumulative_budget() {
        let mut runtime = JsRuntime::new().expect("runtime");
        host_cell(&mut runtime.context)
            .expect("host installed")
            .max_document_write_bytes = 64;
        runtime
            .eval("document.body.innerHTML = '<b>first</b>';", "inner.js")
            .expect("first assignment");
        runtime
            .eval(
                "document.body.innerHTML = '<b>' + 'x'.repeat(200) + '</b>';",
                "inner.js",
            )
            .expect("over-budget assignment");
        let text = {
            let document = runtime.document();
            let body = document.body().expect("body");
            document.text_content(body)
        };
        assert_eq!(text, "first", "the over-budget assignment is ignored");
        let console = runtime.take_console();
        let warnings = console
            .iter()
            .filter(|entry| entry.message.contains("innerHTML assignment ignored"))
            .count();
        assert_eq!(warnings, 1, "reported once: {console:?}");
    }
}
