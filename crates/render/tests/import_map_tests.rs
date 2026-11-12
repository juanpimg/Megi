//! R187 E2E: `<script type="importmap">` remapping, dynamic `import()` and invalid maps.
#![cfg(feature = "js")]

use std::path::PathBuf;

use render::{RenderOptions, ScriptingMode, Viewport};

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("megi-importmap-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Renders `html` from a `file:` document in `dir` so `file:` module imports are allowed
/// (a `data:` document may not read the local filesystem).
fn render_file(html: &str, dir: &std::path::Path, width: u32, height: u32) -> render::RenderOutput {
    let document = dir.join("index.html");
    std::fs::write(&document, html).expect("write document");
    let options = RenderOptions {
        viewport: Viewport::new(width, height).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        ..RenderOptions::default()
    };
    let url = format!("file://{}", document.display());
    render::render_url(&url, &options).expect("render")
}

fn body_attribute(output: &render::RenderOutput, name: &str) -> Option<String> {
    let document = &output.document;
    let body = document.body()?;
    document
        .try_node(body)
        .and_then(|node| node.as_element())
        .and_then(|element| element.get_attribute(name).map(str::to_owned))
}

#[test]
fn import_map_remaps_a_bare_specifier_to_a_file_module() {
    let dir = fixture_dir("map");
    let dep = dir.join("dep.js");
    std::fs::write(
        &dep,
        "document.body.setAttribute('data-mapped', 'yes');\nexport const value = 1;\n",
    )
    .expect("write dep");
    let importmap = format!(r#"{{"imports":{{"dep":"file://{}"}}}}"#, dep.display());
    let html = format!(
        "<!DOCTYPE html><body><script type=\"importmap\">{importmap}</script>\
         <script type=\"module\">import {{ value }} from 'dep';</script></body>"
    );
    let output = render_file(&html, &dir, 240, 160);
    assert_eq!(
        body_attribute(&output, "data-mapped").as_deref(),
        Some("yes")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dynamic_import_evaluates_an_extra_module_and_mutates_the_dom() {
    let dir = fixture_dir("dynamic");
    let dynamic = dir.join("dyn.js");
    std::fs::write(
        &dynamic,
        "document.body.setAttribute('data-dynamic', 'loaded');\nexport const x = 1;\n",
    )
    .expect("write dyn");
    let html = "<!DOCTYPE html><script type=\"module\">import('./dyn.js');</script><body></body>";
    // Serve the document from the temp directory so relative dynamic imports resolve.
    let document = dir.join("index.html");
    std::fs::write(&document, html).expect("write document");
    let output = render_file(html, &dir, 240, 160);
    assert_eq!(
        body_attribute(&output, "data-dynamic").as_deref(),
        Some("loaded"),
        "warnings: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_import_maps_are_warnings_and_do_not_break_independent_modules() {
    let dir = fixture_dir("invalid");
    let module = dir.join("ok.js");
    std::fs::write(
        &module,
        "document.body.setAttribute('data-ok', '1');\nexport const ok = true;\n",
    )
    .expect("write module");
    let html = format!(
        "<!DOCTYPE html><script type=\"importmap\">{{bad json}}</script>\
         <body><script type=\"module\" src=\"file://{}\"></script></body>",
        module.display()
    );
    let output = render_file(&html, &dir, 240, 160);
    assert_eq!(body_attribute(&output, "data-ok").as_deref(), Some("1"));
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("import map")),
        "invalid map warning: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn render_file(html: &str, dir: &std::path::Path, width: u32, height: u32) -> render::RenderOutput {
    let document = dir.join("index.html");
    std::fs::write(&document, html).expect("write document");
    let options = RenderOptions {
        viewport: Viewport::new(width, height).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        ..RenderOptions::default()
    };
    render::render_url(&format!("file://{}", document.display()), &options).expect("render")
}

#[test]
fn dynamic_import_from_a_file_document_loads_and_mutates_the_dom() {
    let dir = fixture_dir("dynamic-file");
    std::fs::write(
        dir.join("dyn.js"),
        "document.body.setAttribute('data-dynamic', 'loaded');\nexport const x = 1;\n",
    )
    .expect("write dyn");
    let output = render_file(
        "<!DOCTYPE html><body><script type=\"module\">import('./dyn.js');</script></body>",
        &dir,
        240,
        160,
    );
    assert_eq!(
        body_attribute(&output, "data-dynamic").as_deref(),
        Some("loaded"),
        "warnings: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn duplicate_import_maps_warn_but_do_not_break_modules() {
    let dir = fixture_dir("duplicate");
    let module = dir.join("ok.js");
    std::fs::write(
        &module,
        "document.body.setAttribute('data-ok', '1');\nexport const ok = true;\n",
    )
    .expect("write module");
    let html = format!(
        "<!DOCTYPE html><script type=\"importmap\">{{\"imports\":{{}}}}</script>\
         <script type=\"importmap\">{{\"imports\":{{}}}}</script>\
         <body><script type=\"module\" src=\"file://{}\"></script></body>",
        module.display()
    );
    let output = render_file(&html, &dir, 240, 160);
    assert_eq!(body_attribute(&output, "data-ok").as_deref(), Some("1"));
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("import map")),
        "duplicate map warning: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}
