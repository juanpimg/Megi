# Changelog

All notable changes to Megi are documented here. The project is pre-1.0 and
does not yet follow strict semantic versioning; the format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Round 17 — flexbox MVP, rounded corners/shadows, editable forms and SVG pattern/text (2026-11-10)

#### Added

- **Flexbox MVP** (`crates/layout/src/flex.rs` + `crates/css/src/flex.rs`,
  commit `082f52d`): `display: flex`/`inline-flex` (inline-flex computes as
  `Display::InlineBlock` + `ComputedStyle::is_inline_flex`, so no new `Display`
  variant), `flex-direction` (4), `justify-content` (5 including `space-evenly`),
  `align-items`/`align-self` (stretch/flex-start/flex-end/center/baseline),
  `flex-wrap` (nowrap/wrap), `gap`/`row-gap`/`column-gap`,
  `flex-grow`/`flex-shrink`/`flex-basis` (auto/0/length/%) and the `flex`
  shorthand; items keep their normal BFC/IFC content and margins never collapse
  between items. Documented out of scope: grid, `order`, `flex-flow`,
  `align-content`, absolute/fixed items and full max-content intrinsic sizing.
  layout 135→**146** tests, css 76→**94** tests, 3 E2E fixtures with verified
  pixel bboxes: `flex_row` `md5 c28ce538…`, `flex_column` `md5 8d06d66c…`,
  `flex_wrap` `7fbf998e…`.
- **Rounded corners and shadows** (`crates/css` + `crates/paint`, commit
  `082f52d`): `border-radius` (1-4 values, elliptical `/`, longhands and
  pre-cascade shorthand), `box-shadow` (none/lists, inset, any colour order,
  2-4 lengths, cap 32) and `text-shadow` (2-3 lengths, colour, inherited).
  Paint gains the `RoundedRect`, `RoundedBorder`, `Push/PopRoundedGroup`,
  `BoxShadow` and `TextShadow` display items, with a **deterministic three-pass
  separable box blur** (radius = blur/2, blur capped at 64 px, ≤32 shadows, first
  shadow painted on top); without radius/shadows the display list stays
  byte-identical. css 76→**94**, paint 130→**141** tests (11 new effects).
- **UA form styles and editable forms** (commit `082f52d` + `67f7c57`):
  approximate UA rules for text/search/email/password/number/file/date inputs,
  submit/button/reset, checkbox/radio (`[checked]`, 13 px), textarea, select,
  label/fieldset/legend, `[disabled]`/`[readonly]`, `[data-megi-focused]` and
  `[data-megi-active]`; the shell adds `src/forms.rs` with a stable
  `data-megi-form-id`, `FocusState`, UTF-8 `TextEdit` with caret, value/checked/
  focus/active patching, a bounded `data:` snapshot re-render, click-to-focus
  (taking precedence over links), typing/Backspace/Tab/Esc, checkbox/radio
  toggles and a button flash; **Enter never submits** and scripts stay disabled
  while editing. Fixture `forms.html` (8 controls); shell 100→**108** tests;
  screenshot 640×480 `md5 b8b9a2047ddae43a6f0a329e7c2e396e`, basic golden
  `7324e851…` intact.
- **SVG `<pattern>` tiling and basic `<text>`** (commit `8f60cb8`): patterns with
  `patternUnits`/`patternContentUnits`/`patternTransform`, arbitrary tile content
  usable from `fill`/`stroke`, cycle-safe reference resolution and a **10,000
  tile cap**; basic text with `x/y/dx/dy`, `text-anchor`, `font-family`/size/
  weight via the internal `crates/fonts` (DejaVu bundled, bold; italic proxied to
  normal), `letter-spacing`, `fill`/`stroke`/opacity and translate/scale
  transforms (no kerning/ligatures/bidi; **1,000 texts / 10,000 chars** caps).
  New internal `svg → fonts` dependency. svg 60→**73** tests, fixtures
  `pattern.svg`/`text.svg`.

#### Changed

- Test suite: **1107 tests, 0 failures** (was 1046): bench 8, **css 94**, dom 28,
  **fonts 17+1 doctest**, html 60, image 45+1, js 71, **layout 146**, net 180,
  **paint 141**, render 123, **shell 108**, **svg 73**, test_harness 11.
- Phase status: Phase 4 practically complete (flex/decorations/forms UA/SVG),
  Phase 5 ≈95 %, **Phase 6 ≈90 %**, **Phase 7 ≈75 %** (editable forms).
- Repository synchronized on GitHub (`main` + `sprint/robustez-fase4` at
  `67f7c57`) with green CI (quality, conformance, MSRV); the Lead fixed five
  rustdoc link warnings before the commit. Goldens stay byte-identical
  (showcase `c95c7eff…`, phase4 PNG `9e957177…`).

### Round 16 — real SVG CSS/gradients, multi-layer backgrounds and browser APIs wired into render (2026-11-10)

#### Added

- **SVG internal CSS and gradients** (`crates/svg`, commit `1477050`):
  `<style>` blocks (including CDATA) with type/class/id compound selectors,
  descendant combinators, selector lists, a simple specificity cascade
  (inline style > id > class > type > presentation attribute) and inheritance;
  `linearGradient`/`radialGradient` with stops (`offset`, `stop-color`,
  `stop-opacity`), `gradientUnits` `objectBoundingBox`/`userSpaceOnUse`,
  `spreadMethod` `pad`/`reflect`/`repeat`, `gradientTransform`, geometry and
  `href`/`xlink:href` inheritance, usable from `fill`/`stroke` with
  deterministic per-pixel evaluation; cyclic/broken references paint nothing
  (no panic). Limits: 1 MiB stylesheet, 4,096 rules, 64 selectors/rule,
  16 descendant compounds, 256 stops and 32 href hops. `<pattern>` remains
  documented as unsupported. svg 41→**60 tests**; gradient fixture/example
  (120×60, md5 `ff59b80a48d2e22c16e6d48555db0b4c`).
- **Multi-layer backgrounds with origin/clip/attachment** (`crates/css` +
  `crates/paint`, commit `ac1c330`): `background-origin`
  (border/padding/content), `background-clip` (parsed; paints border-box),
  `background-attachment` (scroll/fixed), `ComputedStyle::background_layers`
  top-first (cyclic lists, multi-layer shorthand, colour only on the last
  layer, legacy mirror for render/goldens) and real `space`/`round`.
  `background_geometry_explicit` keeps the goldens byte-identical. css
  69→**76**, paint 122→**130** tests. Out of scope: `background-clip: text`
  and local attachment.
- **Browser APIs** (`crates/js`, commit `5fd4ea2`): `matchMedia` with a
  `set_media_provider`, `set_viewport_provider`/`ViewportInfo`
  (innerWidth/Height/DPR/scroll, scrollTo/scrollBy),
  `requestAnimationFrame`/`cancelAnimationFrame` on 16 ms virtual steps with
  `pending_animation_frames`, `document.onerror`/`window.onerror` and richer
  console (`debug`/`assert`/`table`/`dir`), `document.readyState` with
  `set_ready_state`, and the `getComputedStyle` provider verified for 24
  properties. js 58→**71 tests**.
- **Real wiring in the pipeline** (`crates/render`, commit `653626f`): a
  pre-script style pass with the real `getComputedStyle` provider (24 CSS
  properties, inline fallback; snapshot documented, `RenderTimings::styles_pre_script`),
  `matchMedia`/viewport/DPR from the real `MediaContext`, `readyState`
  loading→interactive→complete, **rAF driven by the virtual event loop with a
  fixed virtual epoch (exactly 48 ms; fixes the 49-vs-48 CI flake, identical
  PNG)**, `document.onerror`/`console.warn|error`/`last_event_error` downgraded
  to non-fatal warnings plus `RenderStats::console_errors`,
  `collect_background_references` walking `background_layers` (layers 2..N E2E)
  and hermetic cap tests. render 114→**123 tests**; multi-layer E2E
  `background_layers.html` at 240×260 `md5 c357cc2c43039ce03545be3b4221a814`.

#### Changed

- Test suite: **1046 tests, 0 failures** (was 990): bench 8, **css 76**, dom 28,
  **fonts 17+1 doctest**, html 60, image 45+1, **js 71**, layout 135, net 180,
  **paint 130**, **render 123**, shell 100, **svg 60**, test_harness 11.
- Phase status: Phase 4 practically complete, Phase 5 ≈95 %, **Phase 6 ≈90 %**
  (browser APIs wired), Phase 7 ≈70 %.
- Suite remains deterministic and reproducible (virtual clock, fixed rAF epoch,
  errors as warnings); showcase `c95c7eff…`, `basic.rgba` `60f8984e…` and the
  Phase-4 PNG `9e957177…` stay byte-identical.

### Round 15 — inline SVG, background geometry, IFC −28 % and a deterministic event loop (2026-11-10)

#### Added

- **Inline SVG rendering** (`crates/render`, commit `c267d45`): `<svg>…</svg>`
  elements inside the document are pre-processed after scripts and before
  styles/layout — serialized with a repaired `xmlns`, converted to a base64
  `data:` URL and replaced by a synthetic `<img>` that copies
  `class/id/style/width/height/alt/aria/data-*`. Limits: **256 SVG per document,
  1 MiB per SVG, 8 MiB total, nesting depth 256** (non-fatal warnings), new
  `RenderStats::inline_svgs`; 7 fixtures `svg_inline_*` and **10 tests**
  (render 104→**114**). The doctype sniffing was fixed so direct SVG navigation
  is not mistaken for HTML. Unsupported (documented): SVG-internal CSS/`<style>`,
  gradients, filters, `<text>`, animation and external references; CSS selectors
  targeting the inner `svg` do not reach the synthetic `<img>`.
- **`background-repeat` / `background-size` / `background-position`**
  (`crates/css` + `crates/paint`, commit `b5b07b3`): all repeat keywords and
  pairs, `auto`/`cover`/`contain`/lengths/percentages, keyword/percentage/length
  positions (1-2 values) and the `background:` shorthand with position/size.
  Paint builds `background_tile_size`/`background_tiles` (cover/contain centred,
  tiling clipped to the padding box, **cap 4096**; `space`/`round` render as
  `repeat`, documented) behind **`background_geometry_explicit`**, a legacy flag
  that keeps the existing goldens byte-identical. CSS 61→**69**, paint
  109→**122** tests. Out of scope: `background-origin/clip/attachment`,
  multiple layers and gradients.

#### Changed

- **IFC performance** (`crates/layout`, commit `9090175`): tokenisation with a
  per-IFC text pool (words as ranges instead of one `String` each), style-id
  table (one `Arc` per node), metrics computed once per text node, pre-sized
  buffers; incremental line breaking (no O(n²) rescans) and a `format_line` fast
  path for single-text-node lines; `FontTextMeasurer` memoises parsed families,
  metrics and advances (4 MiB cap, documented per-measurer/database lifetime).
  Interleaved A/B: **layout/realistic-1mb 50.5 → 36.3 ms (−28 %)** and
  **pipeline 132.9 → 120.5 ms (−9.3 %)** with byte-identical geometry.
- Test suite: **990 tests, 0 failures** (was 959): bench 8, **css 69**, dom 28,
  **fonts 17+1 doctest**, html 60, image 45+1, js 58, layout 135, net 180,
  **paint 122**, **render 114**, shell 100, svg 41, test_harness 11.
- Phase status: Phase 4 (layout/paint/typography/images/backgrounds/SVG)
  practically complete, Phase 5 ≈95 %, **Phase 6 ≈80-85 %**, **Phase 7 ≈70 %**.

#### Fixed

- **Deterministic event loop** (`crates/render`, commit `b808d7b`): CI caught
  `run_event_loop` using `run_tasks` (wall clock), which executed timers early on
  slow machines (10 vs 13 tasks). The loop now uses **only `run_until` with the
  virtual clock** in 1 ms steps and computes `virtual_time_ms` from the step
  count; fixture `js_event_timeout` is deterministic (tasks=1, iterations=10,
  virtual=10 ms, `md5 0768208824102e94a6f32574991b7692`). The render suite is
  reproducible regardless of host load.

### Round 14 — SVG wired into render and parser fast-hash regression fix (2026-11-10)

#### Added

- **SVG rendering in the pipeline** (`crates/render`, commit `6971e50`):
  `<img src=*.svg>` and `background-image` SVGs are decoded and rasterized with
  the from-scratch `svg` crate — detection by `Content-Type: image/svg+xml` and
  content sniffing, intrinsic size from `width`/`height`/`viewBox` (300×150
  default), cap **16 Mpx / 64 MiB RGBA** per image, corrupt or oversized files
  produce a **non-fatal warning before fallback**, and direct SVG navigation is
  wrapped in a synthetic HTML page. New `RenderStats::svgs_rendered` (human
  output and `--json`), 5 fixtures (`svg_img`, `svg_viewbox`, `svg_intrinsic`,
  `svg_corrupt`, `svg_background`) plus 6 assets and
  `crates/render/tests/svg_tests.rs` with **11 tests**; render **93→104 tests**
  and workspace **948→959 tests**.

#### Fixed

- **Parser regression `massive-end-p` restored** (`crates/html/src/fast_hash.rs`,
  commit `b7ef1c1`): the byte-chunk hashing loop for short keys is back, so
  `html/massive-end-p` returns to **~19 ms** (18.7-19.3 ms; it was 21.4 ms in
  round 11, 18.2 ms in round 10) and the **pipeline is ~127-134 ms** again. Only
  the hasher changed; the HTML5 conformance gate stays at 3,904/3,904.

### Round 13 — web fonts (@font-face/WOFF2), a from-scratch `svg` crate and shell zoom/refresh (2026-11-10)

#### Added

- **Web fonts end to end (`@font-face` + WOFF2)**: `crates/css` parses
  `@font-face` (`Stylesheet.font_faces`, `FontFaceRule { family, sources,
  weight, style }`); `crates/fonts` gains `decode_web_font`/`is_woff2`
  (WOFF2→sfnt) through the authorized dependency **`woff2-patched` 0.4**
  (Apache-2.0, pure Rust, no C/FFI, only in `crates/fonts`) plus
  `FontDatabase::load_font_data_family(alias, sfnt)`; `crates/render` collects
  the rules from every sheet (including `@import`), deduplicates, fetches them
  through the `ResourceLoader` (`--no-network`, caps **8 fonts / 8 MiB each /
  32 MiB total**), builds a **per-render font database** shared by layout and
  paint and reports `fonts_loaded`/`fonts_failed`/`font_bytes` and
  `timings.fonts` (failures are non-fatal warnings with fallback). Fixtures:
  OFL Open Sans WOFF2 (18,640 B + license); evidence **webfont
  `md5 f38aed032b8d160075097f1e4ad83b49`** vs **fallback
  `e2d76dee98f7c4ef2896c5d003be74f2`**.
- **New `crates/svg` crate (from scratch)**: own XML-subset parser + SVG model
  (`svg/g/defs/use/rect` with rx/ry, `circle/ellipse/line/polyline/polygon/path`,
  nested `svg`, transforms, `viewBox`/`preserveAspectRatio`, inline styles,
  colours, fill non-zero/even-odd, stroke width/caps/joins/miterlimit,
  opacity), path data `M/L/H/V/C/S/Q/T/A/Z` with elliptical arcs→cubics and a
  **deterministic antialiased scanline rasterizer** (4 sub-rows + analytic
  horizontal coverage; strokes via 4×4 supersampling), straight-alpha RGBA,
  `#![forbid(unsafe_code)]`, anti-DoS limits (8 MiB input / depth 256 / 100k
  elements / 1M segments / 16k side / 64 Mpx / `<use>` depth 16), the
  `render_svg`/`intrinsic_size`/`SvgImage`/`SvgOptions`/`Limits`/`SvgError` API
  and `examples/svg_to_png` with its own PNG writer. **41 tests.** Unsupported
  (documented): `<text>`, CSS class/style sheets, gradients/patterns,
  `stroke-dasharray`, markers, filters/masks/clips, script/animation and
  external references. **Pipeline wiring (`<img src=*.svg>`) is the next round.**
- **Shell zoom, auto-refresh and richer status**: zoom (`Ctrl+=/-/0`,
  `--zoom` 0.5–3.0, 50–300 % steps, CSS viewport = physical/(scale·zoom)),
  `Ctrl+L` prints the URL, opt-in `--refresh-ms [MS]` (250 ms default; only when
  `scripts_executed > 0 && script_timers_pending > 0`, generation-checked
  worker, scroll preserved, no loops) and titles with zoom/scroll/warnings/
  errors. **83→100 shell tests**; basic screenshot `md5 7324e851…` unchanged and
  `--zoom 2` (640×400→320×200) `md5 c11903cfe0c4f7095289052d25750bd3`.

#### Changed

- Test suite: **948 tests, 0 failures** (was 877): bench 8, **css 61**, dom 28,
  **fonts 17+1 doctest**, html 60, image 45+1, js 58, layout 135, net 180,
  paint 109, **render 93**, **shell 100**, **svg 41**, test_harness 11. fmt,
  clippy `-D warnings`, `cargo doc`, gate PASS and MSRV 1.88 verified.
- Phase status: Phase 4 practically complete in the engine (web fonts done,
  `svg` crate ready; wiring pending), Phase 5 ≈95 %, **Phase 6 ≈80-85 %**,
  **Phase 7 ≈70 %**.
- Dependency policy (`ARCHITECTURE.md` §3.1): `woff2-patched` 0.4 authorized as
  a WOFF2-format component in `crates/fonts` only; the CI cache will rebuild
  once because of the new dependency.
- Repo synchronized on GitHub (`main` + `sprint/robustez-fase4` at `5234630`)
  and CI (quality, conformance, MSRV) green with the Wayland/X11/xcb system
  packages installed on the runner.
- Goldens unchanged: showcase 80,220 B `md5 c95c7eff…`, render basic 15,498 B
  `md5 7324e851…`, Phase-4 `basic.rgba` `md5 60f8984e…`.

### Round 12 — layout −14.6 %, byte-identical PNG encoder −30.5 %, advanced JS events and a green CI (2026-11-10)

#### Added

- **Advanced DOM events** (`crates/js`): real `currentTarget` and `eventPhase`
  1/2/3 (including the capture phase at the target), `once`/`passive`
  listeners, identity-based `removeEventListener`, inline `onclick`/`onload`/
  `onerror` handlers on elements/`document`/`window`, `document.close()` no-op,
  approximate `estimated_heap_bytes()`, `TypeError` when dispatching a non-`Event`,
  and listener exceptions captured into `last_event_error()` **without aborting
  the remaining dispatch**. 46→**58 js tests**.
- **CI runner system dependencies**: `.github/workflows/ci.yml` now installs the
  Wayland/X11/xcb development packages `winit`/`softbuffer` need (`pkg-config`,
  `libwayland-dev`, `libxkbcommon-dev`, `libxkbcommon-x11-dev`, `libx11-dev`,
  `libx11-xcb-dev`, `libxcursor-dev`, `libxi-dev`, `libxrandr-dev`,
  `libxcb1-dev`, `libxcb-render0-dev`, `libxcb-shape0-dev`,
  `libxcb-xfixes0-dev`, `libxcb-shm0-dev`, `libxcb-randr0-dev`). The
  **quality, conformance and MSRV jobs are green**, and `main` /
  `sprint/robustez-fase4` are synced on GitHub.

#### Changed

- **`LayoutBox.style` is an `Option<Arc<ComputedStyle>>`** (`crates/layout`):
  `style()` and `with_shared_style` added (`LayoutBox::new` keeps accepting
  `Option<ComputedStyle>` for compatibility), the anonymous style is derived
  once per parent and shared by its text/anonymous boxes, and `Space` tokens
  store a `char` instead of a `String` (~150k allocations removed on the 1 MiB
  benchmark). `LayoutBox` shrinks **496 → 176 bytes (−65 %)**.
- **PNG encoder rewrite** (`crates/paint`): filters, LZ77 (precomputed hashes,
  8-byte match candidate) and Adler-32 computed in fused passes,
  `png::encode_pixels` avoids the intermediate RGBA buffer and Adler-32 is
  vectorizable — **−30.5 %** encoding time with a **byte-for-byte identical
  bitstream** (goldens were not regenerated; dynamic Huffman remains a
  ratio-focused follow-up).
- Test suite: **877 tests, 0 failures** (was 862): bench 8, css 57, dom 28,
  fonts 13+1 doctest, html 60, image 45+1, **js 58**, **layout 135**, net 180,
  **paint 109**, render 88, shell 83, test_harness 11. fmt, clippy
  `-D warnings`, `cargo doc`, gate PASS and MSRV 1.88 verified.
- Phase status: Phase 4 functionally complete, Phase 5 ≈95 %, **Phase 6 ≈80 %**
  (advanced events done), **Phase 7 ≈65-70 %**.

#### Performance

- `layout/realistic-1mb`: **60.4-62.2 → 51.3-53.5 ms (−14.6 %)**;
  `pipeline/html-to-png-800x600`: **141-147 → 134-137 ms (−6.3 %)** of a
  160 ms budget; `render-to-png` **13.30 → 9.24 ms** and showcase wall
  **34.1 → 29.5 ms** (PNG **−30.5 %**); `html/tag-40k-attrs` stays at ~12.4 ms.
- Goldens unchanged byte-for-byte: showcase 80,220 B `md5 c95c7eff…`, render
  basic 15,498 B `md5 7324e851…`, Phase-4 `basic.rgba` `md5 60f8984e…`, PNG
  fixture 42,853 B `md5 9e957177…`.

### Round 11 — HTML5 conformance milestone (3,904/3,904 tree), parser-inserted scripts, WP-B and a budgeted event loop (2026-11-10)

#### Added

- **HTML5 conformance milestone — tree 3,904/3,904 (100 %)**: the 62
  `html5lib-tests/tree-construction` suites now pass completely, including the
  four `#script-on` `scripted_*` suites (adoption01 1/1, ark 1/1, foster01 2/2,
  webkit01 2/2). The tree allowlist is **empty**
  (`KNOWN_TREE_FAILURES = []`, `TREE_EXPECTED_PASSED = 3904`) and the gate runs
  PASS with tokenizer 7,050/7,050, tree 3,904/3,904, serializer 230/230 and CSS
  Syntax 30/30.
- **WP-6.4 — parser-inserted scripts** (`crates/html`): `html::scripting::{ScriptExecutor,
  ScriptExecution}`, `TreeBuilder::with_script_executor` and
  `parse_document_with_options_and_executor`/`parse_document_with_script_executor`.
  Inline parser-inserted `<script>` elements execute in the "text" insertion mode
  at `</script>` (or EOF) with the partially-built DOM visible;
  `document.write`/`writeln` output is **re-tokenized in place** through
  `Tokenizer::insert_input` (code-point splice, reconsume-aware, shifted line
  starts/UTF-16 columns, nested written scripts included). External `src`
  scripts are left to the host, executor errors are non-fatal
  (`ParseOutcome::script_errors`) and the no-scripting path is **zero-cost**.
- **Conformance harness with JavaScript** (`crates/test_harness`):
  `js_executor::JsScriptExecutor` built on `JsRuntime::eval_on_document`, with a
  JS prelude that captures `document.write` and polyfills
  `documentElement.remove()`/`document.appendChild()` and an epilogue whose marker
  node Rust extracts and removes. The four `scripted_*` suites run through it in
  `script-on` mode; 3 unit tests + a dedicated `closed_loop` gate test
  (**11 test_harness tests**). The harness now depends on `js` (Boa) — follow-up:
  feature-gate so `--no-default-features` stays Boa-free.
- **WP-B DOM surface** (`crates/js`): complete `CSSStyleDeclaration`
  (`cssText`/`length`/`item`/`getPropertyValue`/`setProperty`/`removeProperty`),
  `getComputedStyle` with an injectable Rust provider, `NodeList` snapshots,
  `matches`/`closest`, `insertAdjacentHTML`/`insertAdjacentText`, wrapper↔node
  **identity caching**, and `document` as an `EventTarget`. Rust-facing APIs:
  `eval_on_document`, `dispatch_simple_event`, `take_document`. **46 js tests**.
- **Budgeted event loop in `render`**: virtual clock, `DOMContentLoaded`/`load`
  dispatched through the document `EventTarget`, delayed timers
  (`max_tasks` 10,000, `max_virtual_time` 1000 ms), a **single re-render** after
  draining the queue and `RenderStats::timings.event_loop`/`script_*` stats.
  6 `js_event_*` fixtures and 11 tests → **88 render tests** (plus the
  `13f3f21` follow-up for `DOMContentLoaded` through the document target).

#### Changed

- Test suite: **862 tests, 0 failures** (was 820): bench 8, css 57, dom 28,
  fonts 13+1 doctest, **html 60**, image 45+1, **js 46**, layout 133, net 180,
  paint 108, **render 88**, shell 83, **test_harness 11**. fmt, clippy
  `-D warnings` (workspace), `cargo doc`, gate PASS and MSRV 1.88 verified.
- Phase status: Phase 4 functionally complete, Phase 5 ≈95 %, **Phase 6
  ≈75-80 %** (WP-6.1/6.2/6.4 + WP-A/WP-B done), **Phase 7 ≈65-70 %**.
- Goldens unchanged: showcase 80,220 B `md5 c95c7eff…`, render basic 15,498 B
  `md5 7324e851…`, Phase-4 `basic.rgba` byte-identical (`md5 60f8984e…`).

#### Fixed

- **Parser performance regression** (`html/tag-40k-attrs`, +28 % in round 10):
  attribute dedupe is back on `std::HashSet`, `FastHasher` is used word-at-a-time
  only for bookkeeping and the character-batching fast path now checks
  `state == Data` first. The benchmark is back to **~12.4 ms**, A/B always below
  the pre-round-10 baseline, while keeping round 10's gains (interleaved pipeline
  ~154-158 ms).

### Round 10 — JS events/timers, a 131 ms pipeline, hit regions in render and advanced networking (2026-11-10)

#### Added

- **Phase 6, WP-A — events and timers** (`crates/js`): `EventTarget` with
  `addEventListener`/`removeEventListener`/`dispatchEvent` on nodes, `document`
  and `window`, **capture phase** (window→document→ancestors→target) and
  **bubbling** back; `Event`/`CustomEvent` (`bubbles`/`cancelable`/`detail`/
  `defaultPrevented`, `preventDefault`/`stopPropagation`/
  `stopImmediatePropagation`, `dispatchEvent` returns false after
  `preventDefault`); timers on the Rust scheduler with an **injectable clock**:
  `setTimeout(args)`/`setInterval`/`clearTimeout`/`clearInterval`, `(due, id)`
  ordering, `advance(Duration)`, `run_until(deadline_ms)`, `now_ms()` and
  `pending_timers()`; `run_tasks` now only runs timers due by the real clock
  (documented semantic change). 14 new tests → **26 js**. Limits: no
  `onclick`/`once`/`passive`, `currentTarget` is null inside listeners, and
  `removeEventListener` does not compare identity.
- **`RenderOutput.hit_regions`** (render/shell): the pipeline builds
  `paint::build_display_list_with_hit_regions` output and exposes
  `RenderOutput.hit_regions` + `RenderStats.hit_regions`; the shell resolves
  clicks with `render::RenderOutput::hit_regions` and `paint::find_link_at`
  (documented layout+DOM fallback), using a `file://` fixture with relative and
  absolute links. An `<a>` without `href` produces no region. Pixels are
  byte-identical (basic `7324e851…`, showcase `c95c7eff…`) at a **+0.6 µs**
  paint cost (121 regions over 80 items). shell 83 tests.
- **Advanced networking end to end** (`render`/`shell`):
  `RenderOptions.cookies` (per-render jar) or a shared session `cookie_jar`
  propagating `Set-Cookie` across document/redirects/subresources and later
  renders; `RenderOptions.origin`/`--origin` (or Origin derived from the
  document URL) enabling CORS/credentials validation, with CORS denials reported
  as **non-fatal warnings**; gzip/deflate verified E2E over loopback (gzipped
  HTML/CSS still apply styles); `RenderStats` gains `hit_regions`,
  `cookies_stored` and `content_encoding`; CLI `--cookies`/`--origin` in
  `render` and `shell`. render 77 tests (+10).
- **Performance work (parse + layout)**: in-body character batching (one
  insertion per run + `append_text(&str)`), `consume_data_run` in the
  tokenizer, single-pass preprocessing, an internal hasher, `allow_cdata`
  caching, a foreign counter and a reused token buffer; layout drops hot
  `ComputedStyle` clones and iterates tables directly.

#### Changed

- Test suite: **820 tests, 0 failures** (was 793): css 57, dom 28, fonts 13+1
  doctest, html 53, image 45+1, **js 26**, layout 133, net 180, paint 108,
  **render 77**, **shell 83**, bench 8, test_harness 7. fmt, clippy
  `-D warnings`, `cargo doc`, 4-suite gate PASS and MSRV 1.88 verified.
- Phase status: Phase 4 functionally complete, **Phase 5 ≈95 %** (advanced
  networking E2E; HTTP/2 and streaming pending), **Phase 6 ≈40 %** (WP-6.1 +
  WP-6.2 host + WP-A; WP-B and parser-inserted pending), **Phase 7 ≈65 %**
  (hit regions wired into render/shell).
- Goldens unchanged: showcase 80,220 B `md5 c95c7eff…`, render basic 15,498 B
  `md5 7324e851…`, Phase-4 `basic.rgba` byte-identical (`md5 60f8984e…`);
  scripting evidence `js_text.html` 15,876 B `md5 e1da51d4…` vs 15,499 B
  `md5 53f67e77…` with `--no-scripting`.

#### Performance

- **Pipeline 800×600: 167.6 → 131.2 ms (−21.7 %)**; `html/realistic-1mb`
  **55.0 → 19.2 ms (−65 %)**; `layout/realistic-1mb` 56.1 → 54.5 ms (−2.8 %).
  The pipeline now uses **65.6 % of its 200 ms budget**. The 4-suite gate is
  untouched (tokenizer 7,050/7,050, tree 3,898/3,904) and the paint golden stays
  byte-identical.

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
