//! Generated browser pages: `file://` directory listings and load-error pages.
//!
//! Both pages are plain deterministic HTML rendered through the normal pipeline from a
//! `data:text/html;charset=utf-8,` URL (no temporary files, no scripting). Directory
//! listing links are **absolute** `file://` URLs so navigation works from the generated
//! data document; names are HTML-escaped and percent-encoded in hrefs.
//!
//! Limits: directory listings are `file://` only (no `http(s)` indexes), entries are
//! read once with `std::fs` (no live refresh), and error pages do not run scripts.

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};

/// One entry of a generated directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListingEntry {
    /// File or directory name (unescaped, single path component).
    pub name: String,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Size in bytes for files (`None` for directories or unknown).
    pub size: Option<u64>,
}

/// Builds the deterministic internal `about:history` page (no scripting) from entries.
///
/// Rows are newest first with the logical timestamp, a link to the URL and the escaped
/// title; an empty history renders a friendly placeholder. The page is plain HTML rendered
/// through the normal pipeline from a `data:` URL.
pub fn history_page(entries: &[crate::history::HistoryEntry]) -> String {
    let mut rows = String::new();
    if entries.is_empty() {
        rows.push_str("<p class=\"empty\">No history yet. Navigate to some pages first.</p>");
    } else {
        rows.push_str("<table><thead><tr><th>#</th><th>Title</th><th>URL</th></tr></thead><tbody>");
        for entry in entries {
            let _ = write!(
                rows,
                "<tr><td>{}</td><td>{}</td><td><a href=\"{}\" title=\"{}\">{}</a></td></tr>",
                entry.logical_time,
                escape_html(&entry.title),
                escape_html(&entry.url),
                escape_html(&entry.title),
                escape_html(&entry.url),
            );
        }
        rows.push_str("</tbody></table>");
    }
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>History</title>\
         <style>body{{font-family:sans-serif;margin:16px;background:#fff;color:#1f2328;}}\
         h1{{font-size:20px}}table{{border-collapse:collapse;width:100%}}\
         th,td{{border-bottom:1px solid #d0d7de;padding:4px 8px;text-align:left;font-size:13px}}\
         a{{color:#1f6feb}}.empty{{color:#57606a}}</style></head>\
         <body><h1>History</h1>{rows}</body></html>"
    )
}

/// Escapes `&`, `<`, `>`, `"` and `'` for safe HTML text/attribute insertion.
pub fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Human-readable file size used by the listing (`1.2 KiB`, `3.4 MiB`, ...).
pub fn human_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Percent-encodes one path segment (RFC 3986 unreserved characters stay literal).
pub fn percent_encode_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~');
        if keep {
            out.push(*byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Reads and sorts the entries of a directory (directories first, then case-insensitive
/// name; unreadable entries are skipped, unreadable directories return the I/O error).
pub fn read_directory(path: &Path) -> io::Result<Vec<ListingEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let is_dir = file_type.is_dir();
        let size = if is_dir {
            None
        } else {
            entry.metadata().ok().map(|metadata| metadata.len())
        };
        entries.push(ListingEntry { name, is_dir, size });
    }
    entries.sort_by(|a, b| {
        (!a.is_dir, a.name.to_lowercase(), a.name.clone()).cmp(&(
            !b.is_dir,
            b.name.to_lowercase(),
            b.name.clone(),
        ))
    });
    Ok(entries)
}

/// Builds the deterministic HTML listing for `path` served as `base_url`.
///
/// `base_url` must be the directory's `file://` URL (normally ending in `/`); links are
/// built as absolute URLs from it so they keep working from the generated `data:` page.
pub fn listing_html(base_url: &str, path: &Path, entries: &[ListingEntry]) -> String {
    let mut entries = entries.to_vec();
    entries.sort_by(|a, b| {
        (!a.is_dir, a.name.to_lowercase(), a.name.clone()).cmp(&(
            !b.is_dir,
            b.name.to_lowercase(),
            b.name.clone(),
        ))
    });
    let entries = &entries[..];
    let base = if base_url.ends_with('/') {
        base_url.to_owned()
    } else {
        format!("{base_url}/")
    };
    let display_path = path.display().to_string();
    let title = format!("Index of {}", escape_html(&display_path));
    let mut html = String::with_capacity(2048);
    let _ = write!(
        html,
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
<style>body{{font-family:sans-serif;margin:16px}}h1{{font-size:18px}}table{{border-collapse:collapse}}\
td,th{{padding:2px 10px;text-align:left;font-size:13px}}a{{color:#0645ad;text-decoration:none}}\
a:hover{{text-decoration:underline}}.size{{text-align:right;color:#555}}</style></head><body>"
    );
    let _ = write!(html, "<h1>{title}</h1>");
    // Breadcrumb: root then each ancestor directory of `path`.
    html.push_str("<div class=\"breadcrumb\">");
    html.push_str("<a href=\"");
    html.push_str(&escape_html(&file_url(Path::new("/"))));
    html.push_str("\">/</a>");
    let mut ancestor = PathBuf::from("/");
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::RootDir | Component::Prefix(_) => {}
            Component::Normal(part) => {
                ancestor.push(part);
                html.push_str(" / <a href=\"");
                html.push_str(&escape_html(&file_url(&ancestor)));
                html.push_str("/\">");
                html.push_str(&escape_html(&part.to_string_lossy()));
                html.push_str("</a>");
            }
            _ => {}
        }
    }
    html.push_str("</div>");
    html.push_str("<table><thead><tr><th>Name</th><th>Type</th><th>Size</th></tr></thead><tbody>");
    if let Some(parent) = path.parent() {
        html.push_str("<tr><td><a href=\"");
        html.push_str(&escape_html(&file_url(parent)));
        html.push_str("/\">..</a></td><td>Directory</td><td class=\"size\">—</td></tr>");
    }
    for entry in entries {
        let href = format!(
            "{base}{}{}",
            percent_encode_segment(&entry.name),
            if entry.is_dir { "/" } else { "" }
        );
        let kind = if entry.is_dir { "Directory" } else { "File" };
        let size = entry.size.map_or_else(|| "—".to_owned(), human_size);
        let _ = write!(
            html,
            "<tr><td><a href=\"{}\">{}</a></td><td>{kind}</td><td class=\"size\">{size}</td></tr>",
            escape_html(&href),
            escape_html(&entry.name)
        );
    }
    html.push_str("</tbody></table></body></html>");
    html
}

/// Builds the deterministic browser error page for a failed load.
pub fn error_html(url: &str, code: &str, message: &str) -> String {
    let escaped_url = escape_html(url);
    let escaped_code = escape_html(code);
    let escaped_message = escape_html(message);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
<title>Error loading page</title><style>body{{font-family:sans-serif;margin:32px;color:#222}}\
h1{{font-size:20px;color:#a00}}code{{background:#f2f2f2;padding:1px 4px}}\
a.button{{display:inline-block;margin-top:12px;padding:6px 14px;background:#0645ad;color:#fff;\
text-decoration:none;border-radius:3px}}</style></head><body>\
<h1>Error loading page</h1><p><code>{escaped_url}</code></p>\
<p>Error code: <code>{escaped_code}</code></p><p>{escaped_message}</p>\
<a class=\"button\" href=\"{escaped_url}\">Reintentar</a>\
<p>Pulsa <code>R</code> para reintentar.</p></body></html>"
    )
}

/// Percent-encodes HTML into a `data:text/html;charset=utf-8,` URL.
pub fn data_url(html: &str) -> String {
    let mut out = String::from("data:text/html;charset=utf-8,");
    for byte in html.as_bytes() {
        let keep =
            byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~' | b'!');
        if keep {
            out.push(*byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Converts an absolute filesystem path into a `file://` URL with percent-encoded spaces.
pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut out = String::from("file://");
    for character in text.chars() {
        match character {
            ' ' => out.push_str("%20"),
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            other => out.push(other),
        }
    }
    out
}

/// Short deterministic code for a render error page (`DNS`, `TLS`, `HTTP`, ...).
pub fn error_code(error: &render::RenderError) -> &'static str {
    match error {
        render::RenderError::Url(_) => "URL",
        render::RenderError::UnsupportedScheme(_) => "SCHEME",
        render::RenderError::NetworkDisabled { .. } => "NETWORK_DISABLED",
        render::RenderError::HttpStatus { .. } => "HTTP",
        render::RenderError::Fetch(fetch) => match fetch {
            net::FetchError::Dns(_) => "DNS",
            net::FetchError::Tls(_) | net::FetchError::TlsUnsupported => "TLS",
            net::FetchError::Timeout => "TIMEOUT",
            net::FetchError::Connect(_) | net::FetchError::Io(_) => "NETWORK",
            net::FetchError::Http2(_) | net::FetchError::InvalidResponse(_) => "HTTP",
            _ => "NETWORK",
        },
        render::RenderError::DocumentTooLarge { .. }
        | render::RenderError::TooManyStylesheets { .. }
        | render::RenderError::StylesheetTooLarge { .. }
        | render::RenderError::CssBudgetExceeded { .. } => "LIMIT",
        _ => "LOAD_FAILED",
    }
}

/// Resolves a `file://` directory URL into a generated listing `data:` URL.
///
/// Returns `Ok(None)` for non-`file:` inputs, `Ok(Some(url))` when the path is a
/// directory (or cannot be read → error page is generated by the caller) and `Ok(None)`
/// for regular files.
pub fn resolve_directory(input: &str) -> Option<Result<String, (String, String)>> {
    let parsed = net::Url::parse(input).ok()?;
    if parsed.scheme() != net::url::Scheme::File {
        return None;
    }
    let path = PathBuf::from(net::url::percent_decode(parsed.path()));
    if !path.is_dir() {
        return None;
    }
    match read_directory(&path) {
        Ok(entries) => Some(Ok(data_url(&listing_html(input, &path, &entries)))),
        Err(error) => Some(Err(("FILE_IO".to_owned(), error.to_string()))),
    }
}

/// Resolves the input for rendering: directory listings become `data:` pages.
///
/// Errors are reported as `(code, message)` so callers can render [`error_html`].
pub fn prepare_input(input: &str) -> Result<String, (String, String)> {
    // Internal generated page: headless renders its empty deterministic state; the window
    // fills it from the live per-tab history before requesting the render.
    if crate::history::is_history_url(input) {
        return Ok(data_url(&crate::history::history_page_html(&[])));
    }
    match resolve_directory(input) {
        Some(Ok(url)) => Ok(url),
        Some(Err(error)) => Err(error),
        None => Ok(input.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool, size: Option<u64>) -> ListingEntry {
        ListingEntry {
            name: name.to_owned(),
            is_dir,
            size,
        }
    }

    #[test]
    fn escapes_html_and_percent_encodes_links() {
        assert_eq!(escape_html("<a&\"'>"), "&lt;a&amp;&quot;&#39;&gt;");
        assert_eq!(percent_encode_segment("a b/c.html"), "a%20b%2Fc.html");
        assert_eq!(
            percent_encode_segment("plain-name_1.txt"),
            "plain-name_1.txt"
        );
    }

    #[test]
    fn human_sizes_are_deterministic() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(3 * 1024 * 1024), "3.0 MiB");
    }

    #[test]
    fn listing_sorts_directories_first_and_escapes_names() {
        let entries = vec![
            entry("zeta.txt", false, Some(10)),
            entry("alpha", true, None),
            entry("<evil>&.html", false, Some(2048)),
        ];
        let html = listing_html("file:///tmp/dir/", Path::new("/tmp/dir"), &entries);
        let alpha = html.find("alpha").expect("alpha row");
        let evil = html.find("&lt;evil&gt;&amp;.html").expect("escaped name");
        let zeta = html.find("zeta.txt").expect("zeta row");
        assert!(alpha < evil && evil < zeta, "dirs first, then names");
        assert!(html.contains("href=\"file:///tmp/dir/%3Cevil%3E%26.html\""));
        assert!(html.contains("2.0 KiB"));
        assert!(html.contains("Index of /tmp/dir"));
    }

    #[test]
    fn breadcrumb_links_are_absolute_file_urls() {
        let html = listing_html("file:///tmp/a/b/", Path::new("/tmp/a/b"), &[]);
        assert!(html.contains("href=\"file:///\">/</a>"));
        assert!(html.contains("href=\"file:///tmp/a/\">a</a>"));
        assert!(html.contains("href=\"file:///tmp/a/b/\">b</a>"));
    }

    #[test]
    fn error_page_contains_url_code_message_and_retry() {
        let html = error_html(
            "https://example.test/<x>?a=1&b=2",
            "DNS",
            "no se pudo resolver <host>",
        );
        assert!(html.contains("https://example.test/&lt;x&gt;?a=1&amp;b=2"));
        assert!(html.contains("<code>DNS</code>"));
        assert!(html.contains("no se pudo resolver &lt;host&gt;"));
        assert!(html.contains(">Reintentar</a>"));
        assert!(html.contains("Pulsa <code>R</code>"));
    }

    #[test]
    fn data_url_is_percent_encoded() {
        let url = data_url("<h1>a b</h1>");
        assert!(url.starts_with("data:text/html;charset=utf-8,"));
        assert!(url.contains("%3Ch1%3E"));
        assert!(!url.contains(' '));
    }

    #[test]
    fn prepare_input_keeps_regular_files_and_http_urls() {
        assert_eq!(
            prepare_input("https://example.test/"),
            Ok("https://example.test/".to_owned())
        );
        let temp = std::env::temp_dir().join("megi-pages-file.html");
        std::fs::write(&temp, "<p>x</p>").expect("write");
        let url = file_url(&temp);
        assert_eq!(prepare_input(&url), Ok(url));
        let _ = std::fs::remove_file(temp);
    }
}
