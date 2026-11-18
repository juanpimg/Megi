//! Frame handling, flow control and failure paths.

use super::*;

impl<'a, S: Read + Write> Session<'a, S> {
    /// Records a per-stream failure and releases the request slot.
    pub(super) fn fail_stream(&mut self, stream_id: u32, error: FetchError) {
        if let Some(stream) = self.streams.remove(&stream_id) {
            self.results[stream.request_index] = Some(Err(error));
            self.submit_until_cap().ok();
        }
    }

    pub(super) fn fail_request(&mut self, request_index: usize, error: FetchError) {
        if self.results[request_index].is_none() {
            self.results[request_index] = Some(Err(error));
        }
    }

    /// Best-effort flow-control replenishment: if the peer already closed the connection the
    /// update is unnecessary (there is no data left to receive), so I/O errors are ignored.
    pub(super) fn send_window_update(&mut self, stream_id: u32, increment: u32) {
        if increment == 0 {
            return;
        }
        let _ = send_frame(
            &mut *self.stream,
            &Frame::window_update(stream_id, increment),
        );
    }

    pub(super) fn handle_frame(&mut self, frame: Frame) -> Result<(), FetchError> {
        let stream_id = frame.header.stream_id;
        match frame.header.kind {
            kind::SETTINGS => {
                if frame.header.flags & flags::ACK != 0 {
                    let _ = frame.parse_settings()?;
                    return Ok(());
                }
                let entries = frame.parse_settings()?;
                for (identifier, value) in entries {
                    match identifier {
                        // `SETTINGS_HEADER_TABLE_SIZE` sizes the *encoder's* table. Our
                        // encoder is stateless, so it has no effect here; letting it widen
                        // the decoder would bypass the advertised 4096-byte cap.
                        settings::HEADER_TABLE_SIZE => {}
                        settings::ENABLE_PUSH => {
                            if value != 0 {
                                return Err(h2_error(
                                    "peer advertised ENABLE_PUSH != 0 (server push is disabled)",
                                ));
                            }
                        }
                        settings::MAX_CONCURRENT_STREAMS => {
                            self.peer_max_concurrent = Some(value as usize);
                        }
                        settings::INITIAL_WINDOW_SIZE => {
                            if value > 0x7fff_ffff {
                                return Err(h2_error(format!(
                                    "peer advertised invalid INITIAL_WINDOW_SIZE {value}"
                                )));
                            }
                            let delta = i64::from(value) - self.peer_initial_window;
                            for stream in self.streams.values_mut() {
                                stream.send_window += delta;
                                if stream.send_window > 0x7fff_ffff {
                                    return Err(h2_error(
                                        "peer INITIAL_WINDOW_SIZE makes a stream window overflow",
                                    ));
                                }
                            }
                            self.peer_initial_window = i64::from(value);
                        }
                        settings::MAX_FRAME_SIZE => {
                            if !(DEFAULT_MAX_FRAME_SIZE..=16_777_215).contains(&value) {
                                return Err(h2_error(format!(
                                    "peer advertised invalid MAX_FRAME_SIZE {value}"
                                )));
                            }
                            self.peer_max_frame_size = value;
                        }
                        _ => {}
                    }
                }
                // Best-effort ACK: the peer may already have closed after its final response.
                let _ = send_frame(&mut *self.stream, &Frame::settings_ack());
                self.peer_settings_received = true;
                Ok(())
            }
            kind::PING => {
                if frame.header.flags & flags::ACK == 0 {
                    if frame.payload.len() != 8 {
                        return Err(h2_error("PING payload must be 8 bytes"));
                    }
                    let mut payload = [0u8; 8];
                    payload.copy_from_slice(&frame.payload);
                    let _ = send_frame(&mut *self.stream, &Frame::ping(payload, true));
                }
                Ok(())
            }
            kind::WINDOW_UPDATE => {
                let increment = frame.window_increment()?;
                if increment == 0 {
                    return Err(h2_error(
                        "FLOW_CONTROL_ERROR: WINDOW_UPDATE with zero increment",
                    ));
                }
                let increment_i64 = i64::from(increment);
                if stream_id == 0 {
                    self.connection_send_window += increment_i64;
                    if self.connection_send_window > 0x7fff_ffff {
                        return Err(h2_error(
                            "FLOW_CONTROL_ERROR: connection send window overflow",
                        ));
                    }
                } else if let Some(stream) = self.streams.get_mut(&stream_id) {
                    stream.send_window += increment_i64;
                    if stream.send_window > 0x7fff_ffff {
                        return Err(h2_error("FLOW_CONTROL_ERROR: stream send window overflow"));
                    }
                }
                Ok(())
            }
            kind::HEADERS | kind::CONTINUATION => {
                let Some((block, end_stream)) = self.assembler.feed(&frame)? else {
                    return Ok(());
                };
                if self.discarded_pushes.contains(&stream_id) {
                    // HPACK's dynamic table is connection-wide: skipping a discarded push's
                    // header block would desynchronise every later block, so decode it too.
                    if block.len() > self.limits.max_header_block {
                        return Err(FetchError::HeadersTooLarge);
                    }
                    let _ = self
                        .decoder
                        .decode(&block)
                        .map_err(|_| h2_error("invalid HPACK header block for a discarded push"))?;
                    return Ok(());
                }
                if self.pushed_streams.contains_key(&stream_id) {
                    if block.len() > self.limits.max_header_block {
                        return Err(FetchError::HeadersTooLarge);
                    }
                    let decoded = self
                        .decoder
                        .decode(&block)
                        .map_err(|_| h2_error("invalid HPACK header block for a pushed stream"))?;
                    let Some(pushed) = self.pushed_streams.get_mut(&stream_id) else {
                        return Ok(());
                    };
                    if pushed.got_headers {
                        // Pushed trailers: accept only as the final block.
                        if !end_stream {
                            return Err(h2_error("pushed-stream trailers must carry END_STREAM"));
                        }
                        self.finish_pushed(stream_id);
                        return Ok(());
                    }
                    pushed.got_headers = true;
                    for (name, value) in decoded {
                        if name == ":status" {
                            pushed.status = Some(value.parse::<u16>().map_err(|_| {
                                h2_error("invalid :status value for a pushed response")
                            })?);
                        } else if !name.starts_with(':') {
                            pushed.headers.push((name, value));
                        }
                    }
                    if end_stream {
                        self.finish_pushed(stream_id);
                    }
                    return Ok(());
                }
                if !self.streams.contains_key(&stream_id) {
                    // Header block for a stream this client never opened (or already closed).
                    return Err(h2_error(format!(
                        "HEADERS on unknown or closed stream {stream_id}"
                    )));
                }
                if block.len() > self.limits.max_header_block {
                    return Err(FetchError::HeadersTooLarge);
                }
                let decoded = self
                    .decoder
                    .decode(&block)
                    .map_err(|_| h2_error("invalid HPACK header block"))?;
                let stream = self
                    .streams
                    .get_mut(&stream_id)
                    .expect("stream existence checked above");
                if stream.got_headers {
                    // Trailers: a second HEADERS block must be header-only and end the stream.
                    if !end_stream {
                        return Err(h2_error("HTTP/2 trailers must carry END_STREAM"));
                    }
                    for (name, value) in decoded {
                        if name.starts_with(':') {
                            return Err(h2_error(
                                "HTTP/2 trailers must not contain pseudo-headers",
                            ));
                        }
                        stream.trailers.push((name, value));
                    }
                    self.finish_stream(stream_id);
                    return Ok(());
                }
                // Interim (1xx) responses: 100-continue releases the request body.
                if let Some(status) = decoded
                    .iter()
                    .find(|(name, _)| name == ":status")
                    .and_then(|(_, value)| value.parse::<u16>().ok())
                    && (100..200).contains(&status)
                {
                    if status == 100 && stream.awaiting_continue {
                        stream.awaiting_continue = false;
                    }
                    return Ok(());
                }
                stream.got_headers = true;
                for (name, value) in decoded {
                    if name == ":status" {
                        stream.status = Some(
                            value
                                .parse::<u16>()
                                .map_err(|_| h2_error("invalid :status value"))?,
                        );
                    } else if !name.starts_with(':') {
                        stream.headers.push((name, value));
                    }
                }
                if end_stream {
                    self.finish_stream(stream_id);
                }
                Ok(())
            }
            kind::DATA => {
                let payload = frame.data_payload()?;
                let length = i64::try_from(payload.len()).unwrap_or(i64::MAX);
                if self.discarded_pushes.contains(&stream_id) {
                    // Frames for a cancelled promise may still be in flight; DATA still
                    // consumes the connection window, so replenish it to keep the peer going.
                    self.send_window_update(0, u32::try_from(payload.len()).unwrap_or(u32::MAX));
                    return Ok(());
                }
                if let Some(pushed) = self.pushed_streams.get(&stream_id) {
                    let length = i64::try_from(payload.len()).unwrap_or(i64::MAX);
                    if length > self.connection_receive_window {
                        return Err(h2_error(
                            "FLOW_CONTROL_ERROR: pushed DATA exceeds the connection receive window",
                        ));
                    }
                    if length > pushed.receive_window {
                        return Err(h2_error(
                            "FLOW_CONTROL_ERROR: pushed DATA exceeds the stream receive window",
                        ));
                    }
                    let new_body_len = pushed.body.len().saturating_add(payload.len());
                    let new_total_len = self.pushed_total_bytes.saturating_add(new_body_len);
                    let body_ok = new_body_len <= self.limits.max_pushed_body;
                    let total_ok = new_total_len <= self.limits.max_pushed_total;
                    let end_stream = frame.header.flags & flags::END_STREAM != 0;
                    self.connection_receive_window -= length;
                    self.connection_receive_window += length;
                    self.send_window_update(0, u32::try_from(payload.len()).unwrap_or(u32::MAX));
                    if !body_ok || !total_ok {
                        self.discard_pushed(stream_id);
                        return Ok(());
                    }
                    let pushed = self
                        .pushed_streams
                        .get_mut(&stream_id)
                        .expect("pushed stream checked above");
                    pushed.receive_window -= length;
                    pushed.receive_window += length;
                    pushed.body.extend_from_slice(payload);
                    self.send_window_update(
                        stream_id,
                        u32::try_from(payload.len()).unwrap_or(u32::MAX),
                    );
                    if end_stream {
                        self.finish_pushed(stream_id);
                    }
                    return Ok(());
                }
                let Some(stream) = self.streams.get_mut(&stream_id) else {
                    return Err(h2_error(format!("DATA on unknown stream {stream_id}")));
                };
                if length > self.connection_receive_window {
                    return Err(h2_error(
                        "FLOW_CONTROL_ERROR: DATA exceeds the connection receive window",
                    ));
                }
                if length > stream.receive_window {
                    return Err(h2_error(
                        "FLOW_CONTROL_ERROR: DATA exceeds the stream receive window",
                    ));
                }
                self.connection_receive_window -= length;
                stream.receive_window -= length;
                if let Some(callback) = self.on_data.take() {
                    let outcome = callback(payload);
                    self.on_data = Some(callback);
                    match outcome {
                        Ok(()) => {}
                        Err(error) => {
                            let _ = send_frame(
                                &mut *self.stream,
                                &Frame::rst_stream(stream_id, error_code::CANCEL),
                            );
                            return Err(error);
                        }
                    }
                    self.connection_receive_window += length;
                    let increment = u32::try_from(payload.len()).unwrap_or(u32::MAX);
                    self.send_window_update(0, increment);
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        stream.receive_window += length;
                    }
                    self.send_window_update(stream_id, increment);
                    if frame.header.flags & flags::END_STREAM != 0 {
                        self.finish_stream(stream_id);
                    }
                    return Ok(());
                }
                if stream.body.len().saturating_add(payload.len()) > self.limits.max_body {
                    self.fail_stream(stream_id, FetchError::ResponseTooLarge);
                    return Ok(());
                }
                stream.body.extend_from_slice(payload);
                let end_stream = frame.header.flags & flags::END_STREAM != 0;
                // Replenish the windows as the application consumes the DATA.
                self.connection_receive_window += length;
                let increment = u32::try_from(payload.len()).unwrap_or(u32::MAX);
                if end_stream {
                    self.send_window_update(0, increment);
                    self.finish_stream(stream_id);
                } else {
                    self.send_window_update(0, increment);
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        stream.receive_window += length;
                    }
                    self.send_window_update(stream_id, increment);
                }
                Ok(())
            }
            kind::RST_STREAM => {
                self.discarded_pushes.remove(&stream_id);
                if self.pushed_streams.contains_key(&stream_id) {
                    self.pushed_streams.remove(&stream_id);
                    self.pushed_discarded += 1;
                    return Ok(());
                }
                let code = frame
                    .parse_rst_stream()
                    .unwrap_or(error_code::INTERNAL_ERROR);
                self.fail_stream(
                    stream_id,
                    h2_error(format!("stream reset by peer (error code 0x{code:x})")),
                );
                Ok(())
            }
            kind::GOAWAY => {
                let (last_stream_id, code, _debug) = frame.parse_goaway()?;
                for stream_id in self.streams.keys().copied().collect::<Vec<_>>() {
                    self.fail_stream(
                        stream_id,
                        h2_error(format!(
                            "connection closed by GOAWAY (error code 0x{code:x}, last stream {last_stream_id})"
                        )),
                    );
                }
                while let Some(index) = self.remaining.first().copied() {
                    self.remaining.remove(0);
                    self.fail_request(
                        index,
                        h2_error(format!(
                            "connection closed by GOAWAY before the request was sent (last stream {last_stream_id})"
                        )),
                    );
                }
                Err(h2_error(format!(
                    "connection closed by GOAWAY (error code 0x{code:x}, last stream {last_stream_id})"
                )))
            }
            kind::PUSH_PROMISE => {
                if !self.limits.enable_server_push {
                    return Err(h2_error(
                        "PUSH_PROMISE received but server push is disabled (SETTINGS_ENABLE_PUSH=0)",
                    ));
                }
                if frame.header.flags & flags::END_HEADERS == 0 {
                    return Err(h2_error(
                        "fragmented PUSH_PROMISE header blocks are not supported",
                    ));
                }
                let (promised, block, priority) = frame.parse_push_promise_with_priority()?;
                let weight = priority.map_or(16, |(_depends_on, weight, _exclusive)| weight);
                if promised == 0 || promised % 2 != 0 || promised <= self.last_pushed_stream {
                    return Err(h2_error(format!(
                        "invalid promised stream id {promised} in PUSH_PROMISE"
                    )));
                }
                if block.len() > self.limits.max_header_block {
                    return Err(FetchError::HeadersTooLarge);
                }
                // Keep the HPACK decoder in sync even when the promise is discarded.
                let decoded = self
                    .decoder
                    .decode(block)
                    .map_err(|_| h2_error("invalid HPACK block in PUSH_PROMISE"))?;
                self.last_pushed_stream = promised;
                let mut scheme = None;
                let mut authority = None;
                let mut path = None;
                for (name, value) in &decoded {
                    match name.as_str() {
                        ":scheme" => scheme = Some(value.clone()),
                        ":authority" => authority = Some(value.clone()),
                        ":path" => path = Some(value.clone()),
                        _ => {}
                    }
                }
                let (Some(scheme), Some(authority), Some(path)) = (scheme, authority, path) else {
                    return Err(h2_error(
                        "PUSH_PROMISE is missing :scheme/:authority/:path pseudo-headers",
                    ));
                };
                let url = format!("{scheme}://{authority}{path}");
                if self.pushed_streams.len() + self.pushed_results.len()
                    >= self.limits.max_pushed_resources
                {
                    // Resource cap reached. With server-declared priorities, a
                    // higher-weight promise may evict the lowest-weight *pending*
                    // push; otherwise keep the earlier promise (FIFO fallback/cap).
                    let victim = match self.limits.pushed_priority {
                        PushPriority::ServerWeights => self
                            .pushed_streams
                            .iter()
                            .filter(|(_, pushed)| pushed.weight < weight)
                            .min_by_key(|(_, pushed)| (pushed.weight, pushed.promise_order))
                            .map(|(stream_id, _)| *stream_id),
                        PushPriority::Fifo => None,
                    };
                    match victim {
                        Some(victim) => self.discard_pushed(victim),
                        None => {
                            self.discard_pushed(promised);
                            return Ok(());
                        }
                    }
                }
                let promise_order = self.push_sequence;
                self.push_sequence = self.push_sequence.saturating_add(1);
                self.pushed_streams.insert(
                    promised,
                    PushedStream {
                        url,
                        status: None,
                        headers: Vec::new(),
                        body: Vec::new(),
                        got_headers: false,
                        receive_window: i64::from(self.limits.initial_window_size),
                        weight,
                        promise_order,
                    },
                );
                Ok(())
            }
            kind::PRIORITY => Ok(()),
            _ => Ok(()),
        }
    }
}
