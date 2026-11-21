//! Per-site pooled renderer processes with non-blocking response draining (ADR-004 phase 3).
//!
//! Each effective site gets its own OS process. A reader thread per process turns stdout into
//! an mpsc channel, so the event loop never blocks waiting for a reply — unlike the original
//! synchronous `ProcessRenderSession`. A bounded LRU evicts the least-recently-used site.

use super::*;
use crate::site::SiteKey;
use std::sync::mpsc::Receiver;

/// Max simultaneous renderer processes (mirrors the thread pool cap).
pub const MAX_PROCESS_RENDERERS: usize = 8;

/// Effective site of a process render request. `base_override` wins for patched DOM.
pub fn request_process_site(request: &IpcRenderRequest) -> SiteKey {
    crate::site::site_key(request.base_override.as_deref().unwrap_or(&request.input))
}

struct Slot {
    site: SiteKey,
    renderer: ProcessRenderer,
    responses: Receiver<Result<IpcRenderResponse, IpcError>>,
    last_used: u64,
}

/// A bounded pool of per-site renderer processes.
pub struct ProcessRenderPool {
    slots: Vec<Slot>,
    max: usize,
    clock: u64,
    evictions: usize,
    recoveries: usize,
    /// Explicit renderer executable (integration tests); `None` uses `current_exe()`.
    executable: Option<std::path::PathBuf>,
}

impl ProcessRenderPool {
    pub fn new(max: usize) -> Self {
        Self::with_executable(max, None)
    }

    /// Pool whose renderers are spawned from `executable` (real-binary E2E tests).
    pub fn with_executable(max: usize, executable: Option<std::path::PathBuf>) -> Self {
        Self {
            slots: Vec::new(),
            max: max.clamp(1, MAX_PROCESS_RENDERERS),
            clock: 0,
            evictions: 0,
            recoveries: 0,
            executable,
        }
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
            Some(path) => ProcessRenderer::spawn_with_reader_from(path),
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
            && !self.slots[index].renderer.is_alive()
        {
            let dead = self.slots.remove(index);
            drop(dead.renderer);
            self.recoveries = self.recoveries.saturating_add(1);
        }
        let index = match self.slots.iter().position(|slot| slot.site == site) {
            Some(index) => index,
            None if self.slots.len() < self.max => {
                let (renderer, responses) = self.spawn_renderer()?;
                self.slots.push(Slot {
                    site: site.clone(),
                    renderer,
                    responses,
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
                let old = self.slots.remove(index);
                drop(old.renderer);
                self.evictions = self.evictions.saturating_add(1);
                let (renderer, responses) = self.spawn_renderer()?;
                self.slots.insert(
                    index,
                    Slot {
                        site: site.clone(),
                        renderer,
                        responses,
                        last_used: self.clock,
                    },
                );
                index
            }
        };
        self.slots[index].last_used = self.clock;
        self.slots[index].renderer.write_request(&request)
    }

    /// Drains all available responses without blocking.
    pub fn drain(&mut self) -> Vec<Result<IpcRenderResponse, IpcError>> {
        let mut out = Vec::new();
        let mut dead = Vec::new();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            while let Ok(response) = slot.responses.try_recv() {
                if response.as_ref().is_err_and(|error| error.is_fatal()) {
                    dead.push(index);
                }
                out.push(response);
            }
        }
        for index in dead.into_iter().rev() {
            let slot = self.slots.remove(index);
            drop(slot.renderer);
            self.recoveries = self.recoveries.saturating_add(1);
        }
        out
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
        self.slots.iter().map(|slot| slot.renderer.id()).collect()
    }

    /// Kills the renderer serving `site` without dropping its slot, so the next `send()`
    /// exercises the crash-recovery path (E2E).
    pub fn kill_child(&mut self, site: &SiteKey) -> bool {
        if let Some(slot) = self.slots.iter_mut().find(|slot| &slot.site == site) {
            slot.renderer.kill();
            true
        } else {
            false
        }
    }

    /// Terminates every process (called on shutdown).
    pub fn join(self) {
        for slot in self.slots {
            drop(slot.renderer);
        }
    }
}
