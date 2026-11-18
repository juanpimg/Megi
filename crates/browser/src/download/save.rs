use super::*;

/// Writes `bytes` atomically via a unique sibling temporary (`create_new`, same
/// filesystem), so a crash/interruption never leaves a half-written download and a
/// pre-planted `.<name>.tmp` symlink/file is never followed or truncated.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<usize, String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let (temporary, mut file) = create_new_temp_file(path)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    {
        use std::io::Write as _;
        if let Err(error) = file.write_all(bytes) {
            drop(file);
            let _ = fs::remove_file(&temporary);
            return Err(format!("cannot write {}: {error}", path.display()));
        }
    }
    drop(file);
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("cannot write {}: {error}", path.display())
    })?;
    Ok(bytes.len())
}

/// Returns a unique, unpredictable sibling temporary path for `path` (same
/// filesystem, so the final rename stays atomic).
///
/// `.‍<name>.<pid>.<nanos>.<counter>.tmp` can neither be guessed by a local
/// attacker nor collide between two concurrent/sequential writers.
pub(crate) fn unique_temporary_path(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".to_string());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(
        ".{name}.{}.{nanos}.{count}.tmp",
        std::process::id()
    ))
}

/// Creates a fresh temporary file next to `path` with `create_new` (fails when
/// the candidate already exists, so a planted symlink/file is never followed).
/// Retries on `AlreadyExists` collisions.
pub(crate) fn create_new_temp_file(path: &Path) -> std::io::Result<(PathBuf, fs::File)> {
    for _ in 0..16 {
        let candidate = unique_temporary_path(path);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "cannot create a unique download temporary",
    ))
}

/// Saves PNG bytes produced by the render pipeline as a download, **without re-encoding**.
///
/// `png` must be a complete PNG file (validated by [`is_png`]); the render's `RenderOutput.png`
/// is passed through byte for byte, so `Ctrl+S`/`--save-png` are identical to `--screenshot`.
pub fn save_png(
    dir: &Path,
    url: &megi_webview::net::Url,
    png: &[u8],
) -> Result<SavedDownload, String> {
    if !is_png(png) {
        return Err("not a PNG image (missing signature)".to_string());
    }
    fs::create_dir_all(dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let path = next_download_path(dir, url, DownloadKind::Png.extension());
    let bytes = write_atomic(&path, png)?;
    Ok(SavedDownload {
        path,
        bytes,
        kind: DownloadKind::Png,
    })
}

/// Infers a download extension from image bytes (magic) or the URL path.
pub fn image_extension(bytes: &[u8], url: &megi_webview::net::Url) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return "png";
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return "jpg";
    }
    if bytes.starts_with(b"GIF8") {
        return "gif";
    }
    if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return "webp";
    }
    if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        return "svg";
    }
    let path = url.as_str().to_ascii_lowercase();
    for extension in ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"] {
        if path.ends_with(&format!(".{extension}")) {
            return if extension == "jpeg" {
                "jpg"
            } else {
                extension
            };
        }
    }
    "img"
}

/// Saves fetched image bytes as a download `megi-<host>-<n>.<ext>` (atomic, no re-encode).
pub fn save_image(
    dir: &Path,
    url: &megi_webview::net::Url,
    bytes: &[u8],
) -> Result<SavedDownload, String> {
    if bytes.is_empty() {
        return Err("the image resource is empty".to_string());
    }
    fs::create_dir_all(dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let extension = image_extension(bytes, url);
    let path = next_download_path(dir, url, extension);
    let written = write_atomic(&path, bytes)?;
    Ok(SavedDownload {
        path,
        bytes: written,
        kind: DownloadKind::Image,
    })
}

/// Serializes a DOM as a complete HTML document (doctype prepended when missing).
pub fn serialize_document(document: &megi_webview::core::Document) -> String {
    let mut html = megi_webview::html::serialize_html(document);
    if !html
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("<!doctype")
    {
        html.insert_str(0, "<!DOCTYPE html>\n");
    }
    html
}

/// Writes rendered PNG bytes to an explicit path (`--save-png`), atomically and without
/// re-encoding.
pub fn write_png_bytes_to(path: &Path, png: &[u8]) -> Result<usize, String> {
    if !is_png(png) {
        return Err("not a PNG image (missing signature)".to_string());
    }
    write_atomic(path, png)
}

/// Writes the serialized DOM (doctype included) to an explicit path (`--save-html`).
pub fn write_html_to(
    path: &Path,
    document: &megi_webview::core::Document,
) -> Result<usize, String> {
    let html = serialize_document(document);
    write_atomic(path, html.as_bytes())
}

/// Saves the rendered DOM as an HTML download (the serializer includes the doctype).
pub fn save_html(
    dir: &Path,
    url: &megi_webview::net::Url,
    document: &megi_webview::core::Document,
) -> Result<SavedDownload, String> {
    fs::create_dir_all(dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let path = next_download_path(dir, url, DownloadKind::Html.extension());
    let html = serialize_document(document);
    if html.is_empty() {
        return Err("HTML serializer returned an empty document".to_string());
    }
    let bytes = write_atomic(&path, html.as_bytes())?;
    Ok(SavedDownload {
        path,
        bytes,
        kind: DownloadKind::Html,
    })
}

/// Convenience path helpers used by tests/docs.
pub fn download_file_name(host: &str, number: u32, extension: &str) -> String {
    format!("{DOWNLOAD_PREFIX}-{host}-{number}.{extension}")
}

/// Unused guard used by docs to describe page rectangles; keeps the module's exported
/// geometry type visible for callers building download overlays in the future.
pub type PageRect = CssRect;

/// Returns the PNG signature check used by tests/docs.
pub fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
}
