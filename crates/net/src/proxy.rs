//! HTTP(S) proxy configuration (R205).
//!
//! [`ProxyConfig`] holds the proxy URLs for `http:` and `https:` targets plus the
//! `no_proxy` bypass list, and [`ProxyConfig::from_env`] reads the conventional
//! environment variables. The HTTP client applies it per request/redirect hop:
//! `http:` targets use absolute-form request lines through the proxy and `https:`
//! targets open a `CONNECT` tunnel first, then run the normal TLS stack over it.
//!
//! ## Environment
//!
//! - `http_proxy` / `HTTP_PROXY`: proxy for `http:` URLs;
//! - `https_proxy` / `HTTPS_PROXY`: proxy for `https:` URLs;
//! - `no_proxy` / `NO_PROXY`: comma/space-separated bypass list. Entries are hosts,
//!   domain suffixes (`example.com` matches `api.example.com`), IP literals or
//!   `host:port`; `*` bypasses every host. An `http://`/`https://` prefix is accepted
//!   and ignored.
//!
//! Proxies with `user:pass@` userinfo use HTTP Basic authentication
//! (`Proxy-Authorization: Basic …`). Only `http:`/`https:` proxy URLs are accepted.
//!
//! ## Limitations
//!
//! PAC (`pac_url`, feature `pac`) is evaluated with the JS runtime's
//! `FindProxyForURL(url, host)` and cached per host (5 min TTL); without the feature PAC
//! is ignored. SOCKS5 (`socks5://[user:pass@]host:port`) uses CONNECT with optional
//! username/password. HTTP proxy connections are pooled per proxy origin (cap 4, idle
//! timeout). Proxy auth beyond Basic and nested TLS for `https:` proxies remain out of
//! scope.

use std::fmt;

use crate::FetchError;
use crate::data::base64_encode;
use crate::url::{Scheme, Url, percent_decode};

/// Proxy selection for HTTP(S) requests.
///
/// `from_env()` builds one from the environment; [`crate::http::FetchOptions::proxy`]
/// installs it for a fetch or [`crate::ResourceLoader::set_proxy`] for a loader.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProxyConfig {
    /// Proxy used for `http:` target URLs (`http_proxy`/`HTTP_PROXY`).
    pub http: Option<Url>,
    /// Proxy used for `https:` target URLs (`https_proxy`/`HTTPS_PROXY`).
    pub https: Option<Url>,
    /// Bypass entries for `no_proxy`/`NO_PROXY` (hosts, suffixes, IPs, `host:port`, `*`).
    pub no_proxy: Vec<String>,
    /// Optional SOCKS5 proxy (`socks5://[user:pass@]host:port`), used for both schemes
    /// unless `no_proxy` bypasses the target. Takes precedence over `http`/`https`.
    pub socks: Option<crate::socks::SocksProxy>,
    /// Optional PAC/WPAD script URL for proxy auto-configuration. Requires the `pac`
    /// feature (JavaScript runtime); without it PAC is ignored and direct/static proxies
    /// are used. Evaluated with the PAC `FindProxyForURL(url, host)` contract and cached
    /// per host with a TTL (see `pac` module docs).
    pub pac_url: Option<Url>,
}

impl ProxyConfig {
    /// Builds a config from the conventional environment variables.
    ///
    /// Lowercase variables take precedence over uppercase ones; empty values are
    /// ignored. Invalid proxy URLs are skipped (the corresponding scheme then connects
    /// directly). `no_proxy`/`NO_PROXY` split on commas and whitespace.
    pub fn from_env() -> Self {
        let read = |lower: &str, upper: &str| {
            std::env::var(lower)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    std::env::var(upper)
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                })
        };
        let http = read("http_proxy", "HTTP_PROXY");
        let https = read("https_proxy", "HTTPS_PROXY");
        let no_proxy = read("no_proxy", "NO_PROXY");
        let socks = read("socks_proxy", "SOCKS_PROXY")
            .or_else(|| read("all_proxy", "ALL_PROXY"))
            .and_then(|value| crate::socks::SocksProxy::parse(&value));
        let mut config = Self::from_vars(http.as_deref(), https.as_deref(), no_proxy.as_deref());
        config.socks = socks;
        config
    }

    /// Pure helper behind [`Self::from_env`] (deterministic, used by tests).
    pub fn from_vars(
        http_proxy: Option<&str>,
        https_proxy: Option<&str>,
        no_proxy: Option<&str>,
    ) -> Self {
        Self {
            http: http_proxy.and_then(parse_proxy_url),
            https: https_proxy.and_then(parse_proxy_url),
            socks: None,
            pac_url: None,
            no_proxy: no_proxy
                .map(|list| {
                    list.split([',', ' ', '\t'])
                        .map(str::trim)
                        .filter(|entry| !entry.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Returns the proxy to use for `url`, or `None` when no proxy is configured for
    /// its scheme or `no_proxy` bypasses the target host.
    pub fn proxy_for(&self, url: &Url) -> Option<&Url> {
        let proxy = match url.scheme() {
            Scheme::Http => self.http.as_ref(),
            Scheme::Https => self.https.as_ref(),
            _ => None,
        }?;
        if self.bypasses(url.host(), url.port_or_default()) {
            None
        } else {
            Some(proxy)
        }
    }

    /// Resolves the effective proxy for `url`: PAC (when configured and the `pac` feature
    /// is enabled) first, then the static `http`/`https` field. `no_proxy` bypasses both.
    ///
    /// PAC evaluation errors fall back to the static configuration/direct connection (the
    /// page must not fail just because the auto-config script is unavailable).
    pub fn proxy_for_url(&self, url: &Url) -> Option<Url> {
        if !matches!(url.scheme(), Scheme::Http | Scheme::Https)
            || self.bypasses(url.host(), url.port_or_default())
        {
            return None;
        }
        #[cfg(feature = "pac")]
        if let Some(pac_url) = &self.pac_url
            && let Ok(Some(proxy)) = crate::pac::resolve(url, pac_url)
        {
            return Some(proxy);
        }
        self.proxy_for(url).cloned()
    }

    /// Returns the SOCKS5 proxy to use for `url`, unless `no_proxy` bypasses it.
    pub fn socks_for(&self, url: &Url) -> Option<&crate::socks::SocksProxy> {
        let socks = self.socks.as_ref()?;
        if !matches!(url.scheme(), Scheme::Http | Scheme::Https) {
            return None;
        }
        if self.bypasses(url.host(), url.port_or_default()) {
            None
        } else {
            Some(socks)
        }
    }

    /// Whether the `no_proxy` list bypasses `host` (and `port`, when an entry names one).
    pub fn bypasses(&self, host: &str, port: Option<u16>) -> bool {
        let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
        self.no_proxy
            .iter()
            .any(|entry| no_proxy_entry_matches(entry, &host, port))
    }
}

/// Parses a proxy URL, accepting only `http:`/`https:` proxies.
fn parse_proxy_url(value: &str) -> Option<Url> {
    let url = Url::parse(value.trim()).ok()?;
    matches!(url.scheme(), Scheme::Http | Scheme::Https).then_some(url)
}

/// Matches one `no_proxy` entry against the target host/port.
fn no_proxy_entry_matches(entry: &str, host: &str, port: Option<u16>) -> bool {
    let mut entry = entry.trim().to_ascii_lowercase();
    if entry == "*" {
        return true;
    }
    // Accept and ignore an explicit scheme on the entry.
    if let Some(rest) = entry.strip_prefix("http://") {
        entry = rest.to_owned();
    } else if let Some(rest) = entry.strip_prefix("https://") {
        entry = rest.to_owned();
    }
    // Split an optional `:port` from the host (IPv6 literals keep their brackets).
    let (entry_host, entry_port) = if let Some(rest) = entry.strip_prefix('[') {
        match rest.split_once(']') {
            Some((host, tail)) => (
                host.to_owned(),
                tail.strip_prefix(':')
                    .and_then(|value| value.parse::<u16>().ok()),
            ),
            None => (rest.to_owned(), None),
        }
    } else if entry.matches(':').count() == 1 {
        match entry.split_once(':') {
            Some((host, tail)) => (host.to_owned(), tail.parse::<u16>().ok()),
            None => (entry, None),
        }
    } else {
        (entry, None)
    };
    if let Some(expected_port) = entry_port
        && Some(expected_port) != port
    {
        return false;
    }
    let entry_host = entry_host.trim_matches(['[', ']']);
    if entry_host.is_empty() {
        return false;
    }
    let is_ip = entry_host.parse::<std::net::IpAddr>().is_ok();
    if is_ip {
        return entry_host == host;
    }
    let entry_host = entry_host.strip_prefix('.').unwrap_or(entry_host);
    host == entry_host
        || host
            .strip_suffix(entry_host)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

impl fmt::Display for ProxyConfig {
    /// Renders the configuration without credentials (for logs).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let render = |url: &Option<Url>| match url {
            Some(proxy) => format!(
                "{}://{}:{}",
                proxy.scheme().as_str(),
                proxy.host(),
                proxy.port_or_default().unwrap_or(0)
            ),
            None => "none".to_owned(),
        };
        write!(
            f,
            "ProxyConfig {{ http: {}, https: {}, socks: {:?}, pac: {:?}, no_proxy: [{}] }}",
            render(&self.http),
            render(&self.https),
            self.socks,
            self.pac_url,
            self.no_proxy.join(", ")
        )
    }
}

/// Builds the `Proxy-Authorization` header value from proxy URL userinfo.
///
/// Returns `None` when the proxy URL has no username. Credentials are
/// percent-decoded and Basic-encoded (`user:password`).
pub fn proxy_authorization(proxy: &Url) -> Option<String> {
    if proxy.username().is_empty() {
        return None;
    }
    let user = percent_decode(proxy.username());
    let password = percent_decode(proxy.password());
    let credentials = format!("{user}:{password}");
    Some(format!("Basic {}", base64_encode(credentials.as_bytes())))
}

/// Builds the `Proxy-Authorization` header **value** for the proxy that would serve
/// `url` (or `None` when there is no proxy / no credentials).
pub fn proxy_authorization_for(config: &ProxyConfig, url: &Url) -> Option<String> {
    config.proxy_for(url).and_then(proxy_authorization)
}

/// Returns the typed error for a proxy that answered with `407`.
pub(crate) fn auth_required_error(proxy: &Url) -> FetchError {
    FetchError::ProxyAuthRequired(format!(
        "{}:{}",
        proxy.host(),
        proxy.port_or_default().unwrap_or(0)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).expect("valid URL")
    }

    #[test]
    fn parses_vars_and_filters_invalid_schemes() {
        let config = ProxyConfig::from_vars(
            Some("http://user:pass@proxy.example:8080"),
            Some("socks5://proxy.example:1080"),
            Some("example.com, .internal, 127.0.0.1, [::1], *"),
        );
        assert_eq!(config.http.as_ref().map(Url::host), Some("proxy.example"));
        assert!(config.https.is_none(), "SOCKS proxies are rejected");
        assert_eq!(config.no_proxy.len(), 5);
    }

    #[test]
    fn proxy_for_uses_scheme_and_no_proxy() {
        let config = ProxyConfig::from_vars(
            Some("http://proxy:8080"),
            Some("http://proxy:8080"),
            Some("example.com,.internal,127.0.0.1"),
        );
        assert!(config.proxy_for(&url("http://elsewhere.test/")).is_some());
        assert!(config.proxy_for(&url("http://example.com/")).is_none());
        assert!(config.proxy_for(&url("http://api.example.com/")).is_none());
        assert!(config.proxy_for(&url("http://x.internal/")).is_none());
        assert!(config.proxy_for(&url("http://127.0.0.1:9/")).is_none());
        assert!(config.proxy_for(&url("http://notexample.com/")).is_some());
    }

    #[test]
    fn no_proxy_host_port_and_wildcard() {
        let config = ProxyConfig::from_vars(None, Some("http://proxy:1"), Some("host.test:8080,*"));
        assert!(config.bypasses("host.test", Some(8080)));
        // A different explicit port does not match; `*` still bypasses everything.
        assert!(config.bypasses("host.test", Some(9090)));
        let wildcard = ProxyConfig::from_vars(None, Some("http://proxy:1"), Some("*"));
        assert!(wildcard.bypasses("anything.test", None));
        let port_only =
            ProxyConfig::from_vars(None, Some("http://proxy:1"), Some("host.test:8080"));
        assert!(port_only.bypasses("host.test", Some(8080)));
        assert!(!port_only.bypasses("host.test", Some(9090)));
    }

    #[test]
    fn basic_auth_header_is_percent_decoded() {
        let proxy = url("http://us%65r:p%40ss@proxy.example:8080");
        assert_eq!(
            proxy_authorization(&proxy).as_deref(),
            Some("Basic dXNlcjpwQHNz"),
            "user:p@ss"
        );
        assert!(proxy_authorization(&url("http://proxy:8080")).is_none());
        let config = ProxyConfig {
            http: Some(proxy),
            https: None,
            no_proxy: Vec::new(),
            socks: None,
            pac_url: None,
        };
        assert!(proxy_authorization_for(&config, &url("http://target/")).is_some());
    }

    #[test]
    fn display_hides_credentials() {
        let config = ProxyConfig {
            http: Some(url("http://user:secret@proxy.example:8080")),
            https: None,
            no_proxy: vec!["example.com".to_owned()],
            socks: None,
            pac_url: None,
        };
        let rendered = config.to_string();
        assert!(!rendered.contains("secret"), "{rendered}");
        assert!(rendered.contains("proxy.example:8080"));
    }
}
