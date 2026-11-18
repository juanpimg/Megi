use super::*;

#[test]
fn self_writing_script_loops_are_fuelled() {
    // #639: a parser-inserted script can `document.write` another `<script>`, which executes in
    // turn and may write again — without a fuel cap the parse never finishes. Each execution and
    // each written byte must be budgeted.
    use megi_core::Document;
    use megi_html::scripting::{ScriptExecution, ScriptExecutor};
    use megi_html::{ParseOptions, parse_document_with_options_and_executor};

    struct LoopExecutor {
        runs: usize,
    }

    impl ScriptExecutor for LoopExecutor {
        fn execute(
            &mut self,
            _document: &mut Document,
            _script_text: &str,
            _source_name: &str,
        ) -> ScriptExecution {
            self.runs += 1;
            ScriptExecution {
                written_html: "<script>x</script>".to_string(),
                error: None,
            }
        }
    }

    let started = std::time::Instant::now();
    let outcome = parse_document_with_options_and_executor(
        "<script>x</script>",
        true,
        &ParseOptions::DEFAULT,
        Some(Box::new(LoopExecutor { runs: 0 })),
    );
    let elapsed = started.elapsed();
    let budget = scaled_budget(Duration::from_secs(5));
    assert!(
        elapsed < budget,
        "a self-writing script must be fuelled, took {elapsed:?} (budget {budget:?}, scale {})",
        budget_scale()
    );
    assert!(
        outcome.limit_violations.scripts_skipped > 0
            || outcome.limit_violations.written_bytes_dropped > 0,
        "the fuel cap must be reported, got {:?}",
        outcome.limit_violations
    );
}

#[test]
fn many_small_document_writes_stay_linear() {
    // #639: every `document.write` re-inserted through `Tokenizer::insert_input`, which shifted
    // every line start and rebuilt the UTF-16 prefix table from scratch — quadratic in the
    // number of writes, which is exactly the normal `document.write` pattern.
    use megi_core::Document;
    use megi_html::scripting::{ScriptExecution, ScriptExecutor};
    use megi_html::{ParseOptions, parse_document_with_options_and_executor};

    struct SmallWrites {
        remaining: usize,
    }

    impl ScriptExecutor for SmallWrites {
        fn execute(
            &mut self,
            _document: &mut Document,
            _script_text: &str,
            _source_name: &str,
        ) -> ScriptExecution {
            if self.remaining == 0 {
                return ScriptExecution {
                    written_html: String::new(),
                    error: None,
                };
            }
            self.remaining -= 1;
            // A short write with a line break, repeated: the pathological shape for the old
            // per-write line-table shift.
            ScriptExecution {
                written_html: "<script>x</script>\n<p>y</p>\n".to_string(),
                error: None,
            }
        }
    }

    let started = std::time::Instant::now();
    let outcome = parse_document_with_options_and_executor(
        "<script>x</script>",
        true,
        &ParseOptions::DEFAULT,
        Some(Box::new(SmallWrites { remaining: 2_000 })),
    );
    let elapsed = started.elapsed();
    let budget = scaled_budget(Duration::from_secs(5));
    assert!(
        elapsed < budget,
        "2 000 small writes took {elapsed:?} (budget {budget:?}, scale {})",
        budget_scale()
    );
    assert!(
        outcome.limit_violations.scripts_skipped > 0
            || outcome.limit_violations.written_bytes_dropped > 0
    );
}
