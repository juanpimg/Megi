//! Fixture-driven scripting tests: mutation, loading, budgets and console capture.

use super::common::*;

#[test]
fn default_scripting_mode_is_enabled_with_the_feature() {
    assert_eq!(ScriptingMode::default(), ScriptingMode::Enabled);
}

#[test]
fn inline_script_text_and_style_mutations_reach_the_png() {
    let enabled = render_fixture("js_text.html", &options());
    assert_eq!(enabled.stats.scripts_executed, 1);
    assert_eq!(enabled.stats.scripts_failed, 0);
    assert!(enabled.stats.script_bytes > 0);
    assert!(
        count_color(&enabled.pixmap, RED) > 20,
        "the script-set red colour must paint, got {} px",
        count_color(&enabled.pixmap, RED)
    );

    let mut disabled_options = options();
    disabled_options.scripting = ScriptingMode::Disabled;
    let disabled = render_fixture("js_text.html", &disabled_options);
    assert_eq!(disabled.stats.scripts_executed, 0);
    assert_eq!(count_color(&disabled.pixmap, RED), 0);
    assert_ne!(
        enabled.png, disabled.png,
        "scripting must change the rendered PNG"
    );
}

#[test]
fn created_and_appended_element_is_laid_out_and_painted() {
    let enabled = render_fixture("js_create.html", &options());
    assert_eq!(enabled.stats.scripts_executed, 1);
    assert!(
        count_color(&enabled.pixmap, GREEN) > 5_000,
        "created 160x80 green box must paint, got {} px",
        count_color(&enabled.pixmap, GREEN)
    );

    let mut disabled_options = options();
    disabled_options.scripting = ScriptingMode::Disabled;
    let disabled = render_fixture("js_create.html", &disabled_options);
    assert_eq!(count_color(&disabled.pixmap, GREEN), 0);
}

#[test]
fn external_script_from_file_url_is_loaded_and_executed() {
    let output = render_fixture("js_external.html", &options());
    assert_eq!(output.stats.scripts_executed, 1);
    assert_eq!(output.stats.scripts_failed, 0);
    assert!(
        output.stats.script_bytes > 0,
        "external script bytes counted"
    );
    assert!(
        count_color(&output.pixmap, BLUE) > 20,
        "external script colour must paint, got {} px",
        count_color(&output.pixmap, BLUE)
    );
}

#[test]
fn document_write_content_is_rendered_after_scripting() {
    let output = render_fixture("js_document_write.html", &options());
    assert_eq!(output.stats.scripts_executed, 1);
    assert!(
        count_color(&output.pixmap, MAGENTA) > 20,
        "document.write content must paint, got {} px",
        count_color(&output.pixmap, MAGENTA)
    );
}

#[test]
fn disabled_scripting_ignores_scripts_entirely() {
    let mut disabled_options = options();
    disabled_options.scripting = ScriptingMode::Disabled;
    let output = render_fixture("js_create.html", &disabled_options);
    assert_eq!(output.stats.scripts_executed, 0);
    assert_eq!(output.stats.scripts_failed, 0);
    assert_eq!(output.stats.script_bytes, 0);
    assert!(output.stats.console.is_empty());
    assert_eq!(count_color(&output.pixmap, GREEN), 0);
    // The base document still renders (white canvas with the empty host div).
    assert!(!output.png.is_empty());
}

#[test]
fn script_exception_is_a_warning_and_render_continues() {
    let output = render_fixture("js_throw.html", &options());
    assert_eq!(output.stats.scripts_executed, 0);
    assert_eq!(output.stats.scripts_failed, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("boom") || warning.contains("failed")),
        "exception must warn: {:?}",
        output.stats.warnings
    );
    assert!(
        !output.png.is_empty(),
        "render must survive a throwing script"
    );
}

#[test]
fn infinite_loop_is_stopped_by_the_budget() {
    let mut bounded = options();
    bounded.scripting_limits = ScriptingLimits {
        budget: ScriptBudget {
            loop_iterations: 20_000,
            ..ScriptBudget::SMALL
        },
        total_loop_iterations: 20_000,
        ..ScriptingLimits::default()
    };
    let start = Instant::now();
    let output = render_fixture("js_loop.html", &bounded);
    let elapsed = start.elapsed();
    assert!(
        output.stats.scripts_failed >= 1,
        "the loop must exceed the budget"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.to_ascii_lowercase().contains("budget")),
        "budget warning expected: {:?}",
        output.stats.warnings
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "budgeted script must not hang the renderer (took {elapsed:?})"
    );
    assert!(!output.png.is_empty());
}

#[test]
fn console_output_is_captured_in_stats_and_warnings() {
    let output = render_fixture("js_console.html", &options());
    assert_eq!(output.stats.console.len(), 3);
    let levels: Vec<&str> = output
        .stats
        .console
        .iter()
        .map(|entry: &ConsoleMessage| entry.level.as_str())
        .collect();
    assert_eq!(levels, vec!["log", "warn", "error"]);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("console.warn: careful"))
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("console.error: bad"))
    );
}

#[test]
fn scripting_pipeline_is_deterministic() {
    let first = render_fixture("js_text.html", &options());
    let second = render_fixture("js_text.html", &options());
    assert_eq!(
        first.png, second.png,
        "two scripted renders must be identical"
    );
    assert_eq!(first.stats.console, second.stats.console);

    let mut disabled_options = options();
    disabled_options.scripting = ScriptingMode::Disabled;
    let third = render_fixture("js_text.html", &disabled_options);
    let fourth = render_fixture("js_text.html", &disabled_options);
    assert_eq!(third.png, fourth.png);
}
