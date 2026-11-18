//! Session construction, run loop and request scheduling.

use super::*;

impl<'a, S: Read + Write> Session<'a, S> {
    pub(super) fn new(
        stream: &'a mut S,
        requests: Vec<H2Request>,
        limits: &'a Limits,
        on_data: Option<DataCallback<'a>>,
    ) -> Self {
        Self {
            stream,
            limits,
            decoder: {
                let mut decoder = Decoder::new(4096);
                decoder.set_max_header_list_size(limits.max_header_list_size);
                decoder
            },
            assembler: HeaderAssembler::default(),
            connection_receive_window: i64::from(limits.initial_window_size),
            connection_send_window: 65_535,
            peer_max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            peer_max_concurrent: None,
            peer_initial_window: 65_535,
            peer_settings_received: false,
            streams: BTreeMap::new(),
            results: (0..requests.len()).map(|_| None).collect(),
            remaining: (0..requests.len()).collect(),
            requests,
            opened_stream_ids: HashSet::new(),
            stream_weights: BTreeMap::new(),
            pushed_streams: BTreeMap::new(),
            pushed_results: Vec::new(),
            pushed_total_bytes: 0,
            pushed_discarded: 0,
            on_data,
            discarded_pushes: HashSet::new(),
            last_pushed_stream: 0,
            next_stream_id: 1,
            push_sequence: 0,
        }
    }

    pub(super) fn run(&mut self) -> Result<(), FetchError> {
        if self.requests.is_empty() {
            return Ok(());
        }
        // Connection preface + our SETTINGS (server push disabled, stream/window caps).
        let max_streams = u32::try_from(self.limits.max_concurrent_streams).unwrap_or(u32::MAX);
        let initial_window = self.limits.initial_window_size.min(0x7fff_ffff);
        self.stream.write_all(CONNECTION_PREFACE).map_err(map_io)?;
        self.stream.flush().map_err(map_io)?;
        send_frame(
            self.stream,
            &Frame::settings(&[
                (
                    settings::ENABLE_PUSH,
                    if self.limits.enable_server_push { 1 } else { 0 },
                ),
                (settings::MAX_CONCURRENT_STREAMS, max_streams),
                (settings::INITIAL_WINDOW_SIZE, initial_window),
                (
                    settings::MAX_FRAME_SIZE,
                    self.limits.max_frame_size.max(DEFAULT_MAX_FRAME_SIZE),
                ),
                (settings::HEADER_TABLE_SIZE, 4096),
                (
                    settings::MAX_HEADER_LIST_SIZE,
                    u32::try_from(self.limits.max_header_list_size).unwrap_or(u32::MAX),
                ),
            ]),
        )?;
        // Start the first request immediately (HTTP/2 permits it before the peer SETTINGS);
        // further streams are scheduled once the peer SETTINGS are known, so our advertised
        // MAX_CONCURRENT_STREAMS/peer caps are respected.
        self.submit_next()?;
        self.send_pending_bodies()?;
        loop {
            if self.all_done() {
                // Client requests are complete: pushes still in flight are cancelled (the
                // connection may stay open, so waiting could block indefinitely).
                for stream_id in self.pushed_streams.keys().copied().collect::<Vec<_>>() {
                    self.discard_pushed(stream_id);
                }
                break;
            }
            self.send_pending_bodies()?;
            let Some(frame) =
                FrameReader::new(&mut *self.stream, self.limits.max_frame_size).read_frame()?
            else {
                return Err(h2_error(
                    "connection closed before all HTTP/2 responses completed",
                ));
            };
            self.handle_frame(frame)?;
            self.submit_until_cap()?;
            self.send_pending_bodies()?;
        }
        Ok(())
    }

    pub(super) fn all_done(&self) -> bool {
        self.remaining.is_empty() && self.streams.is_empty()
    }

    /// Chooses the next request to open: ready requests (dependency already open, or
    /// root) with the highest weight win, ties broken by submission order. Dependencies
    /// that are not open yet fall back to the first pending request (stream ids are
    /// monotonic, so the dependency is opened first in every acyclic case).
    pub(super) fn pick_next_request(&mut self) -> Option<usize> {
        let mut best: Option<(u16, usize, usize)> = None; // (weight, order, index)
        let mut ready = false;
        for (order, &index) in self.remaining.iter().enumerate() {
            let Some(priority) = self.requests[index].priority else {
                let candidate = (1u16, order, index);
                ready = true;
                if best.is_none_or(|(weight, best_order, _)| {
                    candidate.0 > weight || (candidate.0 == weight && order < best_order)
                }) {
                    best = Some(candidate);
                }
                continue;
            };
            if priority.depends_on != 0 && !self.opened_stream_ids.contains(&priority.depends_on) {
                continue;
            }
            ready = true;
            if best.is_none_or(|(weight, best_order, _)| {
                priority.weight > weight || (priority.weight == weight && order < best_order)
            }) {
                best = Some((priority.weight, order, index));
            }
        }
        if !ready && !self.remaining.is_empty() {
            return Some(self.remaining[0]);
        }
        best.map(|(_, _, index)| index)
    }

    pub(super) fn active_streams(&self) -> usize {
        self.streams.len()
    }

    pub(super) fn stream_cap(&self) -> usize {
        let peer_cap = self.peer_max_concurrent.unwrap_or(usize::MAX);
        self.limits.max_concurrent_streams.min(peer_cap).max(1)
    }

    /// Submits one pending request immediately (used for the first stream).
    pub(super) fn submit_next(&mut self) -> Result<(), FetchError> {
        let Some(request_index) = self.pick_next_request() else {
            return Ok(());
        };
        if let Some(position) = self
            .remaining
            .iter()
            .position(|&index| index == request_index)
        {
            self.remaining.remove(position);
        }
        self.open_stream(request_index)?;
        self.send_pending_bodies()
    }

    /// Opens pending requests while the scheduler has capacity.
    pub(super) fn submit_until_cap(&mut self) -> Result<(), FetchError> {
        if !self.peer_settings_received {
            return Ok(());
        }
        while !self.remaining.is_empty() && self.active_streams() < self.stream_cap() {
            let Some(request_index) = self.pick_next_request() else {
                break;
            };
            if let Some(position) = self
                .remaining
                .iter()
                .position(|&index| index == request_index)
            {
                self.remaining.remove(position);
            }
            self.open_stream(request_index)?;
            self.send_pending_bodies()?;
        }
        Ok(())
    }
}
