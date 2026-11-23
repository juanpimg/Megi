//! Per-site renderer pool (ADR-004 phase 2).
//!
//! Each effective site (`SiteKey`) gets its own render worker, so two pages from different
//! origins never share a JS realm or render thread. When the pool reaches its cap the
//! least-recently-used renderer is reused for the new site and the reuse is reported by
//! [`RendererPool::evictions`] — a controlled degradation, never a hard failure.

use super::*;
use crate::site::SiteKey;
use std::collections::VecDeque;

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
    pending: VecDeque<u64>,
    last_generation: Option<u64>,
}

struct RetiredSlot {
    receiver: Receiver<WorkerMessage>,
    handle: JoinHandle<()>,
    pending: VecDeque<u64>,
    last_generation: Option<u64>,
}

fn take_pending_generation(pending: &mut VecDeque<u64>, generation: u64) {
    if pending.front() == Some(&generation) {
        pending.pop_front();
    } else if let Some(pos) = pending.iter().position(|&g| g == generation) {
        pending.remove(pos);
    }
}

fn panic_reason(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

fn drain_slot_ready(
    receiver: &Receiver<WorkerMessage>,
    pending: &mut VecDeque<u64>,
    out: &mut Vec<WorkerMessage>,
) {
    while let Ok(message) = receiver.try_recv() {
        take_pending_generation(pending, message.generation());
        out.push(message);
    }
}

impl RetiredSlot {
    fn drain_ready(&mut self, out: &mut Vec<WorkerMessage>) {
        drain_slot_ready(&self.receiver, &mut self.pending, out);
    }

    fn finish_join(self, out: &mut Vec<WorkerMessage>, panics: &mut usize) {
        let Self {
            receiver,
            handle,
            mut pending,
            last_generation,
        } = self;
        let join_result = handle.join();
        drain_slot_ready(&receiver, &mut pending, out);
        if let Err(payload) = join_result {
            *panics = panics.saturating_add(1);
            let reason = panic_reason(payload);
            eprintln!("megi: evicted render worker panicked: {reason}");
            let message = format!("evicted render worker panicked: {reason}");
            if pending.is_empty() {
                out.push(WorkerMessage::Failed {
                    generation: last_generation.unwrap_or(0),
                    message,
                });
            } else {
                while let Some(generation) = pending.pop_front() {
                    out.push(WorkerMessage::Failed {
                        generation,
                        message: message.clone(),
                    });
                }
            }
        } else {
            while let Some(generation) = pending.pop_front() {
                out.push(WorkerMessage::Failed {
                    generation,
                    message: "evicted render worker exited before completing render".to_owned(),
                });
            }
        }
    }
}

/// A bounded pool of render workers keyed by effective site.
pub struct RendererPool {
    slots: Vec<Slot>,
    retired: Vec<RetiredSlot>,
    retired_messages: Vec<WorkerMessage>,
    max: usize,
    clock: u64,
    evictions: usize,
    evicted_panics: usize,
}

impl RendererPool {
    /// Creates an empty pool. `factory` builds one worker (sender/receiver/handle).
    pub fn new(max: usize) -> Self {
        Self {
            slots: Vec::new(),
            retired: Vec::new(),
            retired_messages: Vec::new(),
            max: max.clamp(1, MAX_SITE_RENDERERS),
            clock: 0,
            evictions: 0,
            evicted_panics: 0,
        }
    }

    fn reap_retired_nonblocking(&mut self, out: &mut Vec<WorkerMessage>) {
        let retired = std::mem::take(&mut self.retired);
        for mut slot in retired {
            slot.drain_ready(out);
            if slot.handle.is_finished() {
                slot.finish_join(out, &mut self.evicted_panics);
            } else {
                self.retired.push(slot);
            }
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
                    pending: VecDeque::new(),
                    last_generation: None,
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
                        pending: VecDeque::new(),
                        last_generation: None,
                    },
                );
                // Close channel so the evicted thread exits once its queue drains, and
                // retain its handle so a non-blocking reaper and `join()` always observe it (#2192).
                drop(old.sender);
                self.retired.push(RetiredSlot {
                    receiver: old.receiver,
                    handle: old.handle,
                    pending: old.pending,
                    last_generation: old.last_generation,
                });
                let mut retired_out = std::mem::take(&mut self.retired_messages);
                self.reap_retired_nonblocking(&mut retired_out);
                self.retired_messages = retired_out;
                self.evictions = self.evictions.saturating_add(1);
                index
            }
        };
        let generation = request.generation;
        self.slots[index].last_used = self.clock;
        self.slots[index]
            .sender
            .send(request)
            .map_err(|_| "render worker channel closed".to_owned())?;
        self.slots[index].pending.push_back(generation);
        self.slots[index].last_generation = Some(generation);
        Ok(())
    }

    /// Drains every worker's message queue and reaps finished evicted workers without
    /// blocking the UI thread (#2192).
    pub fn drain_messages(&mut self) -> Vec<WorkerMessage> {
        let mut out = std::mem::take(&mut self.retired_messages);
        for slot in &mut self.slots {
            drain_slot_ready(&slot.receiver, &mut slot.pending, &mut out);
        }
        self.reap_retired_nonblocking(&mut out);
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

    /// Total evicted worker threads that panicked when reaped/joined (diagnostics/tests).
    pub fn evicted_panics(&self) -> usize {
        self.evicted_panics
    }

    /// Evicted worker threads still finishing before being reaped (diagnostics/tests).
    pub fn retired_len(&self) -> usize {
        self.retired.len()
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

    /// Joins all active and previously evicted worker threads (called when shutting down).
    pub fn join(self) {
        for slot in self.slots {
            drop(slot.sender);
            let _ = slot.handle.join();
        }
        for retired in self.retired {
            let _ = retired.handle.join();
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

    #[test]
    fn evicted_worker_panic_is_reaped_and_surfaced_as_failed_message() {
        let mut pool = RendererPool::new(1);
        let panic_factory = || {
            let (req_tx, req_rx) = std::sync::mpsc::channel::<RenderRequest>();
            let (_msg_tx, msg_rx) = std::sync::mpsc::channel::<WorkerMessage>();
            let handle = std::thread::spawn(move || {
                while let Ok(_req) = req_rx.recv() {}
                panic!("simulated worker panic on eviction");
            });
            (req_tx, msg_rx, handle)
        };
        pool.send(request("https://a.test/"), panic_factory)
            .expect("send a");
        pool.send(
            request("https://b.test/"),
            factory(Arc::new(AtomicUsize::new(0))),
        )
        .expect("send b evicts a");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut failed_msgs = Vec::new();
        while std::time::Instant::now() < deadline {
            for msg in pool.drain_messages() {
                if let WorkerMessage::Failed { message, .. } = msg {
                    failed_msgs.push(message);
                }
            }
            if !failed_msgs.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            failed_msgs
                .iter()
                .any(|m| m.contains("simulated worker panic on eviction")),
            "expected panic failure message, got {failed_msgs:?}"
        );
        assert_eq!(pool.evicted_panics(), 1);
        assert_eq!(pool.retired_len(), 0);
        pool.join();
    }

    #[test]
    fn shutdown_join_waits_for_evicted_workers_without_blocking_eviction() {
        use std::sync::atomic::AtomicBool;
        let finished = Arc::new(AtomicBool::new(false));
        let finished_flag = Arc::clone(&finished);
        let mut pool = RendererPool::new(1);

        let slow_exit_factory = move || {
            let (req_tx, req_rx) = std::sync::mpsc::channel::<RenderRequest>();
            let (_msg_tx, msg_rx) = std::sync::mpsc::channel::<WorkerMessage>();
            let flag = Arc::clone(&finished_flag);
            let handle = std::thread::spawn(move || {
                while let Ok(_req) = req_rx.recv() {}
                std::thread::sleep(std::time::Duration::from_millis(60));
                flag.store(true, Ordering::SeqCst);
            });
            (req_tx, msg_rx, handle)
        };

        pool.send(request("https://slow.test/"), slow_exit_factory)
            .expect("send slow");
        pool.send(
            request("https://fast.test/"),
            factory(Arc::new(AtomicUsize::new(0))),
        )
        .expect("evict slow");
        assert!(
            !finished.load(Ordering::SeqCst),
            "eviction must not block the UI thread synchronously"
        );
        assert_eq!(pool.retired_len(), 1);

        pool.join();
        assert!(
            finished.load(Ordering::SeqCst),
            "shutdown join must wait for retired evicted workers too"
        );
    }
}
