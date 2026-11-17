# Documentation vs code audit (phase F3)

Checkout: upstream `main` (9c94e86) @ `9c94e86` (2026-11-17). Read-only; no `cargo` run.
Scope: every tracked `*.md` except the 8 previously-audited docs
(`README.md`, `SECURITY.md`, `CONTRIBUTING.md`, `ARCHITECTURE.md` — root file does not
exist, `CHANGELOG.md`, `docs/JAVASCRIPT-STRATEGY.md`, `docs/PHASE4-SUPPORT.md`,
`docs/FUZZING.md`). 25 of the 32 tracked markdown files were in scope.

## Per-file results

| File | Lines | Verdict | Evidence |
|---|---|---|---|
| `.github/PULL_REQUEST_TEMPLATE.md` | 27 | ok | checklist matches repo gates (fmt/clippy/test/gate/doc/unsafe) |
| `PROGRESS.md` | 208 | stale | :62 lists "bidi" as pending although `crates/layout/src/bidi.rs` and CSS `unicode-bidi` (`crates/css/src/properties.rs:707-724`, `crates/layout/src/inline.rs:513-535`) are shipped; :126 fetch-streaming/workers pending vs shipped (`crates/js/src/runtime.rs:4971-5004`); :196 "16 libFuzzer targets" vs 25 in `fuzz/fuzz_targets/` |
| `crates/bench/fixtures/README.md` | 34 | ok | generator snippet consistent; `crates/image/tests/fixtures/png/large_800x600.png` = 13,947 B as stated :10 |
| `crates/image/tests/fixtures/README.md` | 52 | ok | listed `generate*.py` and fixtures exist; `basic.png` command matches `crates/paint/examples/html_to_png.rs:17` usage |
| `docs/ARCHITECTURE.md` | 195 | skipped | already audited per assignment (read only) |
| `docs/CONTRIBUTOR-ONBOARDING.md` | 98 | stale | good-first-issue table :18-31 lists #467/#468/#469/#471 as open work; those fuzz targets already exist (`fuzz/fuzz_targets/{layout_resolve,layout_tree,paint_display_list,fonts_shape,net_surface,webp,html_parse}.rs`), #970 (labels/board) shipped via `.github/labeler.yml`, `CODEOWNERS` |
| `docs/GETTING-STARTED.md` | 99 | ok | Rust 1.91 matches `Cargo.toml`; shell flags match `crates/shell/src/cli.rs:32-34`; `--no-default-features` features exist in `crates/js/Cargo.toml`/`crates/render/Cargo.toml` |
| `docs/HANDOFF.md` | 122 | stale | :33 "MSRV 1.88" vs `Cargo.toml:rust-version = "1.91"` (own :73 uses `+1.91`); :41 2300 tests / :59 1929 tests vs `progress.toml` tests=2478; :64,:103 "22 targets, corpus 37" vs 25 targets / 123 corpus files; :116 "Boa 0.21.1" vs `boa_engine = "0.22"`; :120 layout budget ≤43 vs `docs/PERFORMANCE.md:265` ≤45 |
| `docs/MILESTONES.md` | 149 | ok | dated round log; r31/r32 per-crate test counts match `docs/TOOLING.md` r31/r32; 1.88/Boa 0.21 entries are historical round notes |
| `docs/PERFORMANCE.md` | 405 | stale | :35 "mediana de 5 pasadas válidas" contradicts :13 "7 pasadas, mediana por fila" (and `docs/status/bench-baseline.json` method = 7); :40 "load 1,0–1,83" contradicts :15 "8,1–13,0" (stale r32 copy) |
| `docs/RELEASE.md` | 43 | ok | matches `.github/workflows/release.yml` (3 targets, `.sha256`, `cargo cyclonedx` SBOM) |
| `docs/ROADMAP.md` | 709 | wrong | see issue 1: Fase 5-7 status, Boa/MSRV, "sin módulos ES", shell limitations, stale HEAD/test counts, metrics table, Fase A/B tasks already shipped, internal contradiction :21 vs :532 |
| `docs/SHOWCASE.md` | 70 | stale | :69 `cargo run --release -p paint --example html_to_png` without arguments; example requires `<input.html> <output.png>` (`crates/paint/examples/html_to_png.rs:17,45-50`) and exits with usage |
| `docs/TOOLING.md` | 787 | wrong | see issue 2: fuzz §2.8 (4 targets / 17 corpus / 300 s) vs 25/123 and 9×600 s + 17×300 s; bench "8 tests" :152; image "111 tests" :207; shell "100 tests" and "Limitaciones: formularios/pestañas/descargas" :228 vs `forms.rs`/`tabs.rs`/`download.rs`; Boa 0.21.x :259; SVG "No soporta `<text>`, CSS class/style sheets, gradientes/patterns" :347 vs `svg/src/{text,css,gradient,pattern}.rs`; MSRV 1.88 :521 |
| `docs/audit/FINDINGS.md` | 254 | stale | :194-198 area column says `image` for #1152-#1156 although titles are `[layout]` |
| `docs/audit/README.md` | 96 | stale | :37 F5 points at `docs/GAPS.md`, which does not exist |
| `docs/status/COVERAGE.md` | 60 | ok | counting rules match `docs/status/coverage.toml` (70 capabilities, 2 partial) and `tools/update_coverage.py` exists |
| `docs/superpowers/plans/2026-11-17-honest-progress.md` | 111 | stale | 28 unchecked `- [ ]` steps (:42-52) although the artifacts shipped (`docs/status/coverage.toml`, `coverage.json/html/svg`, `badge-coverage.svg`, `tools/update_coverage.py`, `tools/real_site_check.py`) |
| `docs/superpowers/specs/2026-11-17-honest-progress-design.md` | 73 | ok | design; implemented as described (coverage.json headline, `--check` in CI, real-sites report) |
| `tests/fixtures/phase4/README.md` | 88 | stale | :82-84 "PNG writer uses stored DEFLATE blocks (no compression) on purpose" contradicts :20-25 (adaptive row filters + in-tree LZ77 DEFLATE) and the real encoder |
| `tests/fixtures/render/README.md` | 123 | wrong | :88 "modules are not supported yet" contradicts :40-43 module fixtures and module tests (`crates/render/tests/scripting_tests.rs`); ES modules shipped |
| `tests/fixtures/render/showcase/README.md` | 55 | ok | golden 80,220 B `md5 c95c7eff…` matches `docs/images/showcase.png`; bg images 185 B = pattern 93 + bullet 92 |
| `tests/html5lib-tests/README.md` | 3 | ok | upstream pointer only |
| `tests/html5lib-tests/tokenizer/README.md` | 108 | ok | format description matches harness (14 `.test` files, `xmlViolation` variant) |
| `tests/wpt/css/css-syntax/README.md` | 59 | ok | SHA-256 of the 3 fixtures match; 30-result gate matches `progress.toml` gate (`css_syntax 30/30`) |

Totals: 12 ok, 1 skipped, 9 stale, 3 wrong (25 files, 4,028 lines).

## Issues to file (see returned JSON for ready-to-paste bodies)

1. **P1** ROADMAP.md stale on phases 5-7, stack versions, shell limits, HEAD/test counts and the metrics table.
2. **P1** TOOLING.md stale "current state" facts: fuzz targets/corpus/CI time, bench/image/shell test counts, shell/svg limitations, Boa and MSRV.
3. **P2** HANDOFF.md + PERFORMANCE.md stale current-state numbers and methodology (MSRV, Boa, fuzz counts, test counts, 5-vs-7 passes, stale load).
4. **P2** PROGRESS.md pending lists and fuzz-target count contradict shipped code.
5. **P2** tests/fixtures/render/README.md claims ES modules unsupported.
6. **P3** tests/fixtures/phase4/README.md claims the PNG writer uses stored DEFLATE.
7. **P2** docs/SHOWCASE.md ships a broken `html_to_png` command.
8. **P3** docs/audit/README.md references missing `docs/GAPS.md`; FINDINGS.md mislabels layout rows as `image`.
9. **P3** docs/CONTRIBUTOR-ONBOARDING.md good-first-issue list is stale.
10. **P3** docs/superpowers plan checkboxes remain unchecked for completed work.

Cross-doc contradictions found: ROADMAP:21 vs ROADMAP:532 (Fase 5 done vs WP-5.9 partial);
HANDOFF:120 (layout ≤43) vs PERFORMANCE:265 (≤45); PERFORMANCE:13 (7 passes) vs PERFORMANCE:35
(5 passes); HANDOFF:73 (`cargo +1.91`) vs HANDOFF:33 ("MSRV 1.88").
