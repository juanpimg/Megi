# HTML + CSS gap matrix — Megi @ 9c94e86

Read-only analysis of upstream `main` (9c94e86) (upstream main, commit 9c94e86). No cargo
was run; every claim is from source reading plus targeted `rg` probes (commands quoted
inline where the evidence is an *absence*). Status legend: **full** = spec-conformant
enough for real pages; **partial** = implemented for a subset or headless-only;
**absent** = no handling at all; **wrong** = handled but observably off-spec.

Capability manifest: there is no `docs/status/capabilities.*`; the hand-maintained source
of truth is [`docs/status/coverage.toml`](`docs/status/progress.toml`) (71
`[[capability]]` rows, `state = done|partial|missing` with `issue` ids), rendered into
`PROGRESS.md` by `tools/update_coverage.py`. Its self-reported 77% (54/70) is
*feature-bucket* coverage, not page-realism coverage — hence this matrix.

HTML conformance is genuinely strong at the parser layer: vendored html5lib suites live
in `tests/html5lib-tests` (169 files: 15 tokenizer, 62 tree-construction `.dat` + 62 JSON,
5 serializer, 5 encoding) and drive `crates/test_harness/src/main.rs`; CSS Syntax WPT
lives in `tests/wpt/css/css-syntax`. The gaps below are *rendering and scripting*
semantics on top of a correct parse.

---

## 1. HTML — element matrix

| Element | Status | Evidence (file:line) | What is missing / wrong |
|---|---|---|---|
| `html`, `head`, `body`, `title`, `meta`, `link`, `style`, `base` | full | `crates/css/src/user_agent.rs:37-42`; `crates/render/src/stylesheets.rs:1` | — (charset via `crates/render/src/charset.rs`; `<meta name=viewport>` has no effect, desktop-only viewport) |
| `div`, `p`, `h1-h6`, `ul/ol/li`, `dl/dt/dd`, `blockquote`, `pre`, `hr`, `address`, `figure/figcaption`, sectioning tags | full | `crates/css/src/user_agent.rs:23-32` | — |
| `a` | full (render) / partial (nav) | `crates/shell/src/links.rs:119`; `crates/shell/src/window.rs:65` | `target`, `download`, `rel`, `ping` ignored; `javascript:`/`mailto:` explicitly non-navigable (`window.rs:65`) |
| inline semantics `em/strong/i/b/code/small/sub/sup/abbr/cite/q` | partial | `crates/css/src/user_agent.rs:45-70` | `q` quotes, `abbr` titles, `sub/sup` vertical raise all absent |
| `ruby/rt/rp`, `bdi/bdo`, `wbr`, `mark`, `time` | absent | `crates/css/src/user_agent.rs` has no rule; `rg '"ruby"|"bdo"|"mark"' crates/render/src crates/user_agent` finds none | parsed as plain inline; no ruby annotation layout, no `bdi` isolation, no `mark` highlight |
| `img` (`src`, `width`/`height`, `alt`) | partial | `crates/render/src/images.rs:90-113`; `crates/layout/src/tree_builder.rs:421` | `srcset`, `sizes`, `loading=lazy`, `decoding`, `fetchpriority` ignored; failed images paint a gray placeholder (`crates/paint/src/painter.rs:268`), `alt` text is not painted |
| `picture` + `source` (art direction / formats) | absent | `images.rs:90-113` only walks `img[src]`; `rg '"source"|"picture"' crates/render/src` → none | responsive/format negotiation never happens: `<picture>` children are parsed but the chosen `srcset` candidate is never applied |
| `srcset` / `sizes` | absent | `crates/shell/src/images.rs:6-8` documents "no lazily loaded/`srcset` selection" | DPR/size-appropriate image selection missing; `<img srcset>` without `src` renders **no image at all** |
| `video`, `audio`, `track` | absent | `crates/layout/src/tree_builder.rs:228-244` (replaced box only); `rg 'HTMLMediaElement|currentTime|play\(' crates/js/src crates/render/src` → 0 | no playback, no controls, no poster, no captions; element paints as a fixed placeholder/aspect box |
| `canvas` | absent | `tree_builder.rs:228-244` (replaced box, default 300×150 at `:511`); `rg 'getContext|CanvasRenderingContext' crates` → 0 | no 2D context, no bitmap; every chart/game/canvas app renders an empty box |
| `iframe` | absent | `tree_builder.rs:228-244`; `rg 'top layer|frame tree|nested browsing' crates/render/src crates/html/src` → 0 | no nested browsing context, no `srcdoc`/`sandbox`/`allow`; embeds (maps, video, payment) are blank boxes |
| `object`, `embed` | absent | `tree_builder.rs:228-244` | replaced/fallback semantics unimplemented |
| `form`, `fieldset`, `legend`, `label`, `button`, `input`, `select`, `textarea`, `output` | partial | `crates/shell/src/forms.rs:82-108` (kind mapping), `:55` (`select` renders first option, no dropdown) | **no submission**: `crates/shell/src/lib.rs:134`, `crates/shell/src/forms.rs:20` — "`Enter` never submits (no backend)"; no `action`/`method`/`enctype`/`FormData`/validation API; `input type=hidden` is only observed by the tree builder (`crates/html/src/tree_builder.rs:3214`) and still laid out as a box |
| `input` types beyond text-like | partial | `crates/shell/src/forms.rs:82-108`: checkbox/radio/submit/reset/button have real kinds, everything else (date, color, range, number, file, email, password, search) collapses to `TextInput` | no pickers, no spinners, no file chooser, no type validation; `pattern`/`min`/`max` ignored |
| `progress`, `meter`, `datalist` | absent | `rg '"progress"|"meter"|"datalist"' crates/render/src crates/layout/src` → 0 UA rules; only tokenizer/tree-builder name lists | no native rendering; `datalist` suggestions never appear |
| `details` / `summary` | absent | `crates/css/src/user_agent.rs` has no rule; `rg 'details|summary' crates/render/src crates/layout/src` → 0 | closed `<details>` **renders all children** (no toggle, no marker, no `open` semantics) |
| `dialog` | absent | `rg 'showModal|top layer|dialog' crates/render/src crates/shell/src` → 0; no `[open]`/`<dialog>` UA rule in `user_agent.rs` | no top layer, no backdrop, no modality: `<dialog open>` is just an in-flow block |
| `contenteditable`, `draggable`, `inert`, `popover`, `autofocus` | absent | `rg 'contenteditable|draggable|popover|autofocus|inert' crates/*/src` → only `a11y.rs` reading `inert` for focus order (`crates/shell/src/a11y.rs:20`) | no editing, no drag, no popovers; `autofocus` never focuses |
| `table`, `thead/tbody/tfoot`, `tr`, `td/th`, `caption`, `colgroup/col` | full | `crates/layout/src/table.rs:13-17`; `crates/css/src/user_agent.rs:122-160` | good CSS 2.1 tables incl. `colspan`/`rowspan` distribution; presentational attrs missing (below) |
| obsolete `center`, `font`, `big`, `strike`, `tt`, `nobr`, `marquee`, `frameset`, `applet`, `basefont`, `bgsound`, `plaintext`, `xmp` | partial (parse) / absent (render) | parser handles them (`crates/html/src/tree_builder.rs`, obsolete tag list) but `rg 'bgcolor|cellpadding|cellspacing|"valign"|"face"|"color"' crates/css/src/lib.rs crates/layout/src/tree_builder.rs` → no presentational-hint mapping | legacy presentation attributes (`align`, `valign`, `bgcolor`, `cellpadding`, `cellspacing`, `border`, `font size/face/color`, `<center>`) are ignored by the cascade; old pages lose their entire visual intent |
| `svg` (inline), `svg` as image | partial | `crates/render/src/inline_svg.rs:1-20`: inline SVG is serialized to a `data:image/svg+xml` **`<img>`** | CSS from the document does not reach inside the `data:` image (no `fill: currentColor`, no inherited `font-family`, no `:hover` on SVG children); external `<use href>`, CSS-in-SVG interactions limited to what `crates/svg` implements |
| `math` (MathML) | absent | foreign content parsed (`crates/html/src/tree_builder.rs`, MathML name sets) but `rg 'mathml|"math"' crates/render/src` → 0 | math is laid out as ordinary inline HTML text; no fraction/radical/scripts layout |
| `template` | partial | `crates/css/src/user_agent.rs:37-39` (`display:none`) | template **content** is not treated as an inert `DocumentFragment` (no `template.content` semantics in `crates/dom/src/lib.rs`) |
| `noscript` | full (parse) | `crates/html/src/tree_builder.rs:2866-2879`, `:3460` (scripting-aware insertion modes) | scripting disabled/enabled handled at parse time; no separate paint gap |

### 1.1 Global and element attributes

| Attribute | Status | Evidence | Note |
|---|---|---|---|
| `id`, `class`, `style`, `title`, `data-*`, `aria-*`, `role` | full | `crates/dom/src/lib.rs`; `crates/shell/src/a11y.rs:12-25` (role/name/disabled/hidden) | a11y is a minimal model (no computed name algorithm, no live regions) |
| `lang` | partial | parsed/stored; `:lang()` selector unimplemented — `crates/css/src/selector.rs:18-20` lists it as pending | language-dependent typography/quotes unavailable |
| `dir` | **absent** | `rg 'get_attribute\("dir"\)|"dir"\)' crates --glob '!*/tests/*'` → only tree-builder attribute tables; `crates/layout/src/bidi.rs:1-6` runs UAX#9 from *computed* `direction` | `<html dir="rtl">`/`dir="auto"` never sets the paragraph direction: RTL pages render LTR unless the page also sets `direction` in CSS |
| `hidden` | **absent** | no `[hidden]` rule in `crates/css/src/user_agent.rs`; `rg '"hidden"' crates/layout/src crates/css/src crates/render/src` → only `overflow:hidden`/`border-style:hidden` values | `<div hidden>`, `<section hidden>` etc. stay **visible and occupy layout**; `crates/shell/src/a11y.rs:20` does exclude them from the focus order, so a11y and rendering disagree |
| `tabindex` | partial | `crates/shell/src/a11y.rs` deterministic Tab order; `rg 'tabindex' crates/*/src` → no attribute read | tab order is document order; `tabindex` values/`-1` ignored |
| `width`/`height` on `img`/`canvas`/`video` | full | `crates/layout/src/tree_builder.rs:421` (`attribute_number`), `:511` defaults | — |
| `href` URL parsing (`<base>` too) | full | `crates/render/src/stylesheets.rs:1`, `crates/shell/src/links.rs:119` | — |
| `colspan`/`rowspan` | full | `crates/layout/src/table.rs:13-17` | `rowspan=0` handled as 1, overrun clamped |
| presentational table attrs (`border`, `cellpadding`, `cellspacing`, `align`, `valign`, `bgcolor`, `width` on `td`) | absent | see obsolete row above | — |

---

## 2. CSS — property matrix

`ComputedStyle` in `crates/css/src/properties.rs` exposes ~180 `pub` fields
(`properties.rs:1962-2164` lists the applied set). Grouped coverage:

| Group | Supported | Status | Evidence |
|---|---|---|---|
| Box / layout | `display` (inline, block, inline-block, none, flex, grid, list-item, table family), `position` (static/relative/absolute/fixed/sticky), `top/right/bottom/left`, `z-index`, `box-sizing`, `width/height/min/max`, `margin/padding` (+ logical `-block/-inline` shorthands), `overflow`, `float`, `clear`, `visibility`, `opacity` | full (flex/grid detailed below) | `properties.rs:1962-1995`; `crates/layout/src/block.rs:776-782` (clear), `:849` (fixed containing block) |
| Display values | inline/block/inline-block/none/flex/grid/list-item/table family | partial | `crates/css/src/properties.rs:430` — no `contents`, `flow-root`, `ruby*`, `math`, `inline-table` only as table type; `inline-flex`/`inline-grid` are booleans (`:2076`, `:2090`, parse `:2696-2702`) |
| Color / backgrounds | `color`, `background-color`, `background-image` (url + gradients), `-repeat`, `-size`, `-position`, `-origin`, `-clip` (border/padding/content), `-attachment: fixed`, multi-layer backgrounds, gradients (`linear`/`radial`/`conic`, repeating, interpolation spaces incl. oklab) | full / partial | `properties.rs:1998-2030`, `crates/paint/src/background.rs:3-24`; `background-clip: text` parsed but unsupported (`background.rs:16-18`); `background-attachment: local` out of scope (`:25`) |
| Color values | hex, `rgb[a]`, `hsl[a]`, named, `currentColor`, `lab()`, `lch()`, `oklab()`, `oklch()`, `hwb()`, `color()`, `color-mix()` | full | `crates/css/src/color.rs:1-2`; relative color syntax out of scope (`color.rs:7`) |
| Fonts | `font-family` (+ generic families, bundled DejaVu), `font-size`, `font-weight` (100-900), `font-style`, `line-height`, `font` shorthand, `@font-face` (WOFF2 only, 8 faces/8 MiB/32 MiB caps) | partial | `properties.rs:2117-2134`; `crates/render/src/webfonts.rs:1-20`; `unicode-range`, `font-display`, `local()`, `size-adjust` ignored (`crates/css/src/parser.rs:544`, `webfonts.rs:170`) |
| Text | `text-align` (incl. `start`/`end` mapped via `direction`), `text-decoration` line/color, `text-transform`, `text-shadow`, `white-space`, `word-spacing`, `letter-spacing`, `vertical-align`, `direction`/`unicode-bidi`, `text-align: justify` | partial | `properties.rs:2128-2157`, `crates/layout/src/inline.rs:438`, `:948`; **absent**: `text-indent`, `text-overflow` (ellipsis), `word-break`/`overflow-wrap`/`hyphens`, `writing-mode` (`rg 'text-indent|text-overflow|writing-mode' crates/css/src` → 0) |
| Borders / effects | `border-*` (width/style/color incl. dotted/dashed/double/groove/ridge`, `border-radius`, `border-collapse`, `border-spacing`, `box-shadow`, `text-shadow`, `filter` (10 functions), `mix-blend-mode` (6), `backdrop-filter` | full / partial | `properties.rs:1038-1043`, `:1572-1615`, `:2032-2042`; `mix-blend-mode` misses 10 of 16 modes (`properties.rs:1615`), no `border-image`, `outline`, `clip-path`, `mask` (`rg 'border-image|"outline"|clip-path' crates/css/src/properties.rs` → 0) |
| Flexbox | `flex-direction`, `flex-wrap`, `flex-grow/shrink/basis`, `flex` shorthand, `justify-content`, `align-items`, `align-self`, `order`, `gap`/`row-gap`/`column-gap`, margins-as-auto, min/max sizing, percentages | full | `properties.rs:2056-2076`; `crates/layout/src/flex.rs:18-33` |
| Grid | `grid-template-columns/rows`, `grid-auto-rows`, `grid-auto-flow`, `grid-column/row-start/end`, `grid-area`, `grid-template-areas`, `gap`, `justify-items/self`, `justify-content/align-content` | full | `properties.rs:2080-2112`; `crates/layout/src/grid.rs` |
| Tables | `border-collapse`, `border-spacing`, `table-layout`, `caption-side`, `empty-cells`, colspan/rowspan | full | `properties.rs:1991-1995`, `crates/layout/src/table.rs:13-17` |
| Lists / generated content | `list-style-type`, `list-style-image`, list markers (`disc/circle/square`, `Marker` display item) | partial | `properties.rs:2141-2145`, `crates/paint/src/display_list.rs:81-85,152`; **no `content` property and no `::before`/`::after` boxes** — `crates/css/src/selector.rs:18-20` lists pseudo-element boxes as pending; `rg '"content"' crates/css/src` → 0 |
| Transform / transition / animation | `transition` (property/duration/timing/ delay), `animation` (name/duration/timing/delay/iteration/direction/fill/play-state), `@keyframes`, runtime applied once at `--time` | partial | `properties.rs:2162-2164`; `crates/css/src/shorthand.rs:375,474`; `crates/css/src/animation_runtime.rs`; `crates/render/src/lib.rs:405,2018`; **no `transform`/`translate`/`rotate`/`scale` property at all** (`rg '"transform"' crates/css/src` → 0); animation is a single-frame sample, no per-frame playback |
| Container queries | `container-type` (normal/inline-size/size), `container-name`, `@container` | full (size only) | `properties.rs:2047-2049`, `crates/css/src/container.rs`; `style`/`scroll-state` queries documented unsupported |
| Logical properties / RTL | `margin/padding-block/inline` shorthands, `text-align: start/end` | partial | `crates/css/src/shorthand.rs`; no `inset-*`/`border-inline*`/`padding-inline-start`, `direction` only via CSS |

### 2.1 At-rules, selectors, media

| Feature | Status | Evidence | Note |
|---|---|---|---|
| `@media` | partial | `crates/css/src/media.rs:402` | only `width`, `height`, `min/max-width/height`, `orientation` implemented; every other feature → `MediaFeature::Unsupported` which evaluates **false** (`media.rs:445-446`): `prefers-color-scheme`, `prefers-reduced-motion`, `hover`, `pointer`, `resolution`, `aspect-ratio`, `scripting`, `display-mode` blocks are silently dropped |
| `@media print` / print stylesheets | absent | `crates/css/src/media.rs:310-312` parses `Print`, but `crates/css/src/cascade.rs:1075` and `crates/render/src/lib.rs:85,184` always evaluate `screen`; shell has no print path | `@media print` never applies |
| `@import` | full | `crates/css/src/parser.rs:655`, `crates/css/src/cascade.rs:445` | media list on `@import` parsed (`parser.rs:662`) |
| `@font-face` | partial | `crates/css/src/parser.rs:545`, `crates/css/src/cascade.rs:452`, `crates/render/src/webfonts.rs:1-20` | WOFF2 only; descriptors (`unicode-range`, `font-display`, `local()`) ignored (`parser.rs:544`); no TTF/OTF/WOFF fetching |
| `@keyframes` | full (parse) / partial (runtime) | `crates/css/src/parser.rs:459,485`, `crates/css/src/animation_runtime.rs`, `crates/render/src/lib.rs:2018` | sampled once at `--time` (`render/src/lib.rs:405`), not animated over frames; tracked as missing `F2-ANIM-RUNTIME` (issue 941) — *already filed, not re-reported* |
| `@layer` | full | `crates/css/src/cascade.rs:522,460-483,854` | layer order + `revert-layer` |
| `@container` | full (size queries) | `crates/css/src/cascade.rs:510`, `crates/css/src/container.rs` | — |
| `@supports` | **absent** | `crates/css/src/parser.rs:235-...` consumes any at-rule into `AtRule`, but `rg '"supports"' crates/css/src` → 0: unknown at-rules are dropped with their blocks | progressive-enhancement stylesheets lose the whole guarded block instead of the unsupported half |
| `@scope`, `@property`, `@page`, `@namespace`, `@starting-style`, `@counter-style` | absent | same: `rg '"scope"|"property"|"page"|"namespace"|"counter-style"' crates/css/src/parser.rs` → 0 | custom-property registration, page margins, CSS scoping unavailable |
| Selectors: type/class/id/attr/descendant/child/sibling | full | `crates/css/src/selector.rs` | — |
| `:is()`, `:where()`, `:not()`, `:has()` | full / partial | `selector.rs` (all four functional pseudos) | `:has()` relative anchoring/`:scope` limited (`selector.rs:18-20`); no `:lang()`, `:nth-col()` |
| `::before`/`::after`, `::first-line`, `::first-letter`, `::marker`, `::selection`, `::placeholder` | **absent** (`::placeholder` via `:placeholder-shown` only) | `selector.rs:18-20` — "pseudo-element BOXES" pending; no `content` property | decorative icons, clearfixes, quotes, counters that use pseudo-elements render as nothing |
| Dynamic pseudos | partial | `crates/css/src/state.rs`: `:hover` (`data-megi-hover`), `:focus`/`:focus-visible`, `:checked/:disabled/:enabled/:required`, `:placeholder-shown`, `read-only` | `:active`, `:target`, `:visited`, `:valid`/`:invalid`, `:indeterminate`, `:focus-within` absent; untracked pseudos match nothing |
| Custom properties / `var()` | full | `properties.rs:2127`, `crates/css/src/custom.rs` | `var()` fallback and `env()`? (`env()` not probed) |

---

## 3. Missing buckets — consequence on real pages

Format: **missing → what the user sees → what a real page expected**.

1. **Form submission absent** (`crates/shell/src/lib.rs:134`, `crates/shell/src/forms.rs:20`) → pressing Enter or clicking a submit button does nothing, no navigation, no `FormData`, no validation → every login/search/checkout flow is dead on arrival; real pages rely on the browser to POST/GET and navigate.
2. **No `::before`/`::after` + no `content`** (`selector.rs:18-20`) → icon fonts drop, decorative underlines/badges disappear, `clearfix` hacks collapse layouts → most Bootstrap/Tailwind-era pages lose visual structure.
3. **No CSS transforms** (`properties.rs` has no transform field; `rg '"transform"' crates/css/src` → 0) → carousels, rotated icons, centered modals (`translate(-50%,-50%)`), scale-on-hover all static and positionally wrong → modern layouts visibly broken.
4. **No Shadow DOM / custom elements** (`crates/html/src/tree_builder.rs:322` is the only `shadow` hit, `feDropShadow`) → `<my-widget>` renders as an empty inline element; frameworks (Lit, Stencil, many design systems) show blank regions.
5. **`hidden` attribute ignored** (no `[hidden]` UA rule; see §1.1) → menus/dialogs/panels marked `hidden` remain visible and consume height → page looks broken the moment JS toggles `hidden`.
6. **`srcset`/`sizes`/`<picture>` unsupported** (`images.rs:90-113`) → `<img srcset>` with no `src` loads nothing; DPR- and width-appropriate selection never happens → image-heavy pages show blanks or oversized images.
7. **`<iframe>` has no browsing context** (`tree_builder.rs:228-244`) → map/video/payment/login embeds are gray or empty boxes → embedded content (extremely common) is missing.
8. **`<canvas>` has no 2D API** (`rg 'getContext' crates` → 0) → charts, games, signature pads, PDF.js, most dashboards blank.
9. **`<dialog>` + top layer absent** → `<dialog>` content renders inline in the flow (or is missing) → modal designs break; no backdrop, no `showModal`, no `close`.
10. **`<details>/<summary>` absent** → collapsed content is always visible (or renders without marker/toggle) → FAQ/accordion patterns break, often duplicating text.
11. **`video`/`audio` playback absent** → media elements are inert boxes → video sites and audio players unusable.
12. **`@supports` blocks dropped entirely** (`parser.rs:235`+no `"supports"` handler) → pages that guard modern CSS in `@supports (display:grid)` lose the whole rule block (not just the unsupported half) → fallback layouts regress.
13. **`@media` features limited to width/height/orientation** (`crates/css/src/media.rs:402`) → `prefers-color-scheme` dark modes, `hover`/`pointer` touch adaptations and `prefers-reduced-motion` blocks vanish → dark-mode users get the light theme, touch layouts get desktop sizing.
14. **`position: sticky` cannot stick** — `crates/layout/src/block.rs:1379-1385` documents "no scroll container; sticky modelled as stays in normal-flow slot", while the shell **does** scroll vertically (`crates/shell/src/window.rs:15`) → sticky headers/toolbars scroll away → classic sticky-nav pages break while scrolling.
15. **`float` layout is block-granular** (`crates/layout/src/block.rs:10-11`: "a full float-aware inline layout is out of scope") → text does not wrap around floated images → legacy magazine/news layouts, thumbnail+text cards and `float:right` sidebars overlap or leave gaps.
16. **RTL driven only by CSS `direction`** (`bidi.rs:1-6` uses computed style; no `dir` attribute mapping) → `<html dir="rtl">` pages lay out LTR → Arabic/Hebrew pages read in the wrong order/punctuation placement.
17. **Legacy presentational attributes ignored** (`bgcolor`, `align`, `valign`, `font`, `center`, `cellpadding/cellspacing`, `border`) → old intranet/legacy pages lose their entire visual intent.
18. **`overflow: auto/scroll` has no scroll container** (`crates/paint/src/painter.rs:49,528-532` clips only `hidden|clip`) → scrollable panes either overflow visibly or get clipped with no way to scroll.
19. **Inline SVG is flattened to `<img>`** (`crates/render/src/inline_svg.rs:1-20`) → `currentColor` fills, CSS hover, inherited fonts never reach SVG children → themable icons render in their authored colors (often black).
20. **No `text-overflow`/`text-indent`/`writing-mode`/`word-break`** → single-line truncation with ellipsis, indented paragraphs, vertical text and CJK wrapping behave wrong (CJK fonts separately tracked as #1015).
21. **`input` types collapse to text** (`crates/shell/src/forms.rs:82-108`) → date/color/range/file/number controls look and behave like text boxes → booking forms etc. unreadable.
22. **`<select>` renders first option only, no dropdown** (`forms.rs:55`) → choice UIs are unusable.
23. **`mix-blend-mode` only 6 of 16 modes; no `border-image`, `clip-path`, `outline`, `mask`, `@font-face` descriptors** (properties.rs:1615, parser.rs:544) → decorative effects degrade (lower severity, cosmetic).

Non-gaps worth recording (don't regress): HTML/CSS parsing conformance (html5lib + CSS-Syntax WPT pass), flexbox/grid/tables/container-queries/bidi/`color-mix`/gradients all real implementations, `@layer` cascade, image decoders (PNG/JPEG/WebP/GIF) with EXIF, section 508-ish a11y focus order (`crates/shell/src/a11y.rs`), form *rendering* of checkboxes/radios/buttons.

---

## 4. Top 15 gaps ranked by real-web breakage

| # | Gap | Why it ranks here | Cite |
|---|---|---|---|
| 1 | Form submission absent | kills login/search/checkout; no workaround inside the page | `crates/shell/src/lib.rs:134`, `crates/shell/src/forms.rs:20` |
| 2 | CSS transforms absent | layout/position breakage on most modern pages | `crates/css/src/properties.rs` (no field; grep 0) |
| 3 | `::before`/`::after` + `content` absent | icon systems, clearfix, decorations gone | `crates/css/src/selector.rs:18-20` |
| 4 | Shadow DOM / custom elements absent | component-driven sites render blank | `crates/html/src/tree_builder.rs:322` |
| 5 | `hidden` attribute ignored | JS-driven show/hide broken, elements float into layout | no `[hidden]` rule in `crates/css/src/user_agent.rs` |
| 6 | `srcset`/`sizes`/`picture` absent | images missing/oversized on responsive pages | `crates/render/src/images.rs:90-113` |
| 7 | `iframe` without browsing context | embedded maps/video/payment blank | `crates/layout/src/tree_builder.rs:228-244` |
| 8 | `canvas` without 2D context | charts/games/Pdf.js blank | grep `getContext` → 0 |
| 9 | `dialog`/top layer absent | modals render in-flow, no backdrop/modality | grep `showModal|top layer` → 0 |
| 10 | `details`/`summary` absent | accordions/FAQ break, duplicate content | `crates/css/src/user_agent.rs` (no rule) |
| 11 | `video`/`audio` playback absent | media sites unusable | grep `HTMLMediaElement` → 0 |
| 12 | `@supports` blocks dropped | fallback CSS lost wholesale | `crates/css/src/parser.rs:235` (only listed at-rules interpreted) |
| 13 | `@media` feature set (color-scheme/hover/pointer/reduced-motion) | dark mode and touch layouts lost | `crates/css/src/media.rs:402,445-446` |
| 14 | `position: sticky` inert while shell scrolls | sticky headers fail exactly when scrolling | `crates/layout/src/block.rs:1379-1385` vs `crates/shell/src/window.rs:15` |
| 15 | `dir` attribute / RTL propagation absent | RTL pages render LTR | `crates/layout/src/bidi.rs:1-6` (computed `direction` only) |

Next tier (16-23): float block-granularity, RTL/Arabic shaping (#987 filed), presentational
attributes, scroll containers, inline-SVG styling isolation, text-overflow/writing-mode,
input type pickers, `<select>` dropdown, blend/filter/border-image cosmetics.
