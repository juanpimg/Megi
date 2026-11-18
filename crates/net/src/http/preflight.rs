//! CORS `OPTIONS` preflight entry point.

use crate::cors;
use crate::error::FetchError;
use crate::url::Url;

use super::core::fetch_once;
use super::headers::is_token_byte;
use super::*;

/// Performs a CORS preflight (`OPTIONS`) for a non-simple request, per the Fetch standard
/// subset described in [`crate::cors`].
///
/// This is the explicit entry point for non-`GET` methods/custom headers: the plain
/// [`fetch`] only issues `GET` and therefore never preflights. Requirements:
///
/// - [`FetchOptions::origin`] must be set (otherwise there is no CORS context and the
///   function returns [`FetchError::Cors`]).
/// - The `OPTIONS` response must be `2xx`, allow the origin
///   (`Access-Control-Allow-Origin`, exact or `*` without credentials), list `method` in
///   `Access-Control-Allow-Methods`, and list every non-safelisted request header in
///   `Access-Control-Allow-Headers`.
/// - Preflights do not send cookies (Fetch: preflight credentials mode); the cookie jar
///   still controls whether the real request will be credentialed for the ACAO/ACAC check.
///
/// Redirects are not followed for preflights in this subset (a non-`2xx` response is a
/// CORS failure). `Access-Control-Max-Age` caching is not implemented.
pub fn preflight(
    url: &Url,
    options: &FetchOptions,
    method: &str,
    request_headers: &[(&str, &str)],
) -> Result<(), FetchError> {
    let checked_url;
    let url = if let Some(policy) = options.mixed_content {
        checked_url = policy.apply(url)?;
        &checked_url
    } else {
        url
    };
    let origin = options.origin.as_deref().ok_or_else(|| {
        FetchError::Cors("preflight requires FetchOptions::origin to be set".to_owned())
    })?;
    let method = method.trim().to_ascii_uppercase();
    if method.is_empty() || !method.bytes().all(is_token_byte) {
        return Err(FetchError::Cors(format!(
            "invalid CORS preflight method `{method}`"
        )));
    }
    if !cors::requires_preflight(&method, request_headers) {
        return Ok(());
    }
    let requested_headers: Vec<String> = request_headers
        .iter()
        .filter(|(name, _)| !cors::is_simple_header(name))
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect();
    let mut extra: Vec<(String, String)> =
        vec![("Access-Control-Request-Method".to_owned(), method.clone())];
    if !requested_headers.is_empty() {
        extra.push((
            "Access-Control-Request-Headers".to_owned(),
            requested_headers.join(", "),
        ));
    }
    let extra_refs: Vec<(&str, &str)> = extra
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let raw = fetch_once(url, options, "OPTIONS", &extra_refs, None, false, &[], None)?;
    if !(200..300).contains(&raw.status) {
        return Err(FetchError::Cors(format!(
            "preflight response status {} is not ok",
            raw.status
        )));
    }
    cors::validate_preflight_response(
        url,
        origin,
        &method,
        request_headers,
        &raw.headers,
        options.cookie_jar.is_some(),
    )
}
