//! Public HTTP/2 request/response types, priorities and client limits.

use crate::body::RequestBody;
use crate::error::FetchError;

use super::{DEFAULT_MAX_FRAME_SIZE, h2_error};

/// A minimal HTTP/2 response: status, headers (lowercased) and buffered body.
#[derive(Debug)]
pub struct H2Response {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Trailing headers (a final HEADERS block with END_STREAM after the body).
    pub trailers: Vec<(String, String)>,
    /// Server-pushed responses collected while this exchange was in flight (opt-in via
    /// [`Limits::enable_server_push`]). Pushes are attached to the first successful
    /// response of a batch; resource caps in [`Limits`] bound count/body sizes.
    pub pushed: Vec<H2PushedResponse>,
}

/// One server-pushed HTTP/2 response (RFC 9113 §8.4).
#[derive(Clone, Debug)]
pub struct H2PushedResponse {
    /// Absolute URL reconstructed from the `PUSH_PROMISE` pseudo-headers
    /// (`:scheme://:authority:path`).
    pub url: String,
    /// Response status.
    pub status: u16,
    /// Canonical reason phrase for `status`.
    pub reason: String,
    /// Response headers (lowercased, pseudo-headers removed).
    pub headers: Vec<(String, String)>,
    /// Buffered response body.
    pub body: Vec<u8>,
    /// Effective push weight (1..=256): explicit PUSH_PROMISE PRIORITY weight when
    /// present, otherwise the HTTP default (16). Results are ordered by descending
    /// weight (FIFO for equal weights) when [`PushPriority::ServerWeights`] is used.
    pub weight: u16,
    /// Promise arrival order (stable FIFO tie-break for equal weights).
    pub promise_order: usize,
}

/// Streaming response callback: receives DATA payloads as frames arrive.
pub type DataCallback<'a> = &'a mut dyn FnMut(&[u8]) -> Result<(), FetchError>;

/// Stream priority (RFC 9113 §5.3): weight 1..=256, `depends_on` stream id
/// (0 = root) and the exclusive bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Priority {
    pub depends_on: u32,
    pub weight: u16,
    pub exclusive: bool,
}

/// One multiplexable HTTP/2 request (GET/POST/PUT/... with an optional body).
pub struct H2Request {
    /// HTTP method (sent as `:method`; uppercased and token-validated).
    pub method: String,
    /// `https` in production, `http` in plain-loopback tests.
    pub scheme: String,
    /// `:authority` value (`host[:port]`).
    pub authority: String,
    /// Request-target (`:path`, `/path?query`).
    pub target: String,
    /// Extra request headers (connection-specific and pseudo headers are dropped).
    pub extra_headers: Vec<(String, String)>,
    /// Optional request body (bytes or a streaming producer with backpressure).
    pub body: Option<RequestBody>,
    /// Optional stream priority (sent in the HEADERS priority section and used to
    /// order concurrent `request_many` submissions).
    pub priority: Option<Priority>,
    /// Optional request trailers: sent as a final HEADERS block with END_STREAM after
    /// the request DATA (requires a request body; no pseudo-headers; size-capped).
    pub trailers: Vec<(String, String)>,
}

impl H2Request {
    /// Builds a GET request.
    pub fn get(
        scheme: &str,
        authority: &str,
        target: &str,
        extra_headers: &[(String, String)],
    ) -> Self {
        Self::new("GET", scheme, authority, target, extra_headers, None)
    }

    /// Builds an arbitrary method with an optional body. `Expect: 100-continue` in
    /// `extra_headers` delays the DATA frames until an interim `:status 100` HEADERS.
    pub fn new(
        method: &str,
        scheme: &str,
        authority: &str,
        target: &str,
        extra_headers: &[(String, String)],
        body: Option<Vec<u8>>,
    ) -> Self {
        Self::new_with_body(
            method,
            scheme,
            authority,
            target,
            extra_headers,
            body.map(RequestBody::Bytes),
        )
    }

    /// Builds a request with an arbitrary [`RequestBody`] (bytes or streaming).
    pub fn new_with_body(
        method: &str,
        scheme: &str,
        authority: &str,
        target: &str,
        extra_headers: &[(String, String)],
        body: Option<RequestBody>,
    ) -> Self {
        Self {
            method: method.to_owned(),
            scheme: scheme.to_owned(),
            authority: authority.to_owned(),
            target: target.to_owned(),
            extra_headers: extra_headers.to_vec(),
            body,
            priority: None,
            trailers: Vec::new(),
        }
    }

    /// Sets a streaming/bytes body (builder).
    pub fn with_body(mut self, body: RequestBody) -> Self {
        self.body = Some(body);
        self
    }

    /// Sets the stream priority (weight 1..=256; invalid values fail the request
    /// with a typed HTTP/2 error).
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Sets request trailers (sent after the DATA as a final HEADERS block with
    /// END_STREAM). Requires a request body.
    pub fn with_trailers(mut self, trailers: &[(String, String)]) -> Self {
        self.trailers = trailers.to_vec();
        self
    }

    pub(super) fn clone_bytes_only(&self) -> Result<Self, FetchError> {
        let body =
            match &self.body {
                Some(body) => Some(body.try_clone().ok_or_else(|| {
                    h2_error("streaming request bodies require request_many_owned")
                })?),
                None => None,
            };
        Ok(Self {
            method: self.method.clone(),
            scheme: self.scheme.clone(),
            authority: self.authority.clone(),
            target: self.target.clone(),
            extra_headers: self.extra_headers.clone(),
            body,
            priority: self.priority,
            trailers: self.trailers.clone(),
        })
    }
}

impl std::fmt::Debug for H2Request {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("H2Request")
            .field("method", &self.method)
            .field("scheme", &self.scheme)
            .field("authority", &self.authority)
            .field("target", &self.target)
            .field("body", &self.body)
            .field("priority", &self.priority)
            .field("trailers", &self.trailers)
            .finish()
    }
}

/// How collected server pushes are ordered and prioritized (R212).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PushPriority {
    /// Order complete pushes by descending PUSH_PROMISE PRIORITY weight (FIFO for
    /// equal weights); a higher-weight promise arriving after the resource cap
    /// evicts the lowest-weight *pending* promise. Default.
    #[default]
    ServerWeights,
    /// Ignore declared weights and keep the previous FIFO promise order.
    Fifo,
}

/// Client-side limits for HTTP/2 exchanges.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum accepted frame payload size (default 16 KiB; also our advertised setting).
    pub max_frame_size: u32,
    /// Maximum assembled header block size (default
    /// [`crate::h2::MAX_HEADER_BLOCK_BYTES`]).
    pub max_header_block: usize,
    /// Maximum decoded header list size, `name + value + 32` per field (default
    /// [`crate::h2::hpack::DEFAULT_MAX_HEADER_LIST_SIZE`]).
    pub max_header_list_size: usize,
    /// Maximum buffered response body per stream.
    pub max_body: usize,
    /// Maximum number of streams this client keeps open concurrently (default 8).
    pub max_concurrent_streams: usize,
    /// Initial receive window advertised for the connection and every stream (default
    /// 65535; tests can shrink it to exercise FLOW_CONTROL errors).
    pub initial_window_size: u32,
    /// Maximum accepted request body size (default 16 MiB).
    pub max_request_body: usize,
    /// Advertise `SETTINGS_ENABLE_PUSH=1` and collect promised responses into
    /// [`H2Response::pushed`]. Default `false`: PUSH_PROMISE is a clean protocol error.
    pub enable_server_push: bool,
    /// Maximum number of pushed responses collected per exchange (default 8). Extra
    /// promises are cancelled with `RST_STREAM(CANCEL)` and ignored.
    pub max_pushed_resources: usize,
    /// Maximum body size of a single pushed response (default 1 MiB). Larger pushes are
    /// cancelled and dropped (the normal response is unaffected).
    pub max_pushed_body: usize,
    /// Maximum total body bytes of all pushed responses collected (default 4 MiB).
    pub max_pushed_total: usize,
    /// Push ordering policy (default [`PushPriority::ServerWeights`]). With
    /// [`PushPriority::Fifo`] the previous promise-arrival order is kept and the
    /// resource cap discards later promises.
    pub pushed_priority: PushPriority,
    /// Allow the HTTP/1.1 `Upgrade: h2c` handshake in [`h2c_request`].
    pub allow_h2c: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            max_header_block: crate::h2::MAX_HEADER_BLOCK_BYTES,
            max_header_list_size: crate::h2::hpack::DEFAULT_MAX_HEADER_LIST_SIZE,
            max_body: 16 * 1024 * 1024,
            max_concurrent_streams: 8,
            initial_window_size: 65_535,
            max_request_body: 16 * 1024 * 1024,
            enable_server_push: false,
            max_pushed_resources: 8,
            max_pushed_body: 1024 * 1024,
            max_pushed_total: 4 * 1024 * 1024,
            pushed_priority: PushPriority::default(),
            allow_h2c: false,
        }
    }
}
