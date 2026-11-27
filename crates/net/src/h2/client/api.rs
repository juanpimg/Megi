//! Public request helpers (buffered/streaming/multiplexed) and the h2c upgrade.

use crate::http::validate_request_headers;

use super::*;

/// Splits an encoded header block into HEADERS + CONTINUATION frames honouring the peer
/// maximum frame size. Returns the frames and their count for diagnostics/tests.
pub(super) fn fragment_headers_with_end(
    stream_id: u32,
    block: &[u8],
    max_frame_size: u32,
    end_stream: bool,
) -> (Vec<Frame>, usize) {
    let max = usize::try_from(max_frame_size.max(1))
        .unwrap_or(16_384)
        .max(1);
    let mut frames = Vec::new();
    let mut offset = 0usize;
    let mut first = true;
    loop {
        let end = (offset + max).min(block.len());
        let chunk = &block[offset..end];
        let last = end == block.len();
        if first {
            let mut frame_flags = if end_stream { flags::END_STREAM } else { 0 };
            if last {
                frame_flags |= flags::END_HEADERS;
            }
            frames.push(Frame::new(
                kind::HEADERS,
                frame_flags,
                stream_id,
                chunk.to_vec(),
            ));
            first = false;
        } else {
            let mut frame_flags = 0;
            if last {
                frame_flags |= flags::END_HEADERS;
            }
            frames.push(Frame::new(
                kind::CONTINUATION,
                frame_flags,
                stream_id,
                chunk.to_vec(),
            ));
        }
        offset = end;
        if last {
            break;
        }
    }
    let length = frames.len();
    (frames, length)
}

/// Performs a single HTTP/2 `GET` on `stream` (the public `load` path).
///
/// `scheme` is `https` for production (the TLS path) and `http` may be used by in-process
/// tests; `authority` is the `Host`-style authority and `target` the request-target
/// (`/path?query`). `extra_headers` are sent after the pseudo headers; connection-specific
/// names are dropped (RFC 9113 §8.2.2).
pub fn get<S: Read + Write>(
    stream: &mut S,
    scheme: &str,
    authority: &str,
    target: &str,
    extra_headers: &[(String, String)],
    limits: &Limits,
) -> Result<H2Response, FetchError> {
    let request = H2Request::get(scheme, authority, target, extra_headers);
    let mut results = get_many(stream, &[request], limits)?;
    match results.pop() {
        Some(result) => result,
        None => Err(h2_error("HTTP/2 request produced no response")),
    }
}

/// Sends `requests` concurrently over one HTTP/2 connection.
///
/// Returns one result per request in submission order; a per-stream failure (e.g.
/// `RST_STREAM`) is returned as `Err` for that stream while the others continue.
/// Connection-level problems (GOAWAY, protocol/flow-control errors, EOF) return
/// `Err(FetchError::Http2)` for the whole batch.
pub fn get_many<S: Read + Write>(
    stream: &mut S,
    requests: &[H2Request],
    limits: &Limits,
) -> Result<Vec<Result<H2Response, FetchError>>, FetchError> {
    let owned = requests
        .iter()
        .map(H2Request::clone_bytes_only)
        .collect::<Result<Vec<_>, _>>()?;
    run_requests(stream, owned, limits)
}

/// Additive owned-batch API used for streaming request bodies (and by `http`).
///
/// Identical to [`request_many`] but takes ownership of the requests so
/// [`RequestBody::Stream`] producers can be consumed incrementally (single-shot).
pub fn request_many_owned<S: Read + Write>(
    stream: &mut S,
    requests: Vec<H2Request>,
    limits: &Limits,
) -> Result<Vec<Result<H2Response, FetchError>>, FetchError> {
    run_requests(stream, requests, limits)
}

fn run_requests<S: Read + Write>(
    stream: &mut S,
    requests: Vec<H2Request>,
    limits: &Limits,
) -> Result<Vec<Result<H2Response, FetchError>>, FetchError> {
    let mut session = Session::new(stream, requests, limits, None);
    session.run()?;
    session.sort_pushed_results();
    let pushed = std::mem::take(&mut session.pushed_results);
    let mut results: Vec<Result<H2Response, FetchError>> = session
        .results
        .into_iter()
        .map(|result| result.expect("all requests resolved"))
        .collect();
    if !pushed.is_empty()
        && let Some(Ok(first)) = results.iter_mut().find(|result| result.is_ok())
    {
        first.pushed = pushed;
    }
    Ok(results)
}

/// Streaming variant of [`get_many`]: response DATA is delivered to `on_data` as frames
/// arrive (flow-controlled per window); accumulated bodies are left empty. If the callback
/// returns an error the stream is reset with `RST_STREAM(CANCEL)` and the error is returned.
pub fn request_streaming<S: Read + Write>(
    stream: &mut S,
    requests: Vec<H2Request>,
    limits: &Limits,
    on_data: DataCallback<'_>,
) -> Result<Vec<Result<H2Response, FetchError>>, FetchError> {
    let mut session = Session::new(stream, requests, limits, Some(on_data));
    session.run()?;
    session.sort_pushed_results();
    let pushed = std::mem::take(&mut session.pushed_results);
    let mut results: Vec<Result<H2Response, FetchError>> = session
        .results
        .into_iter()
        .map(|result| result.expect("all requests resolved"))
        .collect();
    if !pushed.is_empty()
        && let Some(Ok(first)) = results.iter_mut().find(|result| result.is_ok())
    {
        first.pushed = pushed;
    }
    Ok(results)
}

/// Single-request convenience wrapper around [`request_streaming`].
pub fn get_streaming<S: Read + Write>(
    stream: &mut S,
    scheme: &str,
    authority: &str,
    target: &str,
    extra_headers: &[(String, String)],
    limits: &Limits,
    on_data: &mut dyn FnMut(&[u8]) -> Result<(), FetchError>,
) -> Result<H2Response, FetchError> {
    let request = H2Request::get(scheme, authority, target, extra_headers);
    let mut results = request_streaming(stream, vec![request], limits, on_data)?;
    match results.pop() {
        Some(result) => result,
        None => Err(h2_error("HTTP/2 streaming request produced no response")),
    }
}

/// Additive API for arbitrary methods/bodies/trailers over one HTTP/2 connection.
///
/// Each [`H2Request`] may carry a body; DATA frames are fragmented to the peer
/// `MAX_FRAME_SIZE`, gated by the connection/stream send windows (peer
/// `WINDOW_UPDATE`s are processed while sending), and END_STREAM is set only on the
/// last DATA frame. `Expect: 100-continue` waits for the interim `:status 100` HEADERS
/// before sending the body (no timer inside the client: the socket read timeout bounds
/// the wait; a final response without 100 cancels the body). A `RST_STREAM` while a body
/// is pending fails only that stream.
pub fn request_many<S: Read + Write>(
    stream: &mut S,
    requests: &[H2Request],
    limits: &Limits,
) -> Result<Vec<Result<H2Response, FetchError>>, FetchError> {
    get_many(stream, requests, limits)
}

/// Opt-in cleartext HTTP/2 upgrade (`Upgrade: h2c`, RFC 7540 §3.2).
///
/// Sends an HTTP/1.1 `GET` with `Upgrade: h2c` and `HTTP2-Settings` (the same
/// SETTINGS this client would send after the preface). If the server answers
/// `101 Switching Protocols`, the request continues as HTTP/2 stream 1 and the
/// decoded response is returned as `Some`. Any other status is returned as `None`
/// (the HTTP/1.1 response head has been consumed; callers that want to fall back
/// to HTTP/1.1 must reconnect). Requires [`Limits::allow_h2c`].
pub fn h2c_request<S: Read + Write>(
    stream: &mut S,
    authority: &str,
    target: &str,
    extra_headers: &[(String, String)],
    limits: &Limits,
) -> Result<Option<H2Response>, FetchError> {
    if !limits.allow_h2c {
        return Ok(None);
    }
    let settings = Frame::settings(&[
        (
            settings::ENABLE_PUSH,
            if limits.enable_server_push { 1 } else { 0 },
        ),
        (
            settings::MAX_CONCURRENT_STREAMS,
            u32::try_from(limits.max_concurrent_streams).unwrap_or(u32::MAX),
        ),
        (settings::INITIAL_WINDOW_SIZE, limits.initial_window_size),
        (
            settings::MAX_FRAME_SIZE,
            limits.max_frame_size.max(DEFAULT_MAX_FRAME_SIZE),
        ),
        (settings::HEADER_TABLE_SIZE, 4096),
        (
            settings::MAX_HEADER_LIST_SIZE,
            u32::try_from(limits.max_header_list_size).unwrap_or(u32::MAX),
        ),
    ]);
    let mut head = format!(
        "GET {target} HTTP/1.1\r\nHost: {authority}\r\n\
         Connection: Upgrade, HTTP2-Settings\r\nUpgrade: h2c\r\n\
         HTTP2-Settings: {}\r\n",
        base64_url_nopad(&settings.payload)
    );
    let mut emitted = Vec::with_capacity(extra_headers.len());
    for (name, value) in extra_headers {
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "connection"
                | "host"
                | "upgrade"
                | "http2-settings"
                | "keep-alive"
                | "transfer-encoding"
        ) {
            continue;
        }
        emitted.push((name.clone(), value.clone()));
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    // RFC 9110 §5.1/§5.3: the upgrade head is framed by hand, so validate exactly the
    // headers emitted here (and their total size) before any byte reaches the socket (#2358).
    validate_request_headers(&emitted, limits.max_header_block)?;
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).map_err(map_io)?;
    stream.flush().map_err(map_io)?;

    let mut response_head = Vec::new();
    let mut byte = [0u8; 1];
    let max_head = limits.max_header_block.min(64 * 1024);
    while !response_head.ends_with(b"\r\n\r\n") {
        let count = stream.read(&mut byte).map_err(map_io)?;
        if count == 0 {
            return Err(h2_error(
                "connection closed during the h2c upgrade handshake",
            ));
        }
        response_head.push(byte[0]);
        if response_head.len() > max_head {
            return Err(FetchError::HeadersTooLarge);
        }
    }
    let first_line_end = response_head
        .windows(2)
        .position(|window| window == b"\r\n")
        .ok_or_else(|| h2_error("malformed h2c upgrade response"))?;
    let status_line = std::str::from_utf8(&response_head[..first_line_end])
        .map_err(|_| h2_error("non-ASCII h2c upgrade status line"))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| h2_error("malformed h2c upgrade status line"))?;
    if status != 101 {
        return Ok(None);
    }
    let request = H2Request::get("http", authority, target, extra_headers);
    let mut results = request_many(stream, &[request], limits)?;
    match results.pop() {
        Some(result) => result.map(Some),
        None => Err(h2_error("h2c upgrade produced no response")),
    }
}

fn base64_url_nopad(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        output.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        output.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(triple & 0x3f) as usize] as char);
        }
    }
    output
}
