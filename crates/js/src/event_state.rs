//! Event-state sub-cell for the JS ↔ host bridge.
//!
//! During event dispatch a binding must read the registered listeners, take a
//! snapshot, and then invoke a JavaScript callback. Holding the whole `Host`
//! borrow across that callback is what used to panic with `already borrowed`
//! (a listener that dispatches a nested event or mutates the DOM re-enters the
//! bridge). Splitting the event fields into their own sub-state keeps that
//! borrow narrow: DOM state (`document`, `nodes`, `node_index`) stays in `Host`
//! and is never held while JS runs.
//!
//! This module is intentionally NOT wired into `lib.rs` yet. It is staged for
//! the Task 3 migration and will not compile until Task 1 lands
//! (`crate::host::host_cell`) and the event fields are moved into the
//! `Host::events` field.

use boa_engine::{Context, JsResult};

use crate::host::{DispatchState, InlineHandler, Listener};

/// Mutable state for event registration and dispatch.
///
/// These are the four event fields lifted verbatim out of `Host`; their
/// callback values still live in the JS-side rooted table (only the numeric
/// `slot` is stored here). Keeping them together lets [`with_events`] hand out a
/// short, isolated borrow instead of locking the entire host for the duration
/// of a dispatch.
pub(crate) struct EventState {
    /// Registered `addEventListener` listeners, in registration order.
    pub listeners: Vec<Listener>,
    /// Reusable slots in the JS `__megi_listeners` array freed by
    /// `removeEventListener` / `once` (#567).
    pub free_listener_slots: Vec<u32>,
    /// Inline `on<event>` handlers (callbacks live in the JS-side rooted table).
    pub inline_handlers: Vec<InlineHandler>,
    /// Dispatch state of the event currently being delivered, if any.
    pub dispatch: Option<DispatchState>,
}

/// Runs `f` with exclusive access to the event sub-state of the host cell
/// installed in `context`.
///
/// # Borrow discipline (spec §3 "golden rules" and `events-map.md`)
///
/// 1. The borrow is taken on entry and is dropped as soon as `f` returns. Never
///    call `callback.call`, `obj.get`/`obj.set`, `array_*`, a provider, or any
///    other function that needs `&mut Context` while inside `f`: doing so would
///    try to re-borrow the same cell and panic (or, after Task 6, return a typed
///    `JsError`). Snapshot first, then call JS *outside* this function.
/// 2. Never store a `Ref`/`RefMut` — or anything borrowed from `EventState`,
///    such as a `NodeId`, a listener index, or a `&Listener` — across a JS call.
///    Copy the flags you need (`slot`, `once`, `passive`, `capture`, the
///    `stop_*`/`prevent_default` bits) into owned values before leaving.
/// 3. For dispatch, compute the `Vec<(PropPhase, TargetKey)>` propagation path
///    and a snapshot `Vec<(slot, capture, once, passive)>` plus an optional
///    inline slot while the borrow is held, release it, and only then walk the
///    snapshot calling listeners. Re-entrant dispatches therefore see a free
///    cell instead of panicking.
/// 4. `DispatchStateGuard::drop` must receive `&mut Context` or `&mut
///    EventState` explicitly (`dismiss_on_success(previous)` with an explicit
///    context); it must not rely on the old TLS to find the cell.
/// 5. If a re-entrant path is genuinely unavoidable, use `try_borrow_mut()`
///    and surface a typed `JsError` rather than panicking.
///
/// # Status
///
/// This is written against the planned interface: `host_cell` is produced by
/// Task 1 and the `Host::events` field by Task 3. Until both land this file is
/// not compiled (it is not declared in `lib.rs`) and will not type-check.
pub(crate) fn with_events<R>(
    context: &mut Context,
    f: impl FnOnce(&mut EventState) -> R,
) -> JsResult<R> {
    let mut host = crate::host::host_cell(context)?;
    Ok(f(&mut host.events))
}
