use super::*;

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
    history_page_with_locale(entries, crate::i18n::Locale::current())
}

/// Locale-aware variant of [`history_page`].
pub fn history_page_with_locale(
    entries: &[crate::history::HistoryEntry],
    locale: crate::i18n::Locale,
) -> String {
    let strings = crate::i18n::strings(locale);
    let lang = locale.html_lang();
    let mut rows = String::new();
    if entries.is_empty() {
        let _ = write!(rows, "<p class=\"empty\">{}</p>", strings.no_history);
    } else {
        let _ = write!(
            rows,
            "<table><thead><tr><th>#</th><th>{}</th><th>{}</th></tr></thead><tbody>",
            strings.title_column, strings.url_column
        );
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
        "<!DOCTYPE html>\n<html lang=\"{lang}\" dir=\"ltr\"><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>body{{font-family:sans-serif;margin:16px;background:#fff;color:#1f2328;}}\
         h1{{font-size:20px}}table{{border-collapse:collapse;width:100%}}\
         th,td{{border-bottom:1px solid #d0d7de;padding:4px 8px;text-align:left;font-size:13px}}\
         a{{color:#1f6feb}}.empty{{color:#57606a}}</style></head>\
         <body><h1>{title}</h1>{rows}</body></html>",
        title = strings.history
    )
}

/// Builds the deterministic reader-mode page: centered 700 px column, serif fallback,
/// escaped title/paragraphs and a source link; no scripts or timers.
pub fn reader_page(title: &str, paragraphs: &[String], source_url: &str) -> String {
    reader_page_with_locale(
        title,
        paragraphs,
        source_url,
        crate::i18n::Locale::current(),
    )
}

/// Locale-aware variant of [`reader_page`].
pub fn reader_page_with_locale(
    title: &str,
    paragraphs: &[String],
    source_url: &str,
    locale: crate::i18n::Locale,
) -> String {
    let strings = crate::i18n::strings(locale);
    let lang = locale.html_lang();
    let mut body = String::new();
    for paragraph in paragraphs {
        let _ = write!(body, "<p>{}</p>", escape_html(paragraph));
    }
    format!(
        "<!DOCTYPE html>\n<html lang=\"{lang}\" dir=\"ltr\"><head><meta charset=\"utf-8\"><title>{}</title>\
         <style>html{{background:#fbfbf8}}body{{max-width:700px;margin:32px auto;padding:0 20px;\
         font-family:Georgia,'Times New Roman',serif;font-size:19px;line-height:1.65;color:#222;}}\
         h1{{font-size:30px;line-height:1.2;margin:0 0 24px}}p{{margin:0 0 18px}}\
         .source{{font-family:sans-serif;font-size:13px;color:#666;margin-top:36px;\
         border-top:1px solid #ddd;padding-top:12px}}</style></head><body>\
         <h1>{}</h1>{body}<div class=\"source\">{source} <a href=\"{}\">{}</a></div>\
         </body></html>",
        escape_html(title),
        escape_html(title),
        escape_html(source_url),
        escape_html(source_url),
        source = strings.read_from,
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
