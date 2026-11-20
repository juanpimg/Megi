//! Byte determinism, CLI output, stats consistency and miscellaneous regressions.

use super::common::*;

#[test]
fn renders_are_byte_deterministic() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/style.css></head>\
             <body><h1>Determinism</h1><p>real fonts</p></body></html>",
        ),
    );
    routes.insert(
        "/style.css".to_string(),
        RouteResponse::css("html{background:#101820} h1{color:#f2aa4c} p{color:#fff}"),
    );
    let server = TestServer::spawn(routes, 4);
    let first = render_url(&server.url("/page.html"), &options(320, 200)).expect("first");
    let second = render_url(&server.url("/page.html"), &options(320, 200)).expect("second");
    assert_eq!(first.png, second.png, "two renders must be byte-identical");
    assert_eq!(first.stats.display_items, second.stats.display_items);
    server.join();
}

#[test]
fn cli_renders_data_url_and_prints_json() {
    let unique = format!(
        "megi-render-cli-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    );
    let output_path = std::env::temp_dir().join(unique);
    let command = std::process::Command::new(env!("CARGO_BIN_EXE_render"))
        .arg("data:text/html,<html><body style=\"background:red\"><h1>CLI</h1></body></html>")
        .arg(&output_path)
        .args(["--width", "160", "--height", "90", "--json"])
        .output()
        .expect("run render CLI");
    let stdout = String::from_utf8_lossy(&command.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&command.stderr).into_owned();
    assert!(
        command.status.success(),
        "CLI failed: status={:?} stderr={stderr}",
        command.status
    );
    assert!(
        stdout.contains("\"width\":160") && stdout.contains("\"height\":90"),
        "unexpected --json output: {stdout}"
    );
    let png = std::fs::read(&output_path).expect("CLI must write the PNG");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "output must be a PNG");
    let _ = std::fs::remove_file(&output_path);
}

#[test]
fn output_stats_are_consistent() {
    let mut routes = HashMap::new();
    routes.insert(
        "/stats.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><style>p{color:red}</style></head>\
             <body><p>stats</p></body></html>",
        ),
    );
    let server = TestServer::spawn(routes, 1);
    let output = render_url(&server.url("/stats.html"), &options(200, 100)).expect("render");
    assert_render_output_consistency(&output);
    server.join();
}

/// Regression: only `<style>`/`<link>` sources have a meaningful `media`
/// attribute. Evaluating media on every element wrongly counted and warned
/// about unrelated elements such as `<meta media=...>`.
#[test]
fn non_stylesheet_media_attributes_are_ignored() {
    let url = "data:text/html,<html><head><meta media=\"(prefers-color-scheme: dark)\">\
               <style>p{color:red}</style></head><body><p>x</p></body></html>";
    let output = render_url(url, &options(100, 100)).expect("render");
    assert_eq!(output.stats.media_skipped, 0);
    assert!(
        output.stats.warnings.iter().all(|w| !w.contains("<meta")),
        "unexpected metadata warning: {:?}",
        output.stats.warnings
    );
}

#[test]
fn oversized_inline_style_in_render_pipeline_does_not_panic() {
    let prefix = "background-color: rgb(0, 180, 0); width: 80px; height: 80px; ";
    let mut style = String::from(prefix);
    style.push_str(&" ".repeat(megi_css::MAX_INLINE_STYLE_BYTES - prefix.len() - 1));
    style.push('🦀');
    style.push_str("; background-color: rgb(255, 0, 0);");
    assert!(!style.is_char_boundary(megi_css::MAX_INLINE_STYLE_BYTES));

    let html = format!(
        "<!doctype html><html><body style=\"margin:0\"><div style=\"{style}\"></div></body></html>"
    );
    let mut routes = HashMap::new();
    routes.insert("/inline_cap.html".to_string(), RouteResponse::html(&html));
    let server = TestServer::spawn(routes, 1);
    let output = render_url(&server.url("/inline_cap.html"), &options(100, 100)).expect("render");
    assert_render_output_consistency(&output);
    let green_pixels = output
        .pixmap
        .pixels
        .iter()
        .filter(|px| px.g > 150 && px.r < 40 && px.b < 40)
        .count();
    assert!(
        green_pixels > 5000,
        "expected oversized inline style prefix declarations to paint green box, got {green_pixels}"
    );
    server.join();
}

#[test]
fn centered_nested_table_is_not_a_blank_page() {
    // F3-TABLE-CENTER / #1011: HN-style `<center><table>` must produce real display items.
    let mut routes = HashMap::new();
    routes.insert(
        "/hn.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><body><center><table id='hnmain' \
             width='85%'><tr><td><b>Story title</b><p>42 points by user</td></tr>\
             </table></center></body></html>",
        ),
    );
    let server = TestServer::spawn(routes, 1);
    let output = render_url(&server.url("/hn.html"), &options(1024, 768)).expect("render HN page");
    assert!(
        output.stats.display_items > 0,
        "a centered nested table must not collapse to a blank page"
    );
    // The story text must actually be painted, not just the table background.
    let text_items = output
        .display_list
        .items
        .iter()
        .filter(|item| matches!(item, megi_display::DisplayItem::Text { .. }))
        .count();
    assert!(
        text_items > 0,
        "story text must be painted ({text_items} items)"
    );
    server.join();
}
