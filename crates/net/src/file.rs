//! `file://` URL loader.
//!
//! Reads local files named by a parsed [`Url`] with the `file:` scheme. The URL path is
//! percent-decoded (so `file:///tmp/a%20b.txt` reads `/tmp/a b.txt`) and interpreted as a
//! UTF-8 Unix path — Windows drive-letter forms are not supported (see crate docs).
//!
//! Only the empty host and `localhost` are accepted; `file://server/share` returns
//! [`FetchError::UnsupportedFileHost`].

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::FetchError;
use crate::url::{Scheme, Url, percent_decode};

/// Returns the local filesystem path named by a `file:` URL.
///
/// The path is percent-decoded but not canonicalized (no symlink resolution, no `..`
/// beyond what the URL parser already normalized).
pub fn path_from_file_url(url: &Url) -> Result<PathBuf, FetchError> {
    if url.scheme() != Scheme::File {
        return Err(FetchError::UnsupportedScheme(
            url.scheme().as_str().to_owned(),
        ));
    }
    if !url.host().is_empty() && !url.host().eq_ignore_ascii_case("localhost") {
        return Err(FetchError::UnsupportedFileHost(url.host().to_owned()));
    }
    Ok(PathBuf::from(percent_decode(url.path())))
}

/// Maximum bytes read from one `file:` URL.
///
/// The renderer applies tighter per-resource caps; this bound only stops a local file
/// (`/dev/zero` included) from being read without limit before those caps are consulted.
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

/// Reads the local file named by `url` (a `file:` URL) into memory.
///
/// Reads at most [`MAX_FILE_BYTES`]; larger files return
/// [`FetchError::ResponseTooLarge`] (the same typed error the network path uses for its
/// response cap).
pub fn load_file_url(url: &Url) -> Result<Vec<u8>, FetchError> {
    load_file_url_with_limit(url, MAX_FILE_BYTES)
}

/// [`load_file_url`] with an explicit size bound (at most `max_bytes + 1` is read).
pub fn load_file_url_with_limit(url: &Url, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
    let path = path_from_file_url(url)?;
    let file = std::fs::File::open(&path).map_err(|error| map_file_error(error, &path))?;
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| map_file_error(error, &path))?;
    if bytes.len() > max_bytes {
        return Err(FetchError::ResponseTooLarge);
    }
    Ok(bytes)
}

/// Maps an I/O error for `path` to the typed file variants.
fn map_file_error(error: std::io::Error, path: &Path) -> FetchError {
    match error.kind() {
        std::io::ErrorKind::NotFound => FetchError::FileNotFound(path.to_path_buf()),
        std::io::ErrorKind::PermissionDenied => {
            FetchError::FilePermissionDenied(path.to_path_buf())
        }
        _ => FetchError::FileIo(error),
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn file_path_is_percent_decoded() {
        let url = Url::parse("file:///tmp/a%20b/c.txt").expect("valid file URL");
        assert_eq!(
            path_from_file_url(&url).expect("local host"),
            PathBuf::from("/tmp/a b/c.txt")
        );
    }

    #[test]
    fn file_reads_are_bounded() {
        let path = std::env::temp_dir().join(format!(
            "megi-file-cap-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::write(&path, b"0123456789").expect("write temp file");
        let url = Url::parse(&format!("file://{}", path.display())).expect("file URL");
        assert_eq!(
            load_file_url_with_limit(&url, 10).expect("exact fit").len(),
            10
        );
        assert!(matches!(
            load_file_url_with_limit(&url, 4),
            Err(FetchError::ResponseTooLarge)
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn remote_file_host_is_rejected() {
        let url = Url::parse("file://server/share/x").expect("valid file URL");
        assert!(matches!(
            path_from_file_url(&url),
            Err(FetchError::UnsupportedFileHost(host)) if host == "server"
        ));
    }

    #[test]
    fn non_file_url_is_rejected() {
        let url = Url::parse("http://example.com/x").expect("valid URL");
        assert!(matches!(
            path_from_file_url(&url),
            Err(FetchError::UnsupportedScheme(_))
        ));
    }
}
