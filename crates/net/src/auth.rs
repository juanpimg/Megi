//! HTTP authentication: `WWW-Authenticate` / `Proxy-Authenticate` challenge parsing and a
//! per-origin `CredentialStore` with a one-shot Basic retry (#1533).
//!
//! The store is shared through [`SharedCredentialStore`] so a `ResourceLoader` session can
//! learn credentials from the host once and reuse them for every challenged request. Only
//! the `Basic` scheme is implemented; unknown schemes keep their scheme/realm so callers can
//! report them, but never fabricate a `Authorization` header.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A parsed `WWW-Authenticate` / `Proxy-Authenticate` challenge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthChallenge {
    /// Lower-cased auth scheme, e.g. `"basic"`.
    pub scheme: String,
    /// `realm` parameter when present (quotes stripped).
    pub realm: Option<String>,
}

/// Parses the first supported-looking challenge from a header value.
///
/// Header lists may contain several comma-separated challenges; only the first is used,
/// which is enough for the Basic retry and avoids promoting a weaker later scheme.
pub fn parse_challenge(value: &str) -> Option<AuthChallenge> {
    let first = value.split(',').next()?.trim();
    let scheme_end = first.find(char::is_whitespace).unwrap_or(first.len());
    let scheme = first[..scheme_end].to_ascii_lowercase();
    if scheme.is_empty() {
        return None;
    }
    // Params may contain spaces inside quotes (`realm="Secure Area"`), so scan the
    // remainder for `realm=` instead of splitting on whitespace.
    let rest = &first[scheme_end..];
    let lower = rest.to_ascii_lowercase();
    let realm = lower.find("realm=").map(|index| {
        let value = rest[index + "realm=".len()..].trim_start();
        if let Some(quoted) = value.strip_prefix('"') {
            quoted
                .find('"')
                .map(|end| quoted[..end].to_owned())
                .unwrap_or_else(|| quoted.trim().to_owned())
        } else {
            value
                .chars()
                .take_while(|character| !character.is_whitespace() && *character != ',')
                .collect()
        }
    });
    Some(AuthChallenge { scheme, realm })
}

/// Returns the first `WWW-Authenticate` challenge, if any.
pub fn www_authenticate(headers: &[(String, String)]) -> Option<AuthChallenge> {
    header_challenge(headers, "www-authenticate")
}

/// Returns the first `Proxy-Authenticate` challenge, if any.
pub fn proxy_authenticate(headers: &[(String, String)]) -> Option<AuthChallenge> {
    header_challenge(headers, "proxy-authenticate")
}

fn header_challenge(headers: &[(String, String)], name: &str) -> Option<AuthChallenge> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, value)| parse_challenge(value))
}

/// Builds a Basic `Authorization` header value from a `user:password` pair.
///
/// The value is Base64-encoded, so control characters and colons in the password cannot
/// inject headers.
pub fn basic_authorization(user: &str, password: &str) -> String {
    format!(
        "Basic {}",
        crate::data::base64_encode(format!("{user}:{password}").as_bytes())
    )
}

/// Per-origin credentials keyed by `scheme://host:port` (the URL's `origin()` shape).
#[derive(Clone, Debug, Default)]
pub struct CredentialStore {
    entries: HashMap<String, (String, String)>,
}

impl CredentialStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores credentials for `origin`, replacing any previous pair.
    pub fn set(&mut self, origin: &str, user: &str, password: &str) {
        self.entries
            .insert(origin.to_owned(), (user.to_owned(), password.to_owned()));
    }

    /// Returns the credentials previously stored for `origin`.
    pub fn get(&self, origin: &str) -> Option<(String, String)> {
        self.entries.get(origin).cloned()
    }

    /// Removes the credentials for `origin`.
    pub fn clear(&mut self, origin: &str) {
        self.entries.remove(origin);
    }
}

/// Shareable [`CredentialStore`] used by [`crate::FetchOptions`].
pub type SharedCredentialStore = Arc<Mutex<CredentialStore>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_challenge_and_realm() {
        let c = parse_challenge("Basic realm=\"Secure Area\"").expect("challenge");
        assert_eq!(c.scheme, "basic");
        assert_eq!(c.realm.as_deref(), Some("Secure Area"));
    }

    #[test]
    fn parses_first_challenge_only_and_ignores_unknown() {
        let c = parse_challenge("Digest realm=\"a\", Basic realm=\"b\"").expect("challenge");
        assert_eq!(c.scheme, "digest");
        assert_eq!(c.realm.as_deref(), Some("a"));
        assert_eq!(parse_challenge(""), None);
    }

    #[test]
    fn finds_challenge_headers_case_insensitively() {
        let headers = vec![
            ("Content-Type".to_owned(), "text/plain".to_owned()),
            (
                "WWW-Authenticate".to_owned(),
                "Basic realm=\"x\"".to_owned(),
            ),
            (
                "Proxy-Authenticate".to_owned(),
                "Basic realm=\"y\"".to_owned(),
            ),
        ];
        assert_eq!(
            www_authenticate(&headers).unwrap().realm.as_deref(),
            Some("x")
        );
        assert_eq!(
            proxy_authenticate(&headers).unwrap().realm.as_deref(),
            Some("y")
        );
    }

    #[test]
    fn credential_store_is_per_origin() {
        let mut store = CredentialStore::new();
        store.set("http://a.test", "u", "p");
        assert_eq!(store.get("http://a.test"), Some(("u".into(), "p".into())));
        assert_eq!(store.get("http://b.test"), None);
        store.clear("http://a.test");
        assert_eq!(store.get("http://a.test"), None);
        assert_eq!(basic_authorization("u", "p"), "Basic dTpw");
    }
}
