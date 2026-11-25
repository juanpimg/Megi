# Zen-inspired UI evolution: end-to-end implementation plan

Status: **proposed** — ready to execute in the isolated worktree `docs/ui-zen-plan`.
Owner: repository agent. Base: `origin/main` at `065ae4ca`.

## 1. Goal and non-goals

### Goal

Make Megi's shell feel calm, focused and immediately legible without weakening the
browser engine. The user-facing result is a **Zen-inspired workspace rail**:
a slim vertical tab rail that can collapse to a hover-peek strip, plus a compact
mode and a small set of design tokens that keep every future shell surface
coherent. The change is delivered as a sequence of reviewable, independently
releasable slices.

### Non-goals

- Rebuilding Megi on Firefox/Chromium code, or copying Zen Browser's XUL patches.
- Split View, Glance, Workspaces backed by containers, Mods or Sync: those need
  multiple live documents / webviews or remote services. Megi currently has a single
  rendered page per window (`crates/browser/src/window.rs`, `crates/browser/src/ipc.rs`)
  and `MAX_TABS = 32` (`crates/browser/src/tabs.rs:28`).
- Changing HTML/CSS parsing, layout, paint, WPT results or the conformance gate.
  The shell chrome is a Rust overlay painted after the page frame
  (`crates/browser/src/chrome.rs:1-29`); it is not a DOM document.
- Reopening closed capability F7 as "unfinished". `docs/status/coverage.toml:505-550`
  records window, navigation, chrome overlay, save-as, a11y and isolation as done.
  This plan adds **new** capabilities on top of that baseline.

## 2. What is borrowed from Zen Browser, and how

Sources consulted: [Zen Browser](https://zen-browser.app/),
[Workspaces](https://docs.zen-browser.app/user-manual/workspaces),
[Compact Mode](https://docs.zen-browser.app/user-manual/compact-mode),
[URL Bar](https://docs.zen-browser.app/user-manual/urlbar),
[Keyboard shortcuts](https://docs.zen-browser.app/user-manual/shortcuts),
[Zen source overview](https://deepwiki.com/zen-browser/desktop).

| Zen pattern | Megi translation | Phase |
|---|---|---|
| Vertical tab rail with two widths and hover peek | Optional left rail painted as a pure chrome overlay; collapsed width `12 px`, expanded `168 px`; hover/peek expands to `220 px` without moving the page origin permanently | 1–2 |
| "Hide before shrink" compact mode | Toggle that hides the tab strip labels and reduces the top band from `58 px` to `40 px` while keeping the address pill and all hit targets >= `22 px` | 2 |
| Lightweight workspaces / contexts | Local named groups of tab IDs stored in the shell session; no containers, no network, no new process model | 3 |
| Floating URL bar / "new tab as search" | Keep the existing address bar semantics (`resolve_address` at `crates/browser/src/chrome/address_bar.rs:322-379`); only restyle it. Do not turn it into a real search engine | 4 |
| Design-token-driven, low-chrome aesthetic | New `chrome/tokens.rs` holding palette, radii and spacing; existing constants remain re-exports so current goldens stay byte-identical | 0 |
| Motion respects `prefers-reduced-motion` | Shell animation flag sourced once from the existing media-query plumbing; only `opacity`/`transform`-style interpolation, never layout reflow per frame | 4 |
| Every edge gesture has a keyboard equivalent | Central shortcut table with conflict detection; rail toggle, compact toggle, workspace cycle all reachable from the keyboard | 0–3 |

Explicitly rejected Zen features: Split View, Glance, Live Folders, Mods and Sync
(reasons above).

## 3. Current architecture facts that constrain the design

- All chrome lives in `crates/browser`; `crates/webview/src/lib.rs:1-5` states the
  WebView knows nothing about windows, event loops or chrome. `browser` depends on
  `webview`, never on `paint`, `layout`, `html` or `css` directly
  (`crates/browser/Cargo.toml`, `docs/ARCHITECTURE.md` DAG).
- The top chrome is a `58 px` CSS band painted **after** the page:
  `CHROME_HEIGHT = TAB_STRIP_HEIGHT + ADDRESS_BAR_HEIGHT`
  (`crates/browser/src/chrome.rs:82-86`), `present::chrome_band_pixels`
  (`crates/browser/src/present.rs:58-61`), `present_region_at`
  (`crates/browser/src/present.rs:89-147`). Input hit-testing uses
  `chrome::in_chrome` (`crates/browser/src/chrome.rs:179-181`) and
  `ChromeView` (`crates/browser/src/chrome.rs:190-233`).
- `WindowApp::present` builds the frame and calls `draw_chrome`
  (`crates/browser/src/window/presentation.rs:86-313`, chrome call at `:260`).
  `click_at_cursor` and `chrome_click` own pointer dispatch
  (`crates/browser/src/window/input.rs:238-390`).
- Headless screenshots use a separate fast path `draw_chrome_over_pixmap`
  (`crates/browser/src/chrome.rs:305-362`, `crates/browser/src/headless.rs:121-240`).
  Default headless chrome must remain byte-identical.
- Generated pages in `crates/browser/src/pages/` are real WHATWG documents;
  changing them can move WPT/goldens. The rail is **not** one of them.
- Past visual decisions are binding: PR #1556 (single accent, coherent dark
  surfaces), #1558 (rounded tabs, 2 px accent line, pill address bar) and #1559
  (`ADDRESS_TEXT_SIZE = 13`). Any deviation must be called out with old/new md5.
- A11y follows ADR-002 (`docs/ACCESSIBILITY-STRATEGY.md:1-33`): AccessKit only in
  `crates/browser`, behind feature `a11y`, never in `render` or headless.
- Isolation follows ADR-004 (`docs/ISOLATION-STRATEGY.md:1-8`); tab identity is
  `SiteKey`-based and IPC rendering is opt-in. Workspaces must reuse tab IDs.
- Hard limits: no `unsafe`, no new `Rc`/`Weak`, no new lifetimes crossing crates,
  new `.rs` file <= 400 lines (500 hard), no generated-artifact edits, TDD with
  observable tests, `deslop-please` clean before "green".

## 4. Design decisions

1. **The rail is an overlay, not a DOM sidebar.** It reuses the proven top-band
   technique: the page keeps its own CSS viewport, but the shell reserves horizontal
   space and offsets the presented image. No layout-engine change is needed.
2. **Off by default.** Rail and compact mode are opt-in per window, persisted in
   the shell session. Headless rendering keeps rail off; `--chrome-rail` is the only
   way to produce a rail screenshot, and that screenshot gets a new golden with an
   explicit old/new md5 record in the commit.
3. **One accent, one token source.** New surfaces read `chrome/tokens.rs`; the old
   `pub const` palette stays as re-exports so existing pixels do not change.
4. **Narrow interfaces.** Rail layout, hit testing and painting are pure functions
   on owned/simple data, modelled on `chrome/tab_strip.rs:61-133,286-383`. Window
   state only selects which pure function to call.
5. **Accessibility parity.** The rail exposes `tablist`/`tab` roles and names from
   the same tab titles used by the horizontal strip; collapsed mode keeps the same
   accessible tree. Keyboard activation is mandatory, not optional.
6. **No new dependencies.** winit/softbuffer, fonts and AccessKit already exist;
   nothing new is added to `Cargo.toml`.

## 5. Phases

### Phase 0 — Guardrails and tokens (foundation, no visible change)

**Outcome:** a central token module and a shortcut registry; zero pixel changes.

Files:
- add `crates/browser/src/chrome/tokens.rs` (<= 160 lines): `Palette`, `Radius`,
  `Spacing`, `HEIGHT_COMPACT`; `impl Default` uses today's constants.
- edit `crates/browser/src/chrome.rs`: re-export existing constants from tokens,
  add `pub const RAIL_COLLAPSED_WIDTH`, `RAIL_EXPANDED_WIDTH`, `RAIL_PEEK_WIDTH`,
  `COMPACT_CHROME_HEIGHT`.
- add `crates/browser/src/window/shortcuts.rs` (<= 200 lines): `ShellAction` enum
  (ToggleRail, ToggleCompact, CycleWorkspace), `resolve_shortcut(mods, key)`,
  and `check_conflicts()` used by a test.
- edit `crates/browser/src/window/events.rs:175-257`: route shortcuts through the
  registry instead of ad-hoc matches, preserving every existing binding.

Tests: token parity with old constants; every existing shortcut still resolves to
the same action; new shortcuts report no conflict; no change to chrome paint tests.

Acceptance: `cargo test -p megi-browser chrome::`, `cargo test -p megi-browser
window::tests`, `cargo test -p megi-browser --test shell_tests tabs` pass; no golden
needs updating.

### Phase 1 — Vertical rail v1 (core slice)

**Outcome:** `Ctrl+B` toggles a left rail showing the existing tabs; default off.

Files:
- add `crates/browser/src/chrome/rail.rs` (<= 380 lines):
  - `pub struct RailView<'a> { tabs: &'a [String], active: usize, hover: bool, expanded: bool, ... }`
    (borrows stay inside `browser`; no cross-crate lifetime).
  - `pub struct RailLayout { rect, tabs: Vec<RailTabRect>, new_tab, toggle }`
  - `layout_rail(width, height, scale, view) -> RailLayout`
  - `rail_hit(layout, x, y) -> Option<RailHit>` (`Activate`, `Close`, `NewTab`, `Toggle`)
  - `paint_rail(pixmap, scale, view, layout)` using `chrome/drawing.rs` primitives.
- edit `crates/browser/src/chrome.rs`: add `rail: Option<RailView<'a>>` to
  `ChromeView` (`:190-233`), call `paint_rail` in `draw_chrome` (`:238-294`) and in
  the pixmap path (`:305-362`) through a new `draw_rail_over_pixmap`.
- edit `crates/browser/src/present.rs`: add
  `chrome_rail_pixels(scale, dest_width) -> u32` and
  `present_region_with_insets(frame, scroll_y, css_viewport_height, css_viewport_width, dest_x_offset, dest_y_offset, dest, dest_width, dest_height) -> bool`.
  `present_region_at` delegates with zero insets and the frame width, keeping all
  current callers and pixels unchanged.
- edit `crates/browser/src/window/desktop.rs:214-335` and
  `crates/browser/src/window/frontend.rs:5-133`: `rail_open: bool`,
  `rail_expanded: bool` state; initialize both to false.
- edit `crates/browser/src/window/presentation.rs:103,191-200,225-260`: compute rail
  inset in physical pixels, pass reduced `css_viewport_width` to the new present
  function, and populate `ChromeView.rail`.
- edit `crates/browser/src/window/input.rs:238-321,324-390`: classify rail clicks
  before page clicks, subtract the rail offset in `page_coordinates`, and handle
  `RailHit` (activate, close, new tab, toggle).
- edit `crates/browser/src/window/keyboard.rs` and `window/shortcuts.rs`: bind
  `Ctrl+B` through the registry.

Tests (TDD order, each red first):
1. `rail_layout_collapses_and_expands`: geometry and clamped widths.
2. `rail_hit_distinguishes_tabs_close_new_and_toggle`.
3. `rail_paint_is_deterministic_and_does_not_touch_page_rows`: pixmap equality
   with two consecutive runs; only rail columns differ from the no-rail frame.
4. `present_region_with_insets_shifts_columns_and_keeps_zoom_mapping`.
5. `chrome_band_pixels_and_rail_pixels_clamp_to_one_remaining_row`.
6. `shell_tests`: clicking rail activates a tab; clicking the page beside the rail
   uses shifted coordinates; `Ctrl+B` toggles rail.
7. `headless_chrome_default_is_byte_identical_without_rail` (existing test).
8. A11y: rail tabs appear as `tablist`/`tab` with the same names as the strip.

Acceptance: all above green; `update_progress.py --check` unchanged; new files
registered in the ledger; no new golden unless `--chrome-rail` is added later.

Commit sequence (one logical change each, DCO):
`feat(chrome): add pure rail layout and paint functions` →
`feat(browser): reserve horizontal chrome inset for the page` →
`feat(browser): wire vertical tab rail toggle`.

### Phase 2 — Compact mode and peek (comfort)

**Outcome:** `Ctrl+Shift+B` toggles compact mode (top band `58 → 40 px`); hovering
the collapsed rail edge peeks the labels without changing the page viewport.

Files: `chrome/tokens.rs`, `chrome.rs:82-86,238-294`, `headless.rs:121-240`,
`window/desktop.rs`, `window/presentation.rs`, `window/input.rs`,
`window/events.rs` (hover deadline already exists at `:432-482`).

Tests: compact hit targets remain >= `22 px`; peek never changes presented page
pixels; reduced-motion flag suppresses animation; headless default unchanged;
explicit `--chrome-compact` golden with old/new md5 recorded in the commit.

Acceptance: `cargo test -p megi-browser --test shell_tests` and `tools/png_check.py`
green; new golden declared explicitly.

### Phase 3 — Local workspaces (Zen-inspired, no services)

**Outcome:** name and switch groups of tab IDs locally; `Ctrl+1..3` cycles.

Files: `crates/browser/src/tabs.rs` (add `workspace: u8` to `Tab`, <= 320 lines
stays), `crates/browser/src/window/session.rs` or `window/ui.rs` for persistence,
`chrome/rail.rs` (group separators), `i18n.rs:48-179` (labels in En/Es with parity
test), `shortcuts.rs`.

Tests: switching workspace preserves tabs and scroll per tab; persisted session
round-trip; no network/process involvement; a11y names include workspace label.

Acceptance: session tests green; `M5` a11y evidence still valid via
`tools/check_milestone.py M5 3`; ledger clean.

### Phase 4 — Motion, focus polish and settings page

**Outcome:** 100–200 ms opacity/translation transitions gated by reduced-motion;
a `megi://settings` page built as a real generated HTML document
(`crates/browser/src/pages/`) for toggles, avoiding a second chrome UI stack.

Files: `chrome/tokens.rs`, `pages/settings.rs` (new, <= 400 lines), `pages/mod.rs`,
`i18n.rs`, `window/ui.rs`.

Tests: settings page markup passes the existing HTML/WPT-derived page tests;
disabling reduced-motion changes only chrome pixels; `focus-visible` parity.

Acceptance: WPT gate and page goldens unchanged unless explicitly declared.

### Phase 5 — Hardening, docs and release

- Run the full AGENTS.md §6 gate plus `deslop-please --allow .deslop-please.allow`.
- Regenerate `tools/audit_coverage.py --generate`, mark every new file covered
  after reading it; ledger must report 0 pending.
- Update `docs/ARCHITECTURE.md` (shell section only), `docs/AGENT-INDEX.md` and this
  plan status from `proposed` to `implemented`.
- Open/track issues for deferred Zen features (Split View, Glance, Mods) as
  follow-ups, not as silent omissions.
- Merge to `main` only after every phase acceptance is green.

## 6. Verification matrix

Every phase repeats this matrix, scoped as early as possible:

```
cargo fmt --all -- --check
cargo clippy -p megi-browser --all-targets --all-features -- -D warnings
cargo check -p megi-browser --all-targets
cargo test -p megi-browser chrome::
cargo test -p megi-browser window::tests
cargo test -p megi-browser --test shell_tests tabs
cargo test -p megi-webview --no-default-features
cargo run --release -p megi-test-harness -- --summary         # PASS
python3 tools/check_arch.py
python3 tools/check_docs_links.py
python3 tools/update_progress.py --check
python3 tools/update_coverage.py --check
python3 tools/audit_coverage.py --check --strict
deslop-please --allow .deslop-please.allow
```

Full workspace gate (AGENTS.md §6) is mandatory before merge. Disk budget: use the
installed huacnlee/cargo fork (GC keeps `~/.cargo/targets` bounded); do not run
parallel full-workspace builds; never delete a cache used by another cargo process.

## 7. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Rail changes page coordinates and breaks link/form clicks | New insets function with zero-inset delegation; dedicated shell test for shifted coordinates |
| Golden PNG churn | Rail and compact off by default in headless; new goldens only with explicit flags and md5 in commit message |
| A11y regressions | ADR-002 boundary respected; rail uses existing AccessKit bridge; parity test against tab titles |
| File-size violations | `rail.rs` <= 380 lines, `tokens.rs` <= 160, `shortcuts.rs` <= 200; `check_arch.py` in every phase |
| Hidden shortcut conflicts | Central registry plus `check_conflicts()` test |
| Disk exhaustion during verification | Fork GC, single build at a time, `df` check before and after; no destructive cache cleanup while cargo runs |
| Scope creep into Zen's multi-document features | Non-goals list above; deferred items recorded as issues in phase 5 |

## 8. Definition of done

- Phases 0–4 each landed as atomic Conventional/DCO commits with green acceptance.
- Phase 5 gate output pasted in the final report, including limitations by name.
- `docs/superpowers/plans/2026-11-25-zen-inspired-ui-plan.md` marked implemented.
- The user can toggle rail and compact mode from the keyboard and from the rail,
  sees no visual change by default, and can audit every pixel change through a
  declared golden.
