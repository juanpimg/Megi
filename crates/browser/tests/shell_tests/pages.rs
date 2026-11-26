use super::common::*;

#[test]
fn worker_frame_exposes_image_regions_for_the_context_menu() {
    let request = RenderRequest {
        generation: 1,
        input: "data:text/html,<img src='data:image/png;base64,iVBORw0KGgo=' width=20 height=20>"
            .to_string(),
        css_width: 120,
        css_height: 80,
        raster_scale: 1.0,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let frame = megi_browser::worker::render_document(&request).expect("render");
    assert!(!frame.images.is_empty(), "img region extracted");
    assert!(frame.images[0].is_usable());
    assert!(frame.images[0].rect.width > 0.0);
}

#[test]
fn headless_chrome_context_menu_is_deterministic() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render/basic.html");
    let first = temp_path("r182-menu-a.png");
    let second = temp_path("r182-menu-b.png");
    let run = |path: &PathBuf| {
        std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
            .arg(&fixture)
            .args(["--chrome", "--chrome-menu", "--screenshot"])
            .arg(path)
            .args(["--width", "640", "--height", "400"])
            .status()
            .expect("shell binary runs")
    };
    assert!(run(&first).success());
    assert!(run(&second).success());
    let a = std::fs::read(&first).expect("png");
    let b = std::fs::read(&second).expect("png");
    assert_eq!(a, b, "menu screenshots deterministic");
    assert_eq!(png_dimensions(&a), (640, 400));
    assert!(a.starts_with(&[0x89, b'P', b'N', b'G']));
    assert!(a.len() > 1000);
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
}

#[test]
fn headless_directory_listing_renders_names_and_is_deterministic() {
    let dir = temp_path("dir-listing");
    std::fs::create_dir_all(dir.join("sub dir")).expect("mkdir");
    std::fs::write(dir.join("alpha.txt"), "hello").expect("write alpha");
    std::fs::write(dir.join("sub dir").join("inside.html"), "<p>x</p>").expect("write inside");
    let url = format!("file://{}", dir.display());
    let first = render_target(&url, 320, 200).expect("directory listing render");
    let text = first
        .document
        .text_content(first.document.document_element().expect("document element"));
    assert!(
        text.contains("Index of") || text.contains("Índice de"),
        "localized listing header: {text}"
    );
    assert!(text.contains("alpha.txt"), "file name in listing: {text}");
    assert!(
        text.contains("sub dir"),
        "directory name in listing: {text}"
    );
    assert!(!text.contains("Reintentar"));
    assert!(!first.png.is_empty(), "listing produces PNG bytes");
    let second = render_target(&url, 320, 200).expect("second listing render");
    assert_eq!(first.png, second.png, "listing PNG is deterministic");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_missing_file_renders_a_deterministic_error_page() {
    let url = "file:///nonexistent-megi-r189/page.html";
    let first = render_target(url, 320, 200).expect("error page render");
    let text = first
        .document
        .text_content(first.document.document_element().expect("document element"));
    assert!(
        text.contains("Error loading page") || text.contains("Error al cargar la página"),
        "localized error title: {text}"
    );
    assert!(text.contains("Reintentar"), "retry link: {text}");
    assert!(text.contains(url), "failed URL shown: {text}");
    let second = render_target(url, 320, 200).expect("second error render");
    assert_eq!(first.png, second.png, "error page PNG is deterministic");
}

#[test]
fn headless_chrome_draws_a_deterministic_favicon_dot() {
    let options = megi_webview::render::RenderOptions::default();
    let first = megi_browser::headless::render_target_with_chrome(
        "data:text/html,<h1>favicon dot</h1>",
        320,
        220,
        &options,
        None,
    )
    .expect("chrome render");
    let second = megi_browser::headless::render_target_with_chrome(
        "data:text/html,<h1>favicon dot</h1>",
        320,
        220,
        &options,
        None,
    )
    .expect("second chrome render");
    let first_png = first.pixmap.to_png();
    let second_png = second.pixmap.to_png();
    assert_eq!(first_png, second_png, "chrome + favicon dot deterministic");
    let plain = render_target("data:text/html,<h1>favicon dot</h1>", 320, 220).expect("plain");
    assert_ne!(plain.png, first_png, "chrome changes the screenshot");
}

#[test]
fn headless_directory_screenshot_cli_writes_a_valid_png() {
    let dir = temp_path("dir-cli");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("report.txt"), "data").expect("write");
    let png = temp_path("dir-cli.png");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_megi"))
        .arg(format!("file://{}", dir.display()))
        .arg("--screenshot")
        .arg(&png)
        .arg("--width")
        .arg("320")
        .arg("--height")
        .arg("200")
        .status()
        .expect("run megi");
    assert!(status.success(), "directory screenshot exits successfully");
    let bytes = std::fs::read(&png).expect("PNG written");
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    assert_eq!(png_dimensions(&bytes), (320, 200));
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_file(png);
}

#[test]
fn headless_chrome_decodes_a_file_favicon_in_the_tab() {
    let dir = temp_path("favicon-page");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../image/tests/fixtures/png/rgba8.png");
    std::fs::copy(&fixture, dir.join("favicon.ico")).expect("copy icon");
    std::fs::write(
        dir.join("page.html"),
        "<!doctype html><html><head><link rel=\"icon\" href=\"favicon.ico\"></head>\
         <body><h1>icon</h1></body></html>",
    )
    .expect("write page");
    let with_icon = format!("file://{}", dir.join("page.html").display());
    let options = megi_webview::render::RenderOptions::default();
    let first =
        megi_browser::headless::render_target_with_chrome(&with_icon, 320, 220, &options, None)
            .expect("chrome render");
    let second =
        megi_browser::headless::render_target_with_chrome(&with_icon, 320, 220, &options, None)
            .expect("second chrome render");
    assert_eq!(
        first.pixmap.to_png(),
        second.pixmap.to_png(),
        "decoded favicon is deterministic"
    );
    let plain = render_target(&with_icon, 320, 220).expect("plain render");
    assert_ne!(
        plain.png,
        first.pixmap.to_png(),
        "favicon changes chrome pixels"
    );
    let _ = std::fs::remove_dir_all(dir);
}
