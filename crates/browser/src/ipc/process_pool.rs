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
}

impl ProcessRenderPool {
    pub fn new(max: usize) -> Self {
        Self {
            slots: Vec::new(),
            max: max.clamp(1, MAX_PROCESS_RENDERERS),
            clock: 0,
            evictions: 0,
        }
    }

    /// Queues a request to the process for its site, spawning one on first use.
    pub fn send(&mut self, request: IpcRenderRequest) -> Result<(), IpcError> {
        let site = request_process_site(&request);
        self.clock = self.clock.saturating_add(1);
        let index = match self.slots.iter().position(|slot| slot.site == site) {
            Some(index) => index,
            None if self.slots.len() < self.max => {
                let (renderer, responses) = ProcessRenderer::spawn_with_reader()?;
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
                let (renderer, responses) = ProcessRenderer::spawn_with_reader()?;
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
        for slot in &self.slots {
            while let Ok(response) = slot.responses.try_recv() {
                out.push(response);
            }
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

    /// Terminates every process (called on shutdown).
    pub fn join(self) {
        for slot in self.slots {
            drop(slot.renderer);
        }
    }
}
