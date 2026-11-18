//! Stylesheet-load caps for failed and successful imports.

use super::common::*;

#[test]
fn failed_import_loads_count_against_the_stylesheet_cap() {
    // `max_stylesheets` used to bound successful imports only: missing sheets were fetched
    // once per unique URL with no limit.
    let dir = std::env::temp_dir().join(format!("megi-import-attempts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let document = dir.join("index.html");
    std::fs::write(
        &document,
        "<!DOCTYPE html><style>\
         @import url('file:///nonexistent-a.css');\
         @import url('file:///nonexistent-b.css');\
         @import url('file:///nonexistent-c.css');\
         </style><body></body>",
    )
    .expect("write document");

    let options = RenderOptions {
        viewport: Viewport::new(200, 100).expect("viewport"),
        max_stylesheets: 2,
        ..RenderOptions::default()
    };
    let output = render_url(&format!("file://{}", document.display()), &options).expect("render");
    let failed_imports = output
        .stats
        .warnings
        .iter()
        .filter(|warning| warning.contains("skipping stylesheet file:///nonexistent"))
        .count();
    assert_eq!(failed_imports, 2, "only two fetches may reach the loader");
    let cap_warnings = output
        .stats
        .warnings
        .iter()
        .filter(|warning| warning.contains("stylesheet loads attempted"))
        .count();
    assert_eq!(
        cap_warnings, 1,
        "the cap is reported once, not per skipped import: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exceeding_the_stylesheet_cap_on_successful_imports_degrades_to_a_warning() {
    // #654: the sheet cap skips further imports with a single warning instead of failing the
    // whole render.
    let dir = std::env::temp_dir().join(format!("megi-import-error-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for name in ["a.css", "b.css", "c.css"] {
        std::fs::write(dir.join(name), "p { color: rgb(255, 0, 0) }").expect("css");
    }
    let document = dir.join("index.html");
    std::fs::write(
        &document,
        "<!DOCTYPE html><style>\
         @import url('a.css'); @import url('b.css'); @import url('c.css');\
         </style><body></body>",
    )
    .expect("write document");

    let options = RenderOptions {
        viewport: Viewport::new(200, 100).expect("viewport"),
        max_stylesheets: 2,
        ..RenderOptions::default()
    };
    let output = render_url(&format!("file://{}", document.display()), &options)
        .expect("the cap must not fail the render");
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("stylesheet cap reached")),
        "expected a cap warning, got {:?}",
        output.stats.warnings
    );
    assert!(output.stats.imports_skipped >= 1);
    let _ = std::fs::remove_dir_all(&dir);
}
