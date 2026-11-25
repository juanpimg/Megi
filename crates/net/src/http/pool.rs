//! HTTP/1.1 keep-alive connection pool and the pooled GET fast path.

use std::io::Write;
use std::time::{Duration, Instant};

use crate::cookies::RequestSite;
use crate::cors;
use crate::error::FetchError;
use crate::url::Scheme;

use super::chunked::ConnReader;
use super::headers::validate_request_header;
use super::redirect::{is_redirect, resolve_redirect_target};
use super::request::{lock_jar, url_basic_authorization};
use super::response::{decode_response_body, parse_head_verbose, read_body};
use super::transport::{
    Transport, absolute_request_target, is_route_failover_error, map_io_error,
    open_proxy_or_direct_transport,
};
use super::*;

/// Maximum number of distinct origins retained in the HTTP/1.1 keep-alive pool (#104).
const MAX_POOLED_ORIGINS: usize = 64;

/// Per-origin idle keep-alive pool for plain HTTP/1.1 GET connections (R185).
#[derive(Debug, Default)]
pub struct H1Pool {
    entries: std::collections::HashMap<String, Vec<PooledH1>>,
    max_per_origin: usize,
    idle_timeout: Duration,
}

#[derive(Debug)]
struct PooledH1 {
    stream: Transport,
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

    fn take(&mut self, origin: &str) -> Option<Transport> {
        let now = Instant::now();
        let timeout = self.idle_timeout;
        let entries = self.entries.get_mut(origin)?;
        let mut selected = None;
        while let Some(pooled) = entries.pop() {
            if now.duration_since(pooled.last_used) < timeout {
                selected = Some(pooled.stream);
                break;
            }
        }
        if entries.is_empty() {
            self.entries.remove(origin);
        }
        selected
    }

    fn put(&mut self, origin: String, stream: Transport) {
        self.prune_expired();
        if !self.entries.contains_key(&origin) && self.entries.len() >= MAX_POOLED_ORIGINS {
            // Evict the origin whose oldest connection has been idle the longest.
            if let Some(oldest_origin) = self
                .entries
                .iter()
                .min_by_key(|(_, list)| list.first().map(|p| p.last_used))
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest_origin);
            }
        }
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

/// Fetches a navigation GET (following redirects) while keeping each hop's keep-alive
/// connection in `pool`.
///
/// Every hop uses the pooled GET path, so an idle connection is reused when available and the
/// connection earned by a redirect hop stays in the pool for the next hop. The final hop's
/// connection is left in the pool so the caller can transfer it to the loader that serves
/// CSS/scripts/images (see [`crate::ResourceLoader::absorb_pool_from`]). Redirect semantics
/// (limit, mixed content, HSTS) mirror the buffered [`fetch`] path.
pub fn fetch_pooled_following_redirects(
    url: &Url,
    options: &FetchOptions,
    pool: &mut H1Pool,
) -> Result<Response, FetchError> {
    let mut current = url.clone();
    for _ in 0..=options.max_redirects {
        let response = fetch_pooled_inner(&current, options, pool, true)?;
        if !is_redirect(response.status) {
            return Ok(response);
        }
        current = resolve_redirect_target(&current, &response.headers, false)?;
    }
    Err(FetchError::TooManyRedirects)
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
    fetch_pooled_inner(url, options, pool, false)
}

fn fetch_pooled_inner(
    url: &Url,
    options: &FetchOptions,
    pool: &mut H1Pool,
    return_redirect_hop: bool,
) -> Result<Response, FetchError> {
    // Mixed-content first, then HSTS upgrade (both may rewrite the URL before the socket opens).
    let checked_url;
    let url = if let Some(policy) = options.mixed_content {
        checked_url = policy.apply(url)?;
        &checked_url
    } else {
        url
    };
    let upgraded_url = crate::hsts::upgrade_url_with(options.hsts_store.as_deref(), url);
    let url = upgraded_url.as_ref().unwrap_or(url);
    let is_https = url.scheme() == Scheme::Https;
    let routes = options.proxy.as_deref().map_or_else(
        || vec![crate::proxy::ProxyRoute::Direct],
        |config| config.routes_for_url(url),
    );
    let has_https_proxy_to_https = routes.iter().any(|route| {
        is_https
            && matches!(route, crate::proxy::ProxyRoute::Http(proxy) if proxy.scheme() == Scheme::Https)
    });
    if !(url.scheme() == Scheme::Http || (is_https && cfg!(feature = "tls")))
        || has_https_proxy_to_https
        || !options.reuse_connections
        || options.accept_push
        || options.range.is_some()
    {
        return fetch(url, options);
    }
    let host_header = match url.port() {
        Some(port) => format!("{}:{port}", url.host()),
        None => url.host().to_owned(),
    };
    let mut last_error = None;
    'routes: for (route_idx, route) in routes.iter().enumerate() {
        let (origin, request_target, proxy_auth) = match route {
            crate::proxy::ProxyRoute::Direct => (origin_key(url), url.request_target(), None),
            crate::proxy::ProxyRoute::Http(proxy) => {
                let proxy_port = proxy.port_or_default().ok_or_else(|| {
                    FetchError::UnsupportedScheme("proxy URL without port".to_owned())
                })?;
                if is_https {
                    let origin = format!(
                        "proxy-connect:{}:{}->{}",
                        proxy.host(),
                        proxy_port,
                        origin_key(url)
                    );
                    (origin, url.request_target(), None)
                } else {
                    let origin = format!(
                        "proxy:{}://{}:{}",
                        proxy.scheme().as_str(),
                        proxy.host(),
                        proxy_port
                    );
                    let target = absolute_request_target(url, &host_header);
                    let auth = crate::proxy::proxy_authorization(proxy);
                    (origin, target, auth)
                }
            }
            crate::proxy::ProxyRoute::Socks5(socks) => (
                format!("socks5:{}:{}->{}", socks.host, socks.port, origin_key(url)),
                url.request_target(),
                None,
            ),
            crate::proxy::ProxyRoute::Socks4(socks4) => (
                format!(
                    "socks4:{}:{}->{}",
                    socks4.host,
                    socks4.port,
                    origin_key(url)
                ),
                url.request_target(),
                None,
            ),
        };
        let target_port = url
            .port_or_default()
            .unwrap_or(if is_https { 443 } else { 80 });
        for attempt in 0..2 {
            let (transport, from_pool) = match pool.take(&origin) {
                Some(transport) => (transport, true),
                None => match open_proxy_or_direct_transport(
                    route,
                    url,
                    url.host_for_dns(),
                    target_port,
                    &host_header,
                    false,
                    options,
                ) {
                    Ok((transport, _, _, _)) => (transport, false),
                    Err(error)
                        if route_idx + 1 < routes.len() && is_route_failover_error(&error) =>
                    {
                        last_error = Some(error);
                        continue 'routes;
                    }
                    Err(error) => {
                        last_error = Some(error);
                        break;
                    }
                },
            };
            let mut reader = ConnReader::new(transport);
            let http_version = if options.force_http10 { "1.0" } else { "1.1" };
            // #1856: the pooled fast path formats caller-supplied values straight into the
            // request head. Validate them like the non-pooled `fetch_once` path does
            // (`core.rs`) so CRLF/control bytes cannot inject headers or a second request
            // (RFC 9110 §5.1/§5.5, request smuggling), and honour max_header_bytes.
            validate_request_header("User-Agent", &options.user_agent)?;
            if let Some(origin_header) = options.origin.as_deref() {
                validate_request_header("Origin", origin_header)?;
            }
            let mut request = format!(
                "GET {request_target} HTTP/{http_version}\r\nHost: {host_header}\r\nUser-Agent: {}\r\nAccept: */*\r\n",
                options.user_agent,
            );
            if let Some(value) = options.accept_encoding.header_value() {
                request.push_str(&format!("Accept-Encoding: {value}\r\n"));
            }
            if let Some(origin_header) = options.origin.as_deref() {
                request.push_str(&format!("Origin: {origin_header}\r\n"));
            }
            let request_site = if options.request_site != RequestSite::SameSite {
                options.request_site
            } else if let Some(referrer) = &options.referrer {
                RequestSite::classify(&referrer.source, url, false)
            } else {
                RequestSite::SameSite
            };
            if let Some(jar) = &options.cookie_jar
                && let Some(cookie_header) =
                    lock_jar(jar).header_for_url_with_site(url, request_site)
            {
                request.push_str(&format!("Cookie: {cookie_header}\r\n"));
            }
            if let Some(referrer) = &options.referrer
                && let Some(referer) = referrer.referer_for(url)
            {
                request.push_str(&format!("Referer: {referer}\r\n"));
            }
            // Same URL-credential rule as `fetch_once` (#310).
            if let Some(auth) = url_basic_authorization(url) {
                request.push_str(&format!("Authorization: {auth}\r\n"));
            }
            if let Some(auth) = proxy_auth.as_ref() {
                request.push_str(&format!("Proxy-Authorization: {auth}\r\n"));
            }
            if options.force_http10 {
                request.push_str("Connection: keep-alive\r\n");
            }
            request.push_str("\r\n");
            // #1856: the request head is framed before the socket write; enforce the same
            // total-size cap the non-pooled path applies via validate_request_headers.
            if request.len() > options.max_header_bytes {
                return Err(FetchError::HeadersTooLarge);
            }
            let result = (|| -> Result<Response, FetchError> {
                reader
                    .stream
                    .write_all(request.as_bytes())
                    .map_err(map_io_error)?;
                reader.stream.flush().map_err(map_io_error)?;
                let head = reader.read_head(options.max_header_bytes)?;
                let (version, status, reason, headers) = parse_head_verbose(&head)?;
                if is_https
                    && let Some(header) = headers
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case("strict-transport-security"))
                {
                    crate::hsts::observe_https_with(
                        options.hsts_store.as_deref(),
                        url.host(),
                        &header.1,
                    );
                }
                if is_redirect(status) && !return_redirect_hop {
                    match options.redirect_mode {
                        RedirectMode::Follow => {
                            return Err(FetchError::InvalidResponse(
                                "redirect in pooled fetch".to_owned(),
                            ));
                        }
                        RedirectMode::Error => {
                            if let Some(req_origin) = options.origin.as_deref() {
                                cors::validate_response(
                                    url,
                                    req_origin,
                                    &headers,
                                    options.cookie_jar.is_some(),
                                )?;
                            }
                            return Err(FetchError::RedirectDisallowed(status));
                        }
                        RedirectMode::Manual => {}
                    }
                }
                let has_length = headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("content-length"));
                let is_chunked = headers.iter().any(|(name, value)| {
                    name.eq_ignore_ascii_case("transfer-encoding")
                        && value.to_ascii_lowercase().contains("chunked")
                });
                let connection_close = headers.iter().any(|(name, value)| {
                    name.eq_ignore_ascii_case("connection")
                        && value
                            .split(',')
                            .any(|token| token.trim().eq_ignore_ascii_case("close"))
                });
                let (raw_body, trailers) = read_body(&mut reader, status, &headers, options)?;
                let body = decode_response_body(&headers, raw_body, options)?;
                if let Some(jar) = &options.cookie_jar {
                    lock_jar(jar).store_from_response(url, &headers);
                }
                if let Some(req_origin) = options.origin.as_deref() {
                    cors::validate_response(
                        url,
                        req_origin,
                        &headers,
                        options.cookie_jar.is_some(),
                    )?;
                }
                let connection_keep_alive = headers.iter().any(|(name, value)| {
                    name.eq_ignore_ascii_case("connection")
                        && value
                            .split(',')
                            .any(|token| token.trim().eq_ignore_ascii_case("keep-alive"))
                });
                let mut keep_alive = if version == (1, 0) {
                    (has_length || is_chunked) && connection_keep_alive && !connection_close
                } else {
                    (has_length || is_chunked) && !connection_close
                };
                if reader.position < reader.buffer.len() {
                    keep_alive = false; // unexpected pipelined bytes: do not reuse
                }
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
                    pooled_stream: keep_alive.then_some((origin.clone(), reader.stream)),
                })
            })();
            match result {
                Ok(mut response) => {
                    if let Some((origin, stream)) = response.pooled_stream.take() {
                        pool.put(origin, stream);
                    }
                    return Ok(response);
                }
                Err(error @ (FetchError::Cors(_) | FetchError::RedirectDisallowed(_))) => {
                    return Err(error);
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt == 0 && from_pool {
                        continue;
                    }
                    break;
                }
            }
        }
        break;
    }
    // Redirects/encoded responses/races fall back to the buffered fetch (semantics first).
    let is_pooled_redirect = matches!(
        &last_error,
        Some(FetchError::InvalidResponse(msg)) if msg == "redirect in pooled fetch"
    );
    match fetch(url, options) {
        Ok(response) => Ok(response),
        Err(error)
            if is_pooled_redirect
                || matches!(
                    error,
                    FetchError::MixedContentBlocked(_) | FetchError::Cors(_)
                ) =>
        {
            Err(error)
        }
        Err(_) => Err(last_error.unwrap_or(FetchError::InvalidResponse(
            "pooled fetch failed".to_owned(),
        ))),
    }
}
