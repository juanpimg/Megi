#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CssRect {
    /// Left edge in CSS px.
    pub x: f32,
    /// Top edge in CSS px.
    pub y: f32,
    /// Width in CSS px.
    pub width: f32,
    /// Height in CSS px.
    pub height: f32,
}

impl CssRect {
    /// Creates a rectangle.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns `true` when `(x, y)` lies inside the half-open rectangle.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    /// Right edge (exclusive).
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// Bottom edge (exclusive).
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// One clickable `<a href>` region in page coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkRegion {
    /// Page-space rectangle covered by the anchor's border box.
    pub rect: CssRect,
    /// Raw `href` attribute value (not resolved).
    pub href: String,
    /// DOM node of the anchor (`None` when it could not be recovered from the hit layer).
    ///
    /// Keyboard focus and the focus ring key off this node so that one anchor is one focus
    /// target even when several painted primitives cover it.
    pub node: Option<megi_webview::core::NodeId>,
}

/// Errors produced by [`resolve_href`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// The `href` was empty after trimming.
    Empty,
    /// The `href` exceeded [`MAX_HREF_LEN`] characters.
    TooLong,
    /// The URL resolver rejected the `href` (malformed or unsupported).
    Invalid(megi_webview::net::UrlError),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty href"),
            Self::TooLong => write!(f, "href exceeds {MAX_HREF_LEN} characters"),
            Self::Invalid(error) => write!(f, "invalid href: {error}"),
        }
    }
}

impl std::error::Error for LinkError {}

/// Maximum accepted `href` length (defensive cap; real links are far shorter).
pub const MAX_HREF_LEN: usize = 4096;

/// Returns `true` for the schemes the shell is allowed to navigate to.
pub fn is_navigable(url: &megi_webview::net::Url) -> bool {
    matches!(
        url.scheme(),
        megi_webview::net::Scheme::Http
            | megi_webview::net::Scheme::Https
            | megi_webview::net::Scheme::File
            | megi_webview::net::Scheme::Data
    )
}

/// Resolves a raw `href` against the current page URL.
///
/// Whitespace around the attribute value is trimmed (HTML attribute semantics); an empty
/// `href` is rejected.
pub fn resolve_href(
    base: &megi_webview::net::Url,
    href: &str,
) -> Result<megi_webview::net::Url, LinkError> {
    let href = href.trim();
    if href.is_empty() {
        return Err(LinkError::Empty);
    }
    if href.chars().count() > MAX_HREF_LEN {
        return Err(LinkError::TooLong);
    }
    match base.join(href) {
        Ok(url) => Ok(url),
        // `Url::join` only resolves relative references against `http(s)`/`file` bases and
        // rejects absolute references with a non-special scheme; accept a standalone
        // absolute URL (`data:`, or a scheme the shell will treat as non-navigable).
        Err(join_error) => megi_webview::net::Url::parse(href).map_err(|parse_error| {
            let _ = join_error;
            LinkError::Invalid(parse_error)
        }),
    }
}

/// Returns the topmost region containing `(x, y)` (reverse paint order).
pub fn find_link_at(regions: &[LinkRegion], x: f32, y: f32) -> Option<&LinkRegion> {
    regions
        .iter()
        .rev()
        .find(|region| region.rect.contains(x, y))
}
