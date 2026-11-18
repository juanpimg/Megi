# JS Host Bridge Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the `Rc<RefCell<Host>>` + TLS bridge between Boa native bindings and Megi's JS host state, replacing it with an owned cell inside Boa's `HostDefined` store, so no binding can panic with `already borrowed` and the engine stays 100 % safe Rust.

**Architecture:** Three independent designs were debated by subagents (field-level cells, command parking, Boa `HostDefined`). The chosen design uses Boa `HostDefined` as the single owner of `Host`; bindings borrow it from the `&mut Context` they already receive, so the compiler prevents holding a borrow across re-entrant JS calls. Domain splitting (`EventState`, `ObserverState`) and immutable `Arc` providers handle the few cases that genuinely need re-entry.

**Tech Stack:** Rust Edition 2024, `boa_engine` 0.22 (`Context::insert_data`, `Context::get_data`, `HostDefined::get_mut`), `std::cell::RefCell`/`RefMut`, `Arc` for providers, no `unsafe`, no new dependency.

**Spec:** `docs/superpowers/specs/2026-11-18-js-host-bridge-design.md`

## Global Constraints

- Zero `unsafe` in `crates/js/src` (`#![forbid(unsafe_code)]`).
- Keep Boa 0.22 and MSRV 1.91. No new dependency, no V8/QuickJS/SpiderMonkey.
- `cargo test -p megi-js` must stay green after **every** task, not only at the end.
- Full workspace gate after every task: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo run --release -p megi-test-harness -- --summary` (PASS), `python3 tools/check_arch.py`.
- Never reintroduce a detached copy of `Host` (it broke 10+ tests with stale DOM node tables).
- `document`, `nodes` and `node_index` must remain in the same state cell.
- No public API may expose `Rc`, `RefCell` or a lifetime parameter.

## Review Focus

1. **Nested event dispatch** — a listener that dispatches another event must not panic. Test: `crates/js/tests/reentrancy.rs::nested_event_dispatch_can_mutate_dom_reentrantly` (exists, keep green).
2. **Provider re-entrancy** — a provider (layout/computed-style) that calls back into JS must find the host parked in the `Context`. Test added in Task 4.
3. **Stale node handles** — DOM nodes created by the outer call must be visible to nested calls. Test added in Task 4 (`handle_for` after `appendChild` inside a nested callback).
4. **Worker/iframe realms** — cross-realm links must not keep a shared mutable `Host`. Gate: existing `workers` + `iframe` test files in `crates/js/tests` and `crates/render/tests`.
5. **Silent hold across JS calls** — any `RefMut` kept while calling `callback.call`/`obj.get` is a latent bug. Verification: `try_borrow_mut` returns a typed `JsError` instead of panicking; audit checklist in Task 6.

---

### Task 1: Install the host cell in Boa's Context (parallel bridge)

**Files:**
- Modify: `crates/js/src/host.rs`
- Modify: `crates/js/src/runtime.rs` (construction sites ~166-232)
- Test: `crates/js/tests/host_cell.rs` (new)

**Interfaces:**
- Consumes: `Host::new(document, url)` (`host.rs:646`).
- Produces: `install_host(&mut Context, Rc<RefCell<Host>>)`, `host_cell(&mut Context) -> JsResult<RefMut<'_, Host>>`, `host_ref(&Context) -> JsResult<Ref<'_, Host>>`.

- [ ] **Step 1: Write the failing test**

```rust
// crates/js/tests/host_cell.rs
use boa_engine::Context;
use megi_js::host_for_test::*;

#[test]
fn host_is_reachable_from_the_context_and_not_from_tls() {
    let mut ctx = Context::default();
    install_host(&mut ctx, demo_host());
    assert!(host_ref(&ctx).is_ok());
    assert_eq!(host_ref(&ctx).unwrap().url, "about:blank");
}
```

`demo_host()` builds the same `Rc<RefCell<Host>>` that the runtime owns. Put this test in `#[cfg(test)] mod tests` inside `host.rs` (the functions are `pub(crate)`; a public test-only module would violate the no-lifetime rule).

- [ ] **Step 2: Run `cargo test -p megi-js --test host_cell`** → FAIL: `install_host` not found.
- [ ] **Step 3: Implement the three functions in `host.rs` using `Context::insert_data(RefCell::new(host))` and `host_defined_mut().get_mut::<RefCell<Host>>()`.** Call `install_host` ONCE at `runtime.rs:219` in `with_document_realm`, passing the same `Rc<RefCell<Host>>`. Do NOT install a second Host; `install_bare_worker_globals` receives no Host and cannot call it.
- [ ] **Step 4: Run the test** → PASS. Run `cargo test -p megi-js` → all existing tests PASS (parallel bridge changes nothing).
- [ ] **Step 5: Commit** `refactor(js): install host cell in Boa context`.

### Task 2: Migrate leaf bindings off `with_host`

**Files:** `crates/js/src/runtime.rs` (console, storage, history, timers, viewport/media sections), `crates/js/src/host.rs`
**Interfaces:** consumes Task 1's `host_cell`; produces no new API.
**Scope:** ~60 `with_host` call sites in functions that do **not** call JS/providers.

- [ ] **Step 1: Add a regression test in `tools/test_check_arch.py` named `test_leaf_bindings_no_longer_use_tls_bridge` that reads `crates/js/src/runtime.rs` and asserts `text.count("with_host(") <= 156` (baseline 216; Task 2 migrates at least 66). Run `python3 -m unittest tools.test_check_arch -v` → FAIL with the current count.
- [ ] **Step 2: Replace `with_host(|h| …)` with `host_cell(context).map(|mut h| …)` in the named sections only.** Keep imports of `with_host` for the remaining sections.
- [ ] **Step 3: Run `cargo test -p megi-js` → PASS; `cargo clippy -p megi-js --all-targets -- -D warnings` → PASS.**
- [ ] **Step 4: Commit** `refactor(js): migrate console/storage/timer bindings to context host cell`.

### Task 3: Split `EventState` and migrate dispatch

**Files:** `crates/js/src/host.rs` (new `EventState`), `crates/js/src/events.rs` (all 53 `with_host` sites), `crates/js/src/runtime.rs` (dispatch entry points)
**Interfaces:**
- Produces: `with_events<R>(context, f) -> JsResult<R>`; `EventState { listeners, free_listener_slots, inline_handlers, dispatch, last_event_error, self_messages, scroll_x, scroll_y }`.
- `DomState` keeps `document`, `nodes`, `node_index` and all other fields.

- [ ] **Step 1: Add a compile-time RED assertion** in `tools/test_check_arch.py` requiring `crates/js/src/events.rs` to contain `with_events(` and no `host_cell(context)?.borrow().listeners`; run → FAIL today. The ignored behaviour tests in `crates/js/tests/host_reentrancy_new.rs` are enabled here and must pass.
- [ ] **Step 2: Move the 7 event fields into `EventState` and add `with_events`.** Migrate `events.rs` dispatch loops: snapshot `Vec<NodeId>` and listener indices, **drop the borrow**, then call JS. Never hold an `EventState` borrow across `callback.call` or `array_*`.
- [ ] **Step 3: Run `cargo test -p megi-js --test wp_a_events_timers --test event_advanced_tests` → PASS; full JS suite PASS.**
- [ ] **Step 4: Commit** `refactor(js): split EventState and migrate event dispatch`.

### Task 4: Split `ObserverState`, providers, and the stale-handle test

**Files:** `crates/js/src/host.rs` (`ObserverState`, `HostProviders`), `crates/js/src/runtime.rs` (mutation observer + ~10 provider call sites), `crates/js/tests/reentrancy.rs`
**Interfaces:**
- Produces: `with_observers<R>(context, f)`, `install_providers(&mut Context, HostProviders)`, `providers(&Context) -> Option<&HostProviders>`.
- All 16 `Box<dyn Fn…>` provider aliases become `Arc<dyn Fn…>`; setter signatures in `runtime.rs` change from `Box` to `Arc` (update callers in `render`/`test_harness` in the same commit).

- [ ] **Step 1: Write two failing tests** in `crates/js/tests/reentrancy.rs`:
  - `provider_can_call_back_into_javascript`: install a computed-style provider that calls `runtime.eval("window.__called = 1")`, trigger `getComputedStyle`, assert `__called == 1`.
  - Enable the already-written ignored test `nested_callback_sees_nodes_created_by_outer` for the same-realm path (the provider probe tests use a short-lived runtime because providers are `'static + Send + Sync`; add a real re-entry handle when the provider API allows it).
- [ ] **Step 2: Implement `ObserverState`, `HostProviders` and migrate the provider call sites** so providers are cloned from `Arc` and invoked with **no active borrow**.
- [ ] **Step 3: Run both tests → PASS; full JS suite → PASS.**
- [ ] **Step 4: Commit** `refactor(js): split ObserverState and move providers out of the borrow`.

### Task 5: Workers, iframes and the final TLS deletion

**Files:** `crates/js/src/runtime.rs` (~156 remaining `with_host` sites, workers/frames sections, module loader), `crates/js/src/host.rs`, `crates/js/src/events.rs`, `crates/js/src/websocket.rs`, `crates/js/src/xhr.rs`, `crates/js/src/canvas.rs`
**Interfaces:** consumes all previous; produces `JsRuntime { context: Context }` with no `host` field.

- [ ] **Step 1: Replace every remaining `with_host(|h| …)` with `host_cell(context).map(...)`.** For helpers without `context`, thread `&mut Context` through; do not add a new TLS.
- [ ] **Step 2: Remove `MegiModuleLoader.host_slot` (`runtime.rs:203-219,9890`) by resolving the host from the `&mut Context` passed to `ModuleLoader::load_imported_module`. Then convert cross-realm links** (`worker_parent`, `parent_outbox`, `worker_outbox`, `frames`) from `Rc<RefCell<…>>` to owned `VecDeque` outboxes drained by the existing `run_worker_tasks`/pump. Keep nested `JsRuntime`s owning their own `Context`.
- [ ] **Step 3: Delete `CURRENT_HOST`, `HostScope`, `current_host`, `with_host`, the `host` field and all `Rc` imports from `crates/js/src`.** Replace residual `borrow_mut()` with `try_borrow_mut()` mapped to a typed `JsError`.
- [ ] **Step 4: Verify:** `grep -Rn 'CURRENT_HOST\|with_host\|Rc<RefCell<Host>>' crates/js/src` → 0 lines. Do NOT require zero `Rc<`: Boa's module loader requires `Rc<Self>` and `Rc<M>`/`Rc<H>` context data. `cargo test -p megi-js` → PASS.
- [ ] **Step 5: Update `docs/architecture-baseline.json`** to drop the `megi-js` legacy `Rc`/`RefCell` entries; `tools/check_arch.py` → OK with fewer legacy items.
- [ ] **Step 6: Commit** `refactor(js): delete TLS host bridge and shared Rc state`.

### Task 6: Whole-branch verification and documentation

**Files:** `docs/ARCHITECTURE.md`, `docs/JAVASCRIPT-STRATEGY.md`, `docs/superpowers/plans/2026-11-18-megi-modular-restructuring.md`
- [ ] **Step 1: Add the audit checklist** (one line per failure mode from Review Focus) to `docs/ARCHITECTURE.md` under the JS section.
- [ ] **Step 2: Update the plan status table** to mark Task 4.1 complete with the new evidence.
- [ ] **Step 3: Run the full gate** and paste output: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check --workspace --no-default-features`, `cargo test --workspace`, gate `PASS`, `python3 tools/check_arch.py`, `cargo run --release -p render -- tests/fixtures/render/basic.html /tmp/opencode/final.png` + md5 comparison with `e943565df9425852c2073298b8c0148c`.
- [ ] **Step 4: Commit** `docs(arch): close JS host bridge migration`.

---

## Self-review

- **Spec coverage:** §1 problem → Tasks 1–5; §2 decision → Tasks 1,3,4; §3 contract → Tasks 1,3,4; §4 migration → Tasks 1–6; §5 acceptance → Tasks 5–6. No gap.
- **Step scan:** each step names a file, a command and expected output. Task 2 Step 1 uses a grep-count assertion because the leaf migration is mechanical and behavior-preserving by construction.
- **Type consistency:** `host_cell`, `with_events`, `with_observers`, `HostProviders` keep the same names in Tasks 1–5. Provider aliases change from `Box` to `Arc` in Task 4 and are consumed that way in Task 5.
- **Review Focus:** all five input classes pin to a test or command (reentrancy.rs, Task 4 tests, iframe/worker suites, Task 5 grep, Task 6 checklist).
- **Proportion:** the plan is shorter than the combined spec + the ~273 call-site migration it describes; no full program bodies are transcribed.
