//! R187 E2E: `<script type="importmap">` remapping, dynamic `import()` and invalid maps.
#![cfg(feature = "js")]

use std::path::PathBuf;

use megi_render::{RenderOptions, ScriptingMode, Viewport};

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("megi-importmap-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn body_attribute(output: &megi_render::RenderOutput, name: &str) -> Option<String> {
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

fn render_file(html: &str, dir: &std::path::Path, width: u32, height: u32) -> megi_render::RenderOutput {
    let document = dir.join("index.html");
    std::fs::write(&document, html).expect("write document");
    let options = RenderOptions {
        viewport: Viewport::new(width, height).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        ..RenderOptions::default()
    };
    megi_render::render_url(&format!("file://{}", document.display()), &options).expect("render")
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

#[test]
fn import_map_scopes_prefer_longest_prefix_fall_back_and_verify_sri() {
    let dir = fixture_dir("scopes");
    std::fs::create_dir_all(dir.join("app/nested")).expect("mkdir nested");
    std::fs::write(dir.join("v1.js"), "export const ver = 'v1';\n").expect("write v1");
    let v2_bytes = b"export const ver = 'v2-sri';\n";
    std::fs::write(dir.join("v2.js"), v2_bytes).expect("write v2");
    std::fs::write(dir.join("shared.js"), "export const shared = 'top';\n").expect("write shared");
    std::fs::write(
        dir.join("app/nested/entry.js"),
        "import { ver } from 'pkg';\n\
         import { shared } from 'shared';\n\
         document.body.setAttribute('data-nested', ver + ':' + shared);\n",
    )
    .expect("write nested entry");
    std::fs::write(
        dir.join("app/entry.js"),
        "import { ver } from 'pkg';\n\
         document.body.setAttribute('data-app', ver);\n",
    )
    .expect("write app entry");

    const V2_SRI: &str = "sha256-QGSa7f5w4ziaWV41HW6uI+FMaVVgOghSXdag+NSvuCc=";

    let v1_url = format!("file://{}", dir.join("v1.js").display());
    let v2_url = format!("file://{}", dir.join("v2.js").display());
    let shared_url = format!("file://{}", dir.join("shared.js").display());
    let app_scope = format!("file://{}/app/", dir.display());
    let nested_scope = format!("file://{}/app/nested/", dir.display());
    let app_entry_url = format!("file://{}", dir.join("app/entry.js").display());
    let nested_entry_url = format!("file://{}", dir.join("app/nested/entry.js").display());
    let blocked_entry_url = format!("file://{}", dir.join("app/nested/blocked.js").display());
    std::fs::write(
        dir.join("app/nested/blocked.js"),
        "import { denied } from 'denied';\ndocument.body.setAttribute('data-blocked', denied);\n",
    )
    .expect("write blocked entry");

    let importmap = format!(
        r#"{{"imports":{{"pkg":"{v1_url}","shared":"{shared_url}","denied":"{shared_url}"}},"scopes":{{"{app_scope}":{{"pkg":"{v1_url}"}},"{nested_scope}":{{"pkg":"{v2_url}","denied":null}}}},"integrity":{{"{v2_url}":"{V2_SRI}"}}}}"#
    );
    let html = format!(
        "<!DOCTYPE html><body>\
         <script type=\"importmap\">{importmap}</script>\
         <script type=\"module\" src=\"{app_entry_url}\"></script>\
         <script type=\"module\" src=\"{nested_entry_url}\"></script>\
         <script type=\"module\" src=\"{blocked_entry_url}\"></script>\
         </body>"
    );
    let output = render_file(&html, &dir, 240, 160);
    assert_eq!(
        body_attribute(&output, "data-app").as_deref(),
        Some("v1"),
        "warnings: {:?}",
        output.stats.warnings
    );
    assert_eq!(
        body_attribute(&output, "data-nested").as_deref(),
        Some("v2-sri:top"),
        "warnings: {:?}",
        output.stats.warnings
    );
    assert_eq!(body_attribute(&output, "data-blocked").as_deref(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dynamic_import_supports_non_literal_expressions_in_module_and_classic_scripts() {
    let dir = fixture_dir("non-literal-dyn");
    std::fs::write(dir.join("piece.js"), "export const tag = 'dyn-ok';\n").expect("write piece");
    let html = "<!DOCTYPE html><body>\
        <script type=\"module\">\
            const base = './pie';\
            const mod = await import(base + 'ce.js');\
            document.body.setAttribute('data-mod', mod.tag);\
        </script>\
        <script>\
            var seg = 'ce';\
            import('./pie' + seg + '.js').then(function (m) {\
                document.body.setAttribute('data-classic', m.tag);\
            });\
        </script>\
        </body>";
    let output = render_file(html, &dir, 240, 160);
    assert_eq!(
        body_attribute(&output, "data-mod").as_deref(),
        Some("dyn-ok"),
        "warnings: {:?}",
        output.stats.warnings
    );
    assert_eq!(
        body_attribute(&output, "data-classic").as_deref(),
        Some("dyn-ok"),
        "warnings: {:?}",
        output.stats.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}
