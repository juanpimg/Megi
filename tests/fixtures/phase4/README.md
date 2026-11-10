# Phase 4 paint fixtures

Self-contained HTML documents (embedded `<style>`, no network, no JavaScript)
used by the WP-4.1 HTML → PNG pipeline tests and golden images.

## Files

| File | Purpose |
|---|---|
| `basic.html` | 800×600 document exercising `h1`, `p`, `br`, colored/bold `span`, `ul/li`, a `div` with padding + border + background, `hr` and `a`. |
| `basic.rgba` | Golden image: raw row-major RGBA bytes (800×600, 4 bytes/pixel, 1 920 000 bytes) of the deterministic render of `basic.html`. Compared byte-for-byte by `crates/paint/tests/end_to_end_html.rs`. It is unaffected by PNG compression changes. |

Golden checksums (md5):

| Artifact | md5 |
|---|---|
| `basic.rgba` | `60f8984ecf01e0db67976867172c0eee` |
| `basic.png` (rendered from the fixture, compressed DEFLATE) | `9e95717795744335bc0dfc7cade782ee` |

`basic.png` is **42 853 bytes** after WP-4.11 compression (it was 1 920 813
bytes with the former *stored* DEFLATE writer: a 44.8× reduction). The encoder
uses adaptive PNG row filters (`None`/`Sub`/`Up`/`Average`/`Paeth`, chosen per
row by the sum of absolute deviations) plus an in-tree DEFLATE implementation
(LZ77, 32 KiB window, hash chains, fixed Huffman codes); the zlib wrapper and
Adler-32 follow RFC 1950. See `crates/paint/src/{png,deflate,inflate}.rs`.

The fixture exercises, besides the WP-4.1 pipeline features:

- per-side borders with distinct colors, widths and styles
  (`#card`: solid blue top, solid teal right, dashed orange bottom, double
  purple left) — painted by the WP-4.3 per-side border model with mitred
  corners;
- antialiased rectangle edges (fractional layout coordinates produce partial
  coverage pixels instead of round-out over-paint, WP-4.11);
- real DejaVu Sans text with per-fragment baselines (WP-4.4);
- list markers (`ul`/`li` discs, WP-4.9) and a `text-decoration: underline`
  link (WP-4.9);
- a `position: relative` stage with two `position: absolute` children
  (`#under` with `z-index: 1`, `#badge` with `z-index: 3` and
  `opacity: 0.7`): the badge must paint over the in-flow/under content and
  blend with it as a single offscreen opacity group (WP-4.6);
- the painter is iterative (explicit stacks) and bounded: 20 000-level layout
  chains are covered by `crates/paint/tests/robustness.rs`, with documented
  caps for stacking-context depth (256), float nesting (64) and display-list
  size (5 000 000 items).

## Regenerating the golden image

The golden must only be regenerated when an intentional rendering change is
made (and reviewed in the diff of the next commit):

```bash
# 1. Render the fixture and dump the canonical RGBA bytes next to the PNG.
cargo run -p paint --example html_to_png -- \
    tests/fixtures/phase4/basic.html /tmp/basic.png \
    --rgba tests/fixtures/phase4/basic.rgba

# 2. Validate the PNG structure (signature, IHDR, chunk CRCs, IDAT, IEND).
python3 tools/png_check.py /tmp/basic.png

# 3. Re-run the end-to-end tests (golden comparison included).
cargo test -p paint --test end_to_end_html
```

The pipeline is deterministic: parsing, style resolution, layout, display list,
rasterization and the dependency-free PNG encoder contain no time, randomness or
map-iteration-order dependencies (the font database is the bundled DejaVu Sans
and the text caches are lookup-only). Rendering the same input twice must
produce byte-identical output; the golden test relies on this. After WP-4.11
the rasterizer keeps per-pass caches of shaped runs, faces and glyph masks
(see `paint::RasterStats`), so the same bytes are produced with cache misses or
hits.

## Notes

- **Real text rendering (WP-4.4 phase 2):** glyphs are shaped with RustyBuzz and
  rasterized from the bundled DejaVu Sans faces in `crates/fonts/assets/`
  (deterministic, independent of system fonts). Text is antialiased and placed
  on the baseline propagated by the IFC; the old 8×8 bitmap stub is only a
  last-resort fallback when no font database is available.

- The PNG writer uses stored DEFLATE blocks (no compression) on purpose, see
  `crates/paint/src/png.rs` and `docs/ARCHITECTURE.md` §3.1 (no external crates
  without an explicit policy amendment).
- The canvas is painted following CSS 2.2 §14.2: an opaque `html` background is
  propagated to the canvas; otherwise the first `body` background is propagated.
  `basic.html` relies on the `html` background (`#eef2f7`) to cover the whole
  viewport.
