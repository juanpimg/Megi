# F4 — measured performance baseline (first pass)

The audit found performance problems by reading code; this file records the ones that
were **measured**, so later work can compare against a number instead of an adjective.
It also records why a complete baseline is not possible yet: the bench harness itself
does not measure what its rows claim (#1197, #1196), so the numbers below come from
targeted probes run during the crate audits, not from `cargo run -p megi-bench`.

Provenance of each row: `M` = measured with a probe/repro (the exact command or the
auditor's probe is named), `E` = estimate derived from the code path and its limits.

## Measured costs

| # | Subsystem | Symptom | Measurement | Tracked as |
| --- | --- | --- | --- | --- |
| 1 | `net` PSL | `public_suffix_labels` is O(N²) in the host length | **M**: a 90 KB host took **102 s** (`crates/net/src/psl.rs:92-109`) | #1291 |
| 2 | `svg` effect inheritance | clip/mask/filter refs cloned per shape (`EffectRef`, 48 B) | **M**: 3,000 rects build in **417 ms @ depth 16, 2.17 s @ 64, 8.17 s @ 128**; extrapolates to GBs | #1328 |
| 3 | `html` tokenizer | mass unknown end tags after a deep generic stack | **M**: `perf_mass_unknown_end_tags_after_deep_generic_stack` **2.048 s** against a 2 s budget on this host (flaky by ~2×) | #1361 |
| 4 | `css` rule nesting | the parser recurses with no depth cap | **M**: ~52 KB of `a{` → **stack overflow, SIGABRT** (`crates/css/src/parser.rs:330,373,389`) | #1412 (fixed) |
| 5 | `render` script budget | the loop budget is debited up front | **M**: 12 scripts in the document → `scripts_executed == 10` | #1299 |
| 6 | `paint` shadows | no charge for shadow scratch memory | **M**: 64 overlapping 1024×1024 blurred shadows allocate 4 scratch buffers each with nothing debited to `PaintBudget` (now charged, #1524) | #1415 (PR #1524) |
| 7 | `svg` scanlines | `fill_scanline` silently drops the fill past `MAX_SCANLINE_WORK` | **M**: a 30k-point polygon paints nothing, an over-budget clipPath erases the element (`raster.rs:1644-1646`) | #1325 (PR #1493) |
| 8 | `layout` floats | float placement re-scans all floats three times | **E**: O(F²) with no `MAX_FLOAT_COUNT`; a 4,096-float page does ~5×10⁷ comparisons | #1417 (PR #1525) |
| 9 | `css` selector matching | `match_scope()` resets the step budget per element | **E**: 100 KB CSS + 100 KB HTML ≈ **3.2×10⁸** map operations before any cap applies | #1413 |
| 10 | `paint` gradients | stop search is a linear scan per pixel | **E**: 64 stops × 16.7 Mpx = **1.07×10⁹** comparisons for one oversized gradient | #1416 |
| 11 | `js` `arrayBuffer()` | builds a per-byte `JsArray` | **E**: an 8 MiB body allocates **8.4×10⁶** JS numbers (~130 MB) | #1143 |
| 12 | `js` module graph | cycle detection was O(n) with `Vec` + a 1,024-module cap | **M**: a 1,024-module chain built in ~0.6 s before the cap (now `HashSet`) | #1071 (fixed) |
| 13 | `dom` `clone_node` | pays `subtree_size()` even for shallow clones | **E**: a 100k-node document clone shallow-copies but walks the whole subtree first | #1119 |
| 14 | `net` cookie jar | assembling the `Cookie` header is O(cookies × rules) with no size cap | **E**: 3,000 cookies per origin → ~10⁶ comparisons per request | #1294 |
| 15 | `bench` harness (meta) | filter-stripped rows measure byte-identical CSS, and the render row declares 0.096 MPix while rendering 0.48 | **M**: 29 lines in / 29 lines out for a `filter:` row | #1197 |

## What blocks a complete baseline

1. **The bench harness is not trustworthy yet.** `filter`-stripped rows render exactly the
   same CSS as their unfiltered twin (#1197), the render row under-reports its pixel count
   by 5×, `heavy` rows silently ignore `--iterations`/`--warmup` (#1198), and several rows
   use the CSS-ratio fallback measurer instead of the real layout measurer (#1201). Any
   baseline captured today would be noise. Fixing those four rows is the prerequisite for
   F4 to become a repeatable measurement.
2. **The host is a micro-SD card with 2.4 GB free.** A full workspace release build does
   not fit alongside the test targets, so the per-subsystem measurements above were taken
   with `cargo test --release -p <crate>` style probes, one crate at a time, with the
   target directory removed afterwards.

## The ten subsystems to measure next

`net` (HTTP/2 multiplexing, cookie jar, PSL), `html` (tokenizer + tree construction),
`css` (parse + cascade + selector matching), `layout` (block/flex/grid/inline/float),
`paint` (rasterizer, blur, gradients), `svg` (document build, raster, effects),
`image` (JPEG/PNG/WebP decode), `js` (runtime, module graph, worker messages),
`render` (pipeline, script budget, stylesheet discovery), `shell` (frame time, session
I/O). Each one needs: a fixture of realistic size, three runs, the median, and the
`MEGI_PERF_BUDGET_SCALE` value used.
