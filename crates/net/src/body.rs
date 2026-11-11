//! Streaming request bodies with backpressure.
//!
//! [`RequestBody`] is either an immutable in-memory buffer ([`RequestBody::Bytes`]) or a
//! pull-based producer ([`RequestBody::Stream`]). The HTTP/2 and HTTP/1.1 clients pull
//! chunks **only when the transport is ready to write them** (HTTP/2 send windows /
//! `MAX_FRAME_SIZE`; HTTP/1.1 chunked Writes), so a large upload is never buffered whole.
//!
//! Contract for a producer:
//!
//! - `next_chunk(max)` returns at most `max` bytes (an empty `Vec`/`None` means EOF);
//! - the producer is called on the same thread that drives the connection (no worker
//!   threads are spawned by the client);
//! - returning a typed [`FetchError`] aborts the upload (HTTP/2 sends
//!   `RST_STREAM(CANCEL)`; HTTP/1.1 closes the connection with a typed error);
//! - streaming bodies are **single-shot**: they cannot be replayed on `307/308`
//!   redirects (the client returns a typed error instead of silently truncating).

use crate::FetchError;

/// A pull-based request-body producer (`max` bytes at most per call).
pub trait BodyProducer: Send {
    /// Returns up to `max` bytes, or `None`/empty for end of stream.
    fn next_chunk(&mut self, max: usize) -> Result<Option<Vec<u8>>, FetchError>;
}

impl<F> BodyProducer for F
where
    F: FnMut(usize) -> Result<Option<Vec<u8>>, FetchError> + Send,
{
    fn next_chunk(&mut self, max: usize) -> Result<Option<Vec<u8>>, FetchError> {
        self(max)
    }
}

/// Request body: bytes or a streaming producer.
pub enum RequestBody {
    /// In-memory body (`Content-Length` on HTTP/1.1, normal DATA frames on HTTP/2).
    Bytes(Vec<u8>),
    /// Streaming body (HTTP/1.1 `Transfer-Encoding: chunked`; HTTP/2 incremental DATA).
    Stream(Box<dyn BodyProducer>),
}

impl RequestBody {
    /// Bytes helper.
    pub fn bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self::Bytes(bytes.into())
    }

    /// Wraps a pull-based producer.
    pub fn stream(producer: impl BodyProducer + 'static) -> Self {
        Self::Stream(Box::new(producer))
    }

    /// Total length when known (`None` for streams).
    pub fn len(&self) -> Option<usize> {
        match self {
            Self::Bytes(bytes) => Some(bytes.len()),
            Self::Stream(_) => None,
        }
    }

    /// Returns `true` when the body is known to be empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Bytes(bytes) if bytes.is_empty())
    }

    /// Clones only in-memory bodies (streams are single-shot).
    pub(crate) fn try_clone(&self) -> Option<Self> {
        match self {
            Self::Bytes(bytes) => Some(Self::Bytes(bytes.clone())),
            Self::Stream(_) => None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn into_bytes(self) -> Option<Vec<u8>> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            Self::Stream(_) => None,
        }
    }
}

impl From<Vec<u8>> for RequestBody {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Bytes(bytes)
    }
}

impl From<&[u8]> for RequestBody {
    fn from(bytes: &[u8]) -> Self {
        Self::Bytes(bytes.to_vec())
    }
}

impl std::fmt::Debug for RequestBody {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytes(bytes) => formatter
                .debug_tuple("RequestBody::Bytes")
                .field(&bytes.len())
                .finish(),
            Self::Stream(_) => formatter.write_str("RequestBody::Stream"),
        }
    }
}
