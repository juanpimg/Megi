use super::*;

/// Maximum directory entries rendered in a generated listing (anti-DoS: a directory with
/// millions of files must not materialize a giant page/data URL).
pub const MAX_LISTING_ENTRIES: usize = 1000;

/// Maximum characters of a single entry name kept in a listing.
pub const MAX_ENTRY_NAME_CHARS: usize = 256;

/// Maximum size of the generated listing HTML before rows are dropped.
pub const MAX_LISTING_BYTES: usize = 2 * 1024 * 1024;

/// Truncates an over-long entry name to [`MAX_ENTRY_NAME_CHARS`] characters (`…` marker).
pub(crate) fn truncate_name(name: String) -> String {
    if name.chars().count() <= MAX_ENTRY_NAME_CHARS {
        return name;
    }
    let mut truncated: String = name.chars().take(MAX_ENTRY_NAME_CHARS - 1).collect();
    truncated.push('…');
    truncated
}

/// Reads and sorts the entries of a directory (directories first, then case-insensitive
/// name; unreadable entries are skipped, unreadable directories return the I/O error).
///
/// At most [`MAX_LISTING_ENTRIES`] entries are kept and each name is truncated to
/// [`MAX_ENTRY_NAME_CHARS`].
pub fn read_directory(path: &Path) -> io::Result<Vec<ListingEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        // Keep one extra entry so the renderer can tell "at the cap" from "truncated".
        if entries.len() > MAX_LISTING_ENTRIES {
            break;
        }
        let Ok(entry) = entry else { continue };
        let name = truncate_name(entry.file_name().to_string_lossy().into_owned());
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
    listing_html_with_locale(base_url, path, entries, crate::i18n::Locale::current())
}

/// Locale-aware variant of [`listing_html`].
pub fn listing_html_with_locale(
    base_url: &str,
    path: &Path,
    entries: &[ListingEntry],
    locale: crate::i18n::Locale,
) -> String {
    let strings = crate::i18n::strings(locale);
    let lang = locale.html_lang();
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
    let title = format!("{} {}", strings.index_of, escape_html(&display_path));
    let mut html = String::with_capacity(2048);
    let _ = write!(
        html,
        "<!doctype html><html lang=\"{lang}\" dir=\"ltr\"><head><meta charset=\"utf-8\"><title>{title}</title>\
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
    let _ = write!(
        html,
        "<table><thead><tr><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
        strings.name, strings.kind, strings.size
    );
    if let Some(parent) = path.parent() {
        html.push_str("<tr><td><a href=\"");
        html.push_str(&escape_html(&file_url(parent)));
        let _ = write!(
            html,
            "/\">..</a></td><td>{}</td><td class=\"size\">—</td></tr>",
            strings.directory
        );
    }
    let mut truncated = entries.len() > MAX_LISTING_ENTRIES;
    for entry in entries.iter().take(MAX_LISTING_ENTRIES) {
        if html.len() >= MAX_LISTING_BYTES {
            truncated = true;
            break;
        }
        let href = format!(
            "{base}{}{}",
            percent_encode_segment(&entry.name),
            if entry.is_dir { "/" } else { "" }
        );
        let kind = if entry.is_dir {
            strings.directory
        } else {
            strings.file
        };
        let size = entry.size.map_or_else(|| "—".to_owned(), human_size);
        let _ = write!(
            html,
            "<tr><td><a href=\"{}\">{}</a></td><td>{kind}</td><td class=\"size\">{size}</td></tr>",
            escape_html(&href),
            escape_html(&entry.name)
        );
    }
    if truncated {
        let _ = write!(
            html,
            "<tr><td colspan=\"3\">… {} (at most {MAX_LISTING_ENTRIES} entries / {MAX_LISTING_BYTES} bytes)</td></tr>",
            strings.truncated
        );
    }
    html.push_str("</tbody></table></body></html>");
    html
}

/// Builds the deterministic browser error page for a failed load.
pub fn error_html(url: &str, code: &str, message: &str) -> String {
    error_html_with_locale(url, code, message, crate::i18n::Locale::current())
}

/// Locale-aware variant of [`error_html`].
pub fn error_html_with_locale(
    url: &str,
    code: &str,
    message: &str,
    locale: crate::i18n::Locale,
) -> String {
    let strings = crate::i18n::strings(locale);
    let lang = locale.html_lang();
    let escaped_url = escape_html(url);
    let escaped_code = escape_html(code);
    let escaped_message = escape_html(message);
    format!(
        "<!doctype html><html lang=\"{lang}\" dir=\"ltr\"><head><meta charset=\"utf-8\">\
<title>{error}</title><style>body{{font-family:sans-serif;margin:32px;color:#222}}\
h1{{font-size:20px;color:#a00}}code{{background:#f2f2f2;padding:1px 4px}}\
a.button{{display:inline-block;margin-top:12px;padding:6px 14px;background:#0645ad;color:#fff;\
text-decoration:none;border-radius:3px}}</style></head><body>\
<h1>{error}</h1><p><code>{escaped_url}</code></p>\
<p>{error_code}: <code>{escaped_code}</code></p><p>{escaped_message}</p>\
<a class=\"button\" href=\"{escaped_url}\">{retry}</a>\
<p>{press_r}</p></body></html>",
        error = strings.error_loading,
        error_code = strings.error_code,
        retry = strings.retry,
        press_r = strings.press_r_html
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
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b'/'
            | b':'
            | b'@'
            | b'!'
            | b'$'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'=' => out.push(*byte as char),
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}
