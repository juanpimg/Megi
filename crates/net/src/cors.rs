//! CORS (Fetch Living Standard subset) response and preflight validation.
//!
//! ## Supported subset
//!
//! - [`Origin`] parses/serializes the tuple origin (`scheme://host[:port]`, default port
//!   elided) used by the `Origin` request header and by same-origin checks.
//! - Response validation ([`validate_response`]): when
//!   [`FetchOptions::origin`](crate::http::FetchOptions::origin) is set and the response
//!   URL is cross-origin, `Access-Control-Allow-Origin` must be the exact serialized
//!   origin or `*` (the wildcard is rejected in credentialed mode), and
//!   `Access-Control-Allow-Credentials` must be `true` when cookies are enabled.
//! - Preflight validation ([`validate_preflight_response`]): `Access-Control-Allow-Methods`
//!   must list the requested method and `Access-Control-Allow-Headers` must list every
//!   requested non-safelisted header (comma-separated, ASCII case-insensitive). The
//!   network send lives in [`crate::http::preflight`].
//! - Simple-method/header classification ([`is_simple_method`], [`is_simple_header`],
//!   [`requires_preflight`]): `GET`/`HEAD`/`POST` and the CORS-safelisted request
//!   headers (`Accept`, `Accept-Language`, `Content-Language`, `Content-Type`).
//!
//! ## Deliberate limitations
//!
//! - Only `GET` is actually issued by [`crate::http::fetch`]; non-simple methods/headers
//!   trigger the preflight path only through [`crate::http::preflight`] (the higher-level
//!   request API is a later round).
//! - No redirect-aware preflight caching (`Access-Control-Max-Age` is ignored), no
//!   `null`/opaque origins, no `Access-Control-Expose-Headers`, no `Vary: Origin`
//!   handling, no private-network access, no `credentials: same-origin` mode beyond the
//!   binary "cookies enabled" flag.
//! - With no `Origin` configured the client behaves exactly as before (no checks, no
//!   `Origin` header): this is deliberate so the resource loader keeps working for
//!   same-origin/offline use cases.
//! - Failures are typed as [`FetchError::Cors`] with a human-readable reason (not a
//!   machine-readable CORS error taxonomy).

use crate::error::FetchError;
use crate::http::Headers;
use crate::url::{Scheme, Url};

/// A serialized tuple origin (`https://example.com`, `http://127.0.0.1:8080`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Origin {
    /// Lowercase scheme (`http` or `https`).
    pub scheme: String,
    /// Lowercase host (IPv6 brackets included when present).
    pub host: String,
    /// Effective port (default ports are normalized to 80/443).
    pub port: u16,
}

impl Origin {
    /// Builds an origin from a parsed URL; `None` for non-HTTP(S) schemes.
    pub fn from_url(url: &Url) -> Option<Self> {
        let scheme = match url.scheme() {
            Scheme::Http => "http",
            Scheme::Https => "https",
            Scheme::File | Scheme::Data => return None,
        };
        let port = url.port_or_default().unwrap_or(0);
        Some(Self {
            scheme: scheme.to_owned(),
            host: url.host().to_ascii_lowercase(),
            port,
        })
    }

    /// Parses an `Origin` header value (`scheme://host[:port]`); `None` when malformed.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() || value.eq_ignore_ascii_case("null") {
            return None;
        }
        let (scheme, rest) = value.split_once("://")?;
        let scheme = scheme.to_ascii_lowercase();
        let default_port = match scheme.as_str() {
            "http" => 80,
            "https" => 443,
            _ => return None,
        };
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
                (host, port.parse::<u16>().ok()?)
            }
            _ => (rest, default_port),
        };
        if host.is_empty() || host.contains('/') {
            return None;
        }
        Some(Self {
            scheme,
            host: host.to_ascii_lowercase(),
            port,
        })
    }

    /// Serializes the origin, eliding default ports.
    pub fn as_str(&self) -> String {
        let default_port = match self.scheme.as_str() {
            "http" => 80,
            "https" => 443,
            _ => 0,
        };
        if self.port == default_port {
            format!("{}://{}", self.scheme, self.host)
        } else {
            format!("{}://{}:{}", self.scheme, self.host, self.port)
        }
    }

    /// Compares two origins (scheme, host and effective port must match).
    pub fn is_same_origin(&self, other: &Self) -> bool {
        self == other
    }
}

/// Returns `true` for the CORS-simple methods (`GET`, `HEAD`, `POST`).
pub fn is_simple_method(method: &str) -> bool {
    matches!(
        method.to_ascii_uppercase().as_str(),
        "GET" | "HEAD" | "POST"
    )
}

/// Returns `true` for CORS-safelisted request header names.
///
/// The `Content-Type` value restrictions are not enforced in this subset (only the header
/// name is classified); documented as a limitation.
pub fn is_simple_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "accept" | "accept-language" | "content-language" | "content-type"
    )
}

/// Returns `true` when issuing a request with `method`/`headers` requires a CORS preflight.
pub fn requires_preflight(method: &str, headers: &[(&str, &str)]) -> bool {
    !is_simple_method(method) || headers.iter().any(|(name, _)| !is_simple_header(name))
}

fn find_header<'a>(headers: &'a Headers, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn header_list_contains(value: &str, needle: &str) -> bool {
    value
        .split(',')
        .map(str::trim)
        .any(|item| item.eq_ignore_ascii_case(needle))
}

fn parse_origin_header(origin: &str) -> Result<Origin, FetchError> {
    Origin::parse(origin)
        .ok_or_else(|| FetchError::Cors(format!("invalid Origin header value `{origin}`")))
}

/// Validates a response against the configured `Origin` (Fetch §"CORS check" subset).
///
/// Same-origin responses never need CORS headers. Cross-origin responses require an
/// `Access-Control-Allow-Origin` equal to the request origin (or `*` when not
/// credentialed), plus `Access-Control-Allow-Credentials: true` when `credentials` (the
/// cookie jar) is enabled.
pub fn validate_response(
    request_url: &Url,
    origin: &str,
    headers: &Headers,
    credentials: bool,
) -> Result<(), FetchError> {
    let request_origin = parse_origin_header(origin)?;
    let response_origin = Origin::from_url(request_url).ok_or_else(|| {
        FetchError::Cors(format!(
            "CORS is only defined for http(s) URLs, got `{}`",
            request_url.scheme().as_str()
        ))
    })?;
    if request_origin.is_same_origin(&response_origin) {
        return Ok(());
    }
    let allow_origin = find_header(headers, "access-control-allow-origin")
        .map(str::trim)
        .ok_or_else(|| {
            FetchError::Cors(format!(
                "missing Access-Control-Allow-Origin for cross-origin response from {}",
                response_origin.as_str()
            ))
        })?;
    let wildcard = allow_origin == "*";
    if wildcard {
        if credentials {
            return Err(FetchError::Cors(
                "Access-Control-Allow-Origin: * is not allowed with credentials".to_owned(),
            ));
        }
    } else {
        let allowed = Origin::parse(allow_origin).ok_or_else(|| {
            FetchError::Cors(format!(
                "malformed Access-Control-Allow-Origin `{allow_origin}`"
            ))
        })?;
        if !allowed.is_same_origin(&request_origin) {
            return Err(FetchError::Cors(format!(
                "origin `{}` is not allowed (server allows `{allow_origin}`)",
                request_origin.as_str()
            )));
        }
    }
    if credentials {
        let allow_credentials = find_header(headers, "access-control-allow-credentials")
            .map(str::trim)
            .unwrap_or("");
        if !allow_credentials.eq_ignore_ascii_case("true") {
            return Err(FetchError::Cors(
                "credentials were sent but Access-Control-Allow-Credentials is not `true`"
                    .to_owned(),
            ));
        }
    }
    Ok(())
}

/// Validates an OPTIONS preflight response (Fetch §"CORS-preflight fetch" subset).
pub fn validate_preflight_response(
    request_url: &Url,
    origin: &str,
    method: &str,
    request_headers: &[(&str, &str)],
    response_headers: &Headers,
    credentials: bool,
) -> Result<(), FetchError> {
    let request_origin = parse_origin_header(origin)?;
    let response_origin = Origin::from_url(request_url).ok_or_else(|| {
        FetchError::Cors(format!(
            "CORS is only defined for http(s) URLs, got `{}`",
            request_url.scheme().as_str()
        ))
    })?;
    if request_origin.is_same_origin(&response_origin) {
        return Ok(());
    }
    // Reuse the response CORS check for ACAO/ACAC.
    validate_response(request_url, origin, response_headers, credentials)?;
    let allow_methods = find_header(response_headers, "access-control-allow-methods")
        .map(str::trim)
        .ok_or_else(|| {
            FetchError::Cors(
                "preflight response is missing Access-Control-Allow-Methods".to_owned(),
            )
        })?;
    if !header_list_contains(allow_methods, method) {
        return Err(FetchError::Cors(format!(
            "method `{method}` is not listed in Access-Control-Allow-Methods (`{allow_methods}`)"
        )));
    }
    let non_simple: Vec<&str> = request_headers
        .iter()
        .filter(|(name, _)| !is_simple_header(name))
        .map(|(name, _)| *name)
        .collect();
    if non_simple.is_empty() {
        return Ok(());
    }
    let allow_headers =
        find_header(response_headers, "access-control-allow-headers").ok_or_else(|| {
            FetchError::Cors(
                "preflight response is missing Access-Control-Allow-Headers for requested headers"
                    .to_owned(),
            )
        })?;
    let missing: Vec<&str> = non_simple
        .iter()
        .copied()
        .filter(|name| !header_list_contains(allow_headers, name))
        .collect();
    if !missing.is_empty() {
        return Err(FetchError::Cors(format!(
            "header(s) {missing:?} not listed in Access-Control-Allow-Headers (`{allow_headers}`)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).expect("test URL parses")
    }

    fn headers(pairs: &[(&str, &str)]) -> Headers {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn origin_parses_and_serializes() {
        let origin = Origin::parse("https://example.com").expect("origin");
        assert_eq!(origin.as_str(), "https://example.com");
        assert_eq!(origin.port, 443);
        let with_port = Origin::parse("http://example.com:8080").expect("origin");
        assert_eq!(with_port.as_str(), "http://example.com:8080");
        assert_eq!(Origin::parse("https://example.com:443").unwrap(), origin);
        assert_eq!(Origin::parse("null"), None);
        assert_eq!(Origin::parse(""), None);
        assert_eq!(Origin::parse("ftp://example.com"), None);
        assert_eq!(Origin::parse("example.com"), None);
    }

    #[test]
    fn origin_from_url() {
        let origin = Origin::from_url(&url("https://Example.COM/path?q")).unwrap();
        assert_eq!(origin.host, "example.com");
        assert_eq!(origin.scheme, "https");
        assert_eq!(
            Origin::from_url(&url("http://127.0.0.1:8080")),
            Origin::parse("http://127.0.0.1:8080")
        );
        assert_eq!(Origin::from_url(&url("file:///tmp/x")), None);
    }

    #[test]
    fn same_origin_comparison() {
        let a = Origin::parse("https://example.com").unwrap();
        let b = Origin::parse("https://example.com:443").unwrap();
        let c = Origin::parse("http://example.com").unwrap();
        assert!(a.is_same_origin(&b));
        assert!(!a.is_same_origin(&c));
    }

    #[test]
    fn simple_methods_and_headers() {
        assert!(is_simple_method("GET"));
        assert!(is_simple_method("post"));
        assert!(!is_simple_method("PUT"));
        assert!(!is_simple_method("OPTIONS"));
        assert!(is_simple_header("Accept"));
        assert!(is_simple_header("content-type"));
        assert!(!is_simple_header("x-custom"));
    }

    #[test]
    fn preflight_classification() {
        assert!(!requires_preflight("GET", &[]));
        assert!(!requires_preflight(
            "POST",
            &[("Content-Type", "text/plain")]
        ));
        assert!(requires_preflight("PUT", &[]));
        assert!(requires_preflight("GET", &[("X-Token", "1")]));
    }

    #[test]
    fn same_origin_response_needs_no_headers() {
        assert!(
            validate_response(
                &url("https://example.com/a"),
                "https://example.com",
                &headers(&[]),
                false
            )
            .is_ok()
        );
    }

    #[test]
    fn cross_origin_response_with_exact_acao_passes() {
        let response = headers(&[("Access-Control-Allow-Origin", "https://app.example")]);
        assert!(
            validate_response(
                &url("https://api.example/data"),
                "https://app.example",
                &response,
                false
            )
            .is_ok()
        );
    }

    #[test]
    fn cross_origin_response_with_wildcard_passes_without_credentials() {
        let response = headers(&[("access-control-allow-origin", "*")]);
        assert!(
            validate_response(
                &url("https://api.example/data"),
                "https://app.example",
                &response,
                false
            )
            .is_ok()
        );
    }

    #[test]
    fn missing_acao_is_rejected() {
        let error = validate_response(
            &url("https://api.example/data"),
            "https://app.example",
            &headers(&[]),
            false,
        )
        .unwrap_err();
        assert!(
            matches!(&error, FetchError::Cors(reason) if reason.contains("missing")),
            "{error}"
        );
    }

    #[test]
    fn wrong_acao_is_rejected() {
        let response = headers(&[("Access-Control-Allow-Origin", "https://other.example")]);
        let error = validate_response(
            &url("https://api.example/data"),
            "https://app.example",
            &response,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(&error, FetchError::Cors(reason) if reason.contains("not allowed")),
            "{error}"
        );
    }

    #[test]
    fn wildcard_with_credentials_is_rejected() {
        let response = headers(&[
            ("Access-Control-Allow-Origin", "*"),
            ("Access-Control-Allow-Credentials", "true"),
        ]);
        let error = validate_response(
            &url("https://api.example/data"),
            "https://app.example",
            &response,
            true,
        )
        .unwrap_err();
        assert!(
            matches!(&error, FetchError::Cors(reason) if reason.contains("credentials")),
            "{error}"
        );
    }

    #[test]
    fn credentials_require_allow_credentials_true() {
        let response = headers(&[("Access-Control-Allow-Origin", "https://app.example")]);
        assert!(
            validate_response(
                &url("https://api.example/data"),
                "https://app.example",
                &response,
                true
            )
            .is_err()
        );
        let response = headers(&[
            ("Access-Control-Allow-Origin", "https://app.example"),
            ("Access-Control-Allow-Credentials", "TRUE"),
        ]);
        assert!(
            validate_response(
                &url("https://api.example/data"),
                "https://app.example",
                &response,
                true
            )
            .is_ok()
        );
    }

    #[test]
    fn invalid_origin_value_is_rejected() {
        let error = validate_response(
            &url("https://api.example/data"),
            "not-an-origin",
            &headers(&[("Access-Control-Allow-Origin", "*")]),
            false,
        )
        .unwrap_err();
        assert!(
            matches!(&error, FetchError::Cors(reason) if reason.contains("invalid Origin")),
            "{error}"
        );
    }

    #[test]
    fn preflight_same_origin_is_a_noop() {
        assert!(
            validate_preflight_response(
                &url("https://example.com/a"),
                "https://example.com",
                "PUT",
                &[("X-Token", "1")],
                &headers(&[]),
                false,
            )
            .is_ok()
        );
    }

    #[test]
    fn preflight_validates_method_and_headers() {
        let response = headers(&[
            ("Access-Control-Allow-Origin", "https://app.example"),
            ("Access-Control-Allow-Methods", "GET, PUT"),
            ("Access-Control-Allow-Headers", "content-type, x-token"),
        ]);
        assert!(
            validate_preflight_response(
                &url("https://api.example/data"),
                "https://app.example",
                "PUT",
                &[("X-Token", "1")],
                &response,
                false,
            )
            .is_ok()
        );
    }

    #[test]
    fn preflight_rejects_unlisted_method() {
        let response = headers(&[
            ("Access-Control-Allow-Origin", "https://app.example"),
            ("Access-Control-Allow-Methods", "GET"),
        ]);
        let error = validate_preflight_response(
            &url("https://api.example/data"),
            "https://app.example",
            "DELETE",
            &[],
            &response,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(&error, FetchError::Cors(reason) if reason.contains("DELETE")),
            "{error}"
        );
    }

    #[test]
    fn preflight_rejects_missing_method_header() {
        let response = headers(&[("Access-Control-Allow-Origin", "https://app.example")]);
        let error = validate_preflight_response(
            &url("https://api.example/data"),
            "https://app.example",
            "PUT",
            &[],
            &response,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(&error, FetchError::Cors(reason) if reason.contains("Allow-Methods")),
            "{error}"
        );
    }

    #[test]
    fn preflight_rejects_unlisted_header() {
        let response = headers(&[
            ("Access-Control-Allow-Origin", "https://app.example"),
            ("Access-Control-Allow-Methods", "PUT"),
            ("Access-Control-Allow-Headers", "content-type"),
        ]);
        let error = validate_preflight_response(
            &url("https://api.example/data"),
            "https://app.example",
            "PUT",
            &[("X-Token", "1")],
            &response,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(&error, FetchError::Cors(reason) if reason.contains("x-token") || reason.contains("X-Token")),
            "{error}"
        );
    }

    #[test]
    fn preflight_requires_headers_list_only_for_non_simple_headers() {
        let response = headers(&[
            ("Access-Control-Allow-Origin", "https://app.example"),
            ("Access-Control-Allow-Methods", "PUT"),
        ]);
        assert!(
            validate_preflight_response(
                &url("https://api.example/data"),
                "https://app.example",
                "PUT",
                &[("Content-Type", "application/json")],
                &response,
                false,
            )
            .is_ok()
        );
    }
}
