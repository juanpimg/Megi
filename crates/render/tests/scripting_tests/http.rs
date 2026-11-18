//! External-script loading over HTTP and load-attempt caps.

use super::common::*;

#[test]
fn external_script_over_http_is_loaded_and_executed() {
    let page = "<!doctype html><html><head><style>\
                html,body{margin:0;padding:0;background:#fff}\
                #target{display:block;margin:8px;font-size:48px;color:#000}\
                </style></head><body><p id=\"target\">plain</p>\
                <script src=\"/mutation.js\"></script></body></html>";
    let script = std::fs::read(fixture_dir().join("assets/js_mutation.js")).expect("script asset");
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html",
        RouteResponse {
            content_type: "text/html; charset=utf-8",
            body: page.as_bytes().to_vec(),
        },
    );
    routes.insert(
        "/mutation.js",
        RouteResponse {
            content_type: "text/javascript",
            body: script,
        },
    );
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/page.html"), &options()).expect("render http page");
    assert_eq!(output.stats.scripts_executed, 1);
    assert!(
        count_color(&output.pixmap, BLUE) > 20,
        "HTTP-loaded external script must paint blue text, got {} px",
        count_color(&output.pixmap, BLUE)
    );
}

#[test]
fn no_network_blocks_remote_scripts() {
    let page = "<!doctype html><html><head><style>\
                html,body{margin:0;background:#fff}</style></head>\
                <body><p id=\"target\">plain</p>\
                <script src=\"http://127.0.0.1:9/mutation.js\"></script></body></html>";
    let path = std::env::temp_dir().join(format!(
        "megi-render-script-nonet-{}.html",
        std::process::id()
    ));
    std::fs::write(&path, page).expect("write temp page");
    let mut offline = RenderOptions {
        no_network: true,
        ..options()
    };
    offline.scripting = ScriptingMode::Enabled;
    let output = render_url(&file_url(&path), &offline).expect("offline render");
    let _ = std::fs::remove_file(&path);
    assert_eq!(output.stats.scripts_executed, 0);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("--no-network")),
        "expected --no-network warning: {:?}",
        output.stats.warnings
    );
    assert!(!output.png.is_empty());
}

#[test]
fn script_load_attempts_are_capped_even_when_every_load_fails() {
    // `max_scripts` used to count successful scripts only: every failing `<script src>` was
    // still fetched. Three missing files with `max_scripts = 2` must stop after two loads.
    let dir = std::env::temp_dir().join(format!("megi-script-attempts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let document = dir.join("index.html");
    std::fs::write(
        &document,
        "<!DOCTYPE html><body>\
         <script src='file:///nonexistent-a.js'></script>\
         <script src='file:///nonexistent-b.js'></script>\
         <script src='file:///nonexistent-c.js'></script></body>",
    )
    .expect("write document");

    let mut options = options();
    options.scripting_limits = ScriptingLimits {
        max_scripts: 2,
        max_script_attempts: 2,
        ..ScriptingLimits::default()
    };
    let output = render_url(&file_url(&document), &options).expect("render");
    let failed_loads = output
        .stats
        .warnings
        .iter()
        .filter(|warning| warning.contains("skipping script file:///nonexistent"))
        .count();
    assert_eq!(failed_loads, 2, "only two fetches may reach the loader");
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("attempted more than 2 script loads")),
        "the cap must be reported: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn repeated_classic_script_src_executes_once_per_element() {
    let dir = std::env::temp_dir().join(format!("megi-script-repeat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("counter.js"), "window.n = (window.n || 0) + 1;").expect("write js");
    let document = dir.join("index.html");
    std::fs::write(
        &document,
        "<!DOCTYPE html><body>\
         <script src='counter.js'></script>\
         <script src='counter.js'></script>\
         <script>document.body.setAttribute('data-count', String(window.n))</script></body>",
    )
    .expect("write document");
    let output = render_url(&file_url(&document), &options()).expect("render");
    assert_eq!(
        output.stats.scripts_executed, 3,
        "two classic external scripts plus the inline check must all execute (#1217): {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_script_attempts_do_not_starve_a_valid_one() {
    // `max_scripts` bounds executions and `max_script_attempts` bounds fetches: two failing
    // URLs followed by a valid script must still execute it (a single shared counter with
    // max_scripts = 1 used to stop after the first failure).
    let dir = std::env::temp_dir().join(format!("megi-script-starve-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("ok.js"),
        "document.body.setAttribute('data-ok', '1');",
    )
    .expect("write js");
    let document = dir.join("index.html");
    std::fs::write(
        &document,
        "<!DOCTYPE html><body>\
         <script src='missing-a.js'></script>\
         <script src='missing-b.js'></script>\
         <script src='ok.js'></script></body>",
    )
    .expect("write document");

    let mut options = options();
    options.scripting_limits = ScriptingLimits {
        max_scripts: 1,
        max_script_attempts: 8,
        ..ScriptingLimits::default()
    };
    let output = render_url(&file_url(&document), &options).expect("render");
    assert_eq!(
        output.stats.scripts_executed, 1,
        "the valid script must run after the two failures: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn module_graph_fetches_are_capped() {
    // Static imports are fetched by the JS module loader, outside load_scripts: three
    // missing imports with max_module_fetches = 2 must stop after two fetches.
    let dir = std::env::temp_dir().join(format!("megi-module-cap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for name in ["m1.js", "m2.js", "m3.js"] {
        std::fs::write(dir.join(name), "export const marker = 1;\n").expect("module");
    }
    let document = dir.join("index.html");
    std::fs::write(
        &document,
        "<!DOCTYPE html><body><script type=\"module\">\n\
         import './m1.js';\n\
         import './m2.js';\n\
         import './m3.js';\n\
         </script></body>",
    )
    .expect("write document");

    let options = RenderOptions {
        viewport: Viewport::new(200, 100).expect("viewport"),
        scripting_limits: ScriptingLimits {
            max_module_fetches: 2,
            ..ScriptingLimits::default()
        },
        ..RenderOptions::default()
    };
    let output = render_url(&file_url(&document), &options).expect("render");
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("max_module_fetches")),
        "the third module fetch must be blocked and reported: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn script_mutating_existing_style_element_invalidates_stylesheet_cache() {
    let dir = std::env::temp_dir().join(format!("megi-style-mutate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let document = dir.join("index.html");
    std::fs::write(
        &document,
        "<!DOCTYPE html><html><head>\
         <style id='s'>div { width: 120px; height: 80px; background: rgb(255, 0, 0); }</style>\
         </head><body><div></div>\
         <script>document.getElementById('s').textContent = 'div { width: 120px; height: 80px; background: rgb(0, 255, 0); }';</script>\
         </body></html>",
    )
    .expect("write document");

    let output = render_url(&file_url(&document), &options()).expect("render");
    assert_eq!(output.stats.scripts_executed, 1);
    assert!(
        count_color(&output.pixmap, GREEN) > 5_000,
        "mutated <style> must turn box green, got {} green / {} red",
        count_color(&output.pixmap, GREEN),
        count_color(&output.pixmap, RED)
    );
    assert_eq!(count_color(&output.pixmap, RED), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
