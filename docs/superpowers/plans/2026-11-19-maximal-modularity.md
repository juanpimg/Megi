# Maximal Modularity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring every Rust source file in the workspace under the 500-line hard cap from `AGENTS.md` by splitting large files into cohesive submodules without changing any behavior.

**Architecture:** Each oversized file becomes a thin facade that declares `mod` submodules and re-exports the public API. Code is moved verbatim; only crate-private visibility wideners (`pub(super)`, `pub(crate)`) and `use` statements are added. Submodules are grouped by responsibility (data types, parsers, state machines, painting phases, tests), never by arbitrary line ranges.

**Tech Stack:** Rust Edition 2024, Cargo workspace (17 crates), no new dependencies, no `unsafe`.

**Spec:** `AGENTS.md` §4 (file-size limits), `docs/ARCHITECTURE.md` (DAG), `docs/superpowers/specs/2026-11-18-megi-modular-architecture-design.md`.

## Global Constraints

- New/modified `.rs` file ≤ 500 lines (hard); target ≤ 400.
- Zero `unsafe`, zero new dependency, no change to crate DAG.
- Public API paths must remain identical (`pub use` from the facade).
- Every split is behavior-preserving: move code only; no algorithmic edits.
- `cargo test --workspace` and the conformance gate must stay green after every commit.
- `python3 tools/check_arch.py` and `python3 tools/audit_coverage.py --check --strict` must be green at the end (regenerate ledger with `--generate`, then mark newly created files `covered` via `--mark`).
- Agents may only edit files they own; create submodules under the same directory (`foo.rs` → `foo/*.rs`) and declare them from `foo.rs` to avoid touching crate roots.

## Review Focus

1. **Public API drift** — a moved type may become private or change path. Test: `cargo check --workspace --all-targets` plus existing crate tests.
2. **Visibility leaks** — widening `pub(super)` to `pub` accidentally. Reviewer checks diff for unintended `pub`.
3. **State machine / parser behavior** — tokenizer and tree-builder splits can change state transitions. Test: conformance gate 7050/7050 + 3904/3904 must remain PASS.
4. **Rendering byte identity** — paint/raster/svg splits can change draw order. Test: PNG md5 of `tests/fixtures/render/basic.html` must stay `e943565df9425852c2073298b8c0148c`.
5. **Lost tests** — moving `#[cfg(test)]` modules can drop coverage. Test: test count per crate must not decrease.

---

## Estado final: COMPLETO — 0 archivos >500 líneas

Todas las oleadas (1-4) terminaron. Verificación final en verde: cargo test --workspace, clippy -D warnings, fmt --check, no-default-features, conformance gate PASS, check_arch OK, PNG byte-idéntico.

## Current inventory (110 files, 130,935 lines)

| Crate | Files | Lines | Largest |
|---|---:|---:|---:|
| css | 18 | 20306 | properties/values.rs 2244 |
| js | 7 | 16964 | runtime.rs 11325 |
| layout | 14 | 15424 | block.rs 2310 |
| net | 17 | 14379 | h2/client.rs 1865 |
| browser | 10 | 9845 | tests/shell_tests.rs 1778 |
| render | 9 | 9364 | lib.rs 2689 |
| image | 7 | 9134 | vp8_decoder.rs 1855 |
| html | 4 | 7276 | tokenizer.rs 3602 |
| svg | 5 | 7041 | document.rs 2936 |
| raster | 3 | 5761 | rasterizer.rs 4493 |
| paint | 6 | 5419 | painter.rs 2170 |
| core | 3 | 3941 | lib.rs 1754 |
| bench | 2 | 2327 | benches.rs 1743 |
| test_harness | 3 | 1923 | js_executor.rs 856 |
| fonts | 2 | 1831 | db.rs 1256 |

## Wave 1 — largest single files (6 parallel tasks)

- [ ] **Task 1.1** `crates/js/src/runtime.rs` (11325) → `runtime/*.rs` by binding domain; facade ≤500. Gate: `cargo test -p megi-js`.
- [ ] **Task 1.2** `crates/raster/src/rasterizer.rs` (4493) → `rasterizer/*.rs`; facade ≤500. Gate: PNG md5 unchanged.
- [ ] **Task 1.3** `crates/html/src/tokenizer.rs` (3602) → `tokenizer/*.rs`; facade ≤500. Gate: tokenizer 7050/7050.
- [ ] **Task 1.4** `crates/svg/src/document.rs` (2936) → `document/*.rs`; facade ≤500. Gate: `cargo test -p megi-svg`.
- [ ] **Task 1.5** `crates/render/src/lib.rs` (2689) → `pipeline/*.rs`; facade ≤500. Gate: `cargo test -p megi-render`.
- [ ] **Task 1.6** `crates/layout/src/block.rs` (2310) → `block/*.rs`; facade ≤500. Gate: `cargo test -p megi-layout`.

## Wave 2 — next tier (6 parallel tasks)

- [ ] **Task 2.1** `crates/css/src/properties/values.rs` (2244)
- [ ] **Task 2.2** `crates/html/src/entities.rs` (2234) — generated table; split into `entities/*.rs` chunks with a facade.
- [ ] **Task 2.3** `crates/render/src/scripts.rs` (2198)
- [ ] **Task 2.4** `crates/svg/src/raster.rs` (2176)
- [ ] **Task 2.5** `crates/paint/src/painter.rs` (2170)
- [ ] **Task 2.6** `crates/layout/src/inline.rs` (2170)

## Wave 3 — remaining >1000-line files

Split the rest of the oversized files, grouped by crate, with one agent per disjoint file set:
- [ ] **Task 3.1** css: `selector.rs`, `lib.rs`, `cascade.rs`, `properties/parse.rs`, `properties/computed.rs` and other >500 css files.
- [ ] **Task 3.2** js: `events.rs` and other >500 js files.
- [ ] **Task 3.3** net: `h2/client.rs` and other >500 net files.
- [ ] **Task 3.4** image: `vp8_decoder.rs`, `jpeg.rs`, `inflate.rs`, `webp.rs` and other >500 image files.
- [ ] **Task 3.5** browser: `forms.rs`, `session.rs`, `tests/shell_tests.rs` and other >500 browser files.
- [ ] **Task 3.6** layout, paint, render, core, bench, test_harness, fonts, raster: all remaining >500 files.

## Final closure

- [ ] Regenerate `docs/audit/coverage.tsv` (`--generate`) and mark new files covered.
- [ ] Rebase `docs/architecture-baseline.json` oversized/pattern keys.
- [ ] Run full gate: test, clippy -D warnings, fmt --check, no-default-features, conformance PASS, check_arch OK, audit --check --strict, progress/coverage checks, PNG md5.
- [ ] Commit and hand off merge decision.
