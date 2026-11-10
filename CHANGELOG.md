# Changelog

All notable changes to Megi are documented here. The project is pre-1.0 and
does not yet follow strict semantic versioning; the format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Round 9 — JavaScript executed in render, paint hit-testing and navigable shell (2026-11-10)

#### Added

- **Phase 6, WP-6.2 host — JavaScript in the render pipeline**: `render::ScriptingMode
  { Enabled, Disabled }` (default Enabled with the `js` feature, Disabled with
  `--no-default-features`), `RenderOptions::{scripting, scripting_limits}`,
  `ScriptingLimits`/`ScriptBudget::SMALL` and the `--no-scripting` CLI flag.
  Classic `<script>` elements are discovered in document order (inline/`src`;
  ES modules warned and skipped, `nomodule` ignored), external scripts are fetched
  through the `ResourceLoader` (`--no-network`, dedupe, caps **8 MiB/script,
  32 MiB total, 64 scripts**), executed with `js::JsRuntime::with_document` under a
  **per-script and cumulative budget**, with drained microtasks and up to 10,000
  timers, captured `console` (`scripts_executed/failed/bytes/console`,
  `timings.scripting`) and a **styles→layout→paint re-run over the mutated DOM**
  (mutations visible in the PNG). 7 `js_*` fixtures and 12 tests → **67** render
  tests. Limitation: execution is **post-parse only** (no `document.write`
  re-tokenization ⇒ WP-6.4 for `scripted_*`), no real `async`/`defer`, modules,
  events or wall-clock event loop.
- **Paint hit-testing (additive)**: `hit_test` module with `HitKind`,
  `HitRegion { rect, node_id, kind }` in paint order with clip applied,
  `hit_test`/`hit_test_all` (topmost wins), `HitLink`/`find_link_at` (walks up to
  the HTML `<a href>`, raw href) and the additive builders
  `build_display_list_with_hit_regions`/`build_display_list_and_hit_regions`.
  Classic entry points stay **byte-identical** and the cost is opt-in. 11 new
  tests → **108** paint. Limits: box/run granularity, no `pointer-events`/
  transforms, page CSS-px coordinates (the host applies scroll/scale).
- **Navigable shell (Phase 7, WP-7.5)**: left click on a link resolves it with
  `Url::join` and renders it in the worker (generation-checked) with history;
  pure `NavigationState` (back/forward/replace/reload, cap 256, forward
  truncation, same-URL no-op, failures keep the entry). Shortcuts:
  Alt+Left/Backspace/`b` back, Alt+Right/`n` forward, `r` reload, `q`/Esc quit,
  full scroll and HiDPI. Dynamic title `<title>` + short URL + scroll % +
  `[recortado]`, error titles `Megi — error: …`; hit regions and title come from
  the **same render as the pixels**; HTTP redirects replace the history entry.
  34 new tests → **80** shell. Follow-up: adopt `paint::find_link_at` once
  `render::RenderOutput` exposes `hit_regions` (one-function change; PNG/golden
  unchanged).
- **Docs**: round-9 synchronization of README/ROADMAP/ARCHITECTURE/HANDOFF/
  PHASE4-SUPPORT/TOOLING/SECURITY/CONTRIBUTING/CHANGELOG/JAVASCRIPT-STRATEGY.

#### Changed

- Test suite: **793 tests, 0 failures** (was 734): css 57, dom 28, fonts 13+1
  doctest, html 53, image 45+1, js 12, layout 133, net 180, **paint 108**,
  **render 67** (56 without the `js` feature), **shell 80**, bench 8,
  test_harness 7. fmt, clippy `-D warnings`, `cargo doc`, 4-suite gate PASS and
  MSRV 1.88 verified.
- Phase status: Phase 4 functionally complete, Phase 5 ≈90 %, **Phase 6 ≈30 %**
  (WP-6.1 + WP-6.2 host done), **Phase 7 ≈60 %** (navigation/history/title done).
- Golden images unchanged by round 9: showcase 80,220 B `md5 c95c7eff…`, render
  basic 15,498 B `md5 7324e851…`, Phase-4 `basic.rgba` byte-identical
  (`md5 60f8984e…`). Evidence for scripting: `js_text.html` 15,876 B
  `md5 e1da51d432804fd540792d40398b72eb` with scripts vs 15,499 B
  `md5 53f67e77a335a8b024e0896497e63999` with `--no-scripting`.

### Round 8 — JavaScript (Boa), cookies/CORS/gzip, CSS backgrounds E2E and table-text fix (2026-11-10)

#### Added

- **Phase 6, WP-6.1 — `crates/js`**: embedded **Boa 0.21.1** runtime behind the
  `js` feature (**enabled by default**; `--no-default-features` builds a std-only
  stub). Persistent realm `eval`, captured `console.*`, full DOM bindings
  (`document.write`, `getElementById`, `querySelector(All)`, `innerHTML`,
  `outerHTML`, `style`, `classList`, tree/attributes/mutations) over a
  generation-checked `NodeId ↔ handle` side table, and a base scheduler
  (`setTimeout`/microtasks). Anti-DoS `Budget` → Boa `RuntimeLimits` (5,000,000
  loop iterations, recursion 512, VM stack 20,480; `Budget::SMALL` for
  pages/tests), handle cap 1,000,000, no network/filesystem APIs. 12 tests.
  Not yet wired into `render` (Phase 6 ≈10 %).
- **`crates/net` — cookies, CORS and compression**: RFC 6265 `CookieJar`
  (capped at 4,096, `FetchOptions.cookie_jar`), CORS subset (`Origin`,
  ACAO/credentials validation, preflight) and `gzip`/`deflate` decoding
  (`Accept-Encoding: gzip` negotiated by default, CRC32/ISIZE, decompression
  caps 64 MiB and 1000:1 ratio). 180 tests.
- **Paint backgrounds**: `BackgroundImages` +
  `build_display_list_with_images_and_backgrounds` — `background-image` stretched
  over the padding box (stand-in for `background-size: auto`) and
  `list-style-image` used as the list marker. 97 paint tests.
- **Render backgrounds end to end**: CSS background-`url(...)` fetch → image
  decode → paint, with caps **32 background images / 8 MiB each / 16 MiB per
  page**, stats and non-fatal warnings; fixture `backgrounds.html` + checker/
  bullet PNG assets. 53 render tests.
- **Layout**: table content-tree model documented and pinned with 4 tests
  (table > row-group/row > `TableCell` > `LineBox` > `TextNode` with content-box
  geometry). 133 tests.

#### Changed

- Showcase golden regenerated after the table-text fix and CSS backgrounds:
  **80,220 B, `md5 c95c7eff83e7bd08f815631aa42afdb0`** (previous golden was 65,151 B); cell text/bold headers, checker background and PNG list
  markers are now visible. The Phase-4 `basic.rgba` golden is **unchanged** and
  the render basic fixture stays 15,498 B `md5 7324e851…`.
- Phase status after round 8: Phase 4 ~100 % functional (remaining:
  `background-repeat/size/position`, `border-collapse` colors, block-in-inline),
  Phase 5 ≈90 %, Phase 6 ≈10 %, Phase 7 ≈40 %.
- README/ROADMAP/ARCHITECTURE/HANDOFF/PHASE4-SUPPORT/TOOLING/SECURITY/
  CONTRIBUTING/CHANGELOG synchronized with round 8 (734 tests).

#### Fixed

- **Table cell text was never painted**: `paint_inline_content` did not descend
  into table boxes (`Table`/`TableRowGroup`/`TableRow`/`TableCell`), so
  backgrounds/borders rendered but cell `LineBox`/`TextNode` content was skipped.
  Table internals now continue the inline traversal; the showcase table shows its
  text and bold headers.

#### Performance

- Test suite: **734 tests** (was 602), all green; fmt, clippy `-D warnings`,
  `cargo doc`, 4-suite conformance gate (tokenizer 7,050/7,050, tree 3,898/3,904,
  serializer 230/230, CSS Syntax 30/30) and MSRV 1.88 verified.
- `layout/realistic-1mb` stays at ≈59 ms (advanced-tables fast path unaffected).

### Round 7 — query APIs, shell usable, advanced tables, @import/showcase and JS ADR accepted (2026-11-10)

#### Added

- **CSS query APIs**: `css::query::{query_selector, query_selector_all, matches}`
  plus fallible `try_*` variants (`SelectorParseError`), document-order traversal
  and `:scope` documented as unsupported; `ComputedStyle` gains
  `background_image` / `list_style_image` parsing (`url(...)`, `none`, color+image
  shorthand; gradients → `None`; painting tracked as follow-up). 57 CSS tests.
- **Render showcase**: `tests/fixtures/render/showcase/` — realistic JS-free page
  with external CSS, recursive `@import`, media queries, two decoded PNG assets,
  a table and a positioned stage. Deterministic 1024×768 golden (regenerated in
  Round 8 after the table-text fix and CSS backgrounds: **80,220 B**,
  `md5 c95c7eff…`; file:// and loopback HTTP), capture documented in
  `docs/images/showcase.png`.
- **`@import` recursive + real media evaluation** in `crates/render`: relative
  resolution against the importing sheet, dedup/cycle/depth-8/caps, screen/print
  media applied per `<link>`, `<style>` and `@import`, per-stage import/media stats.
- **Usable shell (Phase 7)**: vertical scroll (wheel/touchpad, arrows, PageUp/Down,
  Home/End; clamp and 8192 CSS px cap with `[recortado]` marker), mouse/keyboard
  input (click prints URL, r re-render, q/Esc/close quit), HiDPI (CSS viewport =
  physical/scale, nearest-neighbour presentation, re-render on scale change) and an
  off-thread render worker with generation checks, cancellation, coalescing and a
  120 ms debounce via `EventLoopProxy`. 46 shell tests.
- **Advanced tables (Phase 4)**: `rowspan` (occupancy matrix, bottom-up row-height
  distribution), `table-layout: fixed`, atomic `inline-table` (first-row baseline)
  and `border-collapse` conflict resolution (`hidden > width > style > origin`;
  unresolved colors; winning border on both cells) plus a fast path for
  separate/auto/no-rowspan (`layout/realistic-1mb` back to ~59 ms). 129 tests.
- **ADR-001 JavaScript (Phase 6) — ACCEPTED by the owner on 2026-11-10**:
  `docs/JAVASCRIPT-STRATEGY.md` records the decisions — **Boa 0.21.x** engine,
  feature `js` **enabled by default** (CI keeps a `--no-default-features` minimal
  build), full initial scope (`document.write` + DOM bindings + events + timers +
  CSSOM + `querySelector`, WP-6.1–6.7, ≈25–30 person-days). Phase 6 entered
  implementation (WP-6.1); `crates/js` lands in Round 8 (see below).
- `docs/images/showcase.png`: compressed capture of the showcase golden.

#### Changed

- README/ROADMAP/ARCHITECTURE/HANDOFF/PHASE4-SUPPORT/TOOLING/SECURITY/
  CONTRIBUTING/CHANGELOG synchronized with round 7 (602 tests, Phase 4 ≈100 %
  functional, Phase 5 ≈80 %, Phase 7 ≈40 %) and with the accepted JS ADR
  (`ARCHITECTURE.md` §3.1 now lists Boa 0.21.x and the default-on feature).

#### Fixed

- Partial-coverage underflow in `Pixmap::fill_rect` for sub-pixel/fractional
  rectangles: coverage is now exact (4 regression tests); the paint golden stays
  byte-identical. Paint bench slightly better (0.88 → 0.83 ms).
- Shell resize/scale changes now debounce and coalesce through the worker instead
  of re-rendering synchronously on the event loop.

#### Performance

- Layout realistic 1 MB back to ≈59 ms with the advanced-tables fast path
  (separate + auto + no rowspan); paint raster unchanged.
- Test suite: **602 tests** (was 546), all green; fmt, clippy `-D warnings`,
  `cargo doc`, 4-suite conformance gate and MSRV 1.88 verified.


### Round 6 — images, @import/media, browser DOM, shell and layout −43 % (2026-11-10)

#### Added

- **`<img>` end to end**: `paint` gains `ImageStore = HashMap<NodeId,
  Arc<image::RgbaImage>>`, `DisplayItem::Image` and
  `build_display_list_with_images`, with bilinear clamp-to-edge sampling,
  antialiasing/alpha/clip and a gray placeholder. `render` loads `<img src>`
  through `net` (honoring `--no-network`), decodes PNG via `crates/image`
  (other formats → warning + non-fatal placeholder), injects intrinsic sizes
  and paints, with caps (64 images / 8 MiB each / 32 MiB total) and stats.
  Fixture `tests/fixtures/render/images.html` → 800×600, 15,625 B,
  `md5 1de31699…`; the Phase-4 golden stays byte-identical.
- **CSS `@import` + full media queries**: `Stylesheet.imports:
  Vec<ImportRule{url, media}>`, `parse_media_query_list` and `media::evaluate`
  (`and`/`or`/`not`/`only`, query lists, `all`/`screen`/`print`,
  width/height/min/max in px/em/rem/pt/pc/in/cm/mm, `orientation`), plus
  `StyleCollection::add_import_stylesheet` with media gating. (`render` does
  not consume `@import` yet.)
- **Browser DOM API**: `document_element/head/body`, `get_element_by_id`,
  `get_elements_by_tag_name/class_name`, node factories, attribute/child
  manipulation, `text_content`, `clone_node_into` and `DomError`; HTML gains
  `parse_fragment_into`, `TreeBuilder::with_document`, `set_inner_html`,
  `set_outer_html` and `document_write` (table/select/template/rawtext
  contexts, no slot leaks).
- **`crates/shell` (Phase 7 start)**: `winit` 0.30.13 + `softbuffer` 0.4.8
  window (X11/Wayland `dlopen`, `0RGB` presentation, resize debounce, q/Esc/r
  keys, error frame) and a headless `--screenshot` mode without `EventLoop`;
  `megi` CLI and `tools/megi.sh`; 20 tests. Screenshot 640×400
  `md5 7324e851…`, identical to the render pipeline.
- `docs/images/images-e2e.png`: capture of the image pipeline fixture
  (15,625 B) added to the documentation.

#### Changed

- Layout cost cut **43 %** on `layout/realistic-1mb` (100.6 → 57.2 ms) via
  `Arc<ComputedStyle>` in IFC tokens, streaming line breaking, single-pass
  `format_line`, no overflow post-pass when unneeded and no per-element
  lowercase; `LayoutBox` now has an **iterative `Drop`** (50,000 levels with a
  256 KiB stack; construction recursion remains documented).
- README/ROADMAP/ARCHITECTURE/HANDOFF/PHASE4-SUPPORT/TOOLING/SECURITY/
  CONTRIBUTING/CHANGELOG synchronized with round 6 (546 tests, Phase 4 closed,
  Phase 5 ≈70 %, Phase 7 ≈20 %); the candidate "table optimization" work item
  is removed/marked done because the measured cost was in the IFC.
- Performance baseline refreshed after round 5 (`docs/PERFORMANCE.md`, other
  lane) and pipeline HTML→PNG 214.2 → 178.8 ms.

#### Fixed

- Deep `LayoutBox` trees no longer overflow the stack when dropped; the
  painter and its traversals remain iterative with documented caps.
- Browser DOM mutations reject invalid operations via `DomError` and
  `innerHTML`/`document.write` parsing respects parser limits without leaking
  arena slots.

#### Performance

- Inline layout −43 % (100.6 → 57.2 ms on the realistic 1 MB document);
  HTML→PNG pipeline 214.2 → 178.8 ms. PNG encoding unchanged (42,853 B for
  the Phase-4 fixture, 44.8× compression).
- Test suite: **546 tests** (was 475), all green; fmt, clippy `-D warnings`,
  `cargo doc`, 4-suite conformance gate and MSRV 1.88 (including `shell`)
  verified.


### Round 5 — tables, PNG compression, browser pipeline and generational DOM (2026-11-10)

#### Added

- **`crates/render`** (Phase 5): first real browser pipeline — `URL → net → HTML
  decode → DOM → author CSS (inline + external `<link>`/`@import`-less) → layout
  with bundled fonts → display list → deterministic PNG`. Handles charset
  UTF-8/Windows-1252, `<base>`, stylesheet dedup and caps (16 MiB document,
  32 sheets, 1 MiB/sheet, 4 MiB total CSS), per-stage stats and CLI
  (`--width/--height/--base/--no-network/--json`). 22 tests; fixture 640×400 →
  15,498 B `md5 7324e851…`.
- **`crates/image`**: dependency-free PNG decoder + full zlib/DEFLATE inflater
  (stored/fixed/dynamic, 32 KiB window, Adler-32): colour types 0/2/3/4/6,
  bit depth 1–16, filters 0–4, Adam7, `PLTE`/`tRNS`, CRC verification and
  `PngLimits` anti-DoS bounds. Used as the round-trip oracle of the paint PNG
  encoder against `basic.rgba`. 46 tests.
- **CSS tables**: `Display::{InlineTable, TableRowGroup, TableHeaderGroup,
  TableFooterGroup, TableColumnGroup, TableColumn, TableCaption}` and
  `ComputedStyle::{border_spacing, border_collapse, table_layout,
  caption_side, empty_cells}` plus the WHATWG HTML §15 UA table rules.
- **Layout tables/inline-block/replaced**: CSS 2.1 §17 auto table layout
  (min/max-content, `border-spacing`, `colspan`, anonymous table fixups, cell
  `vertical-align`, caption and row-groups); real `inline-block`
  (shrink-to-fit, baseline, vertical-align); `BoxType::Replaced` with intrinsic
  sizes (`img` 300×150, `input` 20×20, `textarea` 160×40) and
  `layout_document_with_replaced_sizes`. 118 layout tests.
- **DOM generations**: `NodeId` packs 20-bit slot + 12-bit generation in 4 B,
  LIFO free list with slot reuse, `remove_node`/`remove_subtree`, stale/foreign
  handle rejection (`try_node`/`owns`), iterative `descendants`/`ancestors`
  with cycle guards, `validate_invariants()` and `estimate_bytes()`. 20 tests.
- **PNG compression in paint**: own DEFLATE (LZ77 32 KiB hash chains + fixed
  Huffman `BTYPE=01`, zlib/Adler-32) and adaptive PNG filters;
  `build_display_list` is now fully iterative with documented caps (stacking
  256, floats 64, items 5 M).

#### Changed

- README/ROADMAP/ARCHITECTURE/HANDOFF/PHASE4-SUPPORT/TOOLING/CONTRIBUTING/
  SECURITY/CHANGELOG synchronized with round 5 (475 tests, Phase 4 ≈99 %,
  Phase 5 ≈60 %); new `docs/images/render-fixture.png` capture (15,498 B).
- Golden PNG regenerated with compression: **1,920,813 B → 42,853 B (44.8×)**
  while `basic.rgba` stays byte-identical (`md5 60f8984e…`, PNG
  `md5 9e957177…`); render-to-PNG 8.6 → 14.2 ms due to compression.

#### Fixed

- Deep display-list recursion removed: the painter is iterative (20,000-level
  trees safe) and nodes beyond the documented caps are painted flat in-flow
  instead of overflowing the stack.
- DOM stale/foreign `NodeId` handles no longer corrupt the arena; invalid
  handles panic with a clear message or return `None` from `try_node`.

#### Performance

- PNG output 44.8× smaller; paint raster/caches unchanged (raster 800×600
  ~0.9 ms), render-to-PNG now 14.2 ms including DEFLATE.
- Test suite: **475 tests** (was 362), all green; `fmt`, `clippy -D warnings`,
  `cargo doc`, 4-suite conformance gate and MSRV 1.88 verified.


### Round 4 — positioning/floats/stacking + TLS/cache + fuzzing (2026-11-10)

#### Added

- **Layout**: `position: relative/absolute/fixed` with positioned containing
  blocks, static position, shrink-to-fit and `fixed` against the viewport
  (`sticky` documented as relative while there is no scrolling); `float:
  left/right` + `clear` (CSS 2.2 §9.5) with float-aware BFC height (§10.6.7);
  `overflow` creates a BFC and propagates an inherited clip rectangle.
  `LayoutBox` gains `position`, `z_index`, `opacity`,
  `establishes_stacking_context`, `clip`, `is_float` and `float_side`.
  +22 tests → 102.
- **Paint**: complete CSS 2.2 Appendix E painting order (stacking contexts and
  `z-index`, negative/auto/positive), `DisplayItem::OpacityGroup` composited
  offscreen once, `PushClip`/`PopClip` driven by `overflow`,
  `text-decoration` (underline/overline/line-through), `visibility: hidden`
  and list markers (`disc`/`circle`/`square`/`decimal`). +11 tests → 63.
- **Net (Phase 5)**: TLS 1.2/1.3 with `rustls` 0.23 (ring backend, no aws-lc)
  + `webpki-roots` trust anchors (feature `tls`, default on; SNI and
  chain/hostname validation, injectable `ClientConfig` via
  `FetchOptions::tls_config`); HTTP cache implementing an RFC 9111 subset
  (`Cache-Control` max-age/no-store/no-cache, `Expires`, `ETag`,
  `Last-Modified`, `Vary`, `304` revalidation); `data:` URLs (RFC 2397) with
  the crate's own Base64 codec; `ResourceLoader` dispatch across
  http/https/file/data. +22 tests → 85.
- **Fuzzing**: standalone `fuzz/` workspace with `cargo-fuzz`/libFuzzer targets
  `html_parse`, `css_parse`, `selectors` and `net_url`, versioned seed corpus,
  `docs/FUZZING.md` and `.github/workflows/fuzz-nightly.yml` (daily 5-minute
  smoke run + dispatch). Local ASan campaign: ~1.06 M executions, 0 crashes.
- All engine crates now declare `#![forbid(unsafe_code)]`.

#### Changed

- README/ROADMAP/ARCHITECTURE/HANDOFF/PHASE4-SUPPORT/TOOLING/CONTRIBUTING/
  SECURITY/CHANGELOG synchronized with round 4 (362 tests, Phase 4 ≈97 %,
  Phase 5 ≈35 %); README embeds the regenerated Phase 4 capture (26 KB).

#### Fixed

- Golden render regenerated after stacking/opacity/decorations
  (`basic.rgba` md5 `60f8984e…`, pipeline PNG md5 `34152025…`, 26 display
  items) and verified deterministic.
- Text glyph placement uses the IFC-propagated baseline; `line-height`
  factor, soft-hyphen and `vertical-align` conformance from round 3 preserved.
- SECURITY.md no longer claims that "fuzzing is not wired into CI": fuzzing is
  now a nightly workflow with documented triage.

#### Performance

- 800×600 raster 1598.8 ms → 0.9 ms (culling), render-to-PNG 1628 → 7.7 ms,
  full HTML→PNG pipeline 1842 → 170.7 ms (10.8×); methodology in
  `docs/PERFORMANCE.md`.
- Test suite: **362 tests** (was 301), all green; `fmt`, `clippy -D warnings`,
  `cargo doc` and the 4-suite conformance gate pass; MSRV 1.88 verified.


### Added

- `crates/fonts`: real typography engine built on `fontdb`, `ttf-parser`,
  `rustybuzz` and `ab_glyph` — font discovery and fallback, OpenType
  metrics, `measure_text`, LTR shaping (kerning/ligatures) and glyph
  rasterization to antialiased coverage masks. Bundled DejaVu fixtures keep
  tests deterministic. Not yet consumed by layout/paint.
- `crates/paint`: end-to-end `HTML → CSS → layout → display list → PNG`
  pipeline with a dependency-free PNG encoder (stored DEFLATE, Adler-32,
  CRC-32), `RenderConfig`, opaque white canvas with CSS 2.2 §14.2 root/body
  background propagation, `<style>` extraction, deterministic golden render
  and the `html_to_png` example.
- `html::ParseOptions` / `parse_document_with_options` / `ParseOutcome`:
  configurable anti-DoS limits (nesting depth, node count, attributes per
  element, input size) reported as `LimitViolations`.
- `Document::try_node` / `try_node_mut`; `append_child` and `insert_before`
  now return `bool` and reject self-insertion and ancestor cycles.
- CSS Phase 4 foundations: per-side border longhands and 1–4 value border
  shorthands, `border-style: none` computing zero width, `float`, `clear`,
  `overflow`, `visibility`, `text-decoration` and `list-style-type` in
  `ComputedStyle`.
- `crates/test_harness`: vendored WPT CSS Syntax fixtures, conformance gate
  with documented known-failure allowlists and exit codes, `.dat`↔`.json`
  parity test, strict tokenizer/shape handling.
- Repository infrastructure: dual MIT/Apache-2.0 license files, GitHub
  Actions CI (fmt, clippy, tests, rustdoc, conformance gate, MSRV job),
  `SECURITY.md`, and `rust-version = "1.88"` (the code uses let-chains).

### Changed

- HTML tokenizer attribute de-duplication is hash-based instead of linear
  (40k attributes: ~1276 ms → ~10 ms).
- The test serializer uses an explicit work stack instead of recursion.
- Layout: anonymous block/text boxes inherit only inherited properties
  (fixes phantom margins and duplicated backgrounds on pretty-printed HTML),
  `<br>` forces a line break, CSS 2.2 §8.3.1 margin collapsing, initial
  containing block sized from the viewport, `box-sizing: border-box`,
  `min/max-width/height`, percentage heights and over-constrained
  horizontal resolution.
- Selectors: unknown pseudo-classes now invalidate the selector, dynamic
  pseudo-classes never match without interaction state, SVG/MathML elements
  match without an HTML-namespace gate, commas inside brackets/strings are
  respected, attribute selectors support `i`/`s` flags.
- `paint` no longer starts from a transparent canvas: 0 % transparent
  pixels instead of 88.7 %; text advances come from layout instead of being
  re-measured 20 % wider.

### Fixed

- Process abort (stack overflow) when parsing deeply nested documents
  (100k–200k nested `<div>` no longer aborts; ~70 ms).
- DOM corruption from `append_child` cycles and `<template>` cloning sharing
  its `DocumentFragment`; `clone_node` now copies template contents.
- "WPT CSS 30/30" is reproducible from a clean clone (fixtures vendored) and
  the previously skipped suites are gated instead of silently ignored.
- Documentation synchronized with reality: real test counts, honest module
  progress, Phase 4 roadmap with WP-4.0…WP-4.13, corrected MSRV and phase
  numbering.


### Round 3 — Phase 4 conformance and Phase 5 bootstrap (2026-11-10)

#### Added

- `html`: public WHATWG HTML §13.3 serializer — `serialize_html`,
  `serialize_fragment` (`innerHTML`), `serialize_outer_html` and
  `*_with_options(HtmlSerializeOptions { scripting_enabled })`; the five
  upstream `html5lib-tests/serializer` suites now run in the gate
  (**230/230**; 18 registered XHTML alternatives are not exercised).
- `crates/net`: Phase 5 bootstrap, std-only and `#![forbid(unsafe_code)]` —
  WHATWG URL subset (`http`/`https`/`file` + relative `join`), HTTP/1.1 `GET`
  over `std::net` (timeouts, `Content-Length`, chunked, 301/302/303/307/308
  redirects with a limit, 16 MiB/64 KiB caps), `file://`, WHATWG MIME sniffing
  subset and an in-memory `ResourceLoader` cache. 57 offline tests.
- `crates/paint`: per-side borders (`BorderSide[4]`: width, style and colour;
  `solid`/`dashed`/`dotted`/`double`/`none`/`hidden`, mitred corners),
  background painted under the border box, exact fractional-rect AA,
  rasterizer culling and per-pass caches (shaped runs, faces, glyph masks).
- `crates/paint`: real shaped text rendering integrated with `crates/fonts`
  (glyph masks, per-run `font_weight`/`font_italic`, baseline propagated by
  the IFC).
- `docs/images/phase4-basic.png`: compressed 800×600 capture of the real
  HTML→PNG pipeline (typography, propagated canvas background and per-side
  borders), embedded in the README.
- `crates/bench` + `tools/bench.sh` + `docs/PERFORMANCE.md`: dependency-free
  reproducible benchmark suite with a published baseline (already committed
  in the previous round; baseline refreshed this round).

#### Changed

- CSS: functional Selectors L4 (`:is`/`:where`/`:not`/`:has`) plus structural
  family (`:nth-of-type`, `:first/last/only-of-type`, `:nth-last-*`), correct
  specificity, shorthand expansion before the cascade, `font` shorthand and
  basic `@media` (`all`/`screen`/`print`, min/max width/height px/em,
  `resolve_document_styles_with_media`).
- CSS: stylesheet rule index reduces `resolve_document_styles` time on 10k
  nodes from 63.6 ms to 30.3 ms (**2.1×**).
- Layout IFC: line breaks only at real opportunities (adjacent inline text
  nodes are no longer split), soft hyphen is zero-width and only materialises
  a hyphen when the line breaks, numeric `line-height` is inherited as a
  factor and recomputed per element, `vertical-align: top/bottom` follows the
  §10.8.1 aligned-subtree rule with unified line-height math, and
  `text-align: justify` is implemented for soft-wrapped lines.
- HTML: tokenizer allowlist is now empty (**7050/7050**); the harness gate
  gained the serializer suite and the documented strict-mode error diff.
- Documentation: README/ROADMAP/ARCHITECTURE/HANDOFF/PHASE4-SUPPORT/TOOLING/
  CONTRIBUTING synchronized with the round-3 reality (301 tests, Phase 4
  ≈90 %, Phase 5 started); README embeds the Phase 4 capture.

#### Fixed

- Text glyph placement: shaped runs now use the baseline propagated by the
  IFC instead of assuming the top of the text box; `line-height` factor and
  `vertical-align` fixes above.
- Quadratic HTML tree construction: `has_element_in_scope` /
  active-formatting reconstruction are no longer per-token linear
  (adversarial inputs 16k/32k `<div>` and attribute storms drop from seconds
  to tens of milliseconds).
- Golden render regenerated and verified deterministic after the real-text
  and border changes (`basic.rgba` md5 `4139fdd8…`; pipeline PNG md5
  `d97c67dc…`).

#### Performance

- Rasterizer 800×600: **1598.8 ms → 0.9 ms** with culling/caches.
- Render-to-PNG: **1628 ms → 7.7 ms**; full HTML→PNG pipeline:
  **1842 ms → 170.7 ms** (**10.8×**). Methodology and full table:
  `docs/PERFORMANCE.md`.
- Test suite: **301 tests** (was 165), all green; `fmt`, `clippy -D warnings`,
  `cargo doc` and the 4-suite conformance gate pass; MSRV 1.88 verified.

## [0.1.0] — 2026-11-09

### Added

- Workspace skeleton with `dom`, `html`, `css`, `layout`, `paint` and
  `test_harness` crates.
- Arena DOM (`NodeId(NonZeroU32)`, intrusive links, O(1) mutations).
- HTML5 input stream preprocessor, 84-state tokenizer, 2,231 named character
  references, 21-mode tree builder (adoption agency, foster parenting,
  fragments) and test serializer.
- CSS Syntax L3 tokenizer/parser, Selectors L4 matching, cascade and
  computed styles plus the WHATWG HTML §15 user-agent stylesheet.
- Block and inline formatting contexts (box tree, anonymous blocks, line
  breaking, strut, `vertical-align`) and a 2D software rasterizer with PPM
  export.
- Closed-loop conformance harness for `html5lib-tests` and WPT CSS Syntax.
