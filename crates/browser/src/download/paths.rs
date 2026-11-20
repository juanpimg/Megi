use super::*;

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
    /// A rendered page image resource (`<img>` bytes fetched on demand).
    Image,
    /// A fetched stylesheet resource.
    Css,
    /// A fetched JavaScript resource.
    Script,
    /// A fetched web-font resource.
    Font,
}

impl DownloadKind {
    /// File extension (`png`/`html`).
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Html => "html",
            Self::Image => "image",
            Self::Css => "css",
            Self::Script => "js",
            Self::Font => "woff2",
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
pub fn sanitize_host(url: &megi_webview::net::Url) -> String {
    let raw = match url.scheme().as_str() {
        "file" => "local".to_string(),
        "data" => "page".to_string(),
        _ => url.host().to_lowercase(),
    };
    let sanitized: String = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '.' || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect();
    // Truncate before collapsing runs: `replace("--", "-")` in a loop is quadratic on a
    // long hostile host, so bound the input first (#426).
    let mut sanitized: String = sanitized.chars().take(MAX_HOST_CHARS).collect();
    while sanitized.contains("--") {
        sanitized = sanitized.replace("--", "-");
    }
    let sanitized = sanitized.trim_matches('-').to_string();
    if sanitized.is_empty() {
        "page".to_string()
    } else {
        sanitized
    }
}

/// Finds the first free `megi-<host>-<n>.<extension>` path in `dir`.
///
/// Naming is deterministic: it starts at 1 and increments while the candidate exists.
pub fn next_download_path(dir: &Path, url: &megi_webview::net::Url, extension: &str) -> PathBuf {
    let host = sanitize_host(url);
    // Reject extensions that could escape the download directory.
    if extension.contains('/') || extension.contains('\\') || extension.contains("..") {
        return dir.join(format!("{DOWNLOAD_PREFIX}-{host}-1.png"));
    }
    for number in 1..=1024u32 {
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

/// Maximum length of a user-supplied Save-as name (before the extension).
pub const MAX_SAVE_AS_CHARS: usize = 80;

/// Sanitizes a user-entered Save-as name into a safe file leaf name (F7-SAVE-AS / #988).
///
/// Path separators and `..` are rejected, runs of unsafe characters collapse to `-`, and the
/// result is capped at [`MAX_SAVE_AS_CHARS`]. The extension is appended when missing.
pub fn sanitize_save_as_name(input: &str, extension: &str) -> String {
    let leaf = input.rsplit(['/', '\\']).next().unwrap_or("");
    let mut name: String = leaf
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .take(MAX_SAVE_AS_CHARS)
        .collect();
    while name.contains("--") {
        name = name.replace("--", "-");
    }
    let name = name.trim_matches(|ch| ch == '-' || ch == '.').to_owned();
    let base = if name.is_empty() {
        "download".to_owned()
    } else {
        name
    };
    if extension.is_empty() || extension.contains('/') || extension.contains('\\') {
        return base;
    }
    let extension = extension.trim_start_matches('.');
    if base
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(extension))
    {
        base
    } else {
        format!("{base}.{extension}")
    }
}

/// Finds the first free `<name>-<n>.<ext>` (or `<name>.<ext>`) path in `dir`.
pub fn next_named_download_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) => (stem.to_owned(), Some(extension)),
        None => (name.to_owned(), None),
    };
    for number in 1..=1024u32 {
        let candidate = match extension {
            Some(extension) => dir.join(format!("{stem}-{number}.{extension}")),
            None => dir.join(format!("{stem}-{number}")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-overflow"))
}
