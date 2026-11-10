# Changelog

All notable changes to Megi are documented here. The project is pre-1.0 and
does not yet follow strict semantic versioning; the format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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
