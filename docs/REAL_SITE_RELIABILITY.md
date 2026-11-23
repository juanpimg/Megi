# Real-site reliability report

Snapshot taken on 2026-11-23 with the release `render` binary against live sites, using
Firefox headless as the reference renderer. All captures are 1024×900 CSS px.

## Method

```
cargo build --release -p megi-render
./target/release/render <URL> out.png --width 1024 --height 900 --json
firefox --headless --no-remote --profile <profile> --screenshot out-firefox.png \
    --window-size=1024,900 <URL>
```

`render --json` reports HTTP status, byte counts, image/SVG/script counters and warnings.
The PNGs are compared visually side by side; the JSON is used to separate "network/parse
failure" from "style/layout divergence".

## Results

| Site | HTTP | Display items | Img (fail) | Scripts (fail) | Megi warnings | Verdict |
|---|---|---|---|---|---|---|
| example.com | 200 | 16 | 1 (0) | 1 (0) | perf budget total 9350 ms | Structure/text OK; **link unstyled** |
| news.ycombinator.com | 200 | 2807 | 3 (0) | 1 (0) | perf + style/image budget | **Author CSS `a:link` ignored**, header background missing |
| rust-lang.org | 200 | 377 | 9 (0) | 4 (0) | perf budget total 9350 ms | Text present; **layout collapsed** (nav as bullets, hero/button/sections missing) |
| en.wikipedia.org/wiki/Rust_(programming_language) | 200 | 1012 | 29 (0) | 4 (0) | perf + style + image budget | **Near-blank page**; article text invisible, stray `[ ]` glyphs |
| developer.mozilla.org/en-US/docs/Web/CSS | 200 | 2610 | 4 (0) | 6 (0) | style budget | Text present; **two-column layout lost**, sidebar overlaps title |

All five returned HTTP 200, decoded the charset correctly (utf-8), loaded images without
network failures, and executed scripts without JS errors. So the divergences are **CSS
selector/style and layout issues, not fetch or scripting failures**.

## Defects, ranked by impact

### 1. `:link` / `:any-link` never match (P1, cross-site)

`a:link`, `a:visited`, `a:any-link` are mapped to `PseudoClass::Never` in the selector
engine (`crates/css/src/selector/chunk5.rs`). Consequences:

- Site rules such as HN's `a:link { color:#000000; text-decoration:none; }` are ignored,
  so titles inherit the body colour instead of the author colour.
- The UA stylesheet has no `a[href]` rule either, so unstyled links (example.com, MDN
  prose) render black and without underline instead of the browser default blue/underline.

Evidence: `example.com` "Learn more" is black/plain in Megi but blue/underlined in
Firefox; HN titles all grey in Megi, black in Firefox.

Fix: add a real `:link` pseudo-class that matches HTML/SVG elements carrying an `href`
(or `xlink:href`) attribute, remove `link`/`any-link` from the dynamic/never list, and add
UA rules `a:link { color: -webkit-link; text-decoration: underline; }` using a concrete
colour. `:visited` stays `Never` (no visited-state tracking), which is correct for a
fresh profile.

### 2. Modern layout rules dropped on complex sites (P1, site-specific)

rust-lang.org, Wikipedia and MDN use flex/grid-heavy stylesheets. Megi parses the markup
and text (1012 display items on Wikipedia) but the visual result collapses: navigation
becomes a bullet list, columns overlap, and on Wikipedia the article text is not visible
at all. This needs per-site triage to identify which specific declarations are rejected or
overridden; it is the largest gap in practical fidelity but too broad for a single fix.

### 3. Performance-budget warnings on every site (P2)

Every site reports `performance budget: ... > 3s` / `styles_pre_script > 200ms` /
`images > 500ms` in F4-PERF-BUDGETS. These are warnings, not errors, and do not affect
correctness of the render.

## Reproducibility

Raw JSON/PPM/PNG artifacts for each site are kept under `/tmp/megi-realsite/sites/`
(`<name>.json`, `<name>.png`, `<name>-firefox.png`). The curated URL list and the
classification harness in `tools/real_site_check.py` can regenerate the Megi side with
`python3 tools/real_site_check.py --live`.
