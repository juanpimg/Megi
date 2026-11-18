//! Mutation-observer sub-cell for the JS ↔ host bridge.
//!
//! Delivering a `MutationObserver` callback needs two kinds of state at once:
//! the observer registrations/index/queues, and the DOM ancestor chain used to
//! match a mutation against `subtree` targets. Holding the whole `Host` borrow
//! across the callback is what used to panic with `already borrowed` (the
//! callback can mutate the DOM or register another observer, re-entering the
//! bridge). Splitting the observer fields into their own sub-state keeps that
//! borrow narrow: DOM state (`document`, `nodes`, `node_index`) stays in `Host`
//! and is never held while JS runs.
//!
//! This module is intentionally NOT wired into `lib.rs` yet. It is staged for
//! the Task 4 migration and will not compile until Task 1 lands
//! (`crate::host::host_cell`) and the observer fields are moved into the
//! `Host::observers` field.

use std::collections::HashMap;

use boa_engine::{Context, JsResult};
use megi_core::NodeId;

use crate::host::{MutationRecord, ObserverRegistration, ObserverTarget};

/// Mutable state for mutation-observer registration and record queuing.
///
/// These are the two observer fields lifted verbatim out of `Host`; the JS
/// callbacks themselves still live in the JS-side rooted table (only the
/// numeric `callback_slot` / `object_slot` are stored in a registration).
/// Keeping them together lets [`with_observers`] hand out a short, isolated
/// borrow instead of locking the entire host while observers are matched.
pub(crate) struct ObserverState {
    /// `MutationObserver` registrations and their per-observer pending records.
    pub observers: Vec<ObserverRegistration>,
    /// Observed target `NodeId` -> `(observer_index_in_observers, target_options)` index (#1134).
    pub observer_index: HashMap<NodeId, Vec<(usize, ObserverTarget)>>,
}

/// Runs `f` with exclusive access to the mutation-observer sub-state of the host
/// cell installed in `context`.
///
/// # Borrow discipline (spec §3 "golden rules")
///
/// 1. The borrow is taken on entry and dropped as soon as `f` returns. `f` gets
///    only `&mut ObserverState` — **no `&mut Context`** — so it structurally
///    cannot call `callback.call`, `obj.get`/`obj.set`, `array_*`, a provider, or
///    anything else that needs the context while the borrow is alive. Snapshot
///    first, then call JS *outside* this function.
/// 2. Never store a `Ref`/`RefMut`, or anything borrowed from `ObserverState`
///    (a `&MutationRecord`, a `&ObserverRegistration`, an observer index),
///    across a JS call. Copy the `slot`s and clone the records you need into
///    owned values before leaving.
/// 3. If a re-entrant path is genuinely unavoidable, use `try_borrow_mut()`
///    and surface a typed `JsError` rather than panicking.
///
/// # Status
///
/// Written against the planned interface: `host_cell` is produced by Task 1 and
/// the `Host::observers` field by Task 4. Until both land this file is not
/// compiled (it is not declared in `lib.rs`) and will not type-check.
pub(crate) fn with_observers<R>(
    context: &mut Context,
    f: impl FnOnce(&mut ObserverState) -> R,
) -> JsResult<R> {
    let mut host = crate::host::host_cell(context)?;
    Ok(f(&mut host.observers))
}

/// Records a DOM mutation for delivery to every matching `MutationObserver`.
///
/// # Snapshot-then-call discipline
///
/// Matching and callback delivery need two disjoint pieces of state, so this
/// function must never hold a single borrow over the whole operation. The
/// intended sequence is:
///
/// 1. **Snapshot the DOM path.** Under `host_cell`, walk `record.target`'s
///    ancestor chain into an owned `Vec<NodeId>` (bounded by `document.len()` to
///    terminate on a cyclic parent chain), then **drop that borrow**. DOM state
///    is never held while JS runs.
/// 2. **Queue records.** Under [`with_observers`], match that snapshot against
///    `observer_index`, honouring `matches_kind`, `subtree` and
///    `attribute_old_value` and the `MAX_OBSERVER_RECORDS` cap. Clone the
///    `MutationRecord` into each matching registration's queue, then snapshot
///    the `(callback_slot, object_slot, Vec<MutationRecord>)` triples to
///    deliver, and **drop the observer borrow**. No `&MutationRecord` or
///    observer index may survive this step.
/// 3. **Call JS.** Only now, with no cell borrowed, invoke each observer's
///    callback with the cloned records. A callback may freely mutate the DOM or
///    register/disconnect observers: re-entrant calls find a free cell and
///    mutate through their own short snapshots instead of panicking with
///    `already borrowed` or observing a stale copy.
///
/// Any re-entrant acquisition that cannot wait uses `try_borrow_mut()` and is
/// surfaced as a typed `JsError`.
///
/// # Status
///
/// Staged signature for the Task 4 migration; the body is filled in once the
/// observer cell, the JS-side callback roots and the DOM snapshot helper land.
/// It is not compiled until this module is declared in `lib.rs`.
pub(crate) fn record_mutation(
    shared_context: &mut Context,
    record: MutationRecord,
) -> JsResult<()> {
    let _ = (shared_context, record);
    todo!("Task 4: snapshot the DOM path, queue records via with_observers, then call callbacks")
}
