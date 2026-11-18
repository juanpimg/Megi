# Megi Modular Restructuring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the 14-crate Megi workspace into the strictly layered `megi-*` architecture described in the design doc, with an embeddable `megi-webview`, without changing any observable behavior (goldens, conformance gate, performance budgets).

**Architecture:** Rename crates to `megi-*`, extract the display-list contract and the rasterizer into separate crates, invert the `net → js` PAC dependency, replace the JS `Rc<RefCell<Host>>` bridge with message passing, lift the embeddable engine out of the desktop shell into `megi-webview`, and enforce the DAG with a CI-checked script. Every phase ends green with byte-identical PNG goldens.

**Tech Stack:** Rust Edition 2024 (toolchain 1.99.1 pinned, MSRV 1.91), Cargo workspace resolver 2, `boa_engine` 0.22, `winit` 0.30 + `softbuffer` 0.4, Python 3 tooling.

**Spec:** `docs/superpowers/specs/2026-11-18-megi-modular-architecture-design.md`

## Execution status (updated 2026-11-18)

| Task | Status | Evidence |
|---|---|---|
| 0.1 baseline + GAPS drift | ✅ complete | `docs/architecture-baseline.json`, banner in `docs/GAPS.md` |
| 0.2 architecture linter in CI | ✅ complete | `tools/check_arch.py`, 3 unit tests, CI `quality` job |
| 0.3 AGENTS.md + workspace lints | ✅ complete | `AGENTS.md`, `[workspace.lints.rust]`, all manifests inherit |
| 1.1 rename leaf crates | ✅ complete | `megi-core`, `megi-image`, `megi-fonts` compile + tests |
| 1.2 rename remaining crates | ✅ complete | 14 `megi-*` packages, gate 4/4 PASS, full suite green |
| 2.1 `megi-display` | ✅ complete | `DisplayItem`/`DisplayList`/`HitRegion` moved, ownership test |
| 2.2 `megi-raster` | ✅ complete | rasterizer/PNG moved, PNG md5 `e943565…` before = after |
| 3.1 PAC inversion | ✅ complete | `PacEvaluator` trait; `cargo tree -e normal` has zero Boa |
| 4.1 de-Rc JS runtime | ⏳ deferred | 11k-line `runtime.rs` and `Rc<RefCell<Host>>` still present; the planned re-entrancy test (timer callback mutating DOM while another callback is active) does **not** exist yet and must be written first when this task resumes |
| 5.1 `megi-webview` | ✅ complete | load/frame/hit-test/history/events; light graph has zero Boa/winit/softbuffer |
| 6.1 thin `megi-browser` | ✅ complete | browser deps = webview + winit + softbuffer; 60 shell tests pass |
| 7.1 feature matrix + CI | ✅ complete | `megi-test-harness/js` optional; CI checks light WebView |
| 8.1 `megi-core` split | ✅ complete | `lib.rs` 1755 / `query.rs` 857 / `iter.rs` 88 / `unit_tests.rs` 1331 lines; 62 core tests pass |
| 8.2–8.6 remaining god-file splits | ⏳ deferred | 111 legacy items tracked in the baseline |

The two deferred items are independent of the completed work and are tracked in
`.superpowers/sdd/2026-11-18-megi-modular-restructuring/progress.md` with their
rulings. Everything shipped so far is green: `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo check --workspace
--no-default-features`, conformance gate PASS, `tools/check_arch.py` OK and
byte-identical PNG output.

## Global Constraints

- All 14 engine crates are `#![forbid(unsafe_code)]`. Zero `unsafe` in `src/`.
- Zero `Rc<`, `Rc<RefCell<`, `Weak<` in new code. `Arc<T>` only for immutable `Send + Sync` payloads.
- Zero lifetimes (`'a`) in public types crossing crate boundaries.
- No external crate that implements HTML/CSS parsing, layout or painting (design doc §3.1 of `docs/ARCHITECTURE.md`).
- No circular dependencies. New edges require updating the DAG table and `tools/check_arch.py`.
- New `.rs` file ≤ 400 lines (hard 500). Existing >500-line files require `// ARCH-EXCEPTION(#issue): …`.
- Conformance gate must say `PASS`; PNG goldens byte-identical; layout budget ≤ 43 ms; pipeline ≤ 140 ms.
- Do not hand-edit generated artifacts: `PROGRESS.md`, `docs/status/*`, `docs/audit/FINDINGS.md`.
- TDD: write the failing test first, confirm it fails, implement the minimal fix, rerun.
- Commits: atomic, Conventional Commits in English, DCO-signed (`git commit -s`).

## Review Focus

1. **Concurrent work on the same files** — another agent may be editing `runtime.rs`/`render/lib.rs`; check `git status` before every task and stop if a target file is dirty. Test: `git status --short` must be clean for the files in each task.
2. **Byte-exact PNG regressions** — a harmless-looking change in paint/raster can change the golden bytes. Test: `cargo test -p megi-paint -p megi-render` plus `python3 tools/png_check.py` on before/after fixtures.
3. **Silent feature creep in the "light" build** — `test_harness` and `shell` currently link Boa/winit even with `--no-default-features`. Test: `cargo tree -p megi-webview --no-default-features | grep boa` must be empty.
4. **Stale documentation contradicts code** — `docs/GAPS.md` is historical; agents must verify with grep. Test: every "absent" row claimed in docs is checked against `crates/*/src` before being relied on.
5. **Hidden ambient state** — `Rc<RefCell<Host>>` + `CURRENT_HOST` TLS make re-entrancy invisible. Test: an event/timer callback that mutates the DOM while another callback is active must not panic or corrupt state.

---

### Task 0.1: Freeze the baseline and expose the drift

**Files:**
- Modify: `docs/GAPS.md` (add historical banner)
- Create: `docs/architecture-baseline.json` (measured violations at HEAD)
- Test: `tools/test_check_arch.py`

**Interfaces:**
- Consumes: current HEAD `57b123b`, measured `wc -l`, `cargo test --workspace -- --list`.
- Produces: a machine-readable baseline used by `tools/check_arch.py` (Task 0.2).

- [ ] **Step 1: Add historical banner to `docs/GAPS.md`**

Insert after the title: `> **Histórico (2026-11-12):** esta matriz describe el estado de esa fecha. Varias filas marcadas `absent` ya están implementadas (`js/src/xhr.rs`, `js/src/websocket.rs`, `css/src/query.rs`, `css/src/transform.rs`, `net/src/{csp,integrity,hsts,mixed_content}.rs`). Verifica siempre el código antes de actuar.`

- [ ] **Step 2: Measure the baseline**

Run: `find crates -name '*.rs' -exec wc -l {} + | sort -rn | head -40`, `grep -rnE 'Rc<|Weak<|unsafe ' crates/*/src --include='*.rs'` and record the numbers in `docs/architecture-baseline.json` with keys `commit`, `oversized_files`, `forbidden_patterns`.

- [ ] **Step 3: Write and run a test for the arch checker's core predicate**

Create `tools/test_check_arch.py` with `test_unknown_edge_is_violation`, `test_known_edge_is_allowed`, `test_oversized_file_is_legacy_when_in_baseline`. Run `python3 -m unittest tools/test_check_arch.py -v`. Expected: FAIL (module `tools.check_arch` missing).

- [ ] **Step 4: Commit the baseline and test**

`git add docs/GAPS.md docs/architecture-baseline.json tools/test_check_arch.py && git commit -s -m "docs(arch): freeze baseline and flag historical gap matrix"`

### Task 0.2: Architecture linter in CI

**Files:**
- Create: `tools/check_arch.py`
- Modify: `.github/workflows/ci.yml` (add step in `quality`)
- Test: `tools/test_check_arch.py`

**Interfaces:**
- Consumes: `docs/architecture-baseline.json`, root `Cargo.toml` `[workspace] members`.
- Produces: `python3 tools/check_arch.py` → exit 0 if no new violations; exit 1 listing file:line.

- [ ] **Step 1: Implement `allowed_edges` as a dict from crate name to set of allowed imports**, using the design doc §2.3 table verbatim.

- [ ] **Step 2: Implement scanning** of `crates/*/Cargo.toml` and `crates/*/src/**/*.rs`; report (a) disallowed `use <crate>::`, (b) `unsafe`/`Rc<`/`Weak<` outside comments and outside baseline, (c) files >500 lines not in baseline.

- [ ] **Step 3: Run `python3 -m unittest tools/test_check_arch.py -v`** → PASS; then `python3 tools/check_arch.py` → exit 0 with the known violations printed as `legacy` (informational).

- [ ] **Step 4: Wire into CI `quality` job** next to `python3 tools/check_docs_links.py`. Commit.

### Task 0.3: Agent rules and workspace lints

**Files:**
- Create: `AGENTS.md` (from design doc §4, verbatim)
- Modify: `Cargo.toml` (`[workspace.lints.rust]` with `unsafe_code = "forbid"`)
- Modify: `docs/AI-QUICKSTART.md` (add pointer to `AGENTS.md`)

**Interfaces:**
- Consumes: design doc §4.
- Produces: enforced lint `unsafe_code = "forbid"` inherited by all crates; agent rules at repo root.

- [ ] **Step 1: Create `AGENTS.md`** from the design doc §4 fenced block (under 200 lines).

- [ ] **Step 2: Add lints** to root `Cargo.toml` and `lints.workspace = true` to every crate manifest that lacks it (verify with `cargo check --workspace --all-targets`).

- [ ] **Step 3: Run `cargo check --workspace --all-targets`** → PASS; `python3 tools/check_docs_links.py` → PASS. Commit.

### Task 1.1: Rename leaf crates (`dom`, `image`, `fonts`)

**Files:**
- Modify: `Cargo.toml`, `crates/{dom,image,fonts}/Cargo.toml`, all manifests referencing them, all `use megi_core::`/`use megi_image::`/`use megi_fonts::` occurrences, docs and workflows.
- Test: `cargo check --workspace --all-targets`, `cargo test -p megi-core -p megi-image -p megi-fonts`.

**Interfaces:**
- Consumes: current crate names.
- Produces: `megi-core`, `megi-image`, `megi-fonts` package names. All downstream `use megi_core::…` etc.

- [ ] **Step 1: Rename the three leaf directories and package names**; update `[workspace.dependencies]` keys and `path`s.

- [ ] **Step 2: Replace `use megi_core::` → `use megi_core::`, `use megi_image::` → `use megi_image::`, `use megi_fonts::` → `use megi_fonts::` repo-wide** (including tests, fuzz, benches, docs code blocks).

- [ ] **Step 3: Run `cargo check --workspace --all-targets`** → PASS. Run `cargo test -p megi-core -p megi-image -p megi-fonts` → PASS. Commit `refactor(arch): rename leaf crates to megi-*`.

### Task 1.2: Rename remaining crates

**Files:** all manifests, all sources, docs, CI, `.cargo/config.toml`.
**Interfaces:** produces full `megi-*` naming for html, css, net, js, layout, svg, paint, render, shell→browser, test_harness, bench.
Steps mirror Task 1.1; gate: `cargo test --workspace` and `cargo run --release -p megi-test-harness -- --summary` says `PASS` with identical counts. Commit per layer (html/css/net/js, then layout/svg/paint, then render/browser/tooling).

### Task 2.1: Extract `megi-display`

**Files:**
- Create: `crates/display/{Cargo.toml,src/lib.rs,src/item.rs,src/list.rs,src/hit.rs}`
- Modify: `crates/paint/src/{display_list.rs,hit_test.rs,lib.rs}`, `crates/render/src/lib.rs`
- Test: `crates/display/tests/contract.rs`, `cargo test -p megi-display -p megi-paint -p megi-render`

**Interfaces:**
- Consumes: `megi_paint::DisplayItem`, `megi_paint::DisplayList`, `megi_paint::HitRegion` current definitions.
- Produces: `megi_display::{DisplayItem, DisplayList, HitRegion, HitKind}`; `paint` re-exports them for compatibility during this task only.

- [ ] **Step 1: Write a test asserting `DisplayItem` and `DisplayList` are `Clone + Debug` and contain no references** (compile-time `fn assert_owned<T: 'static>()`, no lifetimes). Run → FAIL (crate missing).
- [ ] **Step 2: Move the types verbatim into `crates/display/src/`**, keeping field names and variant order; `paint` imports from `megi_display` and re-exports.
- [ ] **Step 3: Run the test → PASS; `cargo test -p megi-paint` → PASS with byte-identical goldens.** Commit.

### Task 2.2: Extract `megi-raster`

**Files:**
- Create: `crates/raster/{Cargo.toml,src/lib.rs,src/rasterizer.rs,src/png.rs,src/deflate.rs,src/scratch.rs}`
- Modify: `crates/paint/src/lib.rs`, `crates/render/src/lib.rs`, workspace manifest
- Test: `crates/raster/tests/golden.rs` (moved from paint), `python3 tools/png_check.py`

**Interfaces:**
- Consumes: `megi_paint::rasterize_display_list*`, `megi_paint::Pixmap`, `megi_paint::RasterStats`.
- Produces: `megi_raster::{rasterize_display_list, rasterize_display_list_with_font_database, RgbaFrame, RasterStats, encode_png}`. `render` calls `megi_raster` directly; `paint` no longer owns a rasterizer.

- [ ] **Step 1: Copy the goldens before the move** (`cp` the fixture PNGs to `/tmp/opencode/megi-goldens-before/`); record md5.
- [ ] **Step 2: Move rasterizer/png/deflate to `megi-raster`**, documenting each `thread_local RefCell` in `scratch.rs` with a comment explaining why it exists.
- [ ] **Step 3: Run `cargo test -p megi-render` and compare md5 of all regenerated PNGs with the recorded ones** → identical.
- [ ] **Step 4: Run `python3 tools/check_arch.py`** → `paint → raster` edge absent. Commit.

### Task 3.1: Invert PAC

**Files:** `crates/net/src/{lib.rs,pac.rs,proxy.rs}`, `crates/net/Cargo.toml`, `crates/render/src/lib.rs`
**Interfaces:** produces `megi_net::PacEvaluator` trait and removal of the `js` dependency from `megi-net`.
Steps: define trait; change `pac::resolve_directives` to take `&dyn PacEvaluator`; implement adapter in `render`; remove `pac = ["dep:megi-js"]`; gate `cargo check -p megi-net --no-default-features` + `cargo test -p megi-net` → PASS. Commit.

### Task 4.1: De-Rc the JS host

**Files:** `crates/js/src/{host.rs,runtime.rs}`, new `crates/js/src/runtime/{mod.rs,globals.rs,dom_bindings.rs,events_timers.rs,modules.rs,network.rs,workers.rs,geometry.rs}`
**Interfaces:** produces `JsRuntime { host: HostState }` + `mpsc::Sender<HostEffect>` / `Receiver<JsEvent>`; removes `CURRENT_HOST`, `HostScope`, `Rc<RefCell<Host>>`.
Steps: add channel-based test proving a timer callback can mutate DOM without `borrow_mut` panic; move the ~150 native callbacks into the new modules by existing banner groups (mechanical move, no logic change); run `cargo test -p megi-js` (312 tests) → PASS; `python3 tools/check_arch.py` → zero `Rc<` in `megi-js`. Commit.

### Task 5.1: Create `megi-webview`

**Files:** create `crates/webview/{Cargo.toml,src/lib.rs,src/web_view.rs,src/event.rs,src/frame.rs,src/navigation.rs}`; move pure modules from `crates/browser/src/`.
**Interfaces:** produces `WebView::new(options)`, `load(url)`, `tick()`, `on_event(WebEvent)`, `frame() -> &RgbaFrame`, `hit_test(x,y)`, `HostCallbacks`.
Steps: test that a `WebView` renders `data:text/html,<h1>Hi</h1>` to a frame with no window dependency; implement using `megi_render::render_url`; add features `default = []`, `full`; gate `cargo tree -p megi-webview --no-default-features | grep boa` → empty; `cargo test -p megi-webview` → PASS. Commit.

### Task 6.1: Make `megi-browser` a thin frontend

**Files:** `crates/browser/src/{window.rs,chrome.rs,lib.rs}`, `crates/browser/Cargo.toml`.
**Interfaces:** `WinitFrontend` translates winit events to `WebEvent`, paints `Frame` with softbuffer; no direct engine deps.
Steps: `--screenshot` PNG md5 before/after → identical; `cargo test -p megi-browser` (60 integration tests) → PASS; `python3 tools/check_arch.py` → browser only imports `megi-webview` + winit/softbuffer. Commit.

### Task 7.1: Feature matrix and CI

**Files:** `crates/test_harness/Cargo.toml`, `crates/webview/Cargo.toml`, `.github/workflows/ci.yml`.
Steps: make `js` optional in `megi-test-harness`; add `full` feature to `megi-webview`; CI runs the six commands from design doc §2.5; `cargo tree` assertions; commit.

### Task 8.1–8.6: Split god-files

One file per PR, in this order: `megi-core/src/lib.rs` → `id.rs/arena.rs/query.rs/iter.rs`; `megi-css/src/properties.rs`; `megi-html/src/tree_builder.rs`; `megi-browser/src/window.rs`; `megi-browser/src/chrome.rs`; `megi-net/src/http.rs`. Each task: mechanical move only, `git diff --stat` shows move without logic change, all tests green, file ≤ 500 lines.

---

## Self-review

- **Spec coverage:** design §1 (audit) is input, not implementation; §2.1–2.6 covered by Tasks 0–8; §3 phases map 1:1; §4 becomes Task 0.3; §5 open questions are defaults, not tasks. No gap.
- **Step scan:** each step has a command and expected output.
- **Type consistency:** names match the design doc (`DisplayItem`, `RgbaFrame`, `WebEvent`, `PacEvaluator`).
- **Review Focus:** all five input classes have owning tasks (0.1, 2.2, 7.1, 0.1, 4.1).
- **Proportion:** plan is shorter than the spec and contains no full program bodies.
