//! Redirect target resolution and 3xx classification shared by every fetch path.

use crate::error::FetchError;
use crate::url::Scheme;

use super::*;

/// Resolves a `Location` redirect target and enforces the HTTPS-to-HTTP downgrade guard
/// after checking whether HSTS upgrades an `http://` hop back to `https://` (#1536).
pub(super) fn resolve_redirect_target(
    current: &Url,
    headers: &Headers,
    allow_https_downgrade: bool,
) -> Result<Url, FetchError> {
    let location = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("location"))
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or(FetchError::RedirectWithoutLocation)?;
    let mut next = current
        .join(location)
        .map_err(FetchError::InvalidRedirect)?;
    if let Some(upgraded) = crate::hsts::upgrade_url(&next) {
        next = upgraded;
    }
    if current.scheme() == Scheme::Https && next.scheme() == Scheme::Http && !allow_https_downgrade
    {
        return Err(FetchError::HttpsDowngradeBlocked(next.as_str().to_owned()));
    }
    Ok(next)
}

pub(super) fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}
