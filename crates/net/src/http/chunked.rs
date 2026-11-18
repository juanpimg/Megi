//! Chunked transfer coding: request-body framing and the buffered chunked reader.

use std::io::{self, Read, Write};

use crate::body::RequestBody;
use crate::error::FetchError;

use super::transport::{Transport, map_io_error};
use super::*;

pub(super) fn send_request_body(
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

// ---------------------------------------------------------------------------
// Buffered reader over the TCP stream
// ---------------------------------------------------------------------------

pub(super) struct ConnReader {
    pub(super) stream: Transport,
    pub(super) buffer: Vec<u8>,
    pub(super) position: usize,
}
impl ConnReader {
    pub(super) fn new(stream: Transport) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
            position: 0,
        }
    }

    /// Ensures at least one buffered byte is available. Returns `false` on clean EOF.
    pub(super) fn fill(&mut self) -> Result<bool, FetchError> {
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

    pub(super) fn read_byte(&mut self) -> Result<Option<u8>, FetchError> {
        if !self.fill()? {
            return Ok(None);
        }
        let byte = self.buffer[self.position];
        self.position += 1;
        Ok(Some(byte))
    }

    /// Reads response headers up to and including the terminating empty line.
    pub(super) fn read_head(&mut self, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
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
    pub(super) fn read_line(&mut self, max_bytes: usize) -> Result<Option<Vec<u8>>, FetchError> {
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

    pub(super) fn read_exact_body(&mut self, length: usize) -> Result<Vec<u8>, FetchError> {
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

    pub(super) fn read_to_end_capped(&mut self, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
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

    pub(super) fn flush(&mut self) -> Result<(), FetchError> {
        self.stream.flush().map_err(map_io_error)
    }

    pub(super) fn read_chunked(
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

pub(super) fn stream_chunked_body(
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
