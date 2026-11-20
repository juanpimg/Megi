//! Per-site renderer pool (ADR-004 phase 2).
//!
//! Each effective site (`SiteKey`) gets its own render worker, so two pages from different
//! origins never share a JS realm or render thread. When the pool reaches its cap the
//! least-recently-used renderer is reused for the new site and the reuse is reported by
//! [`RendererPool::evictions`] — a controlled degradation, never a hard failure.

use super::*;
use crate::site::SiteKey;

/// Maximum number of simultaneous renderer threads (ADR-004 phase 2).
pub const MAX_SITE_RENDERERS: usize = 8;

/// One spawned render worker: request channel, message channel and thread handle.
pub type Worker = (
    Sender<RenderRequest>,
    Receiver<WorkerMessage>,
    JoinHandle<()>,
);

/// Effective site of a render request. `base_override` (patched-DOM snapshots) wins when
/// present because it is the URL relative resources and scripts resolve against.
pub fn request_site(request: &RenderRequest) -> SiteKey {
    crate::site::site_key(request.base_override.as_deref().unwrap_or(&request.input))
}

struct Slot {
    site: SiteKey,
    sender: Sender<RenderRequest>,
    receiver: Receiver<WorkerMessage>,
    handle: JoinHandle<()>,
    last_used: u64,
}

/// A bounded pool of render workers keyed by effective site.
pub struct RendererPool {
    slots: Vec<Slot>,
    max: usize,
    clock: u64,
    evictions: usize,
}

impl RendererPool {
    /// Creates an empty pool. `factory` builds one worker (sender/receiver/handle).
    pub fn new(max: usize) -> Self {
        Self {
            slots: Vec::new(),
            max: max.clamp(1, MAX_SITE_RENDERERS),
            clock: 0,
            evictions: 0,
        }
    }

    /// Routes `request` to the worker for its site, spawning one on first use.
    pub fn send<F>(&mut self, request: RenderRequest, factory: F) -> Result<(), String>
    where
        F: FnOnce() -> Worker,
    {
        let worker = if self
            .slots
            .iter()
            .any(|slot| slot.site == request_site(&request))
        {
            None
        } else {
            Some(factory())
        };
        self.send_prebuilt(request, worker)
    }

    /// Routes `request` using an optionally pre-built worker. The window builds workers
    /// before borrowing the pool (it needs its own cloned render options/proxy).
    pub fn send_prebuilt(
        &mut self,
        request: RenderRequest,
        new_worker: Option<Worker>,
    ) -> Result<(), String> {
        let site = request_site(&request);
        self.clock = self.clock.saturating_add(1);
        let index = match self.slots.iter().position(|slot| slot.site == site) {
            Some(index) => index,
            None if self.slots.len() < self.max => {
                let (sender, receiver, handle) =
                    new_worker.expect("a new site needs a pre-built worker");
                self.slots.push(Slot {
                    site: site.clone(),
                    sender,
                    receiver,
                    handle,
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
                let (sender, receiver, handle) =
                    new_worker.expect("a new site needs a pre-built worker");
                let old = std::mem::replace(
                    &mut self.slots[index],
                    Slot {
                        site: site.clone(),
                        sender,
                        receiver,
                        handle,
                        last_used: self.clock,
                    },
                );
                drop(old.sender); // let the replaced worker exit when its queue drains
                self.evictions = self.evictions.saturating_add(1);
                index
            }
        };
        self.slots[index].last_used = self.clock;
        self.slots[index]
            .sender
            .send(request)
            .map_err(|_| "render worker channel closed".to_owned())
    }

    /// Drains every worker's message queue into one batch for the event loop.
    pub fn drain_messages(&self) -> Vec<WorkerMessage> {
        let mut out = Vec::new();
        for slot in &self.slots {
            while let Ok(message) = slot.receiver.try_recv() {
                out.push(message);
            }
        }
        out
    }

    /// Effective sites currently served, in insertion order.
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

    /// True when a worker already serves this site.
    pub fn has_site(&self, key: &SiteKey) -> bool {
        self.slots.iter().any(|slot| &slot.site == key)
    }

    /// Index of the worker serving `url`, if any (diagnostics/tests).
    pub fn index_for(&self, url: &str) -> Option<usize> {
        let key = crate::site::site_key(url);
        self.slots.iter().position(|slot| slot.site == key)
    }

    /// Joins all worker threads (called when the window shuts down).
    pub fn join(self) {
        for slot in self.slots {
            drop(slot.sender);
            let _ = slot.handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn request(input: &str) -> RenderRequest {
        RenderRequest {
            generation: 1,
            input: input.to_owned(),
            css_width: 100,
            css_height: 100,
            base_override: None,
            scripting: false,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        }
    }

    fn factory(
        counter: Arc<AtomicUsize>,
    ) -> impl Fn() -> (
        Sender<RenderRequest>,
        Receiver<WorkerMessage>,
        JoinHandle<()>,
    ) {
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
            spawn_worker(|_: &RenderRequest| Err("fake renderer".to_owned()), || {})
        }
    }

    #[test]
    fn same_site_reuses_one_worker() {
        let spawns = Arc::new(AtomicUsize::new(0));
        let mut pool = RendererPool::new(MAX_SITE_RENDERERS);
        pool.send(request("https://example.com/a"), factory(spawns.clone()))
            .expect("first");
        pool.send(request("https://example.com/b"), factory(spawns.clone()))
            .expect("second");
        assert_eq!(spawns.load(Ordering::SeqCst), 1, "one worker per site");
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.index_for("https://example.com/c"), Some(0));
    }

    #[test]
    fn different_sites_get_separate_workers() {
        let spawns = Arc::new(AtomicUsize::new(0));
        let mut pool = RendererPool::new(MAX_SITE_RENDERERS);
        pool.send(request("https://example.com/"), factory(spawns.clone()))
            .expect("a");
        pool.send(request("http://other.test/"), factory(spawns.clone()))
            .expect("b");
        assert_eq!(spawns.load(Ordering::SeqCst), 2);
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.index_for("http://other.test/x"), Some(1));
    }

    #[test]
    fn cap_evicts_least_recently_used_worker() {
        let spawns = Arc::new(AtomicUsize::new(0));
        let mut pool = RendererPool::new(1);
        pool.send(request("https://a.test/"), factory(spawns.clone()))
            .expect("a");
        pool.send(request("https://b.test/"), factory(spawns.clone()))
            .expect("b");
        assert_eq!(pool.len(), 1, "cap enforced");
        assert_eq!(pool.evictions(), 1);
        assert_eq!(pool.sites(), vec![crate::site::site_key("https://b.test/")]);
        assert_eq!(pool.index_for("https://a.test/"), None);
    }

    #[test]
    fn base_override_determines_the_site() {
        let req = RenderRequest {
            base_override: Some("https://real.example/".to_owned()),
            ..request("data:text/html,<p>patched</p>")
        };
        assert_eq!(
            request_site(&req),
            crate::site::site_key("https://real.example/")
        );
    }
}
