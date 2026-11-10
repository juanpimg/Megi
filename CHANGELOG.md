# Changelog

All notable changes to Megi are documented here. The project is pre-1.0 and
does not yet follow strict semantic versioning; the format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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
