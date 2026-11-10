# Render showcase fixture

A realistic, JavaScript-free page that exercises the complete Phase 5/7 render
pipeline: `file://`/HTTP loading, external CSS, recursive `@import`, media queries,
decoded PNG image assets, a table, positioned elements and lists.

| File | Purpose |
|---|---|
| `index.html` | Showcase document: `<link rel="stylesheet">`, inline `<style>`, a `media="print"` sheet (must be skipped), hero with PNG logo, floated menu/cards, spec table, `position: relative/absolute` stage, chip list and a decoded PNG tile. |
| `css/site.css` | Main screen stylesheet: `@import url("theme.css")`, wide (`min-width: 900px`) and narrow (`max-width: 700px`) media queries. |
| `css/theme.css` | Recursively imported stylesheet (depth 1) with its own `@media` rules (`min-width: 1000px`, `max-width: 640px`). |
| `css/print-sheet.css` | `media="print"` sheet: must never be fetched or applied on screen. |
| `assets/logo.png`, `assets/tile.png` | PNG `<img>` assets fetched, decoded and painted by the pipeline. |
| `../assets/pattern.png` | 16×16 checker used as `background-image` on `.hero` and `.stage` (WP-5.9; stretched to the padding box, no repeat/size/position). |
| `../assets/bullet.png` | 8×8 magenta diamond used as `list-style-image` on `.feature-list li`. |
| `golden.png` | Golden render at **1024×768**; compared byte-for-byte by `crates/render/tests/import_media_tests.rs`. |

## Regenerating the golden

Only regenerate after an intentional rendering change (layout/paint/CSS/fonts), and
review the PNG diff in the commit:

```bash
cargo run -q -p render -- \
    tests/fixtures/render/showcase/index.html \
    tests/fixtures/render/showcase/golden.png \
    --width 1024 --height 768
python3 tools/png_check.py tests/fixtures/render/showcase/golden.png
```

Expected metadata of the current golden:

- size: **1024 × 768**, PNG color type 6 (RGBA), opaque canvas, **80.220 bytes**.
- `@import`ed stylesheets: **1** (`theme.css`, depth 1); external stylesheets: 1
  (`site.css`); media-skipped sheets: 1 (`print-sheet.css`); `<img>` images loaded: 2;
  background images loaded: **2** (185 bytes); display items: **80**.
- md5: **c95c7eff83e7bd08f815631aa42afdb0** (previous golden
  `570aa554c88c2fdf75481e8cd076bf61`, 65.151 bytes, before table text + backgrounds).

## Known layout limitations reflected in the golden

The fixture stays within the box model the current layout engine paints reliably. The
following are known and tracked as follow-ups (the HTML/CSS intentionally includes them
so the golden will evolve when they land):

- `display: inline-block` boxes and floated/absolutely-positioned **replaced** elements
  (images) are not painted by the in-flight layout lane, so the logo is inline in the
  `h1` and the tile is a normal-flow block image.
- Table cell text (`<td>`/`<th>`) is painted since task-53; the golden now shows the
  Feature/Value rows and bold headers.
- CSS `background-image` is stretched into the padding box (no `background-repeat`,
  `background-size` or `background-position` yet); `list-style-image` replaces the disc
  marker when the image URL is present and `list-style-image` is declared on the item
  itself (this engine does not inherit that property). Canvas propagation propagates only
  the background *color*.
