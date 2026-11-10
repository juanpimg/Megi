//! Headless integration tests for the `shell` crate.
//!
//! These tests never create a winit `EventLoop` and never need a display server: they
//! exercise the pure CLI logic, the pixmap→framebuffer conversion and the headless
//! `--screenshot` path (the exact path CI uses).

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use paint::Color;
use shell::{
    LaunchMode, PIXEL_FORMAT, ShellError, expected_buffer_len, parse_args, parse_cli,
    pixmap_to_0rgb, render_target, screenshot,
};

fn fixture_url() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    format!("file://{}", path.display())
}

fn temp_path(stem: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("megi-shell-{}-{unique}-{stem}", std::process::id()))
}

fn args(values: &[&str]) -> impl Iterator<Item = String> {
    values.iter().map(|value| (*value).to_string())
}

fn png_dimensions(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    let width = u32::from_be_bytes(png[16..20].try_into().expect("width"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("height"));
    (width, height)
}

#[test]
fn cli_parses_window_and_screenshot_modes() {
    let window = parse_cli(args(&["data:text/html,<p>x</p>", "--width", "640"]))
        .expect("window mode parses");
    assert_eq!(window.mode, LaunchMode::Window);
    assert_eq!((window.width, window.height), (640, 600));
    assert!(window.input.starts_with("data:"));

    let shot = parse_cli(args(&[
        "data:text/html,<p>x</p>",
        "--screenshot",
        "/tmp/x.png",
        "--height",
        "200",
        "--title",
        "Test",
    ]))
    .expect("screenshot mode parses");
    assert_eq!(
        shot.mode,
        LaunchMode::Screenshot(PathBuf::from("/tmp/x.png"))
    );
    assert_eq!(shot.title, "Test");
    assert_eq!((shot.width, shot.height), (800, 200));
}

#[test]
fn cli_rejects_usage_errors() {
    assert!(parse_cli(args(&[])).is_err());
    assert!(parse_cli(args(&["data:,x", "--width", "0"])).is_err());
    assert!(parse_cli(args(&["data:,x", "--nope"])).is_err());
    let raw = parse_args(args(&["--help"])).expect("raw args");
    assert!(raw.help);
}

#[test]
fn pixel_conversion_is_0rgb() {
    let mut pixmap = paint::Pixmap::new(2, 2);
    pixmap.set_pixel(0, 0, Color::new(0xAB, 0xCD, 0xEF, 0xFF));
    pixmap.set_pixel(1, 1, Color::WHITE);
    let frame = pixmap_to_0rgb(&pixmap);
    assert_eq!(PIXEL_FORMAT, "0RGB");
    assert_eq!(frame.len(), expected_buffer_len(2, 2).expect("len"));
    assert_eq!(frame[0], 0x00AB_CDEF);
    assert_eq!(frame[3], 0x00FF_FFFF);
}

#[test]
fn headless_screenshot_of_fixture_writes_a_valid_png() {
    let path = temp_path("fixture.png");
    let info = screenshot(&fixture_url(), 320, 200, &path).expect("screenshot");
    assert_eq!((info.width, info.height), (320, 200));
    let png = std::fs::read(&info.path).expect("read screenshot");
    assert_eq!(png.len(), info.png_bytes);
    assert_eq!(png_dimensions(&png), (320, 200));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn headless_screenshot_is_deterministic() {
    let first_path = temp_path("det1.png");
    let second_path = temp_path("det2.png");
    screenshot(&fixture_url(), 256, 160, &first_path).expect("first");
    screenshot(&fixture_url(), 256, 160, &second_path).expect("second");
    let first = std::fs::read(&first_path).expect("read first");
    let second = std::fs::read(&second_path).expect("read second");
    assert_eq!(first, second, "two headless renders must be byte-identical");
    let _ = std::fs::remove_file(&first_path);
    let _ = std::fs::remove_file(&second_path);
}

#[test]
fn headless_screenshot_works_for_data_urls() {
    let path = temp_path("data.png");
    let info = screenshot(
        "data:text/html,<html><body style=\"background:red\"><h1>Hi</h1></body></html>",
        120,
        80,
        &path,
    )
    .expect("data URL screenshot");
    assert_eq!((info.width, info.height), (120, 80));
    let png = std::fs::read(&info.path).expect("read");
    assert_eq!(png_dimensions(&png), (120, 80));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn render_errors_are_typed() {
    let error = render_target("not a url", 100, 100).expect_err("bad URL");
    assert!(matches!(error, ShellError::Render(_)), "{error:?}");

    let error = render_target("data:text/html,<p>x</p>", 0, 100).expect_err("bad viewport");
    assert!(matches!(error, ShellError::Render(_)), "{error:?}");

    let base = temp_path("write-error.png");
    let error = screenshot(&fixture_url(), 100, 100, base.join("nested/missing.png"))
        .expect_err("unwritable path");
    assert!(matches!(error, ShellError::Io { .. }), "{error:?}");
}

#[test]
fn cli_binary_renders_headless_without_a_display() {
    let path = temp_path("cli.png");
    let command = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .arg(fixture_url())
        .args(["--screenshot", path.to_str().expect("utf-8 path")])
        .args(["--width", "300", "--height", "180", "--title", "CI"])
        .output()
        .expect("run megi CLI");
    assert!(
        command.status.success(),
        "headless CLI failed: {}\n{}",
        String::from_utf8_lossy(&command.stdout),
        String::from_utf8_lossy(&command.stderr)
    );
    let png = std::fs::read(&path).expect("CLI screenshot exists");
    assert_eq!(png_dimensions(&png), (300, 180));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_help_exits_successfully() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .arg("--help")
        .output()
        .expect("run megi --help");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("usage: megi"));
}
