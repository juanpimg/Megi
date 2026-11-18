//! End-to-end render tests for Shadow DOM (#1541): `<slot>` projection, fallback content,
//! CSS style encapsulation, `:host`, `:host(...)`, `::slotted(...)`, and Declarative Shadow DOM.

#![cfg(feature = "js")]

use render::{RenderOptions, RenderOutput, ScriptingMode, Viewport, render_url};

fn pixel_at(output: &RenderOutput, x: u32, y: u32) -> css::Color {
    output.pixmap.pixels[(y * output.pixmap.width + x) as usize]
}

fn render_html_temp(name: &str, html: &str, options: RenderOptions) -> RenderOutput {
    let path = std::env::temp_dir().join(format!("megi_{name}_{}.html", std::process::id()));
    std::fs::write(&path, html).expect("write temp html");
    let url = format!("file://{}", path.display());
    let output = render_url(&url, &options).expect("render shadow dom html");
    let _ = std::fs::remove_file(&path);
    output
}

#[test]
fn shadow_dom_slots_and_fallback_render_visually() {
    let html = r#"<!doctype html>
    <html>
      <head>
        <style>
          html, body { margin: 0; padding: 0; background: #ffffff; }
          x-card { display: block; width: 200px; }
          .projected { display: block; width: 100px; height: 30px; background: #00ff00; }
          .unslotted { display: block; width: 100px; height: 30px; background: #ff0000; }
        </style>
      </head>
      <body>
        <x-card id="host">
          <div slot="header" class="projected"></div>
          <div class="unslotted"></div>
        </x-card>
        <script>
          var host = document.getElementById("host");
          var sr = host.attachShadow({ mode: "open" });
          sr.innerHTML = '<style>' +
            '.wrap { display: block; }' +
            '.fb { display: block; width: 100px; height: 30px; background: #0000ff; }' +
            '</style>' +
            '<div class="wrap">' +
            '<slot name="header"></slot>' +
            '<slot name="footer"><div class="fb"></div></slot>' +
            '</div>';
        </script>
      </body>
    </html>"#;

    let output = render_html_temp(
        "shadow_slots",
        html,
        RenderOptions {
            viewport: Viewport::new(200, 120).expect("viewport"),
            ..RenderOptions::default()
        },
    );

    // Top 30px: slotted header child (green).
    assert_eq!(pixel_at(&output, 20, 15), css::Color::new(0, 255, 0, 255));
    // Next 30px (y=45): empty footer slot renders blue fallback; unslotted red light child is NOT rendered.
    assert_eq!(pixel_at(&output, 20, 45), css::Color::new(0, 0, 255, 255));
    // Below (y=75): white canvas (no stray red unslotted element).
    assert_eq!(
        pixel_at(&output, 20, 75),
        css::Color::new(255, 255, 255, 255)
    );
}

#[test]
fn shadow_dom_style_encapsulation_host_and_slotted() {
    let html = r#"<!doctype html>
    <html>
      <head>
        <style>
          html, body { margin: 0; padding: 0; background: #ffffff; }
          .scoped { background: #ff0000; }
          #light-p { display: block; width: 100px; height: 20px; margin: 0; background: #112233; }
          #host { display: block; width: 120px; }
        </style>
      </head>
      <body>
        <x-box id="host" class="active">
          <span></span>
        </x-box>
        <p id="light-p"></p>
        <script>
          var host = document.getElementById("host");
          var sr = host.attachShadow({ mode: "open" });
          sr.innerHTML = '<style>' +
            ':host(.active) { background: #224466; padding-top: 20px; }' +
            '.scoped { display: block; width: 80px; height: 20px; background: #00ff00; }' +
            '::slotted(span) { display: block; width: 80px; height: 20px; background: #0000ff; }' +
            'p { background: #ff00ff; }' +
            '</style>' +
            '<div class="scoped"></div>' +
            '<slot></slot>';
        </script>
      </body>
    </html>"#;

    let output = render_html_temp(
        "shadow_scoping",
        html,
        RenderOptions {
            viewport: Viewport::new(200, 120).expect("viewport"),
            ..RenderOptions::default()
        },
    );

    // Host padding top (y=10): styled by `:host(.active)` -> #224466.
    assert_eq!(
        pixel_at(&output, 10, 10),
        css::Color::new(0x22, 0x44, 0x66, 255)
    );
    // Shadow `.scoped` (y=30): green (#00ff00), outer `.scoped { red }` did NOT leak into shadow.
    assert_eq!(pixel_at(&output, 10, 30), css::Color::new(0, 255, 0, 255));
    // Slotted `span` (y=50): blue (#0000ff) from `::slotted(span)`.
    assert_eq!(pixel_at(&output, 10, 50), css::Color::new(0, 0, 255, 255));
    // Light `#light-p` (y=70): stays #112233; shadow `p { magenta }` did NOT leak out.
    assert_eq!(
        pixel_at(&output, 10, 70),
        css::Color::new(0x11, 0x22, 0x33, 255)
    );
}

#[test]
fn declarative_shadow_dom_renders_with_scripting_disabled() {
    let html = r#"<!doctype html>
    <html>
      <head>
        <style>
          html, body { margin: 0; padding: 0; background: #ffffff; }
        </style>
      </head>
      <body>
        <x-badge>
          <template shadowrootmode="open">
            <style>
              :host { display: block; width: 60px; height: 40px; background: #00cc44; }
              .inner { display: block; width: 30px; height: 20px; background: #ffcc00; }
            </style>
            <div class="inner"></div>
          </template>
        </x-badge>
      </body>
    </html>"#;

    let output = render_html_temp(
        "declarative_shadow",
        html,
        RenderOptions {
            viewport: Viewport::new(120, 80).expect("viewport"),
            scripting: ScriptingMode::Disabled,
            ..RenderOptions::default()
        },
    );

    assert_eq!(
        pixel_at(&output, 10, 10),
        css::Color::new(0xff, 0xcc, 0x00, 255)
    );
    assert_eq!(
        pixel_at(&output, 45, 25),
        css::Color::new(0x00, 0xcc, 0x44, 255)
    );
}
