//! `file://` URL loader.
//!
//! Reads local files named by a parsed [`Url`] with the `file:` scheme. The URL path is
//! percent-decoded (so `file:///tmp/a%20b.txt` reads `/tmp/a b.txt`) and interpreted as a
//! UTF-8 Unix path — Windows drive-letter forms are not supported (see crate docs).
//!
//! Only the empty host and `localhost` are accepted; `file://server/share` returns
//! [`FetchError::UnsupportedFileHost`].

use std::path::PathBuf;

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

/// Reads the local file named by `url` (a `file:` URL) into memory.
pub fn load_file_url(url: &Url) -> Result<Vec<u8>, FetchError> {
    let path = path_from_file_url(url)?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(bytes),
        Err(error) => Err(match error.kind() {
            std::io::ErrorKind::NotFound => FetchError::FileNotFound(path),
            std::io::ErrorKind::PermissionDenied => FetchError::FilePermissionDenied(path),
            _ => FetchError::FileIo(error),
        }),
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
