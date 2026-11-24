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

| Site | HTTP | Display items (before → after) | Img (fail) | Scripts (fail) | Verdict after fixes |
|---|---|---|---|---|---|
| example.com | 200 | 16 → 17 | 1 (0) | 1 (0) | Structure/text OK; links blue/underlined (matches Firefox) |
| news.ycombinator.com | 200 | 2807 → 2839 | 3 (0) | 1 (0) | Author `a:link` honoured; titles render in author colour |
| rust-lang.org | 200 | 377 → 427 | 9 (0) | 4 (0) | Nav horizontal (no bullets), "GET STARTED" + version link restored |
| en.wikipedia.org/wiki/Rust_(programming_language) | 200 | 1012 → 12590 | 29 (0) | 4 (0) | Article/title/links/infobox fully visible (was near-blank) |
| developer.mozilla.org/en-US/docs/Web/CSS | 200 | 2610 → 455 | 4 (0) | 6 (0) | Two-column layout at 1024px: sidebar + wide article (matches Firefox) |

All five returned HTTP 200, decoded the charset correctly (utf-8), loaded images without
network failures, and executed scripts without JS errors. Every divergence below was a
**CSS selector/style/layout bug in the engine, not a fetch or scripting failure** — and
all of them have been fixed and merged (see the table above for the visual improvement).

> **Estado (2026-11-24):** los 5 defects de este informe están corregidos y mergeados en
> `main`: (1) `:link`/`:any-link` + regla UA (`a6c17b5`, PR #2237); (2) style-cache
> aliasing que dejaba Wikipedia en blanco (`99f52c54`, PR #2257); (3) media queries con
> range syntax + `calc()` (`444c48ed`, PR #2263); (4) recuperación del parser ante el IE
> star hack `*zoom:1`, que descartaba todas las reglas posteriores (`6a596dfe`, PR #2265);
> (5) `display:contents` con hoisting de cajas hijas, que reparaba el layout de MDN a
> <1072px (`605e259e`, PR #2268). La tabla muestra display items antes → después de todos
> los fixes. Capturas finales en `/tmp/megi-realsite/final/`.

## Defects, ranked by impact

### 1. `:link` / `:any-link` never match (P1, cross-site) — **RESUELTO**

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

### 2. Modern layout rules dropped on complex sites (P1, site-specific) — **RESUELTO**

rust-lang.org, Wikipedia and MDN use flex/grid-heavy stylesheets. Triage identified three
distinct root causes, each fixed independently:

- **Wikipedia near-blank** — `style_cache_key` in `crates/css/src/cascade/chunk4.rs` aliased
  distinct parent keys (only the XOR of the two 128-bit halves was compared), so matching
  declarations from different parents collided and colour/background were dropped. Fixed
  by preserving both halves and hashing declarations by name + value (PR #2257).
- **rust-lang bullets / missing button** — the IE star hack `*zoom:1` in vendor CSS made
  the parser consume the block's closing `}`, turning all following top-level rules into
  nested children. Fixed the CSS Syntax §5.5.1 error recovery (PR #2265).
- **MDN narrow column at <1072px** — media queries with range syntax + `calc()` were
  discarded (PR #2263), and `display: contents` was not implemented at all, so the
  `.layout__content` wrapper became a phantom grid item. Added `Display::Contents` with
  child-box hoisting in the tree builder (PR #2268). MDN now matches Firefox at 1024px.

Remaining lower-fidelity areas (e.g. custom-element internals, exotic `grid-template-areas`
track counts) are lower impact and tracked as separate issues.

### 3. Performance-budget warnings on every site (P2)

Every site reports `performance budget: ... > 3s` / `styles_pre_script > 200ms` /
`images > 500ms` in F4-PERF-BUDGETS. These are warnings, not errors, and do not affect
correctness of the render.

## Reproducibility

Raw JSON/PPM/PNG artifacts for each site are kept under `/tmp/megi-realsite/sites/`
(`<name>.json`, `<name>.png`, `<name>-firefox.png`). The curated URL list and the
classification harness in `tools/real_site_check.py` can regenerate the Megi side with
`python3 tools/real_site_check.py --live`.
