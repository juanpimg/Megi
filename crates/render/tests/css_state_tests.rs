//! Dynamic CSS pseudo-class state in the render pipeline (R112).
//!
//! Render resolves styles with an empty `css::StateContext` (no pointer/focus ownership) but
//! `css::state` derives `:checked`/`:disabled`/`:placeholder-shown`/... from the DOM and
//! honours the documented `data-megi-*` attributes, so static documents can model host state
//! (the shell injects `data-megi-focused` after interactions).
//!
//! Form controls (`<input>`) are painted as replaced boxes whose gray placeholder covers the
//! content background; the fixture therefore proves `:checked`/`:disabled`/`:focus` on them
//! through their **CSS border colour** and through the returned computed styles, and uses
//! non-replaced elements (`div`/`button`) for full-background pixel checks.

use std::path::Path;

use render::{RenderOptions, Viewport, render_url};

fn fixture_path(name: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render");
    format!("file://{}", dir.join(name).display())
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(400, 400).expect("viewport"),
        ..RenderOptions::default()
    }
}

fn render_fixture(name: &str) -> render::RenderOutput {
    render_url(&fixture_path(name), &options())
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

fn count_color(pixmap: &paint::Pixmap, color: css::Color) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

const GREEN_BORDER: css::Color = css::Color::new(0x00, 0xFF, 0x00, 0xFF);
const DARK_GREEN: css::Color = css::Color::new(0x00, 0x66, 0x00, 0xFF);
const RED: css::Color = css::Color::new(0xFF, 0x00, 0x00, 0xFF);
const BLUE: css::Color = css::Color::new(0x00, 0x00, 0xFF, 0xFF);
const YELLOW: css::Color = css::Color::new(0xFF, 0xFF, 0x00, 0xFF);
const MAGENTA: css::Color = css::Color::new(0xFF, 0x00, 0xFF, 0xFF);
const BLACK: css::Color = css::Color::new(0x00, 0x00, 0x00, 0xFF);

#[test]
fn input_checked_border_paints_in_the_png() {
    let output = render_fixture("css_state.html");
    let green = count_color(&output.pixmap, GREEN_BORDER);
    assert!(
        green > 400,
        "checked <input> must paint its green border, got {green} px"
    );
    assert!(green < 1_000, "only one input is checked, got {green} px");
}

#[test]
fn input_disabled_and_focus_borders_paint_in_the_png() {
    let output = render_fixture("css_state.html");
    let red = count_color(&output.pixmap, RED);
    assert!(
        red > 1_000,
        "disabled button background + disabled input border must paint red, got {red} px"
    );
    let yellow = count_color(&output.pixmap, YELLOW);
    assert!(
        yellow > 1_000,
        "focused div background + data-megi-focused input border must paint, got {yellow} px"
    );
}

#[test]
fn placeholder_shown_border_requires_an_empty_value() {
    let output = render_fixture("css_state.html");
    let magenta = count_color(&output.pixmap, MAGENTA);
    assert!(
        magenta > 1_000,
        "placeholder-shown div + empty placeholder input border must paint magenta, got {magenta} px"
    );
    assert!(
        count_color(&output.pixmap, BLACK) > 800,
        "filled text input and the plain div keep black paint"
    );
}

#[test]
fn not_composites_paint_for_unchecked_enabled_controls() {
    let output = render_fixture("css_state.html");
    // `button:not(:disabled):not(:checked)` paints exactly the enabled button blue.
    let blue = count_color(&output.pixmap, BLUE);
    assert!(
        blue > 700,
        "compound :not() selector must paint the enabled button, got {blue} px"
    );
    assert!(blue < 1_600, "compound :not() must not leak, got {blue} px");
    // `div:checked` paints the data-megi-checked div dark green.
    assert!(
        count_color(&output.pixmap, DARK_GREEN) > 700,
        "data-megi-checked div must match :checked and paint"
    );
}

#[test]
fn computed_styles_reflect_the_dynamic_states() {
    let output = render_fixture("css_state.html");
    let mut input_backgrounds = Vec::new();
    for (node_id, style) in &output.styles {
        if let Some(element) = output.document.node(*node_id).as_element()
            && element.local_name.eq_ignore_ascii_case("input")
        {
            input_backgrounds.push(style.background_color);
        }
    }
    assert_eq!(input_backgrounds.len(), 5, "fixture has five inputs");
    assert!(input_backgrounds.contains(&GREEN_BORDER), "input:checked");
    assert!(input_backgrounds.contains(&RED), "input:disabled");
    assert!(
        input_backgrounds.contains(&YELLOW),
        "input:focus (data-megi-focused)"
    );
    assert!(
        input_backgrounds.contains(&MAGENTA),
        "input:placeholder-shown (empty value)"
    );
    assert!(
        input_backgrounds.contains(&BLACK),
        "filled text input keeps black"
    );
}
