//! CLI JSON stats and worker-message interaction with the timer budget.

use super::common::*;

#[test]
fn cli_json_exposes_event_loop_stats_and_respects_no_scripting() {
    let out_path = std::env::temp_dir().join(format!(
        "megi-event-loop-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    let run = |extra: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_render"))
            .arg(fixture_path("js_event_timeout.html"))
            .arg(&out_path)
            .args(["--width", "400", "--height", "240", "--json"])
            .args(extra)
            .output()
            .expect("render binary runs");
        assert!(
            output.status.success(),
            "render failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("JSON is UTF-8")
    };

    let json = run(&[]);
    assert_eq!(json_number(&json, "script_tasks"), 1);
    assert!(json_number(&json, "script_events") >= 2);
    assert!(json_number(&json, "script_virtual_time_ms") >= 10);
    assert_eq!(json_number(&json, "script_timers_pending"), 0);
    // `timings_us.event_loop` is present in the machine-readable output.
    assert!(
        json.contains("\"event_loop\":"),
        "missing event_loop timing: {json}"
    );
    assert!(out_path.is_file());

    let no_scripting = run(&["--no-scripting"]);
    assert_eq!(json_number(&no_scripting, "script_tasks"), 0);
    assert_eq!(json_number(&no_scripting, "script_events"), 0);
    assert_eq!(json_number(&no_scripting, "scripts_executed"), 0);
    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn worker_deliveries_do_not_consume_the_timer_task_budget() {
    use std::fs;
    let dir = std::env::temp_dir().join(format!("megi-worker-budget-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(
        dir.join("worker.js"),
        "for (var i = 0; i < 100; i++) { postMessage(i); }",
    )
    .expect("worker");
    let page = dir.join("index.html");
    let page_html = "<!DOCTYPE html><body><script>\
         window.__workerMessages = 0;\
         var w = new Worker('file://WORKER_DIR/worker.js');\
         w.onmessage = function () { window.__workerMessages++; };\
         setTimeout(function () {\
           document.body.setAttribute('data-timer', 'ran');\
         }, 0);\
         </script></body>"
        .replace("WORKER_DIR", &dir.display().to_string());
    fs::write(&page, &page_html).expect("page");

    let options = RenderOptions {
        viewport: Viewport::new(400, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        scripting_limits: ScriptingLimits {
            max_tasks: 2,
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output =
        render_url(&format!("file://{}/index.html", dir.display()), &options).expect("render");
    // The single timer must still run even though 100 worker replies were delivered.
    assert_eq!(
        output.stats.script_tasks, 1,
        "worker deliveries must not be counted as timer callbacks (#1221): {:?}",
        output.stats.warnings
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn worker_error_does_not_disclose_local_directory_layout() {
    // #1646: a failed `new Worker` must not echo the resolved absolute file:// path into the
    // error delivered to the page (local directory fingerprinting).
    use std::fs;
    let dir = std::env::temp_dir().join(format!("megi-worker-leak-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    let secret = "directorio_secreto";
    let page_html = format!(
        "<!DOCTYPE html><body><script>\
           var msg = ''; try {{ new Worker('file://{}/{secret}/missing.js'); }} \
           catch (e) {{ msg = e.message; }} \
           var p = document.createElement('p'); p.id = 'err'; p.textContent = msg; \
           document.body.appendChild(p);\
         </script></body>",
        dir.display()
    );
    let page = dir.join("index.html");
    fs::write(&page, &page_html).expect("page");

    let options = RenderOptions {
        viewport: Viewport::new(400, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        ..base_options()
    };
    let output = render_url(&format!("file://{}", page.display()), &options).expect("render");
    let err = output
        .document
        .get_element_by_id("err")
        .map(|node| output.document.text_content(node))
        .unwrap_or_default();
    assert!(
        !err.contains(secret) && !err.contains(&dir.display().to_string()),
        "worker error leaked a local path: {err}"
    );
    let _ = fs::remove_dir_all(&dir);
}
