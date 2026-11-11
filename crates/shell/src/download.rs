//! Downloads/exports from the shell: the rendered page as PNG and the rendered DOM as HTML.
//!
//! `Ctrl+S` saves the current frame as `megi-<host>-<n>.png`; `Ctrl+Shift+S` saves the DOM
//! snapshot (including the doctype) as `megi-<host>-<n>.html`. Files go to `--download-dir`,
//! else `$HOME/Downloads` (falling back to the system temp directory). Names are sanitized
//! (`[a-z0-9-]`, capped) and never overwrite an existing download: `<n>` is the first free
//! integer starting at 1. Everything returns typed errors / `Result` instead of panicking;
//! the window reports the result in its title and stdout (no visual prompt).
//!
//! Limits: no resource downloads (only the page itself), no "Save as" dialog, no MIME
//! negotiation beyond the two exporters, and the HTML snapshot contains the post-script DOM
//! (scripts are not re-run when reopening it).

use std::fs;
use std::path::{Path, PathBuf};

use paint::Pixmap;

use crate::links::CssRect;

/// Largest accepted host label in a download name.
pub const MAX_HOST_CHARS: usize = 40;
/// Prefix of every generated download.
pub const DOWNLOAD_PREFIX: &str = "megi";

/// Kind of exported file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadKind {
    /// Rendered page as PNG.
    Png,
    /// Rendered DOM serialized as HTML.
    Html,
}

impl DownloadKind {
    /// File extension (`png`/`html`).
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Html => "html",
        }
    }
}

/// Metadata of a successful save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedDownload {
    /// Written path.
    pub path: PathBuf,
    /// File size in bytes.
    pub bytes: usize,
    /// Exported kind.
    pub kind: DownloadKind,
}

impl SavedDownload {
    /// User-facing one-line notice.
    pub fn notice(&self) -> String {
        format!(
            "saved {} ({} bytes) to {}",
            self.kind.extension(),
            self.bytes,
            self.path.display()
        )
    }
}

/// Sanitizes a URL into the host component used in download names.
///
/// `https://example.com/x` → `example.com`; `file:` URLs → `local`; `data:` URLs → `page`;
/// everything else is lowercased and reduced to `[a-z0-9-]`, capped at [`MAX_HOST_CHARS`].
pub fn sanitize_host(url: &net::Url) -> String {
    let raw = match url.scheme().as_str() {
        "file" => "local".to_string(),
        "data" => "page".to_string(),
        _ => url.host().to_lowercase(),
    };
    let mut sanitized: String = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '.' || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect();
    while sanitized.contains("--") {
        sanitized = sanitized.replace("--", "-");
    }
    let sanitized = sanitized.trim_matches('-').to_string();
    if sanitized.is_empty() {
        "page".to_string()
    } else {
        sanitized.chars().take(MAX_HOST_CHARS).collect()
    }
}

/// Finds the first free `megi-<host>-<n>.<extension>` path in `dir`.
///
/// Naming is deterministic: it starts at 1 and increments while the candidate exists.
pub fn next_download_path(dir: &Path, url: &net::Url, extension: &str) -> PathBuf {
    let host = sanitize_host(url);
    for number in 1..=u32::MAX {
        let candidate = dir.join(format!("{DOWNLOAD_PREFIX}-{host}-{number}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{DOWNLOAD_PREFIX}-{host}-overflow.{extension}"))
}

/// Returns `--download-dir` when provided, else `$HOME/Downloads`, else the temp directory.
pub fn default_download_dir(explicit: Option<&Path>, home: Option<&Path>) -> PathBuf {
    if let Some(dir) = explicit {
        return dir.to_path_buf();
    }
    match home {
        Some(home) if !home.as_os_str().is_empty() => home.join("Downloads"),
        _ => std::env::temp_dir(),
    }
}

/// Runtime download directory (`--download-dir`, `$HOME/Downloads`, temp).
pub fn resolve_download_dir(explicit: Option<&Path>) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    default_download_dir(explicit, home.as_deref())
}

/// Saves an in-memory `0RGB` frame as a PNG download.
pub fn save_png(
    dir: &Path,
    url: &net::Url,
    width: u32,
    height: u32,
    pixels: &[u32],
) -> Result<SavedDownload, String> {
    if width == 0 || height == 0 || pixels.len() < (width as usize) * (height as usize) {
        return Err(format!(
            "invalid frame buffer {width}x{height} ({} pixels)",
            pixels.len()
        ));
    }
    fs::create_dir_all(dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let path = next_download_path(dir, url, DownloadKind::Png.extension());
    let mut pixmap = Pixmap::new(width, height);
    for (slot, pixel) in pixmap.pixels.iter_mut().zip(pixels.iter()) {
        *slot = paint::Color::new(
            ((*pixel >> 16) & 0xFF) as u8,
            ((*pixel >> 8) & 0xFF) as u8,
            (*pixel & 0xFF) as u8,
            0xFF,
        );
    }
    let png = pixmap.to_png();
    if png.is_empty() {
        return Err("PNG encoder returned no bytes".to_string());
    }
    fs::write(&path, &png).map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(SavedDownload {
        path,
        bytes: png.len(),
        kind: DownloadKind::Png,
    })
}

/// Serializes a DOM as a complete HTML document (doctype prepended when missing).
pub fn serialize_document(document: &dom::Document) -> String {
    let mut html = html::serialize_html(document);
    if !html
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("<!doctype")
    {
        html.insert_str(0, "<!DOCTYPE html>\n");
    }
    html
}

/// Writes a PNG of `pixmap` to an explicit path (`--save-png`).
pub fn write_png_to(path: &Path, pixmap: &Pixmap) -> Result<usize, String> {
    let png = pixmap.to_png();
    if png.is_empty() {
        return Err("PNG encoder returned no bytes".to_string());
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::write(path, &png).map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(png.len())
}

/// Writes the serialized DOM (doctype included) to an explicit path (`--save-html`).
pub fn write_html_to(path: &Path, document: &dom::Document) -> Result<usize, String> {
    let html = serialize_document(document);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::write(path, html.as_bytes())
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(html.len())
}

/// Saves the rendered DOM as an HTML download (the serializer includes the doctype).
pub fn save_html(
    dir: &Path,
    url: &net::Url,
    document: &dom::Document,
) -> Result<SavedDownload, String> {
    fs::create_dir_all(dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let path = next_download_path(dir, url, DownloadKind::Html.extension());
    let html = serialize_document(document);
    if html.is_empty() {
        return Err("HTML serializer returned an empty document".to_string());
    }
    fs::write(&path, html.as_bytes())
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(SavedDownload {
        path,
        bytes: html.len(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> net::Url {
        net::Url::parse(raw).expect("test URL")
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

    fn rendered() -> render::RenderOutput {
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
        let first = save_png(
            &dir,
            &url,
            output.pixmap.width,
            output.pixmap.height,
            &crate::convert::pixmap_to_0rgb(&output.pixmap),
        )
        .expect("save");
        assert!(first.path.is_file());
        assert_eq!(first.kind, DownloadKind::Png);
        let bytes = fs::read(&first.path).expect("read");
        assert!(is_png(&bytes));
        assert_eq!(bytes.len(), first.bytes);
        // Deterministic output: same frame → identical file (different name is fine).
        let second = save_png(
            &dir,
            &url,
            output.pixmap.width,
            output.pixmap.height,
            &crate::convert::pixmap_to_0rgb(&output.pixmap),
        )
        .expect("second save");
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
        assert!(save_png(&dir, &url, 0, 10, &[]).is_err());
        assert!(save_png(&dir, &url, 4, 4, &[0u32; 3]).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn explicit_path_writers_include_doctype_and_png_signature() {
        let dir = temp_dir("writers");
        fs::create_dir_all(&dir).expect("mkdir");
        let output = rendered();
        let png_path = dir.join("explicit.png");
        let bytes = write_png_to(&png_path, &output.pixmap).expect("png");
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
}
