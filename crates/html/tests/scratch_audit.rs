use dom::Document;
use html::scripting::{ScriptExecution, ScriptExecutor};
use html::{ParseOptions, parse_document_with_options_and_executor, set_inner_html};

struct Wipe;

impl ScriptExecutor for Wipe {
    fn execute(
        &mut self,
        document: &mut Document,
        _script_text: &str,
        _source_name: &str,
    ) -> ScriptExecution {
        if let Some(body) = document.body() {
            let _ = set_inner_html(document, body, "<p>wiped</p>");
        }
        ScriptExecution::default()
    }
}

#[test]
fn scratch_executor_innerhtml_midparse_panics() {
    let input = "<html><body><div><script>evil()</script><p>after</p></div></body></html>";
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parse_document_with_options_and_executor(
            input,
            true,
            &ParseOptions::default(),
            Some(Box::new(Wipe) as Box<dyn ScriptExecutor>),
        )
    }));
    println!("executor-innerhtml-midparse panicked: {}", r.is_err());
    assert!(r.is_err(), "expected parser panic after innerHTML wiped open elements");
}
