//! Minimal blocking HTTP/1.1 client (`GET`) on top of [`std::net::TcpStream`], with
//! TLS 1.2/1.3 for `https:` via `rustls` (feature `tls`, enabled by default).
//!
//! It covers the subset needed to load documents and subresources from HTTP(S) servers
//! and is exercised entirely offline by the crate tests, which spin up their own
//! `127.0.0.1:0` listeners (plain TCP and TLS).
//!
//! Features:
//!
//! - request line + `Host`, `User-Agent` (`Megi/0.1`), `Accept: */*`,
//!   `Connection: close`, plus `Accept-Encoding: gzip` (unless disabled),
//!   `Origin` (when [`FetchOptions::origin`] is set) and `Cookie` (when a
//!   [`FetchOptions::cookie_jar`] matches the URL);
//! - configurable connect/read timeouts via [`FetchOptions`];
//! - status line + header parsing (obsolete line folding is rejected);
//! - `Content-Length` and `Transfer-Encoding: chunked` bodies;
//! - redirect following (301/302/303/307/308) with a configurable limit, resolving
//!   relative `Location` headers against the current URL;
//! - hard caps on header size, wire body size and decompressed body size to keep hostile
//!   servers from exhausting memory ([`FetchOptions::max_header_bytes`],
//!   [`FetchOptions::max_response_bytes`], [`FetchOptions::compression_limits`]);
//! - cookie storage/injection ([`FetchOptions::cookie_jar`]) and CORS validation
//!   ([`FetchOptions::origin`]) integrated per request, with the CORS preflight entry
//!   point [`preflight`];
//! - `Content-Encoding: gzip`/`deflate` decoding with CRC-32/ISIZE verification and
//!   anti-bomb caps (see [`crate::compression`]).
//!
//! ## TLS (feature `tls`, on by default)
//!
//! `https:` performs a `rustls` client handshake over the connected `TcpStream`
//! (`rustls::StreamOwned`), sending **SNI** for the URL host, validating the server
//! certificate chain and hostname against the built-in **webpki-roots** (Mozilla) trust
//! anchors. Failures surface as [`FetchError::Tls`]. A custom root store / client config
//! can be injected through [`FetchOptions::tls_config`] for tests or enterprise CAs.
//! With `--no-default-features` the `tls` feature is off and `https:` returns
//! [`FetchError::TlsUnsupported`].
//!
//! ## Deliberate limitations
//!
//! - [`fetch`]/[`fetch_with_headers`] issue `GET`; [`fetch_with_method`] adds arbitrary
//!   methods with optional bodies (Content-Length or chunked), trailers and
//!   `Expect: 100-continue`; redirects follow the fetch rules. Old note: earlier the
//!   client only issued `GET`; other methods and
//!   custom CORS-preflighted requests need the explicit [`preflight`] entry point. No
//!   `Referer`, proxies, HTTP/2, connection reuse or redirect method rewriting.
//!   (R205 adds opt-in HTTP(S) proxy support through [`FetchOptions::proxy`];
//!   HTTP/2, connection reuse and `Referer` exist on their own paths.)
//! - [`fetch_streaming`] parses status/headers first (redirects are resolved without
//!   invoking the callback) and delivers identity `Content-Length`/chunked bodies as
//!   socket chunks arrive (R212); gzip/deflate bodies are buffered and decoded with the
//!   incremental [`megi_image::StreamingInflater`] (R218), so decompressed chunks reach the
//!   callback as the compressed bytes arrive; CRC-32/ISIZE/Adler are validated at the
//!   trailer. brotli remains unsupported.
//! - Compression covers single `gzip`/`deflate` codings; the response body is buffered
//!   and decoded in memory (with the anti-bomb caps). The decoded body keeps the
//!   `Content-Encoding` header on [`Response::headers`] (matching the Fetch standard's
//!   "body is decoded, headers are not rewritten" model).
//! - Cookies use the RFC 6265 subset documented in [`crate::cookies`] (`SameSite` stored
//!   as ignored, no Public Suffix List). CORS is the subset documented in [`crate::cors`]
//!   (no preflight caching, no `Access-Control-Expose-Headers`).
//! - Read/write timeouts and header/body caps apply to TLS streams exactly as to plain
//!   TCP, because `rustls::StreamOwned` reads/writes through the same `TcpStream`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::compression::{AcceptEncoding, CompressionLimits};
use crate::cookies::{CookieJar, RequestSite};
// Kept in scope for the intra-doc links on `FetchOptions`/`RedirectMode`.
#[allow(unused_imports)]
use crate::error::FetchError;
use crate::range::{ByteRange, ContentRange};
use crate::referrer::ReferrerConfig;
use crate::url::Url;

use transport::Transport;

/// Default `User-Agent` sent by [`fetch`].
pub const DEFAULT_USER_AGENT: &str = "Megi/0.1";

/// Default connect timeout.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Default per-read timeout.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Default redirect-following limit.
pub const DEFAULT_MAX_REDIRECTS: usize = 10;
/// Default maximum response body size (16 MiB).
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
/// Default maximum response header size (64 KiB).
pub const DEFAULT_MAX_HEADER_BYTES: usize = 64 * 1024;

/// How HTTP `3xx` redirects are handled by [`fetch`] and its variants (#1536).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedirectMode {
    /// Follow `3xx` redirects up to [`FetchOptions::max_redirects`] (default).
    #[default]
    Follow,
    /// Return the `3xx` redirect [`Response`] to the caller without following `Location`.
    Manual,
    /// Fail with [`FetchError::RedirectDisallowed`] on any `3xx` redirect response.
    Error,
}

/// Configuration for [`fetch`]: timeouts and defensive limits.
#[derive(Clone, Debug)]
pub struct FetchOptions {
    /// TCP connect timeout per resolved address.
    pub connect_timeout: Duration,
    /// Read timeout applied to every blocking socket read.
    pub read_timeout: Duration,
    /// Optional write timeout applied to blocking socket writes (#1635).
    ///
    /// When `None` (the default), HTTP transport falls back to [`Self::read_timeout`]
    /// and WebSocket transport falls back to [`Self::connect_timeout`] so existing
    /// defaults are preserved.
    pub write_timeout: Option<Duration>,
    /// Maximum number of `3xx` redirects followed before returning
    /// [`FetchError::TooManyRedirects`].
    pub max_redirects: usize,
    /// Redirect handling mode for `3xx` responses (default [`RedirectMode::Follow`], #1536).
    pub redirect_mode: RedirectMode,
    /// Allow `https:` → `http:` redirects when not upgraded by HSTS (default `false`, #1536).
    pub allow_https_downgrade: bool,
    /// Maximum response body size in bytes; larger bodies fail with
    /// [`FetchError::ResponseTooLarge`].
    pub max_response_bytes: usize,
    /// Maximum request body size in bytes accepted while streaming an upload.
    pub max_request_body: usize,
    /// Reuse HTTP/1.1 connections through the loader's per-origin keep-alive pool
    /// (plain-`http` GETs with no cookies/range/referrer, see `H1Pool`). Default `true`.
    pub reuse_connections: bool,
    /// Maximum idle HTTP/1.1 connections kept per origin (default 4).
    pub max_connections_per_origin: usize,
    /// Idle timeout for pooled HTTP/1.1 connections (default 30 s).
    pub connection_idle_timeout: Duration,
    /// Send `HTTP/1.0` request lines (default `false` = `HTTP/1.1`). Responses are always
    /// parsed by their own status-line version. HTTP/1.0 requests cannot use chunked
    /// request bodies; `Connection: close` is sent for non-pooled fetches and
    /// `Connection: keep-alive` for pooled reuse.
    pub force_http10: bool,
    /// Accept HTTP/2 server pushes (`PUSH_PROMISE`) and let the resource loader preload
    /// them into a per-load session cache. Default `false`.
    pub accept_push: bool,
    /// Maximum number of server-pushed resources kept by the loader (default 8).
    pub max_pushed_resources: usize,
    /// Maximum body size of a single server-pushed resource (default 1 MiB).
    pub max_pushed_body: usize,
    /// Maximum total body bytes of all server-pushed resources (default 4 MiB).
    pub max_pushed_total: usize,
    /// Maximum response header section size in bytes; larger headers fail with
    /// [`FetchError::HeadersTooLarge`].
    pub max_header_bytes: usize,
    /// `User-Agent` request header value.
    pub user_agent: String,
    /// Optional per-origin credential store used to retry `401` challenges with Basic auth
    /// (#1533). `None` (default) disables challenge-response authentication.
    pub credential_store: Option<crate::auth::SharedCredentialStore>,
    /// Optional cookie jar. When set, `Set-Cookie` response headers are stored for every
    /// hop and the matching `Cookie` request header is sent. `None` (default) means the
    /// client is cookie-less; setting a jar also enables credentialed CORS mode.
    pub cookie_jar: Option<Arc<Mutex<CookieJar>>>,
    /// Approximate site context used to enforce cookie `SameSite` on every hop. Defaults
    /// to [`RequestSite::SameSite`]; when a [`ReferrerConfig`] source is installed and this
    /// field is left at its default, the client auto-classifies subresource requests as
    /// cross-site when the source and target are on different sites (see
    /// [`crate::cookies::RequestSite::classify`]). Set it explicitly to override.
    pub request_site: RequestSite,
    /// Optional CORS `Origin` header value (e.g. `https://app.example`). When set, the
    /// client sends `Origin` and validates cross-origin responses against
    /// `Access-Control-Allow-Origin`/`Access-Control-Allow-Credentials`; `None` (default)
    /// keeps the previous no-policy behaviour.
    pub origin: Option<String>,
    /// Response-compression negotiation: [`AcceptEncoding::Gzip`] (default) sends
    /// `Accept-Encoding: gzip` and decodes `Content-Encoding: gzip`/`deflate` bodies;
    /// [`AcceptEncoding::Identity`] sends no `Accept-Encoding` and leaves encoded bodies
    /// raw.
    pub accept_encoding: AcceptEncoding,
    /// Anti-bomb caps applied when decompressing a response body.
    pub compression_limits: CompressionLimits,
    /// Optional single byte range to request (`Range: bytes=...`). The response may be
    /// `206 Partial Content`; its [`Response::content_range`] describes the slice.
    pub range: Option<ByteRange>,
    /// Optional `Referrer-Policy`/`Referer` configuration. When set, every hop of the
    /// request sends a `Referer` header computed from the source URL with that policy.
    pub referrer: Option<ReferrerConfig>,
    /// Optional HTTP(S) proxy configuration (R205). When set, `http:` targets use
    /// absolute-form requests through the selected proxy and `https:` targets open a
    /// `CONNECT` tunnel before the normal TLS handshake; `no_proxy` bypasses per host.
    /// `None` (default) connects directly. Use [`crate::ProxyConfig::from_env`] to read
    /// the conventional environment variables.
    pub proxy: Option<Arc<crate::proxy::ProxyConfig>>,
    /// Mixed-content policy applied to the initial URL and every redirect hop (#1528).
    ///
    /// When set, `http://` targets from an `https:` document are blocked for active content or
    /// upgraded to `https://` for passive content before any transport or redirect hop connects.
    pub mixed_content: Option<crate::mixed_content::MixedContentPolicy>,
    /// Custom TLS client configuration (root store, client auth, protocol versions).
    ///
    /// `None` (the default) uses the built-in webpki-roots (Mozilla) trust anchors and
    /// the `ring` crypto provider. Tests and applications that need a private CA inject
    /// a `rustls::ClientConfig` here.
    #[cfg(feature = "tls")]
    pub tls_config: Option<Arc<rustls::ClientConfig>>,
    /// Optional session-scoped HSTS store (#1529). When `None` (default), the process-global
    /// HSTS store is used.
    pub hsts_store: Option<crate::hsts::SharedHstsStore>,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
            write_timeout: None,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            redirect_mode: RedirectMode::Follow,
            allow_https_downgrade: false,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_request_body: 16 * 1024 * 1024,
            reuse_connections: true,
            max_connections_per_origin: 4,
            connection_idle_timeout: Duration::from_secs(30),
            force_http10: false,
            accept_push: false,
            max_pushed_resources: 8,
            max_pushed_body: 1024 * 1024,
            max_pushed_total: 4 * 1024 * 1024,
            max_header_bytes: DEFAULT_MAX_HEADER_BYTES,
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            credential_store: None,
            cookie_jar: None,
            request_site: RequestSite::default(),
            origin: None,
            accept_encoding: AcceptEncoding::Gzip,
            compression_limits: CompressionLimits::default(),
            range: None,
            referrer: None,
            proxy: None,
            mixed_content: None,
            #[cfg(feature = "tls")]
            tls_config: None,
            hsts_store: None,
        }
    }
}

/// HTTP header list in wire order: `(name, value)` pairs with the original name casing.
pub type Headers = Vec<(String, String)>;

/// One HTTP/2 server-pushed subresource, surfaced by [`fetch`] when
/// [`FetchOptions::accept_push`] is enabled and consumed by the HTTP cache/preload layer.
#[derive(Clone, Debug)]
pub struct PushedResponse {
    /// Absolute URL reconstructed from the promise pseudo-headers.
    pub url: Url,
    /// Response headers in wire order.
    pub headers: Headers,
    /// Response body bytes (compressed body already decoded by the transport).
    pub body: Vec<u8>,
}

/// A complete HTTP response.
#[derive(Debug)]
pub struct Response {
    /// HTTP status code (e.g. `200`).
    pub status: u16,
    /// Reason phrase from the status line (may be empty).
    pub reason: String,
    /// Response headers in wire order; names keep their original case.
    pub headers: Headers,
    /// Response body bytes (empty for `1xx`/`204`/`304`).
    pub body: Vec<u8>,
    /// Final URL after following redirects.
    pub final_url: Url,
    /// Parsed `Content-Range` header (`206 Partial Content` responses).
    pub content_range: Option<ContentRange>,
    /// `true` when the server advertised `Accept-Ranges: bytes`.
    pub accept_ranges: bool,
    /// Trailing response headers (HTTP/2 trailer HEADERS or HTTP/1.1 chunked trailers).
    pub trailers: Headers,
    /// HTTP/2 server pushes collected while this response was in flight (empty unless
    /// [`FetchOptions::accept_push`] is enabled and the server pushed resources).
    pub pushed: Vec<PushedResponse>,
    /// Internal: idle connection to return to the loader pool (never exposed meaningfully).
    pooled_stream: Option<(String, Transport)>,
}

impl Clone for Response {
    fn clone(&self) -> Self {
        Self {
            status: self.status,
            reason: self.reason.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            final_url: self.final_url.clone(),
            content_range: self.content_range,
            accept_ranges: self.accept_ranges,
            trailers: self.trailers.clone(),
            pushed: self.pushed.clone(),
            pooled_stream: None,
        }
    }
}

impl Response {
    /// Case-insensitive lookup of the first header named `name`.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Returns the `Content-Type` header value, if present.
    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }

    /// Returns `true` when the status code is in the `2xx` range.
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// Returns `true` for a `206 Partial Content` response.
    pub const fn is_partial(&self) -> bool {
        self.status == 206
    }

    /// Returns the server's parsed `Accept-Ranges` header value, if any.
    pub fn accept_ranges_header(&self) -> Option<&str> {
        self.header("accept-ranges")
    }

    /// Parses the first valid `Referrer-Policy` response header, if present.
    ///
    /// The render pipeline uses this (or [`crate::ReferrerPolicy::from_meta_html`]) to
    /// install a [`crate::ReferrerConfig`] for subresource fetches.
    pub fn referrer_policy(&self) -> Option<crate::ReferrerPolicy> {
        crate::ReferrerPolicy::from_headers(&self.headers)
    }
}

mod chunked;
mod core;
mod headers;
mod pool;
mod preflight;
mod redirect;
mod request;
mod response;
mod streaming;
mod tls;
mod transport;

#[cfg(test)]
mod tests;

pub(crate) use pool::origin_key;
pub use pool::{H1Pool, fetch_pooled, fetch_pooled_following_redirects};
pub use preflight::preflight;
pub use request::{
    fetch, fetch_with_headers, fetch_with_method, fetch_with_request_body,
    fetch_with_request_body_and_trailers, fetch_with_trailers,
};
pub use response::parse_head_is_valid;
pub use streaming::{StreamCallback, fetch_streaming};
