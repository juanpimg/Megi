pub(crate) use std::path::PathBuf;
pub(crate) use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) use megi_browser::worker::{Debounce, FrameStore, RenderRequest, spawn_worker};
pub(crate) use megi_browser::{
    LaunchMode, MAX_CONTENT_HEIGHT, PIXEL_FORMAT, ScrollState, ShellError, WorkerMessage,
    expected_buffer_len, logical_to_physical, parse_args, parse_cli, physical_to_logical,
    pixmap_to_0rgb, present_region, render_target, screenshot, viewport_slice, window_metrics,
};
pub(crate) use megi_webview::paint::Color;

pub(crate) fn fixture_url() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    format!("file://{}", path.display())
}

pub(crate) fn temp_path(stem: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("megi-shell-{}-{unique}-{stem}", std::process::id()))
}

pub(crate) fn args(values: &[&str]) -> impl Iterator<Item = String> {
    values.iter().map(|value| (*value).to_string())
}

pub(crate) fn png_dimensions(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    let width = u32::from_be_bytes(png[16..20].try_into().expect("width"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("height"));
    (width, height)
}

pub(crate) fn forms_fixture_url() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/forms.html");
    megi_browser::cli::normalize_input(&path.display().to_string()).expect("forms fixture URL")
}

pub(crate) fn find_fixture_url() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/find.html");
    megi_browser::cli::normalize_input(&path.display().to_string()).expect("find fixture URL")
}
