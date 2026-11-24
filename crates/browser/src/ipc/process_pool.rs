//! Per-site pooled renderer processes with non-blocking response draining (ADR-004 phase 3).
//!
//! Each effective site gets its own OS process. A reader thread per process turns stdout into
//! an mpsc channel, so the event loop never blocks waiting for a reply — unlike the original
//! synchronous `ProcessRenderSession`. A bounded LRU evicts the least-recently-used site.

use super::*;
use crate::site::SiteKey;
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

/// Max simultaneous renderer processes (mirrors the thread pool cap).
pub const MAX_PROCESS_RENDERERS: usize = 8;

/// Effective site of a process render request. `base_override` wins for patched DOM.
pub fn request_process_site(request: &IpcRenderRequest) -> SiteKey {
    crate::site::site_key(request.base_override.as_deref().unwrap_or(&request.input))
}

type TaggedResponse = (Option<u64>, Result<IpcRenderResponse, IpcError>);

enum SlotBackend {
    Process(ProcessRenderer),
    #[cfg(test)]
    Mock {
        alive: bool,
    },
}

impl SlotBackend {
    fn is_alive(&mut self) -> bool {
        match self {
            Self::Process(renderer) => renderer.is_alive(),
            #[cfg(test)]
            Self::Mock { alive } => *alive,
        }
    }

    fn write_request(&mut self, request: &IpcRenderRequest) -> Result<(), IpcError> {
        match self {
            Self::Process(renderer) => renderer.write_request(request),
            #[cfg(test)]
            Self::Mock { alive } => {
                if *alive {
                    Ok(())
                } else {
                    Err(IpcError::Io("mock renderer dead".to_owned()))
                }
            }
        }
    }

    fn kill(&mut self) {
        match self {
            Self::Process(renderer) => renderer.kill(),
            #[cfg(test)]
            Self::Mock { alive } => *alive = false,
        }
    }

    /// Kills the whole renderer process group when the watchdog expires (T5).
    fn kill_group(&mut self) {
        match self {
            Self::Process(renderer) => renderer.kill_group(),
            #[cfg(test)]
            Self::Mock { alive } => *alive = false,
        }
    }

    fn id(&self) -> u32 {
        match self {
            Self::Process(renderer) => renderer.id(),
            #[cfg(test)]
            Self::Mock { .. } => 0,
        }
    }
}

struct Slot {
    site: SiteKey,
    backend: SlotBackend,
    responses: Receiver<Result<IpcRenderResponse, IpcError>>,
    pending: VecDeque<u64>,
    /// Send time for each `pending` generation, in the same order (T5 watchdog).
    sent_at: VecDeque<(u64, Instant)>,
    last_used: u64,
}

impl Slot {
    fn take_pending(&mut self, generation: u64) {
        if self.pending.front() == Some(&generation) {
            self.pending.pop_front();
            self.sent_at.pop_front();
        } else if let Some(pos) = self.pending.iter().position(|&g| g == generation) {
            self.pending.remove(pos);
            self.sent_at.remove(pos);
        }
    }

    fn drain_into(&mut self, out: &mut Vec<TaggedResponse>) -> bool {
        loop {
            match self.responses.try_recv() {
                Ok(Ok(response)) => {
                    self.take_pending(response.generation);
                    out.push((Some(response.generation), Ok(response)));
                }
                Ok(Err(error)) if error.is_fatal() => {
                    let first_gen = self.pending.pop_front();
                    self.sent_at.pop_front();
                    out.push((first_gen, Err(error.clone())));
                    while let Some(generation) = self.pending.pop_front() {
                        self.sent_at.pop_front();
                        out.push((Some(generation), Err(error.clone())));
                    }
                    return true;
                }
                Ok(Err(error)) => {
                    let generation = self.pending.pop_front();
                    self.sent_at.pop_front();
                    out.push((generation, Err(error)));
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => {
                    while let Some(generation) = self.pending.pop_front() {
                        self.sent_at.pop_front();
                        out.push((
                            Some(generation),
                            Err(IpcError::Io("renderer died".to_owned())),
                        ));
                    }
                    return true;
                }
            }
        }
    }
}

/// A bounded pool of per-site renderer processes.
pub struct ProcessRenderPool {
    slots: Vec<Slot>,
    max: usize,
    clock: u64,
    evictions: usize,
    recoveries: usize,
    retired_events: Vec<TaggedResponse>,
    /// Explicit renderer executable (integration tests); `None` uses `current_exe()`.
    executable: Option<std::path::PathBuf>,
    /// Explicit renderer argv; defaults to `--megi-ipc-worker`.
    args: Vec<String>,
}

impl ProcessRenderPool {
    pub fn new(max: usize) -> Self {
        Self::with_executable(max, None)
    }

    /// Pool whose renderers are spawned from `executable` (real-binary E2E tests).
    pub fn with_executable(max: usize, executable: Option<std::path::PathBuf>) -> Self {
        Self::with_executable_and_args(max, executable, vec!["--megi-ipc-worker".to_owned()])
    }

    /// Like [`Self::with_executable`] with explicit argv (real non-responding children).
    pub fn with_executable_and_args(
        max: usize,
        executable: Option<std::path::PathBuf>,
        args: Vec<String>,
    ) -> Self {
        Self {
            slots: Vec::new(),
            max: max.clamp(1, MAX_PROCESS_RENDERERS),
            clock: 0,
            evictions: 0,
            recoveries: 0,
            retired_events: Vec::new(),
            executable,
            args,
        }
    }

    fn retire_slot(&mut self, mut slot: Slot, reason: &str) {
        let _ = slot.drain_into(&mut self.retired_events);
        while let Some(generation) = slot.pending.pop_front() {
            let _ = slot.sent_at.pop_front();
            self.retired_events
                .push((Some(generation), Err(IpcError::Io(reason.to_owned()))));
        }
        drop(slot.backend);
    }

    #[cfg(test)]
    fn push_mock_slot_for_test(
        &mut self,
        site: SiteKey,
        responses: Receiver<Result<IpcRenderResponse, IpcError>>,
        alive: bool,
    ) {
        self.clock = self.clock.saturating_add(1);
        self.slots.push(Slot {
            site,
            backend: SlotBackend::Mock { alive },
            responses,
            pending: VecDeque::new(),
            sent_at: VecDeque::new(),
            last_used: self.clock,
        });
    }

    fn spawn_renderer(
        &self,
    ) -> Result<
        (
            ProcessRenderer,
            Receiver<Result<IpcRenderResponse, IpcError>>,
        ),
        IpcError,
    > {
        match &self.executable {
            Some(path) => ProcessRenderer::spawn_with_reader_from_args(path, &self.args),
            None => ProcessRenderer::spawn_with_reader(),
        }
    }

    /// Queues a request to the process for its site, spawning one on first use.
    pub fn send(&mut self, request: IpcRenderRequest) -> Result<(), IpcError> {
        let site = request_process_site(&request);
        self.clock = self.clock.saturating_add(1);
        let existing = self.slots.iter().position(|slot| slot.site == site);
        // A child that exited (crash or killed renderer) must be replaced before reuse.
        if let Some(index) = existing
            && !self.slots[index].backend.is_alive()
        {
            let dead = self.slots.remove(index);
            self.retire_slot(dead, "renderer died");
            self.recoveries = self.recoveries.saturating_add(1);
        }
        let index = match self.slots.iter().position(|slot| slot.site == site) {
            Some(index) => index,
            None if self.slots.len() < self.max => {
                let (renderer, responses) = self.spawn_renderer()?;
                self.slots.push(Slot {
                    site: site.clone(),
                    backend: SlotBackend::Process(renderer),
                    responses,
                    pending: VecDeque::new(),
                    sent_at: VecDeque::new(),
                    last_used: self.clock,
                });
                self.slots.len() - 1
            }
            None => {
                let index = self
                    .slots
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, slot)| slot.last_used)
                    .map(|(index, _)| index)
                    .expect("non-empty pool");
                let (renderer, responses) = self.spawn_renderer()?;
                let old = std::mem::replace(
                    &mut self.slots[index],
                    Slot {
                        site: site.clone(),
                        backend: SlotBackend::Process(renderer),
                        responses,
                        pending: VecDeque::new(),
                        sent_at: VecDeque::new(),
                        last_used: self.clock,
                    },
                );
                self.retire_slot(old, "renderer evicted");
                self.evictions = self.evictions.saturating_add(1);
                index
            }
        };
        self.slots[index].last_used = self.clock;
        self.slots[index].backend.write_request(&request)?;
        self.slots[index].pending.push_back(request.generation);
        self.slots[index]
            .sent_at
            .push_back((request.generation, Instant::now()));
        Ok(())
    }

    /// Drains all available responses and terminal failure notifications tagged with their
    /// in-flight request `generation` without blocking (#2191).
    pub fn drain_tagged(&mut self) -> Vec<TaggedResponse> {
        // T5 watchdog: a request older than the deadline kills its renderer's whole
        // process group and fails every in-flight generation before normal draining.
        let now = Instant::now();
        let mut timed_out = Vec::new();
        for (index, slot) in self.slots.iter().enumerate() {
            if let Some((_, sent)) = slot.sent_at.front()
                && now.duration_since(*sent) >= RENDER_WATCHDOG_TIMEOUT
            {
                timed_out.push((index, slot.pending.iter().copied().collect::<Vec<u64>>()));
            }
        }
        for (index, generations) in timed_out.into_iter().rev() {
            let mut slot = self.slots.remove(index);
            slot.backend.kill_group();
            for generation in generations {
                self.retired_events
                    .push((Some(generation), Err(IpcError::Watchdog(generation))));
            }
            drop(slot);
            self.recoveries = self.recoveries.saturating_add(1);
        }

        let mut out = std::mem::take(&mut self.retired_events);
        let mut dead = Vec::new();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.drain_into(&mut out) {
                dead.push(index);
            }
        }
        for index in dead.into_iter().rev() {
            let slot = self.slots.remove(index);
            drop(slot.backend);
            self.recoveries = self.recoveries.saturating_add(1);
        }
        out
    }

    /// Drains all available responses without blocking.
    pub fn drain(&mut self) -> Vec<Result<IpcRenderResponse, IpcError>> {
        self.drain_tagged()
            .into_iter()
            .map(|(_, response)| response)
            .collect()
    }

    pub fn sites(&self) -> Vec<SiteKey> {
        self.slots.iter().map(|slot| slot.site.clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn evictions(&self) -> usize {
        self.evictions
    }

    /// Number of crashed renderers replaced so far (diagnostics/tests).
    pub fn recoveries(&self) -> usize {
        self.recoveries
    }

    /// PIDs of the live renderer children, in site insertion order (E2E diagnostics).
    pub fn pids(&self) -> Vec<u32> {
        self.slots.iter().map(|slot| slot.backend.id()).collect()
    }

    /// Kills the renderer serving `site` without dropping its slot, so the next `send()`
    /// exercises the crash-recovery path (E2E).
    pub fn kill_child(&mut self, site: &SiteKey) -> bool {
        if let Some(slot) = self.slots.iter_mut().find(|slot| &slot.site == site) {
            slot.backend.kill();
            true
        } else {
            false
        }
    }

    /// Terminates every process (called on shutdown).
    pub fn join(self) {
        for slot in self.slots {
            drop(slot.backend);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_req(generation: u64, site: &str) -> IpcRenderRequest {
        IpcRenderRequest {
            generation,
            input: site.to_owned(),
            css_width: 200,
            css_height: 100,
            base_override: None,
            scripting: false,
            find_query: None,
            options: IpcRenderOptions::default(),
        }
    }

    #[test]
    fn fatal_ipc_error_notifies_every_in_flight_generation() {
        let mut pool = ProcessRenderPool::new(2);
        let (tx, rx) = std::sync::mpsc::channel();
        pool.push_mock_slot_for_test(crate::site::site_key("https://crash.example/"), rx, true);

        pool.send(test_req(11, "https://crash.example/a"))
            .expect("send 11");
        pool.send(test_req(12, "https://crash.example/b"))
            .expect("send 12");

        tx.send(Err(IpcError::Truncated)).expect("inject fatal");
        let drained = pool.drain_tagged();
        assert_eq!(
            drained.len(),
            2,
            "both in-flight requests must receive errors"
        );
        assert_eq!(drained[0].0, Some(11));
        assert_eq!(drained[1].0, Some(12));
        assert!(drained.iter().all(|(_, res)| res.is_err()));
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.recoveries(), 1);
    }

    #[test]
    fn dead_slot_replaced_by_send_reports_in_flight_failures() {
        let mut pool = ProcessRenderPool::new(2);
        let site = crate::site::site_key("https://dead.example/");
        let (_tx, rx) = std::sync::mpsc::channel();
        pool.push_mock_slot_for_test(site.clone(), rx, true);
        pool.send(test_req(31, "https://dead.example/a"))
            .expect("send 31");
        pool.send(test_req(32, "https://dead.example/b"))
            .expect("send 32");
        assert!(pool.kill_child(&site));

        let retired = pool.slots.remove(0);
        pool.retire_slot(retired, "renderer died");
        let drained = pool.drain_tagged();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].0, Some(31));
        assert_eq!(drained[1].0, Some(32));
        assert!(drained.iter().all(|(_, res)| res.is_err()));
    }

    #[test]
    fn disconnected_reader_notifies_every_in_flight_generation() {
        let mut pool = ProcessRenderPool::new(2);
        let (tx, rx) = std::sync::mpsc::channel();
        pool.push_mock_slot_for_test(crate::site::site_key("https://eof.example/"), rx, true);

        pool.send(test_req(21, "https://eof.example/a"))
            .expect("send 21");
        pool.send(test_req(22, "https://eof.example/b"))
            .expect("send 22");

        drop(tx);
        let drained = pool.drain_tagged();
        assert_eq!(
            drained.len(),
            2,
            "channel close must fail both pending calls"
        );
        assert_eq!(drained[0].0, Some(21));
        assert_eq!(drained[1].0, Some(22));
        assert!(drained.iter().all(|(_, res)| res.is_err()));
        assert_eq!(pool.len(), 0);
    }
}
