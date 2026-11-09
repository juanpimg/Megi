# Phase 4 paint fixtures

Self-contained HTML documents (embedded `<style>`, no network, no JavaScript)
used by the WP-4.1 HTML → PNG pipeline tests and golden images.

## Files

| File | Purpose |
|---|---|
| `basic.html` | 800×600 document exercising `h1`, `p`, `br`, colored/bold `span`, `ul/li`, a `div` with padding + border + background, `hr` and `a`. |
| `basic.rgba` | Golden image: raw row-major RGBA bytes (800×600, 4 bytes/pixel, 1 920 000 bytes) of the deterministic render of `basic.html`. Compared byte-for-byte by `crates/paint/tests/end_to_end_html.rs`. |

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
map-iteration-order dependencies. Rendering the same input twice must produce
byte-identical output; the golden test relies on this.

## Notes

- The PNG writer uses stored DEFLATE blocks (no compression) on purpose, see
  `crates/paint/src/png.rs` and `docs/ARCHITECTURE.md` §3.1 (no external crates
  without an explicit policy amendment).
- The canvas is painted following CSS 2.2 §14.2: an opaque `html` background is
  propagated to the canvas; otherwise the first `body` background is propagated.
  `basic.html` relies on the `html` background (`#eef2f7`) to cover the whole
  viewport.
