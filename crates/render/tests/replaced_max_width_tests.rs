//! #2247: UA `max-width:100%` must not clip an atomic inline with an author width.

use megi_render::{RenderOptions, Viewport, render_url};

fn render(html: &str) -> megi_render::RenderOutput {
    let data = format!(
        "data:text/html,{}",
        html.chars()
            .map(|c| match c {
                ' ' => "%20".into(),
                '<' => "%3C".into(),
                '>' => "%3E".into(),
                '"' => "%22".into(),
                '\n' => "%0A".into(),
                '#' => "%23".into(),
                '{' => "%7B".into(),
                '}' => "%7D".into(),
                ':' => "%3A".into(),
                ';' => "%3B".into(),
                '=' => "%3D".into(),
                '(' => "%28".into(),
                ')' => "%29".into(),
                ',' => "%2C".into(),
                other => other.to_string(),
            })
            .collect::<String>()
    );
    render_url(
        &data,
        &RenderOptions {
            viewport: Viewport::new(800, 200).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render")
}

#[test]
fn inline_atom_keeps_author_width_under_ua_max_width_cap() {
    // The UA stylesheet has `input { max-width:100% }`. Inside a narrow
    // inline formatting context that cap must not shrink a 496px author width.
    let html = "<body style='background:white'><div style='display:inline-block'>\
                <input style='width:496px;height:25px'></div></body>";
    let out = render(html);
    let has_wide_input = out
        .display_list
        .items
        .iter()
        .any(|item| format!("{:?}", item).contains("width: 496.0"));
    assert!(
        has_wide_input,
        "input clipped by UA max-width: {:?}",
        out.stats.warnings
    );
}
