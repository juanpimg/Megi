//! Typed errors for the render pipeline.

use std::fmt;

/// Everything that can go wrong while rendering a page run to a PNG.
#[derive(Debug)]
pub enum RenderError {
    /// The input string is not a valid URL.
    Url(net::UrlError),
    /// The URL uses a scheme the loader cannot fetch.
    UnsupportedScheme(String),
    /// `--no-network` was set but the document (or a required resource) is `http(s)`.
    NetworkDisabled { url: String },
    /// Fetching the main document failed.
    Fetch(net::FetchError),
    /// The main document was served with an HTTP error status (>= 400).
    HttpStatus { url: String, status: u16 },
    /// The document exceeded [`crate::RenderOptions::max_document_bytes`].
    DocumentTooLarge { bytes: usize, max: usize },
    /// The main resource is not HTML and was not recognized as HTML by sniffing.
    UnsupportedContentType(String),
    /// The document declares a character encoding this pipeline cannot decode.
    UnsupportedCharset(String),
    /// The page references more external stylesheets than allowed.
    TooManyStylesheets { count: usize, max: usize },
    /// One external stylesheet exceeded [`crate::RenderOptions::max_css_bytes_per_sheet`].
    StylesheetTooLarge {
        url: String,
        bytes: usize,
        max: usize,
    },
    /// The concatenated author CSS exceeded [`crate::RenderOptions::max_total_css_bytes`].
    CssBudgetExceeded { bytes: usize, max: usize },
    /// The built-in font engine could not be initialized.
    FontEngine(String),
    /// Layout produced no root box (e.g. a document with no element root).
    LayoutFailed,
    /// The viewport dimensions are out of range.
    InvalidViewport { width: u32, height: u32 },
    /// Writing the output file failed (CLI path only).
    Output(std::io::Error),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(error) => write!(f, "invalid URL: {error}"),
            Self::UnsupportedScheme(scheme) => {
                write!(
                    f,
                    "unsupported URL scheme `{scheme}` for the render pipeline"
                )
            }
            Self::NetworkDisabled { url } => write!(
                f,
                "network access is disabled (--no-network) but {url} requires http(s)"
            ),
            Self::Fetch(error) => write!(f, "fetching the document failed: {error}"),
            Self::HttpStatus { url, status } => {
                write!(f, "document {url} returned HTTP status {status}")
            }
            Self::DocumentTooLarge { bytes, max } => write!(
                f,
                "document is too large ({bytes} bytes, configured maximum {max})"
            ),
            Self::UnsupportedContentType(content_type) => {
                write!(f, "document is not HTML (Content-Type: {content_type})")
            }
            Self::UnsupportedCharset(charset) => {
                write!(f, "unsupported document character encoding `{charset}`")
            }
            Self::TooManyStylesheets { count, max } => write!(
                f,
                "page references {count} external stylesheets, more than the configured {max}"
            ),
            Self::StylesheetTooLarge { url, bytes, max } => write!(
                f,
                "stylesheet {url} is too large ({bytes} bytes, configured maximum {max})"
            ),
            Self::CssBudgetExceeded { bytes, max } => write!(
                f,
                "author CSS budget exceeded ({bytes} bytes, configured maximum {max})"
            ),
            Self::FontEngine(reason) => write!(f, "font engine error: {reason}"),
            Self::LayoutFailed => write!(f, "layout produced no root box"),
            Self::InvalidViewport { width, height } => write!(
                f,
                "invalid viewport {width}x{height} (must be 1..=8192 in each axis)"
            ),
            Self::Output(error) => write!(f, "cannot write the output PNG: {error}"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Url(error) => Some(error),
            Self::Fetch(error) => Some(error),
            Self::Output(error) => Some(error),
            _ => None,
        }
    }
}

impl From<net::UrlError> for RenderError {
    fn from(error: net::UrlError) -> Self {
        Self::Url(error)
    }
}

impl From<net::FetchError> for RenderError {
    fn from(error: net::FetchError) -> Self {
        if let net::FetchError::UnsupportedScheme(scheme) = &error {
            Self::UnsupportedScheme(scheme.clone())
        } else {
            Self::Fetch(error)
        }
    }
}

impl From<net::LoadError> for RenderError {
    fn from(error: net::LoadError) -> Self {
        match error {
            net::LoadError::Url(error) => Self::Url(error),
            net::LoadError::Fetch(error) => {
                if let net::FetchError::UnsupportedScheme(scheme) = &error {
                    Self::UnsupportedScheme(scheme.clone())
                } else {
                    Self::Fetch(error)
                }
            }
        }
    }
}
