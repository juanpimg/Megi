# Render pipeline fixtures

Self-contained documents for the `crates/render` integration tests. No external
network, no JavaScript.

| File | Purpose |
|---|---|
| `basic.html` | `<base href="sub/">` + external `theme.css` (green canvas) + a `media="print"` sheet that must be ignored + an inline `<style>`. |
| `sub/theme.css` | Screen stylesheet applied through the `<base>` URL (green `html` background). |
| `sub/print-only.css` | Print-only sheet (`media="print"`); must never be applied. |
| `images.html` | WP-5.7 image pipeline: one local PNG (`assets/px.png`, 4×4 red/green/blue/magenta quadrants) scaled to 40×40 plus one missing image that must become a gray placeholder with a warning. |
| `assets/px.png` | Tiny deterministic PNG asset (82 bytes, generated with Python's stdlib `zlib`) used by `images.html` and served by the loopback HTTP tests. |

The tests assert that the green canvas comes from `sub/theme.css` (resolved through
`<base>`) and not from the print sheet, which proves both the `<base>` handling and the
media screen filter.

`crates/render/tests/images_tests.rs` renders `images.html` from `file:` URLs and
checks that the four PNG quadrants are painted (bilinear scaling keeps each quadrant's
solid interior), that the missing image yields `IMAGE_PLACEHOLDER` gray plus a warning,
and that the HTTP image paths honor `--no-network`, the image caps and non-PNG
placeholders. Regenerate `assets/px.png` only alongside the documented quadrants.

Deterministic acceptance command:

```bash
cargo run -p render -- tests/fixtures/render/images.html /tmp/images.png
python3 tools/png_check.py /tmp/images.png
```
