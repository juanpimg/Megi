use super::common::*;

#[test]
fn navigation_home_target_is_the_first_entry() {
    let mut nav = megi_browser::NavigationState::new();
    nav.navigate_to(megi_webview::net::Url::parse("file:///home/index.html").expect("url"));
    nav.navigate_to(megi_webview::net::Url::parse("https://example.com/page").expect("url"));
    assert_eq!(
        nav.home_target().map(megi_webview::net::Url::as_str),
        Some("file:///home/index.html".to_string())
    );
    assert_eq!(
        nav.current_url().as_deref(),
        Some("https://example.com/page")
    );
}

#[test]
fn screenshot_with_tabs_renders_the_active_first_tab() {
    let first = "data:text/html,%3Ch1%3EFirst%20tab%3C%2Fh1%3E";
    let second = "data:text/html,%3Ch1%3ESecond%20tab%3C%2Fh1%3E";
    let tabs_png = temp_path("tabs-active.png");
    let direct_png = temp_path("tabs-direct.png");
    let tabs_run = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .args([
            first,
            "--tabs",
            second,
            "--screenshot",
            tabs_png.to_str().expect("path"),
            "--width",
            "240",
            "--height",
            "160",
        ])
        .status()
        .expect("shell binary runs");
    assert!(tabs_run.success());
    let direct_run = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .args([
            first,
            "--screenshot",
            direct_png.to_str().expect("path"),
            "--width",
            "240",
            "--height",
            "160",
        ])
        .status()
        .expect("shell binary runs");
    assert!(direct_run.success());
    let tabs_bytes = std::fs::read(&tabs_png).expect("tabs PNG");
    let direct_bytes = std::fs::read(&direct_png).expect("direct PNG");
    assert_eq!(
        tabs_bytes, direct_bytes,
        "headless renders the active/first tab"
    );
    assert_eq!(png_dimensions(&tabs_bytes), (240, 160));
    let _ = std::fs::remove_file(&tabs_png);
    let _ = std::fs::remove_file(&direct_png);
}

#[test]
fn cli_tabs_flag_builds_multiple_tabs_and_documents_headless_policy() {
    let cli = megi_browser::parse_cli(
        [
            "data:text/html,one",
            "--tabs",
            "data:text/html,two",
            "--tabs",
            "data:text/html,three",
        ]
        .into_iter()
        .map(str::to_string),
    )
    .expect("parse CLI");
    assert_eq!(cli.tabs.len(), 3);
    assert_eq!(cli.input, "data:text/html,one");
    let state = megi_browser::TabsState::from_urls(cli.tabs.clone());
    assert_eq!(state.len(), 3);
    assert_eq!(state.active_index(), 0);
    assert_eq!(state.active().url, "data:text/html,one");
    let (output, hits) = megi_browser::headless::render_target_with_find(
        &cli.input,
        240,
        160,
        &megi_webview::render::RenderOptions::default(),
        "",
    )
    .expect("render active tab");
    assert_eq!(output.dimensions(), (240, 160));
    assert_eq!(hits, 0);
}

#[test]
fn chrome_overlay_paints_tab_strip_and_address_bar() {
    let plain =
        megi_browser::render_target("data:text/html,<h1>chrome</h1>", 640, 400).expect("plain");
    let chromed = megi_browser::headless::render_target_with_chrome(
        "data:text/html,<h1>chrome</h1>",
        640,
        400,
        &megi_webview::render::RenderOptions::default(),
        None,
    )
    .expect("chrome");
    assert_eq!(
        plain.dimensions(),
        chromed.dimensions(),
        "chrome is an overlay"
    );
    let tab_strip = megi_browser::chrome::COLOR_TAB_STRIP;
    let address = megi_browser::chrome::COLOR_ADDRESS_BG;
    let count = |output: &megi_webview::render::RenderOutput, color: megi_webview::paint::Color| {
        output
            .pixmap
            .pixels
            .iter()
            .filter(|pixel| pixel.r == color.r && pixel.g == color.g && pixel.b == color.b)
            .count()
    };
    assert_eq!(count(&plain, tab_strip), 0, "plain renders have no chrome");
    assert!(count(&chromed, tab_strip) > 1000, "tab strip painted");
    assert!(count(&chromed, address) > 1000, "address bar painted");
    // This fixture has no content in the sampled row: the chrome band no longer
    // covers page content, so the row below it is plain page background.
    let offset = ((megi_browser::chrome::CHROME_HEIGHT as u32 + 10) * 640) as usize;
    assert_eq!(
        plain.pixmap.pixels[offset..offset + 100],
        chromed.pixmap.pixels[offset..offset + 100]
    );
}

#[test]
fn screenshot_with_chrome_writes_a_valid_png() {
    let plain_path = temp_path("chrome-plain.png");
    let chrome_path = temp_path("chrome-on.png");
    let input = "data:text/html,<h1>chrome png</h1>";
    megi_browser::headless::screenshot_with_options(
        input,
        320,
        220,
        &plain_path,
        &megi_webview::render::RenderOptions::default(),
    )
    .expect("plain screenshot");
    megi_browser::headless::screenshot_with_chrome(
        input,
        320,
        220,
        &chrome_path,
        &megi_webview::render::RenderOptions::default(),
        None,
    )
    .expect("chrome screenshot");
    let plain = std::fs::read(&plain_path).expect("plain PNG");
    let chromed = std::fs::read(&chrome_path).expect("chrome PNG");
    assert_eq!(png_dimensions(&plain), (320, 220));
    assert_eq!(png_dimensions(&chromed), (320, 220));
    assert_ne!(plain, chromed, "chrome changes the screenshot");
    assert!(chromed.starts_with(&[0x89, b'P', b'N', b'G']));
    let _ = std::fs::remove_file(&plain_path);
    let _ = std::fs::remove_file(&chrome_path);
}

#[test]
fn cli_chrome_flag_is_accepted_for_screenshots() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    let plain = temp_path("cli-plain.png");
    let chromed = temp_path("cli-chrome.png");
    let run = |args: &[&str], path: &PathBuf| {
        std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
            .arg(&fixture)
            .args(args)
            .arg("--screenshot")
            .arg(path)
            .args(["--width", "320", "--height", "220"])
            .status()
            .expect("shell binary runs")
    };
    assert!(run(&[], &plain).success());
    assert!(run(&["--chrome"], &chromed).success());
    let plain_bytes = std::fs::read(&plain).expect("plain PNG");
    let chrome_bytes = std::fs::read(&chromed).expect("chrome PNG");
    assert_eq!(png_dimensions(&plain_bytes), (320, 220));
    assert_eq!(png_dimensions(&chrome_bytes), (320, 220));
    assert_ne!(plain_bytes, chrome_bytes);
    let _ = std::fs::remove_file(&plain);
    let _ = std::fs::remove_file(&chromed);
}

#[test]
fn select_options_survive_a_patched_rerender() {
    let request = RenderRequest {
        generation: 1,
        input: forms_fixture_url(),
        css_width: 320,
        css_height: 240,
        raster_scale: 1.0,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let frame = megi_browser::worker::render_document(&request).expect("forms render");
    let select = frame
        .forms
        .iter()
        .find(|control| control.kind == megi_browser::FormControlKind::Select)
        .cloned()
        .expect("select control");
    assert_eq!(select.options.len(), 2, "fixture select has two options");
    assert_eq!(select.options[0].label, "One");
    let mut document = (*frame.document).clone();
    megi_browser::forms::assign_form_ids(&mut document);
    assert!(megi_browser::forms::patch_select(
        &mut document,
        select.form_id,
        1
    ));
    let snapshot = megi_browser::forms::snapshot_data_url(&document);
    let patched_request = RenderRequest {
        generation: 2,
        input: snapshot,
        css_width: 320,
        css_height: 240,
        raster_scale: 1.0,
        base_override: Some(request.input.clone()),
        scripting: false,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let patched = megi_browser::worker::render_document(&patched_request).expect("patched render");
    let patched_select = patched
        .forms
        .iter()
        .find(|control| control.form_id == select.form_id)
        .expect("select persisted");
    assert!(patched_select.options[1].selected, "second option selected");
    assert_eq!(patched_select.value, "Two");
}

#[test]
fn chrome_geometry_scales_with_the_device_scale() {
    let address = megi_browser::chrome::AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let view = megi_browser::chrome::ChromeView {
        favicons: &[],
        tabs: &tabs,
        active_tab: 0,
        address: &address,
        fallback_url: "about:blank",
        dropdown: None,
        scroll_y: 0,
        css_viewport_height: 200,
        css_width: 320.0,
        completion: None,
        loading: false,
        overflow: None,
        hover_url: None,
        menu: None,
        can_go_back: false,
        can_go_forward: false,
        history_menu: None,
        download_status: None,
        history_button: false,
        ime_preedit: "",
        compact: false,
        rail: None,
    };
    let mut dest = vec![0x00FF_FFFFu32; 640 * 400];
    let painted = megi_browser::chrome::draw_chrome(&mut dest, 640, 400, 2.0, &view);
    assert!(painted > 0);
    // The refreshed chrome band is 78 CSS px ⇒ 156 physical rows at scale 2.
    // (Old band: 58 CSS px / 116 rows; updated for the deliberate geometry change.)
    let band = dest[..(156 * 640) as usize]
        .iter()
        .filter(|pixel| **pixel != 0x00FF_FFFF)
        .count();
    assert!(band > 1000, "scaled chrome painted: {band}");
    // A three-row soft shadow (6 physical rows at scale 2) now sits below the
    // band; the page starts clean only after it.
    assert_ne!(
        dest[(158 * 640) as usize],
        0x00FF_FFFF,
        "shadow under the band"
    );
    assert_eq!(
        dest[(164 * 640) as usize],
        0x00FF_FFFF,
        "page below chrome and shadow intact"
    );
}

#[test]
fn single_workspace_chrome_render_is_byte_identical_to_legacy_inputs() {
    // Phase 3 red test: the expected failure before the workspace API exists is
    // `no method named visible_tabs found for struct TabsState`. No PNG assertion is
    // changed: this compares two in-memory paints of the same 78 px chrome band.
    use megi_browser::tabs::TabsState;
    let tabs = TabsState::from_urls(vec![
        "https://example.com/one".to_string(),
        "https://example.com/two".to_string(),
    ]);
    assert_eq!(tabs.workspace(), 0);
    assert_eq!(tabs.workspace_status_text(), None);
    let legacy_titles: Vec<String> = tabs.rail_labels().0;
    let visible_titles: Vec<String> = tabs.visible_tabs().map(|tab| tab.display_title()).collect();
    assert_eq!(visible_titles, legacy_titles);
    assert_eq!(tabs.visible_active_index(), tabs.active_index());

    let address = megi_browser::chrome::AddressBar::new();
    fn view<'a>(
        titles: &'a [String],
        active: usize,
        address: &'a megi_browser::chrome::AddressBar,
    ) -> megi_browser::chrome::ChromeView<'a> {
        megi_browser::chrome::ChromeView {
            favicons: &[],
            tabs: titles,
            active_tab: active,
            address,
            fallback_url: "about:blank",
            dropdown: None,
            scroll_y: 0,
            css_viewport_height: 200,
            css_width: 320.0,
            completion: None,
            loading: false,
            overflow: None,
            hover_url: None,
            menu: None,
            can_go_back: false,
            can_go_forward: false,
            history_menu: None,
            download_status: None,
            history_button: false,
            ime_preedit: "",
            compact: false,
            rail: None,
        }
    }
    let legacy = view(&legacy_titles, tabs.active_index(), &address);
    let visible = view(&visible_titles, tabs.visible_active_index(), &address);
    let mut legacy_pixels = vec![0x00FF_FFFFu32; 640 * 400];
    let mut visible_pixels = vec![0x00FF_FFFFu32; 640 * 400];
    let legacy_painted =
        megi_browser::chrome::draw_chrome(&mut legacy_pixels, 640, 400, 2.0, &legacy);
    let visible_painted =
        megi_browser::chrome::draw_chrome(&mut visible_pixels, 640, 400, 2.0, &visible);
    assert_eq!(legacy_painted, visible_painted);
    assert_eq!(
        legacy_pixels, visible_pixels,
        "one workspace paints identical bytes"
    );
}

#[test]
fn ipc_render_renders_the_selected_tab_from_the_process_backend() {
    // F7-ISOLATION E2E: a multi-tab headless render through the real renderer child must
    // select the same tab as the in-process path.
    let first = "data:text/html,%3Ch1%3EFirst%20tab%3C%2Fh1%3E";
    let second = "data:text/html,%3Ch1%3ESecond%20tab%3C%2Fh1%3E";
    let process_png = temp_path("tabs-ipc.png");
    let direct_png = temp_path("tabs-ipc-direct.png");

    let process_run = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .args([
            first,
            "--tabs",
            second,
            "--active-tab",
            "1",
            "--ipc-render",
            "--screenshot",
            process_png.to_str().expect("path"),
            "--width",
            "240",
            "--height",
            "160",
        ])
        .status()
        .expect("shell binary runs");
    assert!(process_run.success(), "process render must succeed");
    let direct_run = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .args([
            second,
            "--screenshot",
            direct_png.to_str().expect("path"),
            "--width",
            "240",
            "--height",
            "160",
        ])
        .status()
        .expect("shell binary runs");
    assert!(direct_run.success());

    let process_bytes = std::fs::read(&process_png).expect("process PNG");
    let direct_bytes = std::fs::read(&direct_png).expect("direct PNG");
    assert_eq!(
        process_bytes, direct_bytes,
        "the process backend must render the selected (second) tab"
    );
    assert_eq!(png_dimensions(&process_bytes), (240, 160));
    let _ = std::fs::remove_file(&process_png);
    let _ = std::fs::remove_file(&direct_png);
}

#[test]
fn ipc_render_rejects_flags_that_need_the_in_process_renderer() {
    // The process backend cannot compose chrome/find overlays (they need the parent's
    // display list); it must say so instead of silently returning a different image.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .args([
            "data:text/html,%3Ch1%3Ehi%3C%2Fh1%3E",
            "--screenshot",
            temp_path("ipc-chrome.png").to_str().expect("path"),
            "--ipc-render",
            "--chrome",
        ])
        .output()
        .expect("shell binary runs");
    assert!(
        !output.status.success(),
        "--ipc-render --chrome must fail with a clear error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ipc-render") && stderr.contains("chrome"),
        "error must explain the incompatible flags: {stderr}"
    );
}
