//! Single-request engine shared by every public fetch entry point (HTTP/1.1 and h2).

use std::io::Write;

use crate::body::RequestBody;
use crate::cookies::RequestSite;
use crate::error::FetchError;
use crate::url::Scheme;

use super::chunked::{ConnReader, send_request_body};
use super::headers::{validate_request_header, validate_request_headers};
use super::request::{lock_jar, url_basic_authorization};
use super::response::{RawResponse, parse_head};
use super::streaming::read_body_dispatch;
use super::transport::{is_route_failover_error, map_io_error, open_proxy_or_direct_transport};
use super::*;

/// #1855: validate CORS on the response head *before* any body bytes reach `on_data`.
///
/// Only the streaming path installs an `on_data` callback; the buffered path validates
/// after `fetch_once` returns, which is still before the body reaches its caller. For
/// streaming, the callback fires inside `read_body_dispatch`, so validation must happen
/// right after `parse_head` or a cross-origin response without ACAO is fully delivered
/// to the consumer before the `Err(Cors)` (Fetch §4.10).
fn validate_streaming_cors(
    url: &Url,
    options: &FetchOptions,
    headers: &Headers,
    streaming: bool,
) -> Result<(), FetchError> {
    if streaming && let Some(origin) = options.origin.as_deref() {
        crate::cors::validate_response(url, origin, headers, options.cookie_jar.is_some())?;
    }
    Ok(())
}

// The request/hop state is carried explicitly (method/headers/body/trailers/cookies/
// redirect-stream state); grouping it would only move the parameters into a struct.
#[allow(clippy::too_many_arguments)]
pub(super) fn fetch_once(
    url: &Url,
    options: &FetchOptions,
    method: &str,
    extra_headers: &[(&str, &str)],
    body: Option<RequestBody>,
    include_cookies: bool,
    trailers: &[(String, String)],
    mut on_data: Option<StreamCallback<'_>>,
) -> Result<RawResponse, FetchError> {
    // `extra_headers` are validated below; the user agent and the Origin value are framed
    // verbatim too, so they get the same treatment before the connection is opened (#635).
    validate_request_header("User-Agent", &options.user_agent)?;
    if let Some(origin) = options.origin.as_deref() {
        validate_request_header("Origin", origin)?;
    }
    // Every caller-supplied header is validated before the request is framed. `net` is the
    // last trusted decision point: the JS layer's `fetch()` also validates, but a future
    // embedder must not be able to inject framing through `extra_headers`.
    for (name, value) in extra_headers {
        validate_request_header(name, value)?;
    }
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
                // This is a local request-side validation failure, not a malformed peer
                // response: use the request-body error type like the "trailers need a body"
                // case above (#1348).
                return Err(FetchError::RequestBody(
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

    // Proxy selection (R205): per scheme, per hop, and PAC failover directives.
    let routes = options.proxy.as_deref().map_or_else(
        || vec![crate::proxy::ProxyRoute::Direct],
        |config| config.routes_for_url(url),
    );
    let mut selected_transport = None;
    for (idx, route) in routes.iter().enumerate() {
        match open_proxy_or_direct_transport(
            route,
            url,
            host,
            port,
            &host_header,
            method != "OPTIONS",
            options,
        ) {
            Ok((stream, h2_negotiated, absolute_form, proxy_auth_header)) => {
                let proxy_for_407 = match route {
                    crate::proxy::ProxyRoute::Http(proxy) => Some(proxy.clone()),
                    _ => None,
                };
                selected_transport = Some((
                    stream,
                    h2_negotiated,
                    absolute_form,
                    proxy_auth_header,
                    proxy_for_407,
                ));
                break;
            }
            Err(error) if idx + 1 < routes.len() && is_route_failover_error(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    let (mut stream, h2_negotiated, absolute_form, proxy_auth_header, proxy_for_407) =
        selected_transport
            .ok_or_else(|| FetchError::ProxyConnect("no proxy routes available".to_owned()))?;
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
    // Fetch standard, "HTTP-network-or-cache fetch": when the request URL carries
    // `user:password@`, those credentials supply a Basic `Authorization` header. The userinfo
    // is part of neither the request target nor the `Host` header, so without this the
    // credentials were parsed, kept in the URL and silently discarded (#310).
    //
    // This runs per hop inside `fetch_once`, so a redirect to a URL without credentials never
    // inherits them; cross-origin hops already drop any `Authorization` in `headers`. A
    // caller-supplied `Authorization` (e.g. a revalidation header) wins over the URL.
    // #1860: preflights run with `include_cookies = false` (credential mode "omit"),
    // so URL userinfo must not be turned into an Authorization header there either.
    // The userinfo is otherwise a legitimate credential source for real requests (#310).
    if include_cookies
        && !request_headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        && let Some(auth) = url_basic_authorization(url)
    {
        request_headers.push(("Authorization".to_owned(), auth));
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
    validate_request_headers(&request_headers, options.max_header_bytes)?;
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
            validate_streaming_cors(url, options, &headers, on_data.is_some())?;
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
        validate_streaming_cors(url, options, &headers, on_data.is_some())?;
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
    validate_streaming_cors(url, options, &headers, on_data.is_some())?;
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
