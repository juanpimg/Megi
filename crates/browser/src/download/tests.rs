use super::*;

fn url(raw: &str) -> megi_webview::net::Url {
    megi_webview::net::Url::parse(raw).expect("test URL")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "megi-download-test-{}-{name}-{}",
        std::process::id(),
        line!()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn rendered() -> megi_webview::render::RenderOutput {
    crate::headless::render_target(
        "data:text/html,<title>Download Page</title><h1>hello</h1>",
        64,
        48,
    )
    .expect("render")
}

#[test]
fn host_sanitization_covers_http_file_and_data() {
    assert_eq!(
        sanitize_host(&url("https://Example.COM/path")),
        "example.com"
    );
    assert_eq!(sanitize_host(&url("file:///tmp/page.html")), "local");
    assert_eq!(sanitize_host(&url("data:text/html,x")), "page");
    assert_eq!(sanitize_host(&url("https://a__b__c/")), "a-b-c");
}

#[test]
fn long_host_is_truncated_before_collapsing_dashes() {
    // A hostile host must be bounded before the quadratic `replace("--", "-")` loop
    // (#426); the result never exceeds the cap.
    let url = url(&format!("https://{}/", "a-".repeat(100)));
    let host = sanitize_host(&url);
    assert!(host.chars().count() <= MAX_HOST_CHARS);
    assert!(!host.contains("--"));
}

#[test]
fn download_names_start_at_one_and_avoid_collisions() {
    let dir = temp_dir("names");
    let page = url("https://example.com/page");
    assert_eq!(
        next_download_path(&dir, &page, "png"),
        dir.join("megi-example.com-1.png")
    );
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(dir.join("megi-example.com-1.png"), b"x").expect("write");
    assert_eq!(
        next_download_path(&dir, &page, "png"),
        dir.join("megi-example.com-2.png")
    );
    assert_eq!(download_file_name("h", 3, "html"), "megi-h-3.html");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn png_download_is_valid_and_deterministic() {
    let dir = temp_dir("png");
    let output = rendered();
    let url = url("https://example.com/");
    let first = save_png(&dir, &url, &output.png).expect("save");
    assert!(first.path.is_file());
    assert_eq!(first.kind, DownloadKind::Png);
    let bytes = fs::read(&first.path).expect("read");
    assert!(is_png(&bytes));
    assert_eq!(bytes.len(), first.bytes);
    // Deterministic output: same frame → identical file (different name is fine).
    let second = save_png(&dir, &url, &output.png).expect("second save");
    assert_ne!(second.path, first.path);
    assert_eq!(fs::read(&second.path).expect("read"), bytes);
    assert!(first.notice().contains("saved png"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn html_download_contains_doctype_and_title() {
    let dir = temp_dir("html");
    let output = rendered();
    let saved =
        save_html(&dir, &url("file:///tmp/page.html"), &output.document).expect("save html");
    let html = fs::read_to_string(&saved.path).expect("read");
    assert!(
        html.trim_start()
            .to_ascii_lowercase()
            .starts_with("<!doctype"),
        "doctype: {html}"
    );
    assert!(
        html.contains("<title>Download Page</title>"),
        "title: {html}"
    );
    assert!(html.contains("<h1>hello</h1>"));
    assert_eq!(saved.bytes, html.len());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn invalid_frames_and_hostile_paths_return_errors() {
    let dir = temp_dir("errors");
    let url = url("https://example.com/");
    assert!(save_png(&dir, &url, b"").is_err());
    assert!(save_png(&dir, &url, b"not a png").is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn explicit_path_writers_include_doctype_and_png_signature() {
    let dir = temp_dir("writers");
    fs::create_dir_all(&dir).expect("mkdir");
    let output = rendered();
    let png_path = dir.join("explicit.png");
    let bytes = write_png_bytes_to(&png_path, &output.png).expect("png");
    let png = fs::read(&png_path).expect("read png");
    assert_eq!(bytes, png.len());
    assert!(is_png(&png));
    let html_path = dir.join("explicit.html");
    let html_bytes = write_html_to(&html_path, &output.document).expect("html");
    let html = fs::read_to_string(&html_path).expect("read html");
    assert_eq!(html_bytes, html.len());
    assert!(html.to_ascii_lowercase().starts_with("<!doctype"));
    assert!(html.contains("Download Page"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn saved_png_is_byte_identical_and_leaves_no_temp_files() {
    let dir = temp_dir("passthrough");
    let output = rendered();
    let saved = save_png(&dir, &url("https://example.com/"), &output.png).expect("save");
    assert_eq!(
        fs::read(&saved.path).expect("read"),
        output.png,
        "no re-encode"
    );
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "atomic write must not leave tmp files"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unwritable_download_directory_returns_an_error() {
    let dir = temp_dir("readonly");
    fs::create_dir_all(&dir).expect("mkdir");
    let output = rendered();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&dir).expect("metadata").permissions();
        permissions.set_mode(0o500);
        fs::set_permissions(&dir, permissions).expect("chmod");
    }
    let result = save_png(&dir, &url("https://example.com/"), &output.png);
    #[cfg(unix)]
    {
        assert!(result.is_err(), "read-only directory must fail cleanly");
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&dir).expect("metadata").permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&dir, permissions).expect("chmod back");
    }
    let _ = result;
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn default_download_directory_uses_explicit_home_or_temp() {
    let home = Path::new("/home/user");
    assert_eq!(
        default_download_dir(Some(Path::new("/tmp/dl")), Some(home)),
        PathBuf::from("/tmp/dl")
    );
    assert_eq!(
        default_download_dir(None, Some(home)),
        PathBuf::from("/home/user/Downloads")
    );
    assert_eq!(default_download_dir(None, None), std::env::temp_dir());
}

#[test]
fn image_downloads_infer_extension_and_keep_bytes() {
    let dir = temp_dir("save-image");
    let png_bytes = [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 1];
    let saved = save_image(&dir, &url("https://example.com/pic"), &png_bytes).expect("save");
    assert_eq!(saved.kind, DownloadKind::Image);
    assert!(saved.path.to_string_lossy().ends_with(".png"));
    assert_eq!(fs::read(&saved.path).expect("read"), png_bytes);
    let jpeg = [0xFFu8, 0xD8, 0xFF, 0x00];
    let saved_jpeg = save_image(&dir, &url("https://example.com/photo"), &jpeg).expect("jpeg");
    assert!(saved_jpeg.path.to_string_lossy().ends_with(".jpg"));
    assert!(save_image(&dir, &url("https://example.com/x"), b"").is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn atomic_write_does_not_clobber_preplanted_predictable_tmp() {
    // w2-st-1176 RED: legacy `.<name>.tmp` must not be truncated/followed.
    let dir = temp_dir("tmp-race");
    fs::create_dir_all(&dir).expect("mkdir");
    let target = dir.join("out.bin");
    let legacy = target.with_file_name(".out.bin.tmp");
    fs::write(&legacy, b"attacker-sentinel").expect("plant");
    let written = write_atomic(&target, b"new-bytes");
    // With the fix the predictable tmp is left alone (unique tmp is used).
    let legacy_bytes = fs::read(&legacy).expect("read legacy tmp");
    assert_eq!(
        legacy_bytes, b"attacker-sentinel",
        "predictable tmp must not be overwritten"
    );
    assert!(written.is_ok(), "write via unique tmp must succeed");
    assert_eq!(
        fs::read(&target).expect("read target"),
        b"new-bytes",
        "target must hold the new bytes"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn atomic_write_does_not_follow_symlink_at_predictable_tmp() {
    // w2-st-1176 RED: a symlink planted at `.<name>.tmp` must not redirect bytes.
    let dir = temp_dir("tmp-symlink");
    fs::create_dir_all(&dir).expect("mkdir");
    let target = dir.join("out.bin");
    let legacy = target.with_file_name(".out.bin.tmp");
    let outside = dir.join("outside.bin");
    fs::write(&outside, b"secret").expect("write outside");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &legacy).expect("plant symlink");
    #[cfg(not(unix))]
    fs::write(&legacy, b"attacker-sentinel").expect("plant");
    let _ = write_atomic(&target, b"new-bytes");
    let outside_bytes = fs::read(&outside).expect("read outside");
    assert_eq!(
        outside_bytes, b"secret",
        "symlink at predictable tmp must not be followed"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unique_temporary_paths_are_distinct_and_unpredictable() {
    let dir = temp_dir("tmp-unique");
    let target = dir.join("out.bin");
    let first = unique_temporary_path(&target);
    let second = unique_temporary_path(&target);
    assert_ne!(first, second, "sequential writers must differ");
    assert_ne!(
        first,
        target.with_file_name(".out.bin.tmp"),
        "must not reuse the legacy predictable name"
    );
    assert_ne!(first.parent(), None);
    assert_eq!(first.parent(), target.parent());
}

#[test]
fn create_new_refuses_a_preexisting_temporary() {
    // A planted file/symlink at the exact candidate must fail instead of
    // being truncated or followed.
    let dir = temp_dir("tmp-create-new");
    fs::create_dir_all(&dir).expect("mkdir");
    let target = dir.join("out.bin");
    let candidate = unique_temporary_path(&target);
    fs::write(&candidate, b"planted").expect("plant");
    let refused = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&candidate)
        .is_err_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists);
    assert!(refused, "create_new must reject a pre-existing tmp");
    assert_eq!(fs::read(&candidate).expect("read"), b"planted");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn download_names_are_unique_and_sanitized() {
    let dir = temp_dir("progress-naming");
    let url = megi_webview::net::Url::parse("https://example.com/some image.PNG?x=1").expect("url");
    let png = [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let first = save_image(&dir, &url, &png).expect("first");
    let second = save_image(&dir, &url, &png).expect("second");
    assert_ne!(first.path, second.path);
    assert!(first.path.to_string_lossy().contains("megi-example"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn save_as_name_is_sanitized_and_extension_appended() {
    assert_eq!(sanitize_save_as_name("My Page", "html"), "My-Page.html");
    assert_eq!(sanitize_save_as_name("report.html", "html"), "report.html");
    assert_eq!(
        sanitize_save_as_name("../../etc/passwd", "png"),
        "passwd.png"
    );
    assert_eq!(sanitize_save_as_name("a//b\\c.jpg", "png"), "c.jpg.png");
    assert_eq!(sanitize_save_as_name("../..", "png"), "download.png");
    assert_eq!(sanitize_save_as_name("  a___b  ", "png"), "a___b.png");
    assert!(
        sanitize_save_as_name(&"x".repeat(200), "png").len() <= MAX_SAVE_AS_CHARS + 4,
        "hostile long name must be capped"
    );
}

#[test]
fn named_download_paths_avoid_collisions() {
    let dir = temp_dir("named-download");
    fs::create_dir_all(&dir).expect("mkdir");
    let first = next_named_download_path(&dir, "page.html");
    assert_eq!(first.file_name().unwrap(), "page.html");
    std::fs::write(&first, b"x").expect("create first");
    let second = next_named_download_path(&dir, "page.html");
    assert_eq!(second.file_name().unwrap(), "page-1.html");
}
