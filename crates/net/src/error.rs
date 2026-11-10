//! Typed error types for the `net` crate.
//!
//! The crate never panics on malformed input: every fallible operation returns one of the
//! enums below ([`UrlError`], [`FetchError`], [`LoadError`]). Callers can pattern-match to
//! decide between retrying, falling back or surfacing a network error to the page.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Errors produced while parsing or resolving a URL (WHATWG URL subset).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UrlError {
    /// The input was empty after stripping ASCII whitespace and C0 controls.
    EmptyInput,
    /// The input has no `scheme:` prefix and cannot be parsed as an absolute URL.
    MissingScheme,
    /// A special-scheme URL (`http`/`https`) had no host.
    MissingHost,
    /// The scheme is syntactically valid but not supported by this crate subset.
    UnsupportedScheme(String),
    /// The host is malformed or contains forbidden characters / non-ASCII labels.
    InvalidHost(String),
    /// The port is not a decimal number in `0..=65535`, or is used with `file:`.
    InvalidPort(String),
    /// A `file:` URL used a non-local host (only the empty host and `localhost` read files).
    FileHostNotSupported(String),
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "URL is empty"),
            Self::MissingScheme => write!(f, "URL has no scheme"),
            Self::MissingHost => write!(f, "URL has no host"),
            Self::UnsupportedScheme(scheme) => write!(f, "unsupported URL scheme `{scheme}`"),
            Self::InvalidHost(host) => write!(f, "invalid URL host `{host}`"),
            Self::InvalidPort(port) => write!(f, "invalid URL port `{port}`"),
            Self::FileHostNotSupported(host) => {
                write!(f, "file: URLs can only name the local host, got `{host}`")
            }
        }
    }
}

impl std::error::Error for UrlError {}

/// Errors produced while fetching a resource over the network or the filesystem.
#[derive(Debug)]
pub enum FetchError {
    /// The URL scheme cannot be fetched by this crate (`ftp:`, `blob:`, ...).
    UnsupportedScheme(String),
    /// The URL is `https:` but this crate was built without the `tls` feature.
    ///
    /// The default feature set enables TLS; this variant is only produced by
    /// `cargo build --no-default-features`.
    TlsUnsupported,
    /// The TLS handshake failed: certificate validation (unknown issuer, expired,
    /// hostname mismatch), protocol negotiation or record-layer I/O.
    ///
    /// The payload is a human-readable reason produced by `rustls`; it is for
    /// diagnostics, not for programmatic matching.
    Tls(String),
    /// DNS resolution of the host failed.
    Dns(String),
    /// The TCP connection could not be established (all resolved addresses failed).
    Connect(io::Error),
    /// Reading/writing the TCP stream failed.
    Io(io::Error),
    /// The socket timed out (connect or read).
    Timeout,
    /// The HTTP response is malformed (status line, header syntax, chunk framing, ...).
    InvalidResponse(String),
    /// The response headers exceeded [`crate::http::FetchOptions::max_header_bytes`].
    HeadersTooLarge,
    /// The response body exceeded [`crate::http::FetchOptions::max_response_bytes`].
    ResponseTooLarge,
    /// The peer closed the connection before the declared body was complete.
    BodyIncomplete,
    /// The redirect chain exceeded [`crate::http::FetchOptions::max_redirects`].
    TooManyRedirects,
    /// A redirect response had no usable `Location` header.
    RedirectWithoutLocation,
    /// The `Location` header could not be resolved against the current URL.
    InvalidRedirect(UrlError),
    /// A `file:` URL pointed at a path that does not exist.
    FileNotFound(PathBuf),
    /// A `file:` URL pointed at a path without read permission.
    FilePermissionDenied(PathBuf),
    /// Reading the local file failed for another I/O reason.
    FileIo(io::Error),
    /// A `file:` URL used a non-local host (only `file:///...` is supported).
    UnsupportedFileHost(String),
    /// A `data:` URL is malformed (bad mediatype, invalid base64, ...).
    InvalidDataUrl(String),
    /// A `Range`/`Content-Range` operation failed (invalid or unsatisfiable range).
    InvalidRange(String),
    /// A `304 Not Modified` response arrived without a cached representation to
    /// revalidate (e.g. the cache was cleared, or the server misbehaves).
    UnconditionalNotModified,
    /// A CORS check failed: the response is cross-origin and its
    /// `Access-Control-Allow-Origin`/`Access-Control-Allow-Credentials` (or a preflight's
    /// `Access-Control-Allow-Methods`/`Access-Control-Allow-Headers`) does not authorize
    /// the request. The payload is a human-readable reason.
    Cors(String),
    /// The response declared a `Content-Encoding` this crate does not support (e.g. `br`,
    /// `zstd`) or stacked encodings (`gzip, br`).
    ContentEncoding(String),
    /// Decoding a compressed response body failed: malformed gzip/DEFLATE stream,
    /// CRC-32/ISIZE mismatch or invalid zlib trailer.
    Decompression(String),
    /// Decompression exceeded the anti-bomb caps configured in
    /// [`crate::http::FetchOptions`] (decompressed size and/or compression ratio).
    DecompressionLimit {
        /// Configured maximum decompressed size in bytes.
        max_output: usize,
        /// Configured maximum compression ratio (`decompressed / compressed`).
        max_ratio: u64,
    },
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedScheme(scheme) => write!(f, "unsupported fetch scheme `{scheme}`"),
            Self::TlsUnsupported => {
                write!(
                    f,
                    "https requires the `tls` feature (build with --features tls)"
                )
            }
            Self::Tls(reason) => write!(f, "TLS failure: {reason}"),
            Self::Dns(host) => write!(f, "DNS resolution failed for `{host}`"),
            Self::Connect(err) => write!(f, "TCP connect failed: {err}"),
            Self::Io(err) => write!(f, "HTTP I/O error: {err}"),
            Self::Timeout => write!(f, "network operation timed out"),
            Self::InvalidResponse(reason) => write!(f, "malformed HTTP response: {reason}"),
            Self::HeadersTooLarge => write!(f, "HTTP response headers are too large"),
            Self::ResponseTooLarge => write!(f, "HTTP response body is too large"),
            Self::BodyIncomplete => write!(f, "HTTP response body ended prematurely"),
            Self::TooManyRedirects => write!(f, "too many HTTP redirects"),
            Self::RedirectWithoutLocation => {
                write!(f, "redirect response without a Location header")
            }
            Self::InvalidRedirect(err) => write!(f, "invalid redirect target: {err}"),
            Self::FileNotFound(path) => write!(f, "file not found: {}", path.display()),
            Self::FilePermissionDenied(path) => {
                write!(f, "permission denied reading file: {}", path.display())
            }
            Self::FileIo(err) => write!(f, "file I/O error: {err}"),
            Self::UnsupportedFileHost(host) => {
                write!(f, "file: URLs can only name the local host, got `{host}`")
            }
            Self::InvalidDataUrl(reason) => write!(f, "invalid data: URL: {reason}"),
            Self::InvalidRange(reason) => write!(f, "invalid byte range: {reason}"),
            Self::UnconditionalNotModified => {
                write!(f, "received 304 Not Modified without a cached response")
            }
            Self::Cors(reason) => write!(f, "CORS check failed: {reason}"),
            Self::ContentEncoding(reason) => write!(f, "unsupported content encoding: {reason}"),
            Self::Decompression(reason) => write!(f, "decompression failed: {reason}"),
            Self::DecompressionLimit {
                max_output,
                max_ratio,
            } => write!(
                f,
                "decompression exceeded anti-bomb limits (max {max_output} bytes, ratio {max_ratio}:1)"
            ),
        }
    }
}

impl std::error::Error for FetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Connect(err) | Self::Io(err) | Self::FileIo(err) => Some(err),
            Self::InvalidRedirect(err) => Some(err),
            _ => None,
        }
    }
}

/// Errors produced by [`crate::ResourceLoader::load`].
#[derive(Debug)]
pub enum LoadError {
    /// The input string could not be parsed as a URL.
    Url(UrlError),
    /// Fetching the resource (network, filesystem or decoding) failed.
    Fetch(FetchError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(err) => write!(f, "{err}"),
            Self::Fetch(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Url(err) => Some(err),
            Self::Fetch(err) => Some(err),
        }
    }
}

impl From<UrlError> for LoadError {
    fn from(err: UrlError) -> Self {
        Self::Url(err)
    }
}

impl From<FetchError> for LoadError {
    fn from(err: FetchError) -> Self {
        Self::Fetch(err)
    }
}
