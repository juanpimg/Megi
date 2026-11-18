//! End-to-end CSS animation frames: the renderer evaluates the animation timeline at the
//! deterministic `RenderOptions::time_ms` supplied by the caller.

use megi_render::{RenderOptions, Viewport, render_url};

const PAGE: &str = r#"<!doctype html><html><head><style>
html, body { margin: 0; background: #ffffff; }
#box { width: 100px; height: 100px; background: #ff0000;
       animation: fade 1000ms linear both; }
@keyframes fade { from { opacity: 0 } to { opacity: 1 } }
</style></head><body><div id="box"></div></body></html>"#;

fn render_at(time_ms: f64) -> megi_render::RenderOutput {
    let dir = std::env::temp_dir().join(format!(
        "megi-animation-{}-{}",
        std::process::id(),
        time_ms as u64
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("index.html");
    std::fs::write(&path, PAGE).expect("write page");
    let options = RenderOptions {
        viewport: Viewport::new(100, 100).expect("viewport"),
        time_ms,
        ..RenderOptions::default()
    };
    let output = render_url(&format!("file://{}", path.display()), &options).expect("render");
    let _ = std::fs::remove_dir_all(&dir);
    output
}

fn center_pixel(output: &megi_render::RenderOutput) -> megi_css::Color {
    output.pixmap.pixels[(50 * 100 + 50) as usize]
}

#[test]
fn animation_frames_render_at_the_requested_time() {
    let start = center_pixel(&render_at(0.0));
    assert!(
        start.r > 240 && start.g > 240 && start.b > 240,
        "opacity 0 at t=0 shows the white page: {start:?}"
    );

    let middle = center_pixel(&render_at(500.0));
    assert!(
        middle.r > 240 && (80..=180).contains(&middle.g) && (80..=180).contains(&middle.b),
        "opacity 0.5 at t=500 blends red over white: {middle:?}"
    );

    let end = center_pixel(&render_at(1000.0));
    assert!(
        end.r > 240 && end.g < 10 && end.b < 10,
        "opacity 1 at t=1000 is opaque red: {end:?}"
    );
}

#[test]
fn animations_without_keyframes_leave_the_page_unchanged() {
    // No `@keyframes` rule: the animation-name resolves to nothing and the box stays red.
    let html = PAGE.replace(
        "@keyframes fade { from { opacity: 0 } to { opacity: 1 } }",
        "",
    );
    let dir = std::env::temp_dir().join(format!("megi-animation-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("index.html");
    std::fs::write(&path, html).expect("write page");
    let options = RenderOptions {
        viewport: Viewport::new(100, 100).expect("viewport"),
        time_ms: 500.0,
        ..RenderOptions::default()
    };
    let output = render_url(&format!("file://{}", path.display()), &options).expect("render");
    let _ = std::fs::remove_dir_all(&dir);
    let pixel = center_pixel(&output);
    assert!(pixel.r > 240 && pixel.g < 10 && pixel.b < 10, "{pixel:?}");
}

#[cfg(feature = "js")]
#[test]
fn script_driven_css_transitions_interpolate_and_fire_events() {
    let html = r#"<!doctype html><html><head><style>
html, body { margin: 0; background: #ffffff; }
#box {
    width: 100px;
    height: 100px;
    background: #ff0000;
    opacity: 0;
    transition: opacity 1000ms linear;
}
#box.visible {
    opacity: 1;
}
</style></head><body><div id="box"></div><div id="log"></div><script>
const box = document.getElementById('box');
const log = document.getElementById('log');
const seen = [];
for (const type of ['transitionrun', 'transitionstart', 'transitionend']) {
    box.addEventListener(type, (e) => {
        seen.push(type + ':' + e.propertyName + ':' + e.elapsedTime);
        log.textContent = seen.join(',');
    });
}
box.className = 'visible';
</script></body></html>"#;

    let dir = std::env::temp_dir().join(format!("megi-transition-js-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("index.html");
    std::fs::write(&path, html).expect("write page");
    let url = format!("file://{}", path.display());

    let run = |time_ms: f64| {
        render_url(
            &url,
            &RenderOptions {
                viewport: Viewport::new(100, 100).expect("viewport"),
                time_ms,
                scripting: megi_render::ScriptingMode::Enabled,
                ..RenderOptions::default()
            },
        )
        .expect("render")
    };

    let t0 = run(0.0);
    let p0 = center_pixel(&t0);
    assert!(
        p0.r > 240 && p0.g > 240 && p0.b > 240,
        "at t=0 transition keeps initial opacity 0: {p0:?}"
    );
    let log0 = t0
        .document
        .get_elements_by_tag_name("div")
        .into_iter()
        .find(|&id| {
            t0.document
                .node(id)
                .as_element()
                .and_then(|e| e.get_attribute("id"))
                == Some("log")
        })
        .map(|id| t0.document.text_content(id))
        .unwrap_or_default();
    assert_eq!(log0, "transitionrun:opacity:0,transitionstart:opacity:0");

    let t_mid = run(500.0);
    let p_mid = center_pixel(&t_mid);
    assert!(
        p_mid.r > 240 && (80..=180).contains(&p_mid.g) && (80..=180).contains(&p_mid.b),
        "at t=500ms transition blends opacity 0.5 over white: {p_mid:?}"
    );

    let t_end = run(1000.0);
    let p_end = center_pixel(&t_end);
    assert!(
        p_end.r > 240 && p_end.g < 10 && p_end.b < 10,
        "at t=1000ms transition reaches final opacity 1: {p_end:?}"
    );
    let log_end = t_end
        .document
        .get_elements_by_tag_name("div")
        .into_iter()
        .find(|&id| {
            t_end
                .document
                .node(id)
                .as_element()
                .and_then(|e| e.get_attribute("id"))
                == Some("log")
        })
        .map(|id| t_end.document.text_content(id))
        .unwrap_or_default();
    assert_eq!(
        log_end,
        "transitionrun:opacity:0,transitionstart:opacity:0,transitionend:opacity:1"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "js")]
#[test]
fn two_frame_host_transition_and_animation_events_end_to_end() {
    let dir = std::env::temp_dir().join(format!("megi-anim-events-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");

    let frame1_path = dir.join("frame1.html");
    let frame2_path = dir.join("frame2.html");
    std::fs::write(
        &frame1_path,
        r#"<!doctype html><html><head><style>
html, body { margin: 0; background: #ffffff; }
#box { width: 100px; height: 100px; background: #0000ff; opacity: 0; transition: opacity 1000ms linear; }
</style></head><body><div id="box"></div></body></html>"#,
    )
    .expect("write frame1");
    std::fs::write(
        &frame2_path,
        r#"<!doctype html><html><head><style>
html, body { margin: 0; background: #ffffff; }
#box { width: 100px; height: 100px; background: #0000ff; opacity: 1; transition: opacity 1000ms linear; }
</style></head><body><div id="box"></div></body></html>"#,
    )
    .expect("write frame2");

    let frame1 = render_url(
        &format!("file://{}", frame1_path.display()),
        &RenderOptions {
            viewport: Viewport::new(100, 100).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render frame1");

    let frame2_mid = render_url(
        &format!("file://{}", frame2_path.display()),
        &RenderOptions {
            viewport: Viewport::new(100, 100).expect("viewport"),
            time_ms: 500.0,
            transition_from_styles: Some(std::sync::Arc::new(frame1.styles)),
            ..RenderOptions::default()
        },
    )
    .expect("render frame2");
    let pixel = center_pixel(&frame2_mid);
    assert!(
        pixel.b > 240 && (80..=180).contains(&pixel.r) && (80..=180).contains(&pixel.g),
        "two-frame transition interpolates blue over white at t=500ms: {pixel:?}"
    );

    let anim_path = dir.join("anim.html");
    std::fs::write(
        &anim_path,
        r#"<!doctype html><html><head><style>
html, body { margin: 0; background: #ffffff; }
#target {
    width: 100px;
    height: 100px;
    background: #ff0000;
    animation: pulse 400ms linear 2 both;
}
@keyframes pulse { from { opacity: 0 } to { opacity: 1 } }
</style></head><body><div id="target"></div><p id="status"></p><script>
const t = document.getElementById('target');
const s = document.getElementById('status');
const events = [];
t.onanimationstart = (e) => events.push('start:' + e.animationName + ':' + e.elapsedTime);
t.onanimationiteration = (e) => events.push('iter:' + e.animationName + ':' + e.elapsedTime);
t.addEventListener('animationend', (e) => {
    events.push('end:' + e.animationName + ':' + e.elapsedTime);
    t.style.backgroundColor = '#00ff00';
    s.textContent = events.join('|');
});
</script></body></html>"#,
    )
    .expect("write anim");
    let anim_out = render_url(
        &format!("file://{}", anim_path.display()),
        &RenderOptions {
            viewport: Viewport::new(100, 100).expect("viewport"),
            time_ms: 800.0,
            scripting: megi_render::ScriptingMode::Enabled,
            ..RenderOptions::default()
        },
    )
    .expect("render anim");

    let status_text = anim_out
        .document
        .get_elements_by_tag_name("p")
        .into_iter()
        .map(|id| anim_out.document.text_content(id))
        .collect::<Vec<_>>()
        .join("");
    assert_eq!(status_text, "start:pulse:0|iter:pulse:0.4|end:pulse:0.8");
    let anim_pixel = center_pixel(&anim_out);
    assert!(
        anim_pixel.g > 240 && anim_pixel.r < 10 && anim_pixel.b < 10,
        "animationend handler painted box green: {anim_pixel:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
