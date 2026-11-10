# Render pipeline fixtures

Self-contained documents for the `crates/render` integration tests. No external
network, no JavaScript.

| File | Purpose |
|---|---|
| `basic.html` | `<base href="sub/">` + external `theme.css` (green canvas) + a `media="print"` sheet that must be ignored + an inline `<style>`. |
| `sub/theme.css` | Screen stylesheet applied through the `<base>` URL (green `html` background). |
| `sub/print-only.css` | Print-only sheet (`media="print"`); must never be applied. |
| `images.html` | WP-5.7 image pipeline: one local PNG (`assets/px.png`, 4×4 red/green/blue/magenta quadrants) scaled to 40×40 plus one missing image that must become a gray placeholder with a warning. |
| `backgrounds.html` | WP-5.9 CSS backgrounds: a `background-image` checker stretched over a box (with a `background-color` behind it), a missing background image that falls back to its color with a warning, a color-only box and a `list-style-image` list. Offline, loaded from `file:` URLs. |
| `assets/px.png` | Tiny deterministic PNG asset (82 bytes, generated with Python's stdlib `zlib`) used by `images.html` and served by the loopback HTTP tests. |
| `assets/pattern.png` | 16×16 orange/blue checker used as `background-image` (93 bytes, deterministic RGBA PNG). |
| `assets/bullet.png` | 8×8 magenta diamond used as `list-style-image` (92 bytes, deterministic RGBA PNG). |

The tests assert that the green canvas comes from `sub/theme.css` (resolved through
`<base>`) and not from the print sheet, which proves both the `<base>` handling and the
media screen filter.

`crates/render/tests/images_tests.rs` renders `images.html` from `file:` URLs and
checks that the four PNG quadrants are painted (bilinear scaling keeps each quadrant's
solid interior), that the missing image yields `IMAGE_PLACEHOLDER` gray plus a warning,
and that the HTTP image paths honor `--no-network`, the image caps and non-PNG
placeholders. Regenerate `assets/px.png` only alongside the documented quadrants.

`crates/render/tests/backgrounds_tests.rs` renders `backgrounds.html` from `file:` URLs
(plus loopback-HTTP variants) and checks the painted pixels: orange/blue checker,
magenta list markers, the green fallback for the missing image, the color-only box,
`--no-network`, the background caps (32/8 MiB/16 MiB) and non-PNG rejection.

Deterministic acceptance commands:

```bash
cargo run -p render -- tests/fixtures/render/images.html /tmp/images.png
python3 tools/png_check.py /tmp/images.png

cargo run -p render -- tests/fixtures/render/backgrounds.html /tmp/backgrounds.png
python3 tools/png_check.py /tmp/backgrounds.png
```

## Showcase fixture (`showcase/`)

`showcase/index.html` is a realistic JavaScript-free page (external CSS + recursive
`@import` + media queries + decoded PNG images + table + positioned elements + lists)
rendered at **1024×768** and compared byte-for-byte against
`showcase/golden.png` by `crates/render/tests/import_media_tests.rs`.

```bash
cargo run -q -p render -- tests/fixtures/render/showcase/index.html \
    tests/fixtures/render/showcase/golden.png --width 1024 --height 768
python3 tools/png_check.py tests/fixtures/render/showcase/golden.png
```

Golden md5: `c95c7eff83e7bd08f815631aa42afdb0` (80.220 bytes). This golden was
regenerated after the task-53 (table cell text) and WP-5.9 (CSS background images /
`list-style-image`) landings; both changes are expected: table cells now paint their
text/bold headers and the showcase header/stage carry a checker pattern while the
feature list uses a PNG bullet marker. See `showcase/README.md` for the statistics and
the known layout limitations reflected in the golden.
