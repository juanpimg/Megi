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
//!   incremental [`image::StreamingInflater`] (R218), so decompressed chunks reach the
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

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use std::sync::{Arc, Mutex};

use crate::body::RequestBody;
use crate::compression::{AcceptEncoding, CompressionLimits, ContentEncoding, decode_body};
use crate::cookies::{CookieJar, RequestSite};
use crate::cors;
use crate::error::FetchError;
use crate::range::{ByteRange, ContentRange};
use crate::referrer::ReferrerConfig;
use crate::url::{Scheme, Url};

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

/// Configuration for [`fetch`]: timeouts and defensive limits.
#[derive(Clone, Debug)]
pub struct FetchOptions {
    /// TCP connect timeout per resolved address.
    pub connect_timeout: Duration,
    /// Read timeout applied to every blocking socket read.
    pub read_timeout: Duration,
    /// Maximum number of `3xx` redirects followed before returning
    /// [`FetchError::TooManyRedirects`].
    pub max_redirects: usize,
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
    /// Custom TLS client configuration (root store, client auth, protocol versions).
    ///
    /// `None` (the default) uses the built-in webpki-roots (Mozilla) trust anchors and
    /// the `ring` crypto provider. Tests and applications that need a private CA inject
    /// a `rustls::ClientConfig` here.
    #[cfg(feature = "tls")]
    pub tls_config: Option<Arc<rustls::ClientConfig>>,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
            max_redirects: DEFAULT_MAX_REDIRECTS,
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
            cookie_jar: None,
            request_site: RequestSite::default(),
            origin: None,
            accept_encoding: AcceptEncoding::Gzip,
            compression_limits: CompressionLimits::default(),
            range: None,
            referrer: None,
            proxy: None,
            #[cfg(feature = "tls")]
            tls_config: None,
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
    pooled_stream: Option<(String, TcpStream)>,
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

/// Per-origin idle keep-alive pool for plain HTTP/1.1 GET connections (R185).
#[derive(Debug, Default)]
pub struct H1Pool {
    entries: std::collections::HashMap<String, Vec<PooledH1>>,
    max_per_origin: usize,
    idle_timeout: Duration,
}

#[derive(Debug)]
struct PooledH1 {
    stream: TcpStream,
    last_used: Instant,
}

impl H1Pool {
    /// Creates a pool with `max_per_origin` idle connections and an idle timeout.
    pub fn new(max_per_origin: usize, idle_timeout: Duration) -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            max_per_origin: max_per_origin.max(1),
            idle_timeout,
        }
    }

    fn take(&mut self, origin: &str) -> Option<TcpStream> {
        let now = Instant::now();
        let entries = self.entries.get_mut(origin)?;
        while let Some(pooled) = entries.pop() {
            if now.duration_since(pooled.last_used) < self.idle_timeout {
                return Some(pooled.stream);
            }
        }
        None
    }

    fn put(&mut self, origin: String, stream: TcpStream) {
        let entries = self.entries.entry(origin).or_default();
        if entries.len() >= self.max_per_origin {
            entries.remove(0);
        }
        entries.push(PooledH1 {
            stream,
            last_used: Instant::now(),
        });
    }

    /// Drops expired idle connections.
    pub fn prune_expired(&mut self) {
        let timeout = self.idle_timeout;
        let now = Instant::now();
        for entries in self.entries.values_mut() {
            entries.retain(|pooled| now.duration_since(pooled.last_used) < timeout);
        }
        self.entries.retain(|_, entries| !entries.is_empty());
    }

    /// Number of idle connections currently pooled (tests/diagnostics).
    pub fn idle_connections(&self) -> usize {
        self.entries.values().map(Vec::len).sum()
    }
}

fn origin_key(url: &Url) -> String {
    format!(
        "{}://{}:{}",
        url.scheme().as_str(),
        url.host(),
        url.port_or_default().unwrap_or(0)
    )
}

/// Fetches a plain-HTTP GET reusing an idle connection from `pool` when possible.
///
/// MVP semantics (documented): only `http:` GET requests without cookies/range/referrer/
/// compression are pooled; redirects, `https:`, cookies and encoded responses fall back to
/// the regular buffered [`fetch`] (same response semantics, just no connection reuse for
/// that request). The connection is returned to the pool only when the body was fully
/// delimited (Content-Length/chunked) and the server kept it alive.
pub fn fetch_pooled(
    url: &Url,
    options: &FetchOptions,
    pool: &mut H1Pool,
) -> Result<Response, FetchError> {
    let pool_proxy = options
        .proxy
        .as_deref()
        .and_then(|config| config.proxy_for_url(url))
        .filter(|proxy| proxy.scheme() == Scheme::Http);
    let socks_configured = options
        .proxy
        .as_deref()
        .is_some_and(|config| config.socks_for(url).is_some());
    if url.scheme() != Scheme::Http
        || socks_configured
        || options
            .proxy
            .as_deref()
            .is_some_and(|config| config.proxy_for_url(url).is_some() && pool_proxy.is_none())
        || !options.reuse_connections
        || options.cookie_jar.is_some()
        || options.origin.is_some()
        || options.range.is_some()
        || options.referrer.is_some()
        || options.accept_encoding != AcceptEncoding::Identity
    {
        return fetch(url, options);
    }
    let (connect_host, connect_port, origin, request_target) = match &pool_proxy {
        Some(proxy) => {
            let proxy_port = proxy.port_or_default().ok_or_else(|| {
                FetchError::UnsupportedScheme("proxy URL without port".to_owned())
            })?;
            let origin = format!(
                "proxy:{}://{}:{}",
                proxy.scheme().as_str(),
                proxy.host(),
                proxy_port
            );
            let target = absolute_request_target(
                url,
                &match url.port() {
                    Some(port) => format!("{}:{port}", url.host()),
                    None => url.host().to_owned(),
                },
            );
            (proxy.host_for_dns().to_owned(), proxy_port, origin, target)
        }
        None => (
            url.host().to_owned(),
            url.port_or_default().unwrap_or(80),
            origin_key(url),
            url.request_target(),
        ),
    };
    let host_header = match url.port() {
        Some(port) => format!("{}:{port}", url.host()),
        None => url.host().to_owned(),
    };
    let proxy_auth = pool_proxy
        .as_ref()
        .and_then(crate::proxy::proxy_authorization);
    let mut last_error = None;
    for attempt in 0..2 {
        let stream = match pool.take(&origin) {
            Some(stream) => stream,
            None => {
                connect_tcp(&connect_host, connect_port, options)?
            }
        };
        stream.set_read_timeout(Some(options.read_timeout)).ok();
        stream.set_write_timeout(Some(options.read_timeout)).ok();
        let mut reader = ConnReader::new(Transport::Plain(stream));
        let http_version = if options.force_http10 { "1.0" } else { "1.1" };
        let keep_alive_header = if options.force_http10 {
            "Connection: keep-alive\r\n"
        } else {
            ""
        };
        let mut request = format!(
            "GET {request_target} HTTP/{http_version}\r\nHost: {host_header}\r\nUser-Agent: {}\r\nAccept: */*\r\n{}{keep_alive_header}\r\n",
            options.user_agent,
            proxy_auth
                .as_ref()
                .map(|auth| format!("Proxy-Authorization: {auth}\r\n"))
                .unwrap_or_default()
        );
        if options.accept_encoding != AcceptEncoding::Identity {
            request = format!(
                "GET {request_target} HTTP/{http_version}\r\nHost: {host_header}\r\nUser-Agent: {}\r\nAccept: */*\r\nAccept-Encoding: {}\r\n{}{keep_alive_header}\r\n",
                options.user_agent,
                options.accept_encoding.header_value().unwrap_or("identity"),
                proxy_auth
                    .as_ref()
                    .map(|auth| format!("Proxy-Authorization: {auth}\r\n"))
                    .unwrap_or_default()
            );
        }
        let result = (|| -> Result<Response, FetchError> {
            reader
                .stream
                .write_all(request.as_bytes())
                .map_err(map_io_error)?;
            reader.stream.flush().map_err(map_io_error)?;
            let head = reader.read_head(options.max_header_bytes)?;
            let (version, status, reason, headers) = parse_head_verbose(&head)?;
            if (300..400).contains(&status) {
                return Err(FetchError::InvalidResponse(
                    "redirect in pooled fetch".to_owned(),
                ));
            }
            if headers.iter().any(|(name, _)| {
                name.eq_ignore_ascii_case("content-encoding")
                    || name.eq_ignore_ascii_case("transfer-encoding")
            }) {
                return Err(FetchError::InvalidResponse(
                    "encoded pooled response".to_owned(),
                ));
            }
            let has_length = headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("content-length"));
            let connection_close = headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("connection")
                    && value
                        .split(',')
                        .any(|token| token.trim().eq_ignore_ascii_case("close"))
            });
            let (body, trailers) = read_body(&mut reader, status, &headers, options)?;
            let connection_keep_alive = headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("connection")
                    && value
                        .split(',')
                        .any(|token| token.trim().eq_ignore_ascii_case("keep-alive"))
            });
            let mut keep_alive = if version == (1, 0) {
                // HTTP/1.0 keeps the connection alive only when explicitly requested.
                has_length && connection_keep_alive && !connection_close
            } else {
                has_length && !connection_close
            };
            if reader.position < reader.buffer.len() {
                keep_alive = false; // unexpected pipelined bytes: do not reuse
            }
            let stream = match reader.stream {
                Transport::Plain(stream) => Some(stream),
                #[cfg(feature = "tls")]
                Transport::Tls(_) => None,
            };
            Ok(Response {
                status,
                reason,
                headers,
                body,
                final_url: url.clone(),
                content_range: None,
                accept_ranges: false,
                trailers,
                pushed: Vec::new(),
                pooled_stream: stream
                    .filter(|_| keep_alive)
                    .map(|stream| (origin.clone(), stream)),
            })
        })();
        match result {
            Ok(mut response) => {
                if let Some((origin, stream)) = response.pooled_stream.take() {
                    pool.put(origin, stream);
                }
                return Ok(response);
            }
            Err(error) => {
                last_error = Some(error);
                if attempt == 0 {
                    continue;
                }
            }
        }
    }
    // Redirects/encoded responses/races fall back to the buffered fetch (semantics first).
    fetch(url, options).or(Err(last_error.unwrap_or(FetchError::InvalidResponse(
        "pooled fetch failed".to_owned(),
    ))))
}

/// Buffered fallback streaming fetch: the response is fetched normally (redirects,
/// cookies, cache validators and compression all respected by [`fetch`]), then the body is
/// delivered to `on_data` in 16 KiB chunks. A callback error aborts with that error.
pub fn fetch_streaming(
    url: &Url,
    options: &FetchOptions,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<Response, FetchError> {
    // Real HTTP/1.1 streaming: status/headers are parsed first (so redirects are
    // followed without invoking the callback), then the identity body is delivered as
    // chunks arrive. HTTP/2 uses the h2 client's DATA callback. Compressed bodies are
    // currently collected/decoded before the callback (documented limitation: the
    // decoder is complete-stream); identity bodies and chunked framing are incremental.
    let mut current = url.clone();
    let mut redirects = 0usize;
    loop {
        let raw = fetch_once(
            &current,
            options,
            "GET",
            &[],
            None,
            true,
            &[],
            Some(&mut *on_data),
        )?;
        if let Some(jar) = &options.cookie_jar {
            lock_jar(jar).store_from_response(&current, &raw.headers);
        }
        if !is_redirect(raw.status) {
            if let Some(origin) = options.origin.as_deref() {
                cors::validate_response(
                    &current,
                    origin,
                    &raw.headers,
                    options.cookie_jar.is_some(),
                )?;
            }
            let content_range = raw
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-range"))
                .and_then(|(_, value)| ContentRange::parse(value));
            let accept_ranges = raw.headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("accept-ranges")
                    && value
                        .split(',')
                        .any(|token| token.trim().eq_ignore_ascii_case("bytes"))
            });
            return Ok(Response {
                status: raw.status,
                reason: raw.reason,
                headers: raw.headers,
                body: raw.body,
                final_url: current,
                content_range,
                accept_ranges,
                trailers: raw.trailers,
                pushed: raw.pushed,
                pooled_stream: None,
            });
        }
        if redirects >= options.max_redirects {
            return Err(FetchError::TooManyRedirects);
        }
        let location = raw
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("location"))
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .ok_or(FetchError::RedirectWithoutLocation)?;
        current = current
            .join(location)
            .map_err(FetchError::InvalidRedirect)?;
        redirects += 1;
    }
}

/// Fetches `url` over HTTP/1.1 (or HTTPS with the `tls` feature), following redirects.
///
/// `file:` URLs are handled by [`crate::file::load_file_url`]; `data:` URLs by
/// [`crate::data::decode_data_url`] (both also exposed through
/// [`crate::ResourceLoader`]).
pub fn fetch(url: &Url, options: &FetchOptions) -> Result<Response, FetchError> {
    fetch_with_headers(url, options, &[])
}

/// Like [`fetch`], but adds extra request headers to every hop (e.g. conditional
/// revalidation headers `If-None-Match` / `If-Modified-Since` sent by the HTTP cache).
///
/// Caller headers are appended after the mandatory ones; duplicate names are allowed
/// (the server decides how to interpret them). `Host`, `Connection` and `User-Agent`
/// are managed by the client and should not be supplied here.
pub fn fetch_with_headers(
    url: &Url,
    options: &FetchOptions,
    extra_headers: &[(&str, &str)],
) -> Result<Response, FetchError> {
    fetch_with_method(url, options, "GET", extra_headers, None)
}

/// Fetches `url` with an arbitrary method and optional request body.
///
/// - `POST`/`PUT` bodies are sent with `Content-Length`; a caller-supplied
///   `Transfer-Encoding: chunked` makes the HTTP/1.1 path use chunked framing.
/// - `Expect: 100-continue` waits for the interim response before sending the body
///   (HTTP/2: interim HEADERS; HTTP/1.1: `100 Continue`), bounded by the socket timeout.
/// - Redirects: `303` (and `301/302` for non-`GET/HEAD`) switch to `GET` without body;
///   `307/308` preserve method and body.
/// - Trailing headers are exposed on [`Response::trailers`] (HTTP/2 trailers and
///   HTTP/1.1 chunked trailers).
pub fn fetch_with_method(
    url: &Url,
    options: &FetchOptions,
    method: &str,
    extra_headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Result<Response, FetchError> {
    fetch_with_request_body(
        url,
        options,
        method,
        extra_headers,
        body.map(|bytes| RequestBody::Bytes(bytes.to_vec())),
    )
}

/// Fetches with a bytes body and request trailers (h2 final HEADERS / h1 chunked).
pub fn fetch_with_trailers(
    url: &Url,
    options: &FetchOptions,
    method: &str,
    extra_headers: &[(&str, &str)],
    body: Option<&[u8]>,
    trailers: &[(String, String)],
) -> Result<Response, FetchError> {
    fetch_with_request_body_and_trailers(
        url,
        options,
        method,
        extra_headers,
        body.map(|bytes| RequestBody::Bytes(bytes.to_vec())),
        trailers,
    )
}

/// Like [`fetch_with_method`] but accepts a streaming [`RequestBody`] with backpressure.
///
/// - Byte bodies keep `Content-Length`; streaming bodies use `Transfer-Encoding: chunked`
///   on HTTP/1.1 and incremental DATA on HTTP/2 (wired in the h2 client) without ever
///   buffering the whole upload.
/// - `Expect: 100-continue` delays the first body chunk until the interim response.
/// - Streaming bodies are **single-shot**: a `307/308` redirect that would replay them
///   returns [`FetchError::RequestBody`] instead of silently truncating.
/// - `options.max_request_body` caps the bytes produced (typed `RequestBodyTooLarge`);
///   a producer error aborts with the producer's typed [`FetchError`].
pub fn fetch_with_request_body(
    url: &Url,
    options: &FetchOptions,
    method: &str,
    extra_headers: &[(&str, &str)],
    body: Option<RequestBody>,
) -> Result<Response, FetchError> {
    fetch_with_request_body_and_trailers(url, options, method, extra_headers, body, &[])
}

/// Like [`fetch_with_request_body`] but also sends **request trailers** after the body.
///
/// HTTP/2: a final HEADERS block with END_STREAM after the DATA (requires a body).
/// HTTP/1.1: chunked trailers after the `0\r\n` terminator (forces
/// `Transfer-Encoding: chunked`). Trailer names/values are validated (no pseudo-headers,
/// no control characters) and total size is capped by `max_header_bytes`.
pub fn fetch_with_request_body_and_trailers(
    url: &Url,
    options: &FetchOptions,
    method: &str,
    extra_headers: &[(&str, &str)],
    body: Option<RequestBody>,
    trailers: &[(String, String)],
) -> Result<Response, FetchError> {
    let mut method = method.trim().to_ascii_uppercase();
    if method.is_empty()
        || !method
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(FetchError::InvalidResponse(format!(
            "invalid request method `{method}`"
        )));
    }
    let streaming_body = matches!(body, Some(RequestBody::Stream(_)));
    let mut body_state = body;
    let mut first_attempt = true;
    let mut current = url.clone();
    let mut redirects = 0usize;
    loop {
        let attempt_body = match body_state.as_ref() {
            Some(bytes_or_stream) => match bytes_or_stream.try_clone() {
                Some(clone) => Some(clone),
                None if first_attempt => body_state.take(),
                None => {
                    return Err(FetchError::RequestBody(
                        "cannot replay a streaming request body on redirect".to_owned(),
                    ));
                }
            },
            None => None,
        };
        first_attempt = false;
        let raw = fetch_once(
            &current,
            options,
            &method,
            extra_headers,
            attempt_body,
            true,
            trailers,
            None,
        )?;
        if let Some(jar) = &options.cookie_jar {
            lock_jar(jar).store_from_response(&current, &raw.headers);
        }
        if !is_redirect(raw.status) {
            if let Some(origin) = options.origin.as_deref() {
                cors::validate_response(
                    &current,
                    origin,
                    &raw.headers,
                    options.cookie_jar.is_some(),
                )?;
            }
            let body = decode_response_body(&raw.headers, raw.body, options)?;
            let content_range = raw
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-range"))
                .and_then(|(_, value)| ContentRange::parse(value));
            let accept_ranges = raw.headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("accept-ranges")
                    && value
                        .split(',')
                        .any(|token| token.trim().eq_ignore_ascii_case("bytes"))
            });
            return Ok(Response {
                status: raw.status,
                reason: raw.reason,
                headers: raw.headers,
                body,
                final_url: current,
                content_range,
                accept_ranges,
                trailers: raw.trailers,
                pushed: raw.pushed,
                pooled_stream: None,
            });
        }
        if redirects >= options.max_redirects {
            return Err(FetchError::TooManyRedirects);
        }
        if raw.status == 303
            || (matches!(raw.status, 301 | 302) && method != "GET" && method != "HEAD")
        {
            method = "GET".to_owned();
            body_state = None;
        } else if streaming_body && body_state.is_none() {
            return Err(FetchError::RequestBody(
                "cannot replay a streaming request body on redirect".to_owned(),
            ));
        }
        let location = raw
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("location"))
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .ok_or(FetchError::RedirectWithoutLocation)?;
        current = current
            .join(location)
            .map_err(FetchError::InvalidRedirect)?;
        redirects += 1;
    }
}

/// Sends a request body to an HTTP/1.1 socket: raw bytes, one chunked chunk, or an
/// incremental producer (chunked, 16 KiB pulls, capped by `max_request_body`).
fn send_request_body(
    writer: &mut dyn Write,
    body: RequestBody,
    chunked: bool,
    max_request_body: usize,
    trailers: &[(String, String)],
) -> Result<(), FetchError> {
    let write_last_chunk = |writer: &mut dyn Write| -> Result<(), FetchError> {
        writer.write_all(b"0\r\n").map_err(map_io_error)?;
        for (name, value) in trailers {
            writer
                .write_all(format!("{name}: {value}\r\n").as_bytes())
                .map_err(map_io_error)?;
        }
        writer.write_all(b"\r\n").map_err(map_io_error)
    };
    let write_chunk = |writer: &mut dyn Write, chunk: &[u8]| -> Result<(), FetchError> {
        writer
            .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
            .map_err(map_io_error)?;
        writer.write_all(chunk).map_err(map_io_error)?;
        writer.write_all(b"\r\n").map_err(map_io_error)
    };
    match body {
        RequestBody::Bytes(bytes) => {
            if chunked {
                if !bytes.is_empty() {
                    write_chunk(writer, &bytes)?;
                }
                write_last_chunk(writer)
            } else {
                writer.write_all(&bytes).map_err(map_io_error)
            }
        }
        RequestBody::Stream(mut producer) => {
            let mut total = 0usize;
            loop {
                match producer.next_chunk(16 * 1024)? {
                    None => break,
                    Some(chunk) if chunk.is_empty() => break,
                    Some(chunk) => {
                        total = total.saturating_add(chunk.len());
                        if total > max_request_body {
                            return Err(FetchError::RequestBodyTooLarge);
                        }
                        write_chunk(writer, &chunk)?;
                    }
                }
            }
            write_last_chunk(writer)
        }
    }
}

fn lock_jar(jar: &Mutex<CookieJar>) -> std::sync::MutexGuard<'_, CookieJar> {
    // A poisoned jar lock (panic while holding it) must not cascade: cookie state is a
    // best-effort cache, so recover the guard and keep serving requests.
    jar.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Decodes the response body according to `Content-Encoding` when compression is enabled.
///
/// `max_response_bytes` applies to the **decompressed** size: the inflater is capped by
/// the tighter of that limit and [`FetchOptions::compression_limits`]. A body that would
/// exceed `max_response_bytes` maps to [`FetchError::ResponseTooLarge`]; the anti-bomb
/// ratio cap maps to [`FetchError::DecompressionLimit`].
fn decode_response_body(
    headers: &Headers,
    body: Vec<u8>,
    options: &FetchOptions,
) -> Result<Vec<u8>, FetchError> {
    if options.accept_encoding == AcceptEncoding::Identity {
        return Ok(body);
    }
    let content_encoding = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-encoding"))
        .map(|(_, value)| value.as_str());
    let Some(value) = content_encoding else {
        return Ok(body);
    };
    let encoding = match ContentEncoding::parse(value) {
        Ok(ContentEncoding::Identity) => return Ok(body),
        Ok(encoding) => encoding,
        Err(error) => return Err(error),
    };
    let limits = CompressionLimits::new(
        options.compression_limits.max_output,
        options.compression_limits.max_ratio,
    );
    let decoded = decode_body(encoding, &body, limits)?;
    // `max_response_bytes` also caps the decompressed representation (not just the wire
    // bytes); exceeding it is the same typed error as an oversized identity body.
    if decoded.len() > options.max_response_bytes {
        return Err(FetchError::ResponseTooLarge);
    }
    Ok(decoded)
}

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

/// Streaming body callback used by [`fetch_streaming`] and the internal body readers.
pub type StreamCallback<'a> = &'a mut dyn FnMut(&[u8]) -> Result<(), FetchError>;

#[derive(Debug)]
struct RawResponse {
    status: u16,
    reason: String,
    headers: Headers,
    body: Vec<u8>,
    trailers: Headers,
    pushed: Vec<PushedResponse>,
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

// The request/hop state is carried explicitly (method/headers/body/trailers/cookies/
// redirect-stream state); grouping it would only move the parameters into a struct.
#[allow(clippy::too_many_arguments)]
fn fetch_once(
    url: &Url,
    options: &FetchOptions,
    method: &str,
    extra_headers: &[(&str, &str)],
    body: Option<RequestBody>,
    include_cookies: bool,
    trailers: &[(String, String)],
    mut on_data: Option<StreamCallback<'_>>,
) -> Result<RawResponse, FetchError> {
    if !trailers.is_empty() {
        if body.is_none() {
            return Err(FetchError::RequestBody(
                "request trailers require a request body".to_owned(),
            ));
        }
        let mut total = 0usize;
        for (name, value) in trailers {
            let invalid = name.is_empty()
                || name.starts_with(':')
                || name
                    .bytes()
                    .any(|byte| !byte.is_ascii() || byte.is_ascii_control())
                || value.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | 0));
            if invalid {
                return Err(FetchError::InvalidResponse(
                    "invalid request trailer name/value".to_owned(),
                ));
            }
            total = total.saturating_add(name.len() + value.len() + 4);
        }
        if total > options.max_header_bytes {
            return Err(FetchError::HeadersTooLarge);
        }
    }
    let host = url.host_for_dns();
    let port = match url.scheme() {
        Scheme::Http | Scheme::Https => url
            .port_or_default()
            .ok_or_else(|| FetchError::UnsupportedScheme(url.scheme().as_str().to_owned()))?,
        Scheme::File => return Err(FetchError::UnsupportedScheme("file".to_owned())),
        Scheme::Data => return Err(FetchError::UnsupportedScheme("data".to_owned())),
    };
    if host.is_empty() {
        return Err(FetchError::Dns(url.host().to_owned()));
    }
    let host_header = match url.port() {
        Some(port) => format!("{}:{port}", url.host()),
        None => url.host().to_owned(),
    };

    // Proxy selection (R205): per scheme and per hop (`no_proxy` is checked against the
    // current target URL, so redirects can switch between proxied and direct).
    let proxy = options
        .proxy
        .as_deref()
        .and_then(|config| config.proxy_for_url(url));
    let socks_proxy = options
        .proxy
        .as_deref()
        .and_then(|config| config.socks_for(url))
        .cloned();
    let mut absolute_form: Option<String> = None;
    let mut proxy_auth_header: Option<String> = None;
    let proxy_for_407 = proxy.clone();
    let (mut stream, h2_negotiated) = match (socks_proxy, proxy) {
        (Some(socks), _) => {
            let tcp_stream = connect_tcp(&socks.host, socks.port, options)?;
            let mut tunnel = tcp_stream;
            crate::socks::connect(&mut tunnel, &socks, url.host(), port)?;
            match url.scheme() {
                Scheme::Http => (Transport::Plain(tunnel), false),
                Scheme::Https => {
                    let (transport, negotiated_h2) =
                        connect_tls(host, tunnel, options, method != "OPTIONS").map_err(
                            |error| match error {
                                FetchError::Io(reason) => {
                                    FetchError::ProxyTunnel(reason.to_string())
                                }
                                other => other,
                            },
                        )?;
                    (transport, negotiated_h2)
                }
                Scheme::File | Scheme::Data => {
                    return Err(FetchError::UnsupportedScheme(
                        url.scheme().as_str().to_owned(),
                    ));
                }
            }
        }
        (None, Some(proxy_url)) => {
            let proxy_host = proxy_url.host_for_dns();
            let proxy_port = proxy_url.port_or_default().ok_or_else(|| {
                FetchError::UnsupportedScheme(format!(
                    "proxy URL without a usable port: {proxy_url}"
                ))
            })?;
            let auth = crate::proxy::proxy_authorization(&proxy_url);
            let tcp_stream = connect_tcp(proxy_host, proxy_port, options)?;
            if url.scheme() == Scheme::Https {
                if proxy_url.scheme() == Scheme::Https {
                    return Err(FetchError::ProxyConnect(
                        "an https proxy can only forward http targets (nested TLS is not supported)"
                            .to_owned(),
                    ));
                }
                let tunnel = establish_connect_tunnel(
                    tcp_stream,
                    host,
                    port,
                    auth.as_deref(),
                    &proxy_url,
                    options,
                )?;
                let (transport, negotiated_h2) =
                    connect_tls(host, tunnel, options, method != "OPTIONS").map_err(|error| {
                        match error {
                            // IO during the handshake means the proxy closed/broke the
                            // tunnel; certificate/SNI failures stay typed as TLS errors.
                            FetchError::Io(reason) => FetchError::ProxyTunnel(reason.to_string()),
                            other => other,
                        }
                    })?;
                (transport, negotiated_h2)
            } else {
                // `http:` target: absolute-form request through the proxy. An `https:`
                // proxy wraps that hop in TLS (only for plain-http targets).
                absolute_form = Some(absolute_request_target(url, &host_header));
                proxy_auth_header = auth;
                if proxy_url.scheme() == Scheme::Https {
                    let (transport, _) = connect_tls(proxy_host, tcp_stream, options, false)?;
                    (transport, false)
                } else {
                    (Transport::Plain(tcp_stream), false)
                }
            }
        }
        (None, None) => {
            let tcp_stream = connect_tcp(host, port, options)?;
            match url.scheme() {
                Scheme::Http => (Transport::Plain(tcp_stream), false),
                Scheme::Https => {
                    let (transport, negotiated_h2) =
                        connect_tls(host, tcp_stream, options, method != "OPTIONS")?;
                    (transport, negotiated_h2)
                }
                Scheme::File | Scheme::Data => {
                    return Err(FetchError::UnsupportedScheme(
                        url.scheme().as_str().to_owned(),
                    ));
                }
            }
        }
    };
    let host_header = match url.port() {
        Some(port) => format!("{}:{port}", url.host()),
        None => url.host().to_owned(),
    };
    // Request headers shared by HTTP/1.1 and HTTP/2. Connection-specific headers
    // (`Host`, `Connection`) are added only on the HTTP/1.1 path; the HTTP/2 client
    // derives the `:authority` pseudo-header from `host_header` and drops
    // connection-specific names (RFC 9113 §8.2.2).
    let mut request_headers: Vec<(String, String)> = vec![
        ("User-Agent".to_owned(), options.user_agent.clone()),
        ("Accept".to_owned(), "*/*".to_owned()),
    ];
    if let Some(value) = options.accept_encoding.header_value() {
        request_headers.push(("Accept-Encoding".to_owned(), value.to_owned()));
    }
    if let Some(origin) = options.origin.as_deref() {
        request_headers.push(("Origin".to_owned(), origin.to_owned()));
    }
    // Cookie `SameSite` context: an explicit non-default `request_site` wins; otherwise a
    // configured referrer source lets us classify subresource requests automatically.
    let request_site = if options.request_site != RequestSite::SameSite {
        options.request_site
    } else if let Some(referrer) = &options.referrer {
        RequestSite::classify(&referrer.source, url, false)
    } else {
        RequestSite::SameSite
    };
    if include_cookies
        && let Some(jar) = &options.cookie_jar
        && let Some(cookie_header) = lock_jar(jar).header_for_url_with_site(url, request_site)
    {
        request_headers.push(("Cookie".to_owned(), cookie_header));
    }
    if let Some(range) = &options.range {
        request_headers.push(("Range".to_owned(), range.header_value()));
    }
    if let Some(referrer) = &options.referrer
        && let Some(referer) = referrer.referer_for(url)
    {
        request_headers.push(("Referer".to_owned(), referer));
    }
    for (name, value) in extra_headers {
        request_headers.push((name.to_string(), value.to_string()));
    }
    if let Some(auth) = proxy_auth_header {
        request_headers.push(("Proxy-Authorization".to_owned(), auth));
    }
    if absolute_form.is_some() {
        request_headers.push(("Proxy-Connection".to_owned(), "keep-alive".to_owned()));
    }

    if h2_negotiated {
        // TLS ALPN selected `h2`; use the HTTP/2 client and keep the same
        // RawResponse surface so redirects/cache/cookies/CORS/compression above
        // this function are unchanged.
        let limits = crate::h2::client::Limits {
            max_frame_size: crate::h2::MAX_FRAME_SIZE,
            max_header_block: options.max_header_bytes,
            max_body: options.max_response_bytes,
            enable_server_push: options.accept_push,
            max_pushed_resources: options.max_pushed_resources,
            max_pushed_body: options.max_pushed_body,
            max_pushed_total: options.max_pushed_total,
            ..crate::h2::client::Limits::default()
        };
        let mut request = crate::h2::client::H2Request::new_with_body(
            method,
            "https",
            &host_header,
            &url.request_target(),
            &request_headers,
            body,
        );
        if !trailers.is_empty() {
            request = request.with_trailers(trailers);
        }
        let mut results = if let Some(callback) = on_data.as_deref_mut() {
            crate::h2::client::request_streaming(&mut stream, vec![request], &limits, callback)?
        } else {
            crate::h2::client::request_many_owned(&mut stream, vec![request], &limits)?
        };
        let response = results
            .pop()
            .ok_or_else(|| FetchError::InvalidResponse("no HTTP/2 response".to_owned()))??;
        let pushed = response
            .pushed
            .into_iter()
            .filter_map(|pushed| {
                Url::parse(&pushed.url).ok().map(|url| PushedResponse {
                    url,
                    headers: pushed.headers,
                    body: pushed.body,
                })
            })
            .collect();
        return Ok(RawResponse {
            status: response.status,
            reason: response.reason,
            headers: response.headers,
            body: response.body,
            trailers: response.trailers,
            pushed,
        });
    }

    let expect_continue = request_headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("expect") && value.to_ascii_lowercase().contains("100-continue")
    });
    let body_is_stream = matches!(body, Some(RequestBody::Stream(_)));
    let header_chunked = request_headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
    });
    if (body_is_stream || !trailers.is_empty()) && !header_chunked {
        request_headers.push(("Transfer-Encoding".to_owned(), "chunked".to_owned()));
    }
    let chunked = body_is_stream || header_chunked || !trailers.is_empty();
    let has_content_length = request_headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-length"));
    if let Some(body) = body.as_ref()
        && let Some(length) = body.len()
        && !chunked
        && !has_content_length
    {
        request_headers.push(("Content-Length".to_owned(), length.to_string()));
    }
    let h1_target = absolute_form
        .clone()
        .unwrap_or_else(|| url.request_target());
    let http_version = if options.force_http10 { "1.0" } else { "1.1" };
    if options.force_http10 && (body_is_stream || !trailers.is_empty()) {
        return Err(FetchError::RequestBody(
            "chunked request bodies/trailers require HTTP/1.1".to_owned(),
        ));
    }
    let mut head = format!("{method} {h1_target} HTTP/{http_version}\r\nHost: {host_header}\r\n",);
    for (name, value) in &request_headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("Connection: close\r\n\r\n");

    let mut body = body;
    if expect_continue && body.is_some() {
        // Send only the head, wait for the interim response, then stream the body.
        stream.write_all(head.as_bytes()).map_err(map_io_error)?;
        stream.flush().map_err(map_io_error)?;
        let mut reader = ConnReader::new(stream);
        let interim_head = reader.read_head(options.max_header_bytes)?;
        let (status, reason, headers) = parse_head(&interim_head)?;
        if status == 100 {
            let request_body = body.take().expect("checked above");
            send_request_body(
                &mut reader,
                request_body,
                chunked,
                options.max_request_body,
                trailers,
            )?;
            reader.flush()?;
            let final_head = reader.read_head(options.max_header_bytes)?;
            let (status, reason, headers) = parse_head(&final_head)?;
            if status == 407
                && absolute_form.is_some()
                && let Some(proxy) = proxy_for_407.as_ref()
            {
                return Err(crate::proxy::auth_required_error(proxy));
            }
            let (body, trailers) =
                read_body_dispatch(&mut reader, status, &headers, options, &mut on_data)?;
            return Ok(RawResponse {
                status,
                reason,
                headers,
                body,
                trailers,
                pushed: Vec::new(),
            });
        }
        // The server answered without 100-continue: don't send the body.
        let (body, trailers) =
            read_body_dispatch(&mut reader, status, &headers, options, &mut on_data)?;
        return Ok(RawResponse {
            status,
            reason,
            headers,
            body,
            trailers,
            pushed: Vec::new(),
        });
    }

    stream.write_all(head.as_bytes()).map_err(map_io_error)?;
    if let Some(request_body) = body {
        send_request_body(
            &mut stream,
            request_body,
            chunked,
            options.max_request_body,
            trailers,
        )?;
    } else if chunked {
        stream.write_all(b"0\r\n\r\n").map_err(map_io_error)?;
    }
    stream.flush().map_err(map_io_error)?;

    let mut reader = ConnReader::new(stream);
    let final_head = reader.read_head(options.max_header_bytes)?;
    let (status, reason, headers) = parse_head(&final_head)?;
    if status == 407
        && absolute_form.is_some()
        && let Some(proxy) = proxy_for_407.as_ref()
    {
        return Err(crate::proxy::auth_required_error(proxy));
    }
    let (body, trailers) =
        read_body_dispatch(&mut reader, status, &headers, options, &mut on_data)?;
    Ok(RawResponse {
        status,
        reason,
        headers,
        body,
        trailers,
        pushed: Vec::new(),
    })
}

/// Resolves `host:port` and opens a TCP stream with the configured timeouts.
fn connect_tcp(host: &str, port: u16, options: &FetchOptions) -> Result<TcpStream, FetchError> {
    let addresses: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|error| FetchError::Dns(format!("{host}: {error}")))?
        .collect();
    if addresses.is_empty() {
        return Err(FetchError::Dns(host.to_owned()));
    }
    let mut last_error: Option<io::Error> = None;
    let mut tcp_stream = None;
    for address in &addresses {
        match TcpStream::connect_timeout(address, options.connect_timeout) {
            Ok(connected) => {
                tcp_stream = Some(connected);
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    let tcp_stream = tcp_stream.ok_or_else(|| {
        FetchError::Connect(last_error.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::AddrNotAvailable, "no address to connect to")
        }))
    })?;
    tcp_stream
        .set_read_timeout(Some(options.read_timeout))
        .map_err(FetchError::Io)?;
    tcp_stream
        .set_write_timeout(Some(options.read_timeout))
        .map_err(FetchError::Io)?;
    Ok(tcp_stream)
}

/// Absolute-form request target used by `http:` requests through a proxy.
fn absolute_request_target(url: &Url, host_header: &str) -> String {
    format!(
        "{}://{}{}",
        url.scheme().as_str(),
        host_header,
        url.request_target()
    )
}

/// Opens a `CONNECT host:port` tunnel through `proxy`, returning the raw stream.
///
/// Sends `Proxy-Authorization` when the proxy URL carries Basic credentials and maps
/// `407` to [`FetchError::ProxyAuthRequired`], other non-200 statuses to
/// [`FetchError::ProxyConnect`] and early/extra tunnel bytes to
/// [`FetchError::ProxyTunnel`].
fn establish_connect_tunnel(
    mut tcp_stream: TcpStream,
    host: &str,
    port: u16,
    authorization: Option<&str>,
    proxy: &Url,
    options: &FetchOptions,
) -> Result<TcpStream, FetchError> {
    let authority = format!("{host}:{port}");
    let mut request = format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: keep-alive\r\n"
    );
    if let Some(value) = authorization {
        request.push_str("Proxy-Authorization: ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    tcp_stream
        .write_all(request.as_bytes())
        .map_err(FetchError::Io)?;
    tcp_stream.flush().map_err(FetchError::Io)?;
    let mut reader = ConnReader::new(Transport::Plain(tcp_stream));
    let head = reader.read_head(options.max_header_bytes)?;
    let (status, _reason, _headers) = parse_head(&head)?;
    if status == 407 {
        return Err(crate::proxy::auth_required_error(proxy));
    }
    if status != 200 {
        return Err(FetchError::ProxyConnect(format!(
            "proxy CONNECT returned status {status}"
        )));
    }
    if reader.position < reader.buffer.len() {
        return Err(FetchError::ProxyTunnel(
            "unexpected bytes after the CONNECT response".to_owned(),
        ));
    }
    match reader.stream {
        Transport::Plain(stream) => Ok(stream),
        #[cfg(feature = "tls")]
        Transport::Tls(_) => Err(FetchError::ProxyTunnel(
            "TLS transport used for a plain CONNECT tunnel".to_owned(),
        )),
    }
}

/// Establishes the TLS layer over an already-connected TCP stream.
///
/// Advertises ALPN `h2` (then `http/1.1`) when `advertise_h2` is set (plain `GET`
/// requests), drives the handshake to completion and reports whether the server
/// negotiated HTTP/2. The HTTP/1.1 path is unchanged when `h2` is not selected.
#[cfg(feature = "tls")]
fn connect_tls(
    host: &str,
    mut tcp_stream: TcpStream,
    options: &FetchOptions,
    advertise_h2: bool,
) -> Result<(Transport, bool), FetchError> {
    let config = match &options.tls_config {
        Some(config) => (**config).clone(),
        None => (*default_tls_config()?).clone(),
    };
    let mut config = config;
    config.alpn_protocols = if advertise_h2 {
        vec![b"h2".to_vec(), b"http/1.1".to_vec()]
    } else {
        vec![b"http/1.1".to_vec()]
    };
    let server_name = rustls::pki_types::ServerName::try_from(host.to_owned())
        .map_err(|error| FetchError::Tls(format!("invalid TLS server name `{host}`: {error}")))?;
    let mut connection = rustls::ClientConnection::new(Arc::new(config), server_name)
        .map_err(|error| FetchError::Tls(format!("cannot create TLS client: {error}")))?;
    // Complete the handshake eagerly so ALPN (`h2` vs `http/1.1`) is known before
    // choosing the request protocol; the socket read/write timeouts are already set.
    while connection.is_handshaking() {
        connection
            .complete_io(&mut tcp_stream)
            .map_err(map_io_error)?;
    }
    let negotiate_h2 = connection.alpn_protocol() == Some(b"h2".as_slice());
    Ok((
        Transport::Tls(Box::new(rustls::StreamOwned::new(connection, tcp_stream))),
        negotiate_h2,
    ))
}

/// Without the `tls` feature, `https:` URLs fail with a typed, actionable error.
#[cfg(not(feature = "tls"))]
fn connect_tls(
    _host: &str,
    _tcp_stream: TcpStream,
    _options: &FetchOptions,
    _advertise_h2: bool,
) -> Result<(Transport, bool), FetchError> {
    Err(FetchError::TlsUnsupported)
}

/// Built-in `rustls` client configuration: webpki-roots (Mozilla) trust anchors and the
/// `ring` crypto provider with safe default protocol versions (TLS 1.2 + TLS 1.3).
#[cfg(feature = "tls")]
fn default_tls_config() -> Result<Arc<rustls::ClientConfig>, FetchError> {
    static CACHE: std::sync::OnceLock<Option<Arc<rustls::ClientConfig>>> =
        std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| build_default_tls_config().ok())
        .clone()
        .ok_or_else(|| {
            FetchError::Tls("cannot initialize the default rustls client configuration".to_owned())
        })
}

#[cfg(feature = "tls")]
fn build_default_tls_config() -> Result<Arc<rustls::ClientConfig>, rustls::Error> {
    use rustls::RootCertStore;
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// Byte-stream transport for one HTTP request: plain TCP or a rustls session.
enum Transport {
    Plain(TcpStream),
    #[cfg(feature = "tls")]
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Transport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buffer),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.read(buffer),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buffer),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.flush(),
        }
    }
}

/// Reads and parses the status line and header fields.
fn parse_head(head: &[u8]) -> Result<(u16, String, Headers), FetchError> {
    let (_, status, reason, headers) = parse_head_verbose(head)?;
    Ok((status, reason, headers))
}

/// Parses a status line returning `(major, minor)` as well, for HTTP/1.0 keep-alive and
/// connection-reuse decisions.
fn parse_head_verbose(head: &[u8]) -> Result<((u8, u8), u16, String, Headers), FetchError> {
    // HTTP header field values are ISO-8859-1 on the wire; decode lossily instead of
    // rejecting non-UTF-8 bytes (robustness: a wrong charset must not kill the fetch).
    let text = String::from_utf8_lossy(head);
    let mut lines = text.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| FetchError::InvalidResponse("empty status line".to_owned()))?;
    let mut parts = status_line.split_whitespace();
    let version = parts
        .next()
        .ok_or_else(|| FetchError::InvalidResponse("missing HTTP version".to_owned()))?;
    if !version.starts_with("HTTP/1.") {
        return Err(FetchError::InvalidResponse(format!(
            "unsupported protocol `{version}`"
        )));
    }
    let minor = version
        .rsplit_once('.')
        .and_then(|(_, minor)| minor.parse::<u8>().ok())
        .ok_or_else(|| FetchError::InvalidResponse("malformed HTTP version".to_owned()))?;
    let status_text = parts
        .next()
        .ok_or_else(|| FetchError::InvalidResponse("missing status code".to_owned()))?;
    if status_text.len() != 3 || !status_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(FetchError::InvalidResponse(format!(
            "malformed status code `{status_text}`"
        )));
    }
    let status: u16 = status_text
        .parse()
        .map_err(|_| FetchError::InvalidResponse("malformed status code".to_owned()))?;
    let reason = parts.collect::<Vec<_>>().join(" ");

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err(FetchError::InvalidResponse(
                "obsolete header line folding is not allowed".to_owned(),
            ));
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(FetchError::InvalidResponse(format!(
                "malformed header line `{line}`"
            )));
        };
        if name.is_empty() || !name.bytes().all(is_token_byte) {
            return Err(FetchError::InvalidResponse(format!(
                "invalid header name `{name}`"
            )));
        }
        headers.push((name.to_owned(), value.trim().to_owned()));
    }
    Ok(((1, minor), status, reason, headers))
}

fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

/// Reads the response body, streaming it to `on_data` when a callback is installed and
/// the response is not a redirect/interim/empty status (those must be resolved before the
/// body is exposed, so their (small) bodies are buffered as before).
fn read_body_or_stream(
    reader: &mut ConnReader,
    status: u16,
    headers: &Headers,
    options: &FetchOptions,
    on_data: Option<StreamCallback<'_>>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let no_stream =
        (100..200).contains(&status) || status == 204 || status == 304 || is_redirect(status);
    match on_data {
        Some(callback) if !no_stream => {
            stream_response_body(reader, status, headers, options, callback)
        }
        _ => read_body(reader, status, headers, options),
    }
}

/// Reborrow helper so the streaming callback can be used at several call sites.
fn read_body_dispatch(
    reader: &mut ConnReader,
    status: u16,
    headers: &Headers,
    options: &FetchOptions,
    on_data: &mut Option<StreamCallback<'_>>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    match on_data {
        Some(callback) => {
            read_body_or_stream(reader, status, headers, options, Some(&mut **callback))
        }
        None => read_body_or_stream(reader, status, headers, options, None),
    }
}

fn header_value<'a>(headers: &'a Headers, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// Streams a decoded response body to `on_data` (R212).
///
/// Identity bodies (`Content-Length`, chunked or read-to-EOF) are delivered as socket
/// chunks arrive; gzip/deflate responses are decoded incrementally with
/// [`image::StreamingInflater`] and delivered as decompressed chunks (CRC/ISIZE/Adler
/// verified at the trailer). The returned body is always empty (the caller already
/// consumed it); trailers are still returned for chunked bodies.
fn stream_response_body(
    reader: &mut ConnReader,
    _status: u16,
    headers: &Headers,
    options: &FetchOptions,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let encoding = if options.accept_encoding == AcceptEncoding::Identity {
        None
    } else {
        header_value(headers, "content-encoding")
            .and_then(|value| ContentEncoding::parse(value).ok())
            .filter(|encoding| *encoding != ContentEncoding::Identity)
    };
    if let Some(encoding) = encoding {
        return stream_encoded_body(reader, headers, options, encoding, on_data);
    }
    let chunked = header_value(headers, "transfer-encoding").is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
    });
    if chunked {
        return stream_chunked_body(
            reader,
            options.max_response_bytes,
            options.max_header_bytes,
            on_data,
        );
    }
    if let Some(length) = content_length(headers)? {
        if length > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        return stream_fixed_body(reader, length, on_data);
    }
    stream_until_eof(reader, options.max_response_bytes, on_data)
}

/// Streams a gzip/deflate response, decompressing incrementally with
/// [`image::StreamingInflater`] and delivering each decompressed chunk as it arrives.
///
/// The anti-bomb caps run while decoding: the inflater is capped by the tighter of
/// `max_response_bytes` and [`FetchOptions::compression_limits`]`::max_output`, and the
/// 1000:1 ratio (for bodies over 64 KiB) is checked after every feed. A CRC/ISIZE/Adler
/// mismatch at the trailer is a typed [`FetchError::Decompression`] and aborts the socket.
fn stream_encoded_body(
    reader: &mut ConnReader,
    headers: &Headers,
    options: &FetchOptions,
    encoding: ContentEncoding,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let compression = CompressionLimits::new(
        options.compression_limits.max_output,
        options.compression_limits.max_ratio,
    );
    let output_cap = compression.max_output.min(options.max_response_bytes);
    let mut inflater = match encoding {
        ContentEncoding::Gzip => {
            image::StreamingInflater::new_gzip(image::InflateLimits::new(output_cap))
        }
        ContentEncoding::Deflate => {
            image::StreamingInflater::new_auto_deflate(image::InflateLimits::new(output_cap))
        }
        ContentEncoding::Identity => unreachable!("identity responses are streamed directly"),
    };
    let mut compressed_seen = 0usize;
    let mut decoded_seen = 0usize;
    let mut feed = |chunk: &[u8]| -> Result<(), FetchError> {
        compressed_seen = compressed_seen.saturating_add(chunk.len());
        let mut decoded = Vec::new();
        inflater
            .feed(chunk, &mut decoded)
            .map_err(|error| map_stream_inflate_error(error, options))?;
        decoded_seen = decoded_seen.saturating_add(decoded.len());
        if decoded_seen > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        let ratio_exceeded = decoded_seen > 64 * 1024
            && (decoded_seen as u64) > (compressed_seen.max(1) as u64) * compression.max_ratio;
        if ratio_exceeded {
            return Err(FetchError::DecompressionLimit {
                max_output: compression.max_output,
                max_ratio: compression.max_ratio,
            });
        }
        if !decoded.is_empty() {
            on_data(&decoded)?;
        }
        Ok(())
    };
    let chunked = header_value(headers, "transfer-encoding").is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
    });
    let (_, trailers) = if chunked {
        stream_chunked_body(
            reader,
            options.max_response_bytes,
            options.max_header_bytes,
            &mut feed,
        )?
    } else if let Some(length) = content_length(headers)? {
        if length > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        stream_fixed_body(reader, length, &mut feed)?
    } else {
        stream_until_eof(reader, options.max_response_bytes, &mut feed)?
    };
    inflater
        .finish()
        .map_err(|error| map_stream_inflate_error(error, options))?;
    if decoded_seen > options.max_response_bytes {
        return Err(FetchError::ResponseTooLarge);
    }
    let ratio_exceeded = decoded_seen > 64 * 1024
        && (decoded_seen as u64) > (compressed_seen.max(1) as u64) * compression.max_ratio;
    if ratio_exceeded {
        return Err(FetchError::DecompressionLimit {
            max_output: compression.max_output,
            max_ratio: compression.max_ratio,
        });
    }
    Ok((Vec::new(), trailers))
}

fn map_stream_inflate_error(error: image::InflateError, options: &FetchOptions) -> FetchError {
    match error {
        image::InflateError::OutputLimitExceeded { .. } => {
            let compression = CompressionLimits::new(
                options.compression_limits.max_output,
                options.compression_limits.max_ratio,
            );
            if options.max_response_bytes <= compression.max_output {
                FetchError::ResponseTooLarge
            } else {
                FetchError::DecompressionLimit {
                    max_output: compression.max_output,
                    max_ratio: compression.max_ratio,
                }
            }
        }
        other => FetchError::Decompression(other.to_string()),
    }
}

fn stream_fixed_body(
    reader: &mut ConnReader,
    length: usize,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let mut remaining = length;
    while remaining > 0 {
        if !reader.fill()? {
            return Err(FetchError::BodyIncomplete);
        }
        let available = reader.buffer.len() - reader.position;
        let take = available.min(remaining).min(16 * 1024);
        on_data(&reader.buffer[reader.position..reader.position + take])?;
        reader.position += take;
        remaining -= take;
    }
    Ok((Vec::new(), Vec::new()))
}

fn stream_until_eof(
    reader: &mut ConnReader,
    max_bytes: usize,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let mut delivered = 0usize;
    loop {
        if !reader.fill()? {
            return Ok((Vec::new(), Vec::new()));
        }
        let available = reader.buffer.len() - reader.position;
        if delivered + available > max_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        on_data(&reader.buffer[reader.position..])?;
        reader.position = reader.buffer.len();
        delivered += available;
    }
}

fn stream_chunked_body(
    reader: &mut ConnReader,
    max_bytes: usize,
    max_header_bytes: usize,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<(Vec<u8>, Headers), FetchError> {
    let mut delivered = 0usize;
    loop {
        let Some(size_line) = reader.read_line(1024)? else {
            return Err(FetchError::BodyIncomplete);
        };
        let size_token = size_line
            .split(|byte| *byte == b';')
            .next()
            .unwrap_or(&[])
            .trim_ascii();
        let size_text = std::str::from_utf8(size_token)
            .map_err(|_| FetchError::InvalidResponse("chunk size is not ASCII".to_owned()))?;
        let size = u64::from_str_radix(size_text, 16).map_err(|_| {
            FetchError::InvalidResponse(format!("invalid chunk size `{size_text}`"))
        })?;
        if size == 0 {
            // Streaming path: the trailer section must be capped like the buffered one,
            // otherwise a `fetch_streaming` consumer can be fed unbounded memory.
            let mut trailers = Vec::new();
            let mut trailer_bytes = 0usize;
            loop {
                match reader.read_line(1024)? {
                    None => return Ok((Vec::new(), trailers)),
                    Some(line) if line.is_empty() => return Ok((Vec::new(), trailers)),
                    Some(line) => {
                        trailer_bytes = trailer_bytes.saturating_add(line.len() + 2);
                        if trailer_bytes > max_header_bytes {
                            return Err(FetchError::HeadersTooLarge);
                        }
                        let text = String::from_utf8_lossy(&line);
                        if let Some((name, value)) = text.split_once(':') {
                            trailers.push((name.trim().to_owned(), value.trim().to_owned()));
                        }
                    }
                }
            }
        }
        let Ok(size) = usize::try_from(size) else {
            return Err(FetchError::ResponseTooLarge);
        };
        if delivered.saturating_add(size) > max_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        let mut remaining = size;
        while remaining > 0 {
            if !reader.fill()? {
                return Err(FetchError::BodyIncomplete);
            }
            let available = reader.buffer.len() - reader.position;
            let take = available.min(remaining).min(16 * 1024);
            on_data(&reader.buffer[reader.position..reader.position + take])?;
            reader.position += take;
            remaining -= take;
            delivered += take;
        }
        let Some(terminator) = reader.read_line(2)? else {
            return Err(FetchError::BodyIncomplete);
        };
        if !terminator.is_empty() {
            return Err(FetchError::InvalidResponse(
                "missing CRLF after chunk data".to_owned(),
            ));
        }
    }
}

/// Returns the response's `Content-Length`, rejecting differing duplicates (RFC 9112 §6.3).
///
/// Identical duplicates are valid; taking the first one silently would let a malicious
/// server split a response across a pooled connection or a streaming callback.
fn content_length(headers: &Headers) -> Result<Option<usize>, FetchError> {
    let mut values = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.trim());
    let Some(first) = values.next() else {
        return Ok(None);
    };
    if values.any(|other| other != first) {
        return Err(FetchError::InvalidResponse(
            "conflicting Content-Length headers".to_owned(),
        ));
    }
    let length: usize = first
        .parse()
        .map_err(|_| FetchError::InvalidResponse(format!("invalid Content-Length `{first}`")))?;
    Ok(Some(length))
}

fn read_body(
    reader: &mut ConnReader,
    status: u16,
    headers: &Headers,
    options: &FetchOptions,
) -> Result<(Vec<u8>, Headers), FetchError> {
    if (100..200).contains(&status) || status == 204 || status == 304 {
        return Ok((Vec::new(), Vec::new()));
    }
    let header = |name: &str| {
        headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    };
    let chunked = header("transfer-encoding").is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
    });
    if chunked {
        return reader.read_chunked(options.max_response_bytes, options.max_header_bytes);
    }
    if let Some(length) = content_length(headers)? {
        if length > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        return Ok((reader.read_exact_body(length)?, Vec::new()));
    }
    Ok((
        reader.read_to_end_capped(options.max_response_bytes)?,
        Vec::new(),
    ))
}

// ---------------------------------------------------------------------------
// Buffered reader over the TCP stream
// ---------------------------------------------------------------------------

struct ConnReader {
    stream: Transport,
    buffer: Vec<u8>,
    position: usize,
}

impl ConnReader {
    fn new(stream: Transport) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
            position: 0,
        }
    }

    /// Ensures at least one buffered byte is available. Returns `false` on clean EOF.
    fn fill(&mut self) -> Result<bool, FetchError> {
        if self.position < self.buffer.len() {
            return Ok(true);
        }
        self.buffer.resize(8 * 1024, 0);
        self.position = 0;
        loop {
            match self.stream.read(&mut self.buffer) {
                Ok(0) => {
                    self.buffer.clear();
                    return Ok(false);
                }
                Ok(read) => {
                    self.buffer.truncate(read);
                    return Ok(true);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(map_io_error(error)),
            }
        }
    }

    fn read_byte(&mut self) -> Result<Option<u8>, FetchError> {
        if !self.fill()? {
            return Ok(None);
        }
        let byte = self.buffer[self.position];
        self.position += 1;
        Ok(Some(byte))
    }

    /// Reads response headers up to and including the terminating empty line.
    fn read_head(&mut self, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
        let mut head = Vec::with_capacity(1024);
        loop {
            let Some(byte) = self.read_byte()? else {
                return Err(FetchError::InvalidResponse(
                    "connection closed before response headers completed".to_owned(),
                ));
            };
            head.push(byte);
            if head.ends_with(b"\r\n\r\n") {
                return Ok(head);
            }
            if head.len() > max_bytes {
                return Err(FetchError::HeadersTooLarge);
            }
        }
    }

    /// Reads one CRLF-terminated line (returns its content without the line terminator).
    /// Returns `None` when the stream ends before any byte of the line.
    fn read_line(&mut self, max_bytes: usize) -> Result<Option<Vec<u8>>, FetchError> {
        let mut line = Vec::new();
        loop {
            match self.read_byte()? {
                None => {
                    if line.is_empty() {
                        return Ok(None);
                    }
                    return Err(FetchError::BodyIncomplete);
                }
                Some(b'\n') => {
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    return Ok(Some(line));
                }
                Some(byte) => {
                    line.push(byte);
                    if line.len() > max_bytes {
                        return Err(FetchError::InvalidResponse(
                            "HTTP line exceeds the configured maximum".to_owned(),
                        ));
                    }
                }
            }
        }
    }

    fn read_exact_body(&mut self, length: usize) -> Result<Vec<u8>, FetchError> {
        let mut body = Vec::with_capacity(length.min(64 * 1024));
        while body.len() < length {
            if !self.fill()? {
                return Err(FetchError::BodyIncomplete);
            }
            let available = self.buffer.len() - self.position;
            let take = available.min(length - body.len());
            body.extend_from_slice(&self.buffer[self.position..self.position + take]);
            self.position += take;
        }
        Ok(body)
    }

    fn read_to_end_capped(&mut self, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
        let mut body = Vec::new();
        loop {
            if !self.fill()? {
                return Ok(body);
            }
            let available = self.buffer.len() - self.position;
            if body.len() + available > max_bytes {
                return Err(FetchError::ResponseTooLarge);
            }
            body.extend_from_slice(&self.buffer[self.position..]);
            self.position = self.buffer.len();
        }
    }

    fn flush(&mut self) -> Result<(), FetchError> {
        self.stream.flush().map_err(map_io_error)
    }

    fn read_chunked(
        &mut self,
        max_bytes: usize,
        max_header_bytes: usize,
    ) -> Result<(Vec<u8>, Headers), FetchError> {
        let mut body = Vec::new();
        loop {
            let Some(size_line) = self.read_line(1024)? else {
                return Err(FetchError::BodyIncomplete);
            };
            let size_token = size_line
                .split(|byte| *byte == b';')
                .next()
                .unwrap_or(&[])
                .trim_ascii();
            let size_text = std::str::from_utf8(size_token)
                .map_err(|_| FetchError::InvalidResponse("chunk size is not ASCII".to_owned()))?;
            let size = u64::from_str_radix(size_text, 16).map_err(|_| {
                FetchError::InvalidResponse(format!("invalid chunk size `{size_text}`"))
            })?;
            if size == 0 {
                // Trailer headers followed by a terminating empty line, capped like the
                // response head so a peer cannot grow the trailer list without limit.
                let mut trailers = Vec::new();
                let mut trailer_bytes = 0usize;
                loop {
                    match self.read_line(1024)? {
                        None => return Ok((body, trailers)),
                        Some(line) if line.is_empty() => return Ok((body, trailers)),
                        Some(line) => {
                            trailer_bytes = trailer_bytes.saturating_add(line.len() + 2);
                            if trailer_bytes > max_header_bytes {
                                return Err(FetchError::HeadersTooLarge);
                            }
                            let text = String::from_utf8_lossy(&line);
                            if let Some((name, value)) = text.split_once(':') {
                                trailers.push((name.trim().to_owned(), value.trim().to_owned()));
                            }
                        }
                    }
                }
            }
            let Ok(size) = usize::try_from(size) else {
                return Err(FetchError::ResponseTooLarge);
            };
            // `body.len() + size` can wrap for a crafted 16-hex-digit chunk size; compare
            // against the remaining budget instead (RFC 9112 §7.1).
            if size > max_bytes.saturating_sub(body.len()) {
                return Err(FetchError::ResponseTooLarge);
            }
            body.extend_from_slice(&self.read_exact_body(size)?);
            let Some(terminator) = self.read_line(2)? else {
                return Err(FetchError::BodyIncomplete);
            };
            if !terminator.is_empty() {
                return Err(FetchError::InvalidResponse(
                    "missing CRLF after chunk data".to_owned(),
                ));
            }
        }
    }
}

impl Write for ConnReader {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.stream.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

fn map_io_error(error: io::Error) -> FetchError {
    match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => FetchError::Timeout,
        _ => {
            // `rustls::StreamOwned` surfaces handshake/record-layer failures as
            // `io::Error` wrapping `rustls::Error`: classify those as TLS errors so
            // callers can distinguish certificate problems from plain socket I/O.
            #[cfg(feature = "tls")]
            if let Some(source) = error.get_ref()
                && source.downcast_ref::<rustls::Error>().is_some()
            {
                return FetchError::Tls(error.to_string());
            }
            FetchError::Io(error)
        }
    }
}
