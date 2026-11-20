//! Request body streaming and stream opening.

use super::api::fragment_headers_with_end;
use super::*;

impl<'a, S: Read + Write> Session<'a, S> {
    /// Emits pending request bodies as DATA frames, respecting the peer frame size and
    /// both send windows. Streams waiting for `100-continue` are skipped until the
    /// interim response clears `awaiting_continue`. Streaming producers are pulled only
    /// when a window/frame slot is available (backpressure; never buffers the upload).
    pub(super) fn send_pending_bodies(&mut self) -> Result<(), FetchError> {
        let mut ids: Vec<u32> = self
            .streams
            .iter()
            .filter(|(_, stream)| stream.pending_body.is_some() && !stream.awaiting_continue)
            .map(|(id, _)| *id)
            .collect();
        // Higher weight first (RFC 9113 §5.3.2); ties by stream id for determinism.
        ids.sort_by(|a, b| {
            let weight_a = self.stream_weights.get(a).copied().unwrap_or(1);
            let weight_b = self.stream_weights.get(b).copied().unwrap_or(1);
            weight_b.cmp(&weight_a).then_with(|| a.cmp(b))
        });
        for stream_id in ids {
            let mut frames: Vec<Frame> = Vec::new();
            let mut producer_error: Option<FetchError> = None;
            let mut cap_error = false;
            let mut finished = false;
            {
                let Some(stream) = self.streams.get_mut(&stream_id) else {
                    continue;
                };
                let Some(mut active) = stream.pending_body.take() else {
                    continue;
                };
                loop {
                    let available =
                        stream.send_window.min(self.connection_send_window).max(0) as usize;
                    if available == 0 {
                        break;
                    }
                    let max_frame = usize::try_from(self.peer_max_frame_size.max(1))
                        .unwrap_or(DEFAULT_MAX_FRAME_SIZE as usize);
                    let max = available.min(max_frame);
                    match active.next_chunk(max) {
                        Ok(None) => {
                            // Empty DATA with END_STREAM terminates the request body.
                            frames.push(Frame::data(stream_id, &[], true));
                            finished = true;
                            break;
                        }
                        Ok(Some(chunk)) => {
                            if active.total_sent.saturating_add(chunk.len())
                                > self.limits.max_request_body
                            {
                                cap_error = true;
                                break;
                            }
                            active.total_sent += chunk.len();
                            stream.send_window -= chunk.len() as i64;
                            self.connection_send_window -= chunk.len() as i64;
                            let end = active.at_end();
                            frames.push(Frame::data(stream_id, &chunk, end));
                            if end {
                                finished = true;
                                break;
                            }
                            if chunk.is_empty() {
                                // A producer returning an empty chunk means EOF.
                                frames.pop();
                                frames.push(Frame::data(stream_id, &[], true));
                                finished = true;
                                break;
                            }
                        }
                        Err(error) => {
                            producer_error = Some(error);
                            break;
                        }
                    }
                }
                if !finished {
                    stream.pending_body = Some(active);
                }
            }
            if cap_error {
                let _ = send_frame(
                    &mut *self.stream,
                    &Frame::rst_stream(stream_id, error_code::CANCEL),
                );
                self.fail_stream(stream_id, FetchError::RequestBodyTooLarge);
                continue;
            }
            if let Some(error) = producer_error {
                let _ = send_frame(
                    &mut *self.stream,
                    &Frame::rst_stream(stream_id, error_code::CANCEL),
                );
                self.fail_stream(stream_id, error);
                continue;
            }
            let request_trailers = self
                .streams
                .get(&stream_id)
                .map(|stream| stream.request_trailers.clone())
                .unwrap_or_default();
            if finished && !request_trailers.is_empty() {
                // Trailers: remove END_STREAM from the final DATA and send a final
                // HEADERS block with END_STREAM (RFC 9113 §8.1).
                if let Some(last) = frames.last_mut()
                    && last.header.kind == kind::DATA
                {
                    last.header.flags &= !flags::END_STREAM;
                }
                let block = Encoder::new().encode(&request_trailers);
                if block.len() > self.limits.max_header_block {
                    let _ = send_frame(
                        &mut *self.stream,
                        &Frame::rst_stream(stream_id, error_code::CANCEL),
                    );
                    self.fail_stream(stream_id, FetchError::HeadersTooLarge);
                    continue;
                }
                frames.push(Frame::headers(stream_id, &block, true));
            }
            for frame in frames {
                send_frame(&mut *self.stream, &frame)?;
            }
        }
        Ok(())
    }

    pub(super) fn open_stream(&mut self, request_index: usize) -> Result<(), FetchError> {
        let (
            scheme,
            method_raw,
            authority,
            target,
            extra_headers,
            request_priority,
            body,
            request_trailers,
        ) = {
            let request = &mut self.requests[request_index];
            (
                request.scheme.clone(),
                request.method.clone(),
                request.authority.clone(),
                request.target.clone(),
                request.extra_headers.clone(),
                request.priority,
                request.body.take(),
                request.trailers.clone(),
            )
        };
        if body.is_none() && !request_trailers.is_empty() {
            self.fail_request(
                request_index,
                FetchError::RequestBody(
                    "request trailers require a request body on HTTP/2".to_owned(),
                ),
            );
            return Ok(());
        }
        if !request_trailers.is_empty() {
            let total: usize = request_trailers
                .iter()
                .map(|(name, value)| name.len() + value.len() + 4)
                .sum();
            let invalid = request_trailers.iter().any(|(name, value)| {
                name.is_empty()
                    || name.starts_with(':')
                    || name
                        .bytes()
                        .any(|byte| !byte.is_ascii() || byte.is_ascii_control())
                    || value.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | 0))
            });
            if invalid || total > self.limits.max_header_block {
                self.fail_request(
                    request_index,
                    if invalid {
                        // Local request-side validation failure, not a malformed peer response (#1348).
                        FetchError::RequestBody("invalid request trailer name/value".to_owned())
                    } else {
                        FetchError::HeadersTooLarge
                    },
                );
                return Ok(());
            }
        }
        if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
            self.fail_request(request_index, h2_error("unsupported scheme for h2"));
            return Ok(());
        }
        let method = method_raw.trim().to_ascii_uppercase();
        if method.is_empty()
            || method.len() > 32
            || !method
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            self.fail_request(request_index, h2_error("invalid HTTP method"));
            return Ok(());
        }
        if let Some(body) = &body
            && let Some(length) = body.len()
            && length > self.limits.max_request_body
        {
            self.fail_request(request_index, FetchError::RequestBodyTooLarge);
            return Ok(());
        }
        let expect_continue = extra_headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("expect")
                && value.to_ascii_lowercase().contains("100-continue")
        });
        let stream_id = self.next_stream_id;
        self.next_stream_id = self.next_stream_id.saturating_add(2);
        let priority = match request_priority {
            Some(priority) => {
                if priority.weight == 0 || priority.weight > 256 {
                    self.fail_request(
                        request_index,
                        h2_error("priority weight must be in 1..=256"),
                    );
                    return Ok(());
                }
                if priority.depends_on == stream_id {
                    self.fail_request(request_index, h2_error("stream priority depends on itself"));
                    return Ok(());
                }
                Some(priority)
            }
            None => None,
        };
        let mut headers: Vec<(String, String)> = vec![
            (":method".to_owned(), method),
            (":scheme".to_owned(), scheme.to_ascii_lowercase()),
            (":authority".to_owned(), authority),
            (":path".to_owned(), target),
        ];
        for (name, value) in &extra_headers {
            let lower = name.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "connection"
                    | "host"
                    | "keep-alive"
                    | "proxy-connection"
                    | "transfer-encoding"
                    | "upgrade"
                    | "te"
                    | "trailer"
            ) || lower.starts_with(':')
            {
                continue;
            }
            headers.push((lower, value.clone()));
        }
        let block = Encoder::new().encode(&headers);
        if block.len() > self.limits.max_header_block {
            self.fail_request(request_index, FetchError::HeadersTooLarge);
            return Ok(());
        }
        let body = body.filter(|body| !body.is_empty());
        let awaiting_continue = expect_continue && body.is_some();
        let end_stream_on_headers = body.is_none() && !awaiting_continue;
        // #1857: a single-frame HEADERS carrying a priority section gets rebuilt with
        // `Frame::headers_with_priority`, which prepends 5 bytes. Fragment 5 bytes short
        // when a priority is present so the final frame never exceeds MAX_FRAME_SIZE
        // (RFC 9113 §6.5.2). Multi-frame blocks send a separate PRIORITY frame instead.
        let fragment_max = if priority.is_some() {
            self.peer_max_frame_size.saturating_sub(5)
        } else {
            self.peer_max_frame_size
        };
        let (frames, fragment_count) =
            fragment_headers_with_end(stream_id, &block, fragment_max, end_stream_on_headers);
        if fragment_count > 1 && std::env::var_os("H2_DEBUG").is_some() {
            eprintln!("DEBUG: fragmented request headers into {fragment_count} frames");
        }
        if let Some(priority) = priority {
            if let [first] = frames.as_slice() {
                let end_stream = first.header.flags & flags::END_STREAM != 0;
                send_frame(
                    &mut *self.stream,
                    &Frame::headers_with_priority(
                        stream_id,
                        &block,
                        end_stream,
                        priority.exclusive,
                        priority.depends_on,
                        priority.weight,
                    ),
                )?;
            } else {
                send_frame(
                    &mut *self.stream,
                    &Frame::priority(
                        stream_id,
                        priority.exclusive,
                        priority.depends_on,
                        priority.weight,
                    ),
                )?;
                for frame in &frames {
                    send_frame(&mut *self.stream, frame)?;
                }
            }
        } else {
            for frame in &frames {
                send_frame(&mut *self.stream, frame)?;
            }
        }
        self.opened_stream_ids.insert(stream_id);
        self.stream_weights
            .insert(stream_id, priority.map_or(1, |priority| priority.weight));
        self.streams.insert(
            stream_id,
            OpenStream {
                request_index,
                status: None,
                headers: Vec::new(),
                body: Vec::new(),
                receive_window: i64::from(self.limits.initial_window_size),
                send_window: self.peer_initial_window,
                got_headers: false,
                pending_body: if end_stream_on_headers {
                    None
                } else {
                    body.map(ActiveBody::new)
                },
                awaiting_continue,
                trailers: Vec::new(),
                request_trailers,
            },
        );
        Ok(())
    }
}
