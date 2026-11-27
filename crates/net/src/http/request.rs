//! Public fetch entry points and request/hop state assembly (methods, bodies, auth retry).

use std::sync::Mutex;

use crate::body::RequestBody;
use crate::cookies::CookieJar;
use crate::cors;
use crate::error::FetchError;
use crate::range::ContentRange;
use crate::url::{Scheme, Url};

use super::core::fetch_once;
use super::redirect::{is_redirect, resolve_redirect_target};
use super::response::decode_response_body;
use super::*;

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
/// When [`FetchOptions::credential_store`] has credentials for the request origin, a `401`
/// response carrying a Basic `WWW-Authenticate` challenge is retried once with an
/// `Authorization` header (#1533). A request that already carried an `Authorization` header
/// is never retried.
pub fn fetch_with_request_body_and_trailers(
    url: &Url,
    options: &FetchOptions,
    method: &str,
    extra_headers: &[(&str, &str)],
    body: Option<RequestBody>,
    trailers: &[(String, String)],
) -> Result<Response, FetchError> {
    // Streaming bodies are single-shot: send the original and never retry. Buffered
    // bodies can be cloned so the first attempt and the auth retry each get a copy.
    let retry_body = body.as_ref().and_then(|body| body.try_clone());
    let retryable = body.is_none() || retry_body.is_some();
    let first_body = if retryable {
        body.as_ref().and_then(|body| body.try_clone())
    } else {
        body
    };
    let first = fetch_with_request_body_and_trailers_once(
        url,
        options,
        method,
        extra_headers,
        first_body,
        trailers,
    )?;
    if !retryable
        || first.status != 401
        || extra_headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
    {
        return Ok(first);
    }
    if !crate::auth::www_authenticate(&first.headers)
        .is_some_and(|challenge| challenge.scheme == "basic")
    {
        return Ok(first);
    }
    let Some(store) = options.credential_store.as_ref() else {
        return Ok(first);
    };
    let origin = format!(
        "{}://{}:{}",
        url.scheme().as_str(),
        url.host(),
        url.port_or_default().unwrap_or(0)
    );
    let credentials = store
        .lock()
        .ok()
        .and_then(|guard| guard.get(&origin))
        .or_else(|| url_basic_authorization(url).map(|_| (String::new(), String::new())));
    let Some((user, password)) = credentials else {
        return Ok(first);
    };
    if user.is_empty() && password.is_empty() {
        return Ok(first);
    }
    let mut auth_headers: Vec<(&str, String)> = extra_headers
        .iter()
        .map(|(name, value)| (*name, (*value).to_owned()))
        .collect();
    auth_headers.push((
        "Authorization",
        crate::auth::basic_authorization(&user, &password),
    ));
    let header_refs: Vec<(&str, &str)> = auth_headers
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    fetch_with_request_body_and_trailers_once(
        url,
        options,
        method,
        &header_refs,
        retry_body,
        trailers,
    )
}

/// HTTP/1.1: chunked trailers after the `0\r\n` terminator (forces
/// `Transfer-Encoding: chunked`). Trailer names/values are validated (no pseudo-headers,
/// no control characters) and total size is capped by `max_header_bytes`.
fn fetch_with_request_body_and_trailers_once(
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
    let mut headers = extra_headers.to_vec();
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
        // Mixed content (#1528) and HSTS (#1529) apply on the initial URL and every hop.
        if let Some(policy) = options.mixed_content {
            current = policy.apply(&current)?;
        }
        if let Some(upgraded) =
            crate::hsts::upgrade_url_with(options.hsts_store.as_deref(), &current)
        {
            current = upgraded;
        }
        let raw = fetch_once(
            &current,
            options,
            &method,
            &headers,
            attempt_body,
            true,
            trailers,
            None,
        )?;
        if let Some(jar) = &options.cookie_jar {
            lock_jar(jar).store_from_response(&current, &raw.headers);
        }
        // HSTS (#1529): learn from any trusted HTTPS response before `raw` is moved.
        if current.scheme() == Scheme::Https
            && let Some(header) = raw
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("strict-transport-security"))
        {
            crate::hsts::observe_https_with(
                options.hsts_store.as_deref(),
                current.host(),
                &header.1,
            );
        }
        // Per-hop CORS (#1536): intermediate 3xx hops must also pass CORS when `origin` is set.
        if let Some(origin) = options.origin.as_deref() {
            cors::validate_response(&current, origin, &raw.headers, options.cookie_jar.is_some())?;
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
        if !is_redirect(raw.status) {
            let body = decode_response_body(&raw.headers, raw.body, options)?;
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
        match options.redirect_mode {
            RedirectMode::Error => return Err(FetchError::RedirectDisallowed(raw.status)),
            RedirectMode::Manual => {
                let body = decode_response_body(&raw.headers, raw.body, options)?;
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
            RedirectMode::Follow => {}
        }
        if redirects >= options.max_redirects {
            return Err(FetchError::TooManyRedirects);
        }
        // WHATWG Fetch §4.5: a 303 switches to GET only for non-GET/HEAD methods.
        if (raw.status == 303 && method != "GET" && method != "HEAD")
            || (matches!(raw.status, 301 | 302) && method == "POST")
        {
            method = "GET".to_owned();
            body_state = None;
        } else if streaming_body && body_state.is_none() {
            return Err(FetchError::RequestBody(
                "cannot replay a streaming request body on redirect".to_owned(),
            ));
        }
        let next = resolve_redirect_target(&current, &raw.headers, options.allow_https_downgrade)?;
        // Strip sensitive caller headers on cross-origin redirects to prevent credential leakage.
        if next.scheme() != current.scheme()
            || next.host() != current.host()
            || next.port() != current.port()
        {
            headers.retain(|(name, _)| {
                !name.eq_ignore_ascii_case("authorization")
                    && !name.eq_ignore_ascii_case("proxy-authorization")
                    && !name.eq_ignore_ascii_case("cookie")
            });
        }
        current = next;
        redirects += 1;
    }
}

pub(super) fn lock_jar(jar: &Mutex<CookieJar>) -> std::sync::MutexGuard<'_, CookieJar> {
    // A poisoned jar lock (panic while holding it) must not cascade: cookie state is a
    // best-effort cache, so recover the guard and keep serving requests.
    jar.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Builds the `Authorization` value for a URL that carries `user:password@` credentials.
///
/// The userinfo is stored percent-encoded (WHATWG URL), so it is decoded first
/// (`%40` → `@`); the resulting `user:password` bytes are Base64-encoded, which also makes
/// header injection impossible regardless of what the URL contains (#310).
pub(super) fn url_basic_authorization(url: &Url) -> Option<String> {
    if url.username().is_empty() {
        return None;
    }
    let username = crate::url::percent_decode(url.username());
    let password = crate::url::percent_decode(url.password());
    let credentials = format!("{username}:{password}");
    Some(format!(
        "Basic {}",
        crate::data::base64_encode(credentials.as_bytes())
    ))
}
