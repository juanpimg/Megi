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
| `svg_img.html` | WP-5.10: `<img>` pointing at a 64×64 SVG (white background + red circle) rendered through the `svg` crate. |
| `svg_viewbox.html` | 10×10 `viewBox` SVG displayed in a 40×40 CSS box (viewBox scaling). |
| `svg_intrinsic.html` | SVG with no `width`/`height`/`viewBox`: intrinsic size falls back to the documented 300×150 default. |
| `svg_corrupt.html` | Malformed SVG: non-fatal warning + placeholder, while the rest of the page paints. |
| `svg_background.html` | `background-image: url(assets/svg_bg.svg)` on a 160×80 box (SVG stretched like a PNG). |
| `assets/svg_*.svg` | Deterministic SVG fixtures: red circle, viewBox square, default-size rectangle, corrupt payload, background tile and an oversized (20000×20000) document for the render cap. |
| `svg_inline_simple.html` | WP-5.11: inline `<svg>` (64×64 red circle) serialized to a synthetic `<img>` data URL before layout. |
| `svg_inline_viewbox.html` | Inline SVG scaled by CSS width/height (`style="width:40px;height:40px"`) through the synthetic `<img>`. |
| `svg_inline_block.html` | Inline SVG inside a `display:inline-block` container; keeps its 32×32 CSS size. |
| `svg_inline_class.html` | Inline SVG sized by a CSS class (`.icon {width:48px;height:24px}`) copied onto the synthetic `<img>`. |
| `svg_inline_corrupt.html` | Inline SVG that fails to rasterize (`width="0"`): non-fatal warning + placeholder, rest of the page paints. |
| `svg_inline_multiple.html` | Three inline SVG icons in one page (each rasterized and painted). |
| `svg_inline_use.html` | Inline SVG with internal `<use href>` and `<use xlink:href>` references. |
| `browser_api.html` | R96: script reads `getComputedStyle` (color/background/font/box/display/position/text-align/line-height/border/opacity), `matchMedia`, `innerWidth`/`innerHeight`/`devicePixelRatio`, `document.readyState` and verifies the pre-script snapshot limitation. |
| `raf_animation.html` | `requestAnimationFrame` animation: three virtual frames mutate the DOM and the final green frame reaches the PNG. |
| `script_errors.html` | Uncaught timer exception → `window.onerror` runs (mutates the DOM) and the error becomes a non-fatal warning. |
| `console_messages.html` | `console.log/warn/error` capture: warn/error become warnings and `console_errors` counts the errors. |
| `background_layers.html` | R97 multi-layer `background-image`: top-first order (magenta bullet over a checker pattern), per-layer `repeat`/`size`/`position`, `background-origin`/`background-clip: content-box`. Pixel-tested in `background_layers_tests.rs`. |
| `gradient_linear.html` | R113 (paint/css lane): linear 0/45deg/repeating gradients, `background-size`/`position`, radial+radius, `background-clip: text` glyphs. Integration-tested in `gradient_tests.rs`. |
| `gradient_radial.html` | R113 (paint/css lane): circle/ellipse/farthest-side/at-position/repeating radial gradients plus alpha overlay. Integration-tested in `gradient_tests.rs`. |
| `sticky_offsets.html` | R121: `position: sticky` with `top`/`left` offsets — headless model (no scroll): flow slot preserved + relative-like offset (orange box shifted down/right over the flow). Geometry-tested in `crates/layout/tests/typography_sticky.rs`. |
| `text_transform.html` | R121: `text-transform: uppercase/lowercase/capitalize` applied while shaping (colored paragraphs). |
| `word_spacing.html` | R121: `word-spacing`/`letter-spacing` on inline text (`one two three`, `spaced letters`, combined). |
| `white_space.html` | R121: `white-space: normal` vs `pre-wrap` vs `pre-line` with spaces/newlines (colored bands). |
| `css_state.html` | R112 dynamic CSS state: seven inputs showing `:checked` (green), `:disabled` (red), `:not(:checked):not(:disabled)` (blue), `:placeholder-shown` (magenta), `data-megi-focused` ⇒ `:focus` (yellow) and default black. Pixel-tested in `css_state_tests.rs`. |
| `js_text.html` | WP-6.2/6.3: inline classic script changes `textContent` and `style.color` of a paragraph (large red text) after parsing. |
| `js_create.html` | Inline script calls `createElement` + `appendChild` with a green `style.backgroundColor` box that layout/paint must render. |
| `js_external.html` + `assets/js_mutation.js` | External classic script (`src`) over `file:` that mutates text/colour; the same script is served over loopback HTTP by the tests. |
| `js_document_write.html` | `document.write('<p style=...>')` executed after parsing; the written node must appear in the PNG. |
| `js_throw.html` | Script that throws: must become a warning and the page must still render. |
| `js_loop.html` | Infinite loop: the per-script/total runtime budget must stop it without hanging. |
| `js_console.html` | `console.log/warn/error` captured in `RenderStats::console` (warn/error also as warnings). |

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

`crates/render/tests/inline_svg_tests.rs` renders the `svg_inline_*.html` fixtures offline
(pixel checks, viewBox/class sizing, inline-block, corrupt payload, `<use>`, determinism,
`--no-network`, and the 256/1 MiB/8 MiB inline-SVG limits). `crates/render/tests/svg_tests.rs`
renders the `svg_*.html` fixtures offline and checks the
painted pixels (`<img>` shape/colour, viewBox scaling, 300×150 default, background SVG,
corrupt/oversized warnings, Content-Type + sniffing over loopback HTTP, direct SVG
navigation and determinism). `crates/render/tests/scripting_tests.rs` renders these fixtures with the embedded Boa
runtime (`render::ScriptingMode::Enabled`, default with the `js` feature) and checks the
mutated pixels, external `src` loading (`file:` and loopback HTTP), `document.write`,
`--no-network`, console capture, exception/budget warnings and determinism. Scripts execute
**after parsing** (parse-time execution is WP-6.4, needed for the html5lib `scripted_*`
cases); `async`/`defer` ordering and modules are not supported yet.

Deterministic acceptance commands:

```bash
cargo run -p render -- tests/fixtures/render/images.html /tmp/images.png
python3 tools/png_check.py /tmp/images.png

cargo run -p render -- tests/fixtures/render/backgrounds.html /tmp/backgrounds.png
python3 tools/png_check.py /tmp/backgrounds.png

# JavaScript: enabled (default) must paint the script-set red text; --no-scripting must not.
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js-text.png
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js-text-disabled.png --no-scripting
md5sum /tmp/js-text.png /tmp/js-text-disabled.png   # different
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
