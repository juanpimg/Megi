//! Minimal blocking HTTP/1.1 client (`GET`) on top of [`std::net::TcpStream`].
//!
//! This is the Phase 5 network bootstrap: no external crates, no TLS, no HTTP/2. It
//! covers the subset needed to load documents and subresources from plain-HTTP servers
//! (and is exercised entirely offline by the crate tests, which spin up their own
//! `127.0.0.1:0` listener).
//!
//! Features:
//!
//! - request line + `Host`, `User-Agent` (`Megi/0.1`), `Accept: */*`,
//!   `Connection: close`;
//! - configurable connect/read timeouts via [`FetchOptions`];
//! - status line + header parsing (obsolete line folding is rejected);
//! - `Content-Length` and `Transfer-Encoding: chunked` bodies;
//! - redirect following (301/302/303/307/308) with a configurable limit, resolving
//!   relative `Location` headers against the current URL;
//! - hard caps on header size and response body size to keep hostile servers from
//!   exhausting memory ([`FetchOptions::max_header_bytes`],
//!   [`FetchOptions::max_response_bytes`]).
//!
//! Limitations: `https` returns [`FetchError::TlsUnsupported`] until the TLS stack is
//! integrated; cookies, compression, proxies, HTTP/2 and redirect method rewriting beyond
//! `GET` (the client only issues `GET`) are out of scope for this bootstrap.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::error::FetchError;
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
    /// Maximum response header section size in bytes; larger headers fail with
    /// [`FetchError::HeadersTooLarge`].
    pub max_header_bytes: usize,
    /// `User-Agent` request header value.
    pub user_agent: String,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_header_bytes: DEFAULT_MAX_HEADER_BYTES,
            user_agent: DEFAULT_USER_AGENT.to_owned(),
        }
    }
}

/// HTTP header list in wire order: `(name, value)` pairs with the original name casing.
pub type Headers = Vec<(String, String)>;

/// A complete HTTP response.
#[derive(Clone, Debug)]
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
}

/// Fetches `url` over HTTP/1.1, following redirects.
///
/// Only `http:` is supported here (`https:` needs TLS, `file:` is handled by
/// [`crate::file::load_file_url`] / [`crate::ResourceLoader`]).
pub fn fetch(url: &Url, options: &FetchOptions) -> Result<Response, FetchError> {
    let mut current = url.clone();
    let mut redirects = 0usize;
    loop {
        let raw = fetch_once(&current, options)?;
        if !is_redirect(raw.status) {
            return Ok(Response {
                status: raw.status,
                reason: raw.reason,
                headers: raw.headers,
                body: raw.body,
                final_url: current,
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

#[derive(Debug)]
struct RawResponse {
    status: u16,
    reason: String,
    headers: Headers,
    body: Vec<u8>,
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn fetch_once(url: &Url, options: &FetchOptions) -> Result<RawResponse, FetchError> {
    match url.scheme() {
        Scheme::Http => {}
        Scheme::Https => return Err(FetchError::TlsUnsupported),
        Scheme::File => return Err(FetchError::UnsupportedScheme("file".to_owned())),
    }
    let host = url.host_for_dns();
    let port = url
        .port_or_default()
        .ok_or_else(|| FetchError::UnsupportedScheme(url.scheme().as_str().to_owned()))?;
    if host.is_empty() {
        return Err(FetchError::Dns(url.host().to_owned()));
    }
    let addresses: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|error| FetchError::Dns(format!("{host}: {error}")))?
        .collect();
    if addresses.is_empty() {
        return Err(FetchError::Dns(host.to_owned()));
    }

    let mut last_error: Option<io::Error> = None;
    let mut stream = None;
    for address in &addresses {
        match TcpStream::connect_timeout(address, options.connect_timeout) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    let stream = stream.ok_or_else(|| {
        FetchError::Connect(last_error.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::AddrNotAvailable, "no address to connect to")
        }))
    })?;
    stream
        .set_read_timeout(Some(options.read_timeout))
        .map_err(FetchError::Io)?;
    stream
        .set_write_timeout(Some(options.read_timeout))
        .map_err(FetchError::Io)?;

    let host_header = match url.port() {
        Some(port) => format!("{}:{port}", url.host()),
        None => url.host().to_owned(),
    };
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: {}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        url.request_target(),
        options.user_agent,
    );
    let mut stream = stream;
    stream
        .write_all(request.as_bytes())
        .map_err(FetchError::Io)?;
    stream.flush().map_err(FetchError::Io)?;

    let mut reader = ConnReader::new(stream);
    let head = reader.read_head(options.max_header_bytes)?;
    let (status, reason, headers) = parse_head(&head)?;
    let body = read_body(&mut reader, status, &headers, options)?;
    Ok(RawResponse {
        status,
        reason,
        headers,
        body,
    })
}

/// Reads and parses the status line and header fields.
fn parse_head(head: &[u8]) -> Result<(u16, String, Headers), FetchError> {
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
    Ok((status, reason, headers))
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

fn read_body(
    reader: &mut ConnReader,
    status: u16,
    headers: &Headers,
    options: &FetchOptions,
) -> Result<Vec<u8>, FetchError> {
    if (100..200).contains(&status) || status == 204 || status == 304 {
        return Ok(Vec::new());
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
        return reader.read_chunked(options.max_response_bytes);
    }
    if let Some(length_text) = header("content-length") {
        let length: usize = length_text.trim().parse().map_err(|_| {
            FetchError::InvalidResponse(format!("invalid Content-Length `{length_text}`"))
        })?;
        if length > options.max_response_bytes {
            return Err(FetchError::ResponseTooLarge);
        }
        return reader.read_exact_body(length);
    }
    reader.read_to_end_capped(options.max_response_bytes)
}

// ---------------------------------------------------------------------------
// Buffered reader over the TCP stream
// ---------------------------------------------------------------------------

struct ConnReader {
    stream: TcpStream,
    buffer: Vec<u8>,
    position: usize,
}

impl ConnReader {
    fn new(stream: TcpStream) -> Self {
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

    fn read_chunked(&mut self, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
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
                // Consume optional trailer headers up to the terminating empty line.
                loop {
                    match self.read_line(1024)? {
                        None => return Ok(body),
                        Some(line) if line.is_empty() => return Ok(body),
                        Some(_) => {}
                    }
                }
            }
            let Ok(size) = usize::try_from(size) else {
                return Err(FetchError::ResponseTooLarge);
            };
            if body.len() + size > max_bytes {
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

fn map_io_error(error: io::Error) -> FetchError {
    match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => FetchError::Timeout,
        _ => FetchError::Io(error),
    }
}
