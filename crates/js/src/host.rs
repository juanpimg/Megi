//! Host state shared between the Rust runtime and the Boa native callbacks.
//!
//! The bridge is deliberately small and auditable:
//!
//! - **One realm per runtime/document.** [`crate::JsRuntime`] owns a [`dom::Document`] and a
//!   Boa `Context`; a script mutates the document through native functions.
//! - **Side table `NodeId` ↔ JS handle.** JS wrappers carry a numeric handle (index into
//!   the host side table); the actual generation-checked [`NodeId`] stays in Rust. Every access
//!   revalidates it with [`dom::Document::try_node`], so stale/removed nodes raise a typed
//!   JS error instead of panicking (handles are never dereferenced blindly).
//! - **Per-thread current host.** Boa native functions are plain `fn` pointers (no
//!   captures); the runtime publishes its host in a thread-local for the duration of an
//!   `eval`. Tests run in parallel threads, so each thread sees its own runtime.
//!
//! Limits documented for consumers: the side table keeps one entry per distinct node
//! handle ever exposed to JS (bounded by the DOM arena plus removed nodes until wrappers
//! are collected; a generational arena keeps the raw ids distinct). No network/FS APIs are
//! exposed to scripts.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use dom::{Document, NodeId};

/// One captured `console.*` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleEntry {
    /// `log`, `info`, `warn` or `error`.
    pub level: String,
    /// Space-joined stringification of the arguments.
    pub message: String,
}

/// Identity of an event target in the realm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TargetKey {
    Node(u32),
    Window,
    Document,
}

/// A registered event listener (its callback lives in the JS-side rooted table).
#[derive(Clone, Debug)]
pub(crate) struct Listener {
    pub target: TargetKey,
    pub event_type: String,
    pub capture: bool,
    pub slot: u32,
}

/// Dispatch state for the event currently being delivered.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DispatchState {
    pub prevent_default: bool,
    pub stop_propagation: bool,
    pub stop_immediate: bool,
}

/// A scheduled timer.
#[derive(Clone, Debug)]
pub(crate) struct TimerEntry {
    pub id: u64,
    pub due_ms: u64,
    pub interval_ms: Option<u64>,
}

/// Computed-style provider injected by the host (`window.getComputedStyle`).
pub type ComputedStyleProvider =
    Box<dyn Fn(&Document, NodeId) -> Vec<(String, String)> + Send + Sync>;

/// Shared, mutable host state.
pub(crate) struct Host {
    /// The document scripts operate on.
    pub document: Document,
    /// Captured `console` calls (until the host drains them).
    pub console: Vec<ConsoleEntry>,
    /// `document.title` value.
    pub title: String,
    /// `document.URL` value.
    pub url: String,
    /// JS handle → generation-checked node handle (side table).
    pub nodes: Vec<NodeId>,
    /// Raw packed id → handle index, to avoid one entry per wrapper creation.
    node_index: HashMap<u32, usize>,
    /// Maximum number of side-table entries (anti-DoS for wrapper churn).
    pub max_handles: usize,
    /// Registered event listeners.
    pub listeners: Vec<Listener>,
    /// Scheduled timers.
    pub timers: Vec<TimerEntry>,
    /// Next timer id (insertion order for `(due, id)` ordering).
    pub next_timer_id: u64,
    /// Real-clock origin.
    pub started_at: Instant,
    /// Virtual clock override (`Some(ms)` after `advance`/`run_until`).
    pub virtual_now_ms: Option<u64>,
    /// Dispatch state of the event currently being delivered.
    pub dispatch: Option<DispatchState>,
    /// Injected computed-style provider.
    pub computed_style_provider: Option<ComputedStyleProvider>,
}

impl Host {
    /// Creates the host around a document.
    pub fn new(document: Document, url: String) -> Self {
        Self {
            document,
            console: Vec::new(),
            title: String::new(),
            url,
            nodes: Vec::new(),
            node_index: HashMap::new(),
            max_handles: 1_000_000,
            listeners: Vec::new(),
            timers: Vec::new(),
            next_timer_id: 1,
            started_at: Instant::now(),
            virtual_now_ms: None,
            dispatch: None,
            computed_style_provider: None,
        }
    }

    /// Returns (and caches) the numeric JS handle for `node_id`.
    pub fn handle_for(&mut self, node_id: NodeId) -> Result<u32, String> {
        let raw = node_id.get();
        if let Some(index) = self.node_index.get(&raw) {
            return u32::try_from(*index).map_err(|_| "node handle table overflow".to_string());
        }
        if self.nodes.len() >= self.max_handles {
            return Err("too many DOM node handles exposed to JavaScript".to_string());
        }
        let index = self.nodes.len();
        self.nodes.push(node_id);
        self.node_index.insert(raw, index);
        u32::try_from(index).map_err(|_| "node handle table overflow".to_string())
    }

    /// Validates a JS handle and returns the live node id.
    pub fn node_for_handle(&self, handle: u32) -> Result<NodeId, String> {
        let node_id = *self
            .nodes
            .get(handle as usize)
            .ok_or_else(|| format!("invalid DOM node handle {handle}"))?;
        if self.document.try_node(node_id).is_none() {
            return Err(format!(
                "stale DOM node handle {handle}: the node no longer exists"
            ));
        }
        Ok(node_id)
    }

    /// Current scheduler time in milliseconds (virtual clock or real elapsed).
    pub fn now_ms(&self) -> u64 {
        match self.virtual_now_ms {
            Some(now) => now,
            None => self.started_at.elapsed().as_millis() as u64,
        }
    }

    /// Advances the virtual clock by `delta`; returns the new time.
    pub fn advance_clock(&mut self, delta: Duration) -> u64 {
        let now = self.now_ms() + delta.as_millis() as u64;
        self.virtual_now_ms = Some(now);
        now
    }

    /// Sets the virtual clock (never backwards); returns the effective time.
    pub fn set_clock(&mut self, deadline_ms: u64) -> u64 {
        let now = self.now_ms().max(deadline_ms);
        self.virtual_now_ms = Some(now);
        now
    }

    /// Drains captured console entries.
    pub fn take_console(&mut self) -> Vec<ConsoleEntry> {
        std::mem::take(&mut self.console)
    }
}

thread_local! {
    static CURRENT_HOST: RefCell<Option<Rc<RefCell<Host>>>> = const { RefCell::new(None) };
}

/// Publishes `host` as the current thread's host; restores the previous value on drop.
pub(crate) struct HostScope(Option<Rc<RefCell<Host>>>);

impl HostScope {
    pub fn enter(host: &Rc<RefCell<Host>>) -> Self {
        let previous = CURRENT_HOST.with(|cell| cell.borrow_mut().replace(Rc::clone(host)));
        Self(previous)
    }
}

impl Drop for HostScope {
    fn drop(&mut self) {
        CURRENT_HOST.with(|cell| {
            *cell.borrow_mut() = self.0.take();
        });
    }
}

/// Returns a clone of the current thread's host, if a runtime is evaluating.
pub(crate) fn current_host() -> Option<Rc<RefCell<Host>>> {
    CURRENT_HOST.with(|cell| cell.borrow().clone())
}

/// Runs `f` with exclusive access to the current host.
pub(crate) fn with_host<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
    let host = current_host()?;
    let mut borrowed = host.borrow_mut();
    Some(f(&mut borrowed))
}
