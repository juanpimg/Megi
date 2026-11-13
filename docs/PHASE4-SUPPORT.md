# Matriz de Soporte de la Fase 4 (`docs/PHASE4-SUPPORT.md`)

**Objetivo de la Fase 4**: renderizar una página HTML real a una imagen **PNG**, completando el modelo de pintado CSS 2.2 y la tipografía. **Estado global**: Fase 4 **COMPLETADA** y ampliada más allá de la ronda 20 hasta **Pre-Alpha `v0.1.0` + Fases A–D** (**2.383 tests**, gate `PASS` 100 %, **14/14 crates `#![forbid(unsafe_code)]`**): incluye **HITO ronda 11** (árbol HTML5 3.904/3.904), **ronda 12** (layout −14,6 %, encoder PNG −30,5 %), **ronda 13** (`@font-face`+WOFF2 E2E y crate `svg`), **ronda 15–19** (SVG inline, backgrounds multicapa, Flexbox MVP, `border-radius`/`box-shadow`/`text-shadow`, `overflow:hidden`+radius, `background-clip:text`, custom properties/`var()`, `calc()`, gradientes lineales/radiales/repeating, Grid avanzado), **ronda 20–32** (`conic-gradient`/hints/OKLab, `calc()` mixto %+px, `position: sticky`, tipografía/bidi UAX #9, `@layer`/Container Queries, `aspect-ratio`/`object-fit`/`backdrop-filter`, Color 4/5, HTTP/2 completo, JPEG/GIF/WebP VP8 & VP8L, `RenderCache`) y **endurecimiento post-`v0.1.0`** (`calc()` en shrink-to-fit/tablas/floats/inline-block, `VecDeque` en IFC y conformidad WebIDL en `js`).

El pipeline `HTML → CSS → layout → display list → PNG` renderiza texto real, tablas (incluido el texto de sus celdas), `inline-block`, cajas reemplazadas, posicionamiento, floats, `overflow` con clip, stacking/`z-index`/`opacity`, decoraciones/marcadores, **imágenes `<img>` con muestreo bilineal**, **`background-image`/`list-style-image`** (con **`repeat`/`size`/`position` desde la ronda 15**: tiling/clip con cap 4096 y legacy flag para goldens) y comprime PNG con un DEFLATE propio. Quedan `order`/`flex-flow`/`align-content`, grid avanzado (spans/minmax/auto-fill), colores en conflictos de `border-collapse` y block-in-inline. Desde la ronda 9 el pipeline **ejecuta los `<script>` del documento** (`render`, WP-6.2 host); la ronda 10 añade **eventos/timers JS (WP-A)**, hit regions y red avanzada E2E; la ronda 11 añade **WP-B** (CSSOM/`getComputedStyle`/`NodeList`/`matches`/`closest`/identidad), **WP-6.4** (scripts parser-inserted + `document.write` re-tokenizado ⇒ árbol 3.904/3.904) y **event loop presupuestado** en `render` (reloj virtual, lifecycle, timers, re-render único); la **ronda 12** optimiza layout (`Arc<ComputedStyle>` compartido, `Space` sin `String`) y el encoder PNG (filtros fusionados/LZ77 con hashes/`encode_pixels` sin buffer RGBA), con eventos JS avanzados; la **ronda 13** añade **fuentes web `@font-face`+WOFF2 E2E** (dep `woff2-patched` 0.4 solo en `fonts`, DB por render en layout+paint, caps y stats) y el **crate `svg`** desde cero (parser XML+modelo+rasterizador AA, límites anti-DoS), ya **cableado E2E en `render`** (`<img>`/background-image, detección Content-Type/sniffing, intrínseco 300×150 por defecto, cap 16 Mpx/64 MiB RGBA, corrupto ⇒ warning no fatal, `svgs_rendered`); el cierre de ronda arregla `html/fast_hash` (`massive-end-p` ~19 ms); la **ronda 15** añade **backgrounds geometry**, **SVG inline** (`<svg>`→`<img>` sintético, `inline_svgs`), **IFC −28 %** (layout ~36,3 ms, pipeline ~120,5 ms) y **event loop determinista** (sólo reloj virtual).

- Resultados visuales: [`images/phase4-basic.png`](images/phase4-basic.png) · [`images/render-fixture.png`](images/render-fixture.png).
- Plan: [`ROADMAP.md`](ROADMAP.md) §«FASE 4»/«FASE 5» · Estado y deuda: [`HANDOFF.md`](HANDOFF.md) · Rendimiento: [`PERFORMANCE.md`](PERFORMANCE.md).
- Seguridad/fuzzing: [`SECURITY.md`](../SECURITY.md) · [`FUZZING.md`](FUZZING.md).

Leyenda: **HECHO** = implementado y testeado; **PARCIAL** = fundamentos presentes, falta integración/casos; **PENDIENTE** = no implementado.

---

## 1. Modelo de caja y layout (`crates/layout`, 224 tests)

| Capacidad CSS 2.2 | Estado | Notas |
|---|:---:|---|
| Bloques, márgenes, paddings, bordes; cajas anónimas (§9.2.1.1) | **HECHO** | BFC con colapso de márgenes §8.3.1; ICB con viewport |
| `box-sizing`, `min/max`, alturas porcentuales, sobre-restringido | **HECHO** | §10.1/§10.3.3/§10.4/§10.6/§10.7 |
| Auto-colapso de bloques de altura cero (*self-collapsing*) | **PENDIENTE** | Caso menor de §8.3.1 |
| IFC: saltos UAX #14, SHY de ancho cero, `line-height` factor, `vertical-align` §10.8.1, `justify` | **HECHO** | `ifc_conformance2` (14), `fonts_integration` (4) |
| `position: relative/absolute/fixed`; `z-index`/stacking contexts | **HECHO** | §9.3/§9.4.3/§9.6/§9.9; `sticky` documentado como relative (sin scroll) |
| `float`/`clear` §9.5 y BFC que contiene floats §10.6.7 | **HECHO** | Estrechamiento a granularidad de bloque (limitación) |
| `overflow` → BFC + clip rect §11.1 | **HECHO** | El painter emite `PushClip`/`PopClip` |
| **Tablas CSS 2.1 §17**: auto/fixed layout, min/max-content, `border-spacing`, `colspan`/`rowspan`, `inline-table` atómico, `vertical-align` de celdas, caption, row-groups, fixups anónimos y **`border-collapse` con resolución de conflictos** (colores sin resolver) | **HECHO** | fast path separate/auto/sin rowspan (~59 ms); árbol de cajas verificado y fijado con 4 tests (texto de celdas bajo `TableCell`) |
| `inline-block` (shrink-to-fit, baseline, vertical-align §10.8.1) | **HECHO** | 4 tests |
| Cajas reemplazadas (`BoxType::Replaced`) con tamaños intrínsecos (`img` 300×150, `input` 20×20, `textarea` 160×40) | **HECHO** | API `layout_document_with_replaced_sizes`; 6 tests |
| Pintado del contenido de `<img>` (`ImageStore` + `DisplayItem::Image` + bilineal) | **HECHO** | `render` carga/decodifica PNG, inyecta tamaño intrínseco y pinta; placeholder gris no fatal |
| Pintado de `background-image` / `list-style-image` | **HECHO** | paint (`BackgroundImages` + `build_display_list_with_images_and_backgrounds`, bitmap al *padding box*; `list-style-image` como marcador) y render E2E (caps 32/8 MiB/16 MiB); `border-image` PENDIENTE; sin `repeat`/`size`/`position` |
| Texto dentro de celdas de tabla en paint | **HECHO** | Fix ronda 8: `paint_inline_content` no descendía en cajas de tabla; showcase con texto y cabeceras en negrita |
| Block-in-inline | **PENDIENTE** | Fuera del alcance actual |
| `display: list-item` y marcadores (§12.5) | **HECHO** | paint pinta `disc`/`circle`/`square`/`decimal` |
| `visibility: hidden` (§11.2) | **HECHO** | paint no pinta caja ni texto |

## 2. CSS (`crates/css`, 260 tests + 30 WPT)

| Capacidad | Estado | Notas |
|---|:---:|---|
| CSS Syntax L3, Selectores L4 funcionales/estructurales, especificidad, cascada, shorthands, `font`, `@media` básico, índice de reglas (2,1×) | **HECHO** | 30/30 CSS Syntax WPT vendorizado |
| Bordes por lado/shorthands, `border-style: none` ⇒ ancho 0; `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type`; `line_height_factor` | **HECHO** | Bases de Fase 4 |
| Tablas: `display: table-*`/`inline-table`/caption, `border-spacing`, `border-collapse`, `table-layout`, `caption-side`, `empty-cells` + hoja UA §15 | **HECHO** | Propiedades computadas; el layout las consume |
| `@import` (`Stylesheet.imports`) y **Media Queries completas** (parser/evaluador) | **HECHO** | Integrado en `render` con recursión/dedup/ciclos/media real (WP-5.8) |
| Query API (`css::query::{query_selector, query_selector_all, matches}` + `try_*`) y `background-image`/`list-style-image` en `ComputedStyle` | **HECHO** | Pintado en paint+render (ronda 8) |
| **Background geometry (ronda 15)**: `background-repeat` (pares, space/round), `background-size`, `background-position` y shorthand | **HECHO** | Consumido por `paint` (tiling/clip, legacy `background_geometry_explicit`) |
| **Flexbox (ronda 17)**: `display:flex/inline-flex`, `flex-direction/wrap`, `justify-content`, `align-items/self`, `gap`/`row-gap`/`column-gap`, `flex-grow/shrink/basis` y shorthand `flex` | **HECHO** | Consumido por `layout/flex.rs`; grid/`order`/`flex-flow`/`align-content` fuera de alcance |
| **Decoraciones (ronda 17)**: `border-radius` (1-4, slash, longhands/shorthand), `box-shadow` y `text-shadow` + UA styles de formularios | **HECHO** | Radius/sombras pintados por `paint`; UA forms en `user_agent.rs` |
| **Radius y sombras (ronda 17)**: DisplayItems `RoundedRect`/`RoundedBorder`/`Push-PopRoundedGroup`/`BoxShadow`/`TextShadow`; blur determinista de 3 pasadas separables (radio=blur/2), caps 64 px blur y 32 sombras; sin radius/sombras el display list es byte-idéntico | **HECHO** | `paint --test effects` 11 tests; goldens intactos |
| **Clipping real (ronda 18)**: `overflow:hidden`+`border-radius` recorta el subárbol (padding box, radios internos `radius − bordes`, AA 4×4 offscreen, cap 16,7 Mpx); sin clipping el display list es byte-idéntico | **HECHO** | 9 tests `paint --test clipping`; goldens intactos |
| **`background-clip:text` (ronda 18)**: fondo color+imágenes compuesto por máscara de glifos del texto (multlínea, cap 4096 runs) | **HECHO** | `paint --test clipping`; sin clip:text comportamiento byte-idéntico |
| **Gradientes (ronda 19)**: `DisplayItem::Gradient` por tile con AA/sRGB determinista, respetando `background-size/position/repeat` (space/round), `origin/clip`, `border-radius`, `background-clip:text` y opacidad | **HECHO** | 12 tests `paint --test gradients`; sin gradientes el display list es byte-idéntico (goldens intactos); caps de CSS (64 stops/256) |
| **Conic/hints/OKLab (ronda 20)**: rasterizado de `conic-gradient`/`repeating-conic` con AA/sRGB determinista, color hints y espacios sRGB-linear/OKLab; compatibilidad size/position/repeat/origin/clip/radius/clip:text | **HECHO** | `paint --test gradients` (48 tests incl. css+paint); fixtures conic; sin display-p3 |
| **Fast paths de display list (ronda 21)**: retorno barato por caja en `paint_background_border` (28.191 cajas sin fondo/borde/sombra/radio), `DisplayList::with_capacity(16384)`, skip de text-decoration vacío, `first_text_baseline` sin `Vec` y stacks reservados | **HECHO** | A/B build-display 8,62→**7,84 ms** (−9,1 %); goldens intactos |
| **EffectGroup: filtros y blend (ronda 22)**: filtros CSS (blur/brightness/…/drop-shadow) y mix-blend-mode por grupo con buffer offscreen y composición contra backdrop (fórmula CSS) | **HECHO** | caps área 16,7 Mpx/blur 64/funciones 16; fixtures filter_effects/blend_modes/image_set; goldens intactos |
| **Effects analysis (ronda 23)**: `effects_present` agregado por pase + fast path r21-equivalente (sin efectos no se clonan estilos/EffectGroup), DisplayItem cap 20480 e `is_list_item` antes de phase | **HECHO** | build-display **7,34–7,72 ms ≤8** (−4,8 %/−9,6 %) |
| **BackdropFilter/object-fit (ronda 24)**: `DisplayItem::BackdropFilter` (backdrop ya pintado + funciones + blur determinista; caps 64/16/16,7 Mpx) y object-fit con clip al content box y muestredo bilineal | **HECHO** | paint 205 tests; `fill` byte-idéntico; goldens intactos |
| **Pintado Color 4/5 (ronda 25)**: espacios modernos y `color-mix` resueltos a sRGB con matrices/clamp | **HECHO** | paint 208 tests; fixture color_spaces; goldens intactos |
| **Module scripts (ronda 26)**: `<script type=module>` inline/externo con resolver/loader reales, file/http(s)/data, caps por módulo/grafo, fallo no fatal | **HECHO** | render 153 tests; fixtures modules/*.js + module_*.html |
| **Gaps DOM/JS (ronda 27)**: URL/URLSearchParams, dataset, innerText, attributes NamedNodeMap, normalize, compareDocumentPosition, queueMicrotask, idle determinista, Mouse/PointerEvent, focus/blur, atob/btoa, structuredClone | **HECHO** | js 128/dom 36 tests; gaps restantes documentados (geometría sin layout provider, fetch, crypto, Range, NodeList live) |
| **Geometría JS (ronda 28)**: `LayoutProvider` NodeId→LayoutRect, `getBoundingClientRect`/offset/client y `ViewportProvider`; render instala layout pre-script (primer script 0, display:none 0) | **HECHO** | js 130/render 157 tests; sin provider 0 documentado |
| **SVG textPath + SMIL (ronda 29)**: `<textPath>` con glyph advance/startOffset/anchor y `values`+`keyTimes`+`calcMode` spline/keySplines + animación opacity/fill/stroke/color sRGB+alpha | **HECHO** | svg 109 tests; inválidos ignorados byte-idéntico |

| **Backgrounds multicapa (ronda 16)**: `background-origin` (border/padding/content), `background-clip` (text parseado, pinta border-box), `background-attachment` (scroll/fixed), **`background_layers` top-first** (listas cíclicas, shorthand multicapa, color solo última) y `space`/`round` reales | **HECHO** | Espejo legacy para goldens; paint 130 tests |
| **`@font-face` (ronda 13)**: `Stylesheet.font_faces` + `FontFaceRule { family, sources, weight, style }` | **HECHO** | Consumido por `fonts`/`render` para fuentes web (ver §4) |
| **Custom properties/`var()` (ronda 18)**: `--x` en cualquier selector con cascada/herencia/`initial|unset|inherit`, `var(--x[, fallback])` en cualquier valor (incl. shorthands e inline), fallback anidado, ciclos y límite 32 niveles | **HECHO** | `custom.rs`; `ComputedStyle.custom_properties` y `resolve_declarations` |
| **`calc()` (ronda 18)**: + − * / con paréntesis y anidamiento, px/em/rem/%/vh/vw y números; contexto font-size/root/viewport; integrado en `Length::parse` (margin/padding/width/height/font-size/border-width/background-position) | **HECHO** | `calc.rs` (`CalcContext`/`CalcValue`/`evaluate`, `with_calc_viewport`); mezcla %+px ⇒ invalid documentado |
| **Pseudo-clases de estado (ronda 18)**: `:focus`, `:focus-visible`, `:hover`, `:active`, `:checked`, `:disabled`, `:enabled`, `:read-only`, `:read-write`, `:required`, `:placeholder-shown` en compuestos/listas/`:not()`, con `StateContext`/`data-megi-*` | **HECHO** | `state.rs`; `matches_with_state`/`with_state`/`resolve_document_styles_with_media_and_state`; hover/active sin ratón documentados |
| **Grid parsing (ronda 18)**: `display:grid/inline-grid`, `grid-template-columns/rows` (fixed/%/fr/auto/repeat, caps 1024 tracks), gaps y alignment | **HECHO** | Consumido por `layout/grid.rs` (ver §5) |
| **Grid avanzado (ronda 19)**: placement explícito `grid-column/row` + longhands con líneas 1-based/negativas, `span N` y `auto`; auto-placer que salta ocupadas y crece tracks implícitos; `minmax()` (px/%/auto min, px/%/fr/auto max) y `repeat(auto-fill|auto-fit)` (auto-fit = auto-fill, documentado) | **HECHO** | Cap 1024; `order` estable en grid/flex; fixtures `grid_span` (`md5 0c376406…`) y `grid_autofill` (`d2255ca3…`); solapes explícitos no soportados |
| **calc() mixto %+px (ronda 20)**: side table `CalcLengths` junto a `Length` intacto; width/height/min/max, margins/paddings, insets, gaps, background-position/size, border-radius; base indefinida ⇒ %=0; calc anidado em/rem/vh/vw | **HECHO** | Layout 196 tests; fixtures `calc_width`/`calc_margin`/`calc_gap`; `font-size` solo absoluto (documentado) |
| **Sticky y tipografía (ronda 20)**: `position: sticky` (slot de flujo + offsets, top>bottom/left>right, sin afectar hermanos, hook de scroll documentado); `text-transform` en shaping sin mutar DOM; `word-spacing`/`letter-spacing` heredados; `white-space: pre-wrap/pre-line` | **HECHO** | Fixtures `sticky_offsets` (`md5 08d9a63b…`), `text_transform` (`5935cf43…`), `word_spacing` (`a9fcad22…`), `white_space` (`c4134d67…`) |
| **Interning de `ComputedStyle` (ronda 21)**: fingerprint barato + `eq` exacta antes de compartir `Arc` en tree_builder, estilos anónimos perezosos/cacheados por padre, fast path de spacing cero en IFC, `#[inline]` en resolvers y grid por índice | **HECHO** | A/B layout 46,6→**36,4 ms** (−21,8 %, ≤43 ✅), sticky 42,8→32,3; tests layout 206 |
| **Consumo de aspect-ratio (ronda 24)**: w/h derivados en block/replaced/flex/grid, ambos definidos gana definido, replaced auto usa intrínseco y fallback 300×150 | **HECHO** | layout 215 tests; fixture 400×400 `md5 6f021d43…` 6016 B; goldens intactos |
| **Grid completo (ronda 22)**: `grid-auto-flow row/column+dense`, `grid-template-areas`/`grid-area`, líneas nombradas y placement por nombre, `justify/align-content` con stretch y `repeat(auto-fit)` colapsando tracks vacíos | **HECHO** | layout 206 tests; límites: overlaps en orden de documento, auto-fit sólo rango repetido, sin subgrid/masonry |

| **`-webkit-background-clip` (ronda 18)** como alias de `background-clip` | **HECHO** | 5 tests `background_clip_prefixed` |
| **Gradientes CSS (ronda 19)**: `linear-gradient` (ángulos deg/grad/rad/turn, `to side/corner`), `radial-gradient` (circle/ellipse, closest/farthest, radios, posición), `repeating-*` en capas de `background-image` mezclables con `url()`, stops alpha/`currentColor`/0-2 posiciones/hard stops | **HECHO** | Caps 64 stops/256 gradientes por estilo; `BackgroundLayer::Gradient` consumido por paint; fixtures `gradient_linear` (240×220 `md5 6a04fdc6…`)/`gradient_radial`; sin `conic-gradient`/color hints/image-set/border-image |
| **Conic/hints/espacios (ronda 20)**: `conic-gradient`/`repeating-conic` (`from`, `at`, stops angulares, hard stops, alpha, wrap), color hints en linear/radial/conic y `in srgb|srgb-linear|oklab` (default sRGB byte-idéntico) | **HECHO** | Fixtures `conic_gradients` (`md5 fff4b8df…`)/`conic_repeating` (`547c9958…`) y paint `--test gradients`; sin display-p3/espacios avanzados |
| **Cascada + caché de estilos (ronda 21)**: hasher FNV y claves `&str` prestadas en el mapa de ganadores + caché de `ComputedStyle` keyed por 128 bits (identidad de declaraciones + NodeId del padre; sin inline, umbral 2, cap 1024) | **HECHO** | A/B css resolve 38,84→**33,91 ms** (−12,7 %, ≤35 ✅); tests css 175 y gate intactos |
| **Grid completo (ronda 22)**: grid-auto-flow row/column+dense, `grid-template-areas`/`grid-area`, líneas nombradas y placement por nombre, justify/align-content de tracks con stretch, `repeat(auto-fit)` | **HECHO** | layout 206/css 195; fixtures grid_areas `8f9a9eba…`/grid_dense `1789daf8…`/grid_content `123d1a2b…` |
| **Filtros CSS/blend/image-set (ronda 22)**: filter (blur/brightness/contrast/grayscale/invert/opacity/sepia/saturate/hue-rotate/drop-shadow + alias `-webkit`), mix-blend-mode (6 modos) e `image-set()` 1x | **HECHO** | paint `DisplayItem::EffectGroup` offscreen contra backdrop; caps 16,7 Mpx/blur 64/funciones 16; fixtures filter_effects `ffe06406…`/blend_modes `32788e6d…`/image_set `dc638211…` |
| **CSS perf (ronda 23)**: clave de caché por identidad de estilo del padre, move-on-miss y AST UA cacheado (`OnceLock`) | **HECHO** | css resolve 35,81→**26,54 ms** (−25,9 %); 128-bit key/umbral 2/cap 1024 |
| **aspect-ratio/object-fit/backdrop (ronda 24)**: `aspect-ratio` (auto/ratio), `object-fit` (fill/contain/cover/none/scale-down), `object-position` (keywords/%/px) y `backdrop-filter` + alias `-webkit` | **HECHO** | css 210/paint 205; fixtures aspect_ratio `a1e4d950…`/object_fit `b54ddebe…`/backdrop_filter `ff04fac9…` |
| **CSS Color 4/5 (ronda 25)**: `color()` (srgb/srgb-linear/display-p3/a98-rgb/prophoto-rgb/rec2020/xyz), `lab/lch/oklab/oklch`, `hwb()` y `color-mix()` | **HECHO** | css 230/paint 208; fixture color_spaces; matrices CSS Color 4 documentadas, clamp por canal |


## 3. Pintado (`crates/paint`, 208 tests)

| Capacidad | Estado | Notas |
|---|:---:|---|
| Orden CSS 2.2 Apéndice E (stacking contexts, z<0/auto/>0, floats, inline, opacidad) | **HECHO** | 6 tests de stacking |
| `OpacityGroup` offscreen compuesto una vez; `PushClip`/`PopClip` desde `overflow` | **HECHO** | No clipea fondo/borde propios |
| Bordes por lado (`solid/dashed/dotted/double/none/hidden`, esquinas a mitre), fondo bajo el *border box* | **HECHO** | Groove/ridge/inset/outset → sólido |
| Texto real (fuentes integradas), decoraciones (underline/overline/line-through), `visibility`, marcadores | **HECHO** | 5 tests de decoraciones |
| AA exacto, culling y cachés por pasada | **HECHO** | `raster_quality` (14) |
| **Fix de underflow de cobertura parcial** en `fill_rect` (rects <1 px/x fraccional) | **HECHO** | 4 tests de regresión; golden intacto |
| **`background-image`/`list-style-image`** (`BackgroundImages`, `build_display_list_with_images_and_backgrounds`): bitmap estirado al *padding box* (sustituto de `background-size: auto`) y marcador de lista con imagen | **HECHO** | 9 tests; los caps (32 imágenes/8 MiB/16 MiB) los aplica `render` |
| **Texto de celdas de tabla** en el pase de contenido inline | **HECHO** | Fix ronda 8: el arm de tablas no descendía a line/text; golden del showcase regenerado |
| **Background geometry (ronda 15)**: `background_tile_size`/`background_tiles` con cover/contain centrado, tiling con clip al padding box y **cap 4096**; space/round pintan como repeat (documentado); `background_geometry_explicit` mantiene el modo legado y los goldens (`c95c7eff…`, `60f8984e…`, `9e957177…`) | **HECHO** | show case y basic byte-idénticos |
| **Backgrounds multicapa (ronda 16)**: `background-origin`/`clip` (border-box) / `attachment`, capas top-first (`background_layers`), `space`/`round` reales y caps por capa; goldens intactos via `background_geometry_explicit` | **HECHO** | css 76 + paint 130 tests |
| **Encoder PNG −30,5 % (ronda 12)**: filtros fusionados en una pasada, LZ77 con hashes precalculados y match a 8 B, `encode_pixels` sin buffer RGBA intermedio, Adler-32 vectorizable; **bitstream byte a byte idéntico** (PNG 42.853 B `md5 9e957177…`, showcase `md5 c95c7eff…`) | **HECHO** | render-to-PNG 13,30→9,24 ms; goldens sin regenerar; Huffman dinámico queda como follow-up de ratio |
| **Hit-testing aditivo** (ronda 9): `hit_test`/`hit_test_all` (topmost gana), `HitRegion { rect, node_id, kind }` en orden de pintado con clip, `HitLink`/`find_link_at` (sube a `<a href>`) y builders `build_display_list_with_hit_regions`/`build_display_list_and_hit_regions` | **HECHO** | 11 tests nuevos; APIs clásicas y golden **byte-idénticos**; ronda 10: `render` los construye en `RenderOutput.hit_regions` (+0,6 µs, 121 regiones/80 items) y el shell usa `find_link_at`; granularidad caja/run, sin `pointer-events`/transforms |
| **Compresión PNG propia** (DEFLATE LZ77+Huffman fijo, zlib/Adler, filtros adaptativos) | **HECHO** | **1.920.813 → 15.498 B (123,9×)**, `md5 7324e851…`, `basic.rgba` idéntico |
| Painter iterativo con caps (stacking 256, floats 64, items 5M) | **HECHO** | Probado a 20.000 niveles |
| Codificador PNG determinista + PPM de depuración | **HECHO** | Golden byte a byte |
| **Imágenes `<img>`** (`ImageStore = HashMap<NodeId, Arc<RgbaImage>>`, `DisplayItem::Image`, `build_display_list_with_images`) con raster bilineal clamp-to-edge, AA/alfa/clip y placeholder gris | **HECHO** | 7 tests de imágenes |
| API de navegador en el DOM (`getElementById`, `innerHTML`, `document.write`, mutaciones con `DomError`) | **HECHO** | `dom` 28 + `html` 53 tests |

## 4. Tipografía y fuentes web (`crates/fonts`, 17 tests + doctest)

| Capacidad | Estado | Notas |
|---|:---:|---|
| fontdb + ttf-parser + rustybuzz + ab_glyph; fallback, métricas, shaping LTR, máscaras de glifo | **HECHO** | Integrado en layout/paint; fuente DejaVu determinista |
| `font-weight`/`font-style`, baseline en la display list | **HECHO** | — |
| **Fuentes web `@font-face`/WOFF2 (ronda 13)**: `fonts::decode_web_font`/`is_woff2` (WOFF2→sfnt con **`woff2-patched` 0.4**, Apache-2.0, pure Rust sin C/FFI), `FontDatabase::load_font_data_family(alias, sfnt)`; `render` recoge las reglas de todas las hojas (incl. `@import`), deduplica, descarga con `ResourceLoader` (`--no-network`, caps **8 fuentes/8 MiB/32 MiB**) y pasa una **DB por render** a layout+paint, con stats `fonts_loaded/failed/bytes` y `timings.fonts` | **HECHO** | Fixture Open Sans OFL (18.640 B): webfont `md5 f38aed032b8d160075097f1e4ad83b49` vs fallback `e2d76dee98f7c4ef2896c5d003be74f2`; fallos ⇒ warning no fatal |
| Límites web fonts: sin WOFF1, primer `src` soportado, weight/style no eligen cara, sin unicode-range/font-display (bloqueante), sin subsetting/color fonts, DB no cacheada | **PENDIENTE** | Próximas rondas |
| Bidi, scripts complejos, fuentes variables | **PENDIENTE** | Fuera de alcance de Fase 4 |
| **Crate `svg`** (`crates/svg`, **73 tests**, rondas 13/16/17): CSS interno, gradientes, **`<pattern>`** (cap 10k tiles) y **`<text>` básico** (fonts bundled, caps 1.000 texts/10.000 chars); parser XML subset + modelo SVG + rasterizador vectorial AA determinista (shapes, path con arcos, transforms, viewBox, estilos, opacidad; límites anti-DoS; `examples/svg_to_png`). NO soportado: `<text>`, CSS class/style sheets, gradientes/patterns, dasharray, markers, filtros/máscaras/clips, script y refs externas | **HECHO (crate + E2E)** | Cableado en `render` para `<img src=*.svg>` y `background-image`: detección Content-Type/sniffing, tamaño intrínseco (300×150 por defecto), cap 16 Mpx/64 MiB RGBA, corrupto ⇒ warning no fatal, navegación directa en página sintética y stat `svgs_rendered`; 5 fixtures `svg_*` + 11 tests |

## 5. Documento, imagen y red (Fase 5, ~100 %)

| Capacidad | Estado | Notas |
|---|:---:|---|
| URL WHATWG subset, HTTP/1.1 (+ streaming incremental y keep-alive `H1Pool`), **TLS 1.2/1.3** (`rustls`+`webpki-roots`), caché HTTP RFC 9111 subset, `data:`, `file://`, MIME, `ResourceLoader` | **HECHO** | `crates/net`, **331+ tests offline**; cookies/CORS/gzip/deflate streaming E2E; SameSite+PSL real vendored (MPL-2.0), Referrer-Policy, Range/206, trailers y proxy HTTP/HTTPS |
| **JPEG/GIF (ronda 22)**: JPEG baseline y GIF87a/89a desde cero, sniffing PNG|JPEG|GIF en render, PNG decode optimizado (−62,4 %) | **HECHO** | `crates/image` **111 tests** / `crates/render` **175+ tests** |
| **HTTP/2 MVP (ronda 22)**: HPACK completo, framing con caps 16 KiB/64 KiB/2 streams, ALPN h2 + fallback h1 | **HECHO** | Integrado en `net`; multiplexado, flow control, SETTINGS, CONTINUATION, push priority (`ServerWeights`/`Fifo`) y trailers |
| **HTTP/2 avanzado (ronda 23+)**: flow control conexión/stream, multiplexado ≥2 streams, SETTINGS avanzados con ACK, HEADERS+CONTINUATION, HPACK compartido, push priority y trailers | **HECHO** | Loopback tests en `net`; pendientes h2c/request bodies streaming |
| **EXIF/JPEG progresivo/WebP (ronda 23)**: orientación EXIF 1–8 aplicada en decode, subset progresivo SOF2, WebP VP8L + VP8 lossy | **HECHO** | `crates/image` **111 tests**; fixtures exif_o1..o8 y `image_formats.html` |
| **JPEG progresivo color + WebP VP8L (ronda 24)**: progresivo de color completo (Δmáx ≤2, gris Δ0) y VP8L subset (RIFF/header/Huffman simple/literales ARGB) | **HECHO** | `crates/image`; fixtures v8l_* y `webp.html` |
| **WebP VP8L + VP8 lossy completos (rondas 25/31)**: predictor 14, color transform 3.5, subtract-green, color indexing, Huffman complejo/meta, LZ77 y keyframe VP8 | **HECHO** | `crates/image`; fixtures reales píxel-exactas vs `.rgba` |
| **Decode escalado (ronda 26)**: `decode_scaled`/`--max-image-dim`, PNG unfilter+Adam7 sin buffer completo y post-subsample JPEG/GIF/WebP | **HECHO** | `crates/image`; PNG full ×2/−28 %, ×8/−36 %; stage images −30 % |
| **Cookies RFC 6265** (`CookieJar`, cap 4.096), **CORS básico**, **gzip/deflate** (caps 64 MiB + ratio 1000:1), **SameSite** (Strict/Lax/None+Secure), **PSL real (MPL-2.0)** y **Referrer-Policy** (8 políticas, header>meta>default) instalados en el loader | **HECHO** | Ronda 8 + ronda 18 + ronda 19 (`ReferrerConfig` cableado en `render`) + PSL real vendored |
| **Decoder PNG propio + inflate zlib/DEFLATE** (color types 0/2/3/4/6, bit depth 1–16, filtros 0–4, Adam7, CRC, `PngLimits`) | **HECHO** | `crates/image`, **111 tests** (PNG/JPEG/GIF/WebP/EXIF, incluidos VP8X/ALPH y animado ANIM/ANMF) |
| **Pipeline de navegador** `URL → PNG` con CSS externo: charset UTF-8/Windows-1252, `<base>`, `<style>`+`<link rel=stylesheet>` (dedup/media/caps), cascada UA+autor, layout con fuentes y backgrounds, PNG; **`RenderOutput.hit_regions`**, **red avanzada E2E** (cookies por render/sesión, CORS/credentials con denegación no fatal, gzip/deflate, `--cookies`/`--origin`) y **event loop presupuestado** (reloj virtual, `DOMContentLoaded`/`load`, timers, re-render único) | **HECHO** | `crates/render`, **175+ tests**; webfonts E2E, SVG inline, `getComputedStyle` pre-script, matchMedia/viewport, rAF, `readyState`, `onerror`/`console`, clipping, `ReferrerConfig`/SameSite y gradientes |
| **Ejecución de JavaScript en `render`** (Fase 6 WP-6.2 host): `ScriptingMode`/`RenderOptions.scripting` (default Enabled con feature `js`, `--no-scripting`), scripts clásicos inline/`src` y `<script type=module>` con TLA, caps 8 MiB/script + 32 MiB total + 64 scripts, budget por script+total, console/stats y **re-render estilos→layout→paint del DOM mutado**; **event loop** (`DOMContentLoaded`/`load`, timers con reloj virtual) | **HECHO** | Fixtures `js_*` y `js_event_*`; el parser ejecuta además los scripts parser-inserted (WP-6.4, gate 3.904/3.904) |
| `<img>` de extremo a extremo (fetch + decodificación + pintado + placeholder) | **HECHO** | WP-5.7; fixture `images.html` 800×600 → 15.625 B `md5 1de31699…` |
| `background-image` de extremo a extremo | **HECHO** | WP-5.9: fetch+decode+pintado offline/HTTP, caps 32 imágenes / 8 MiB / 16 MiB y stats/warnings; `repeat`/`size`/`position` reales |
| `@import` recursivo + media real en `render` | **HECHO** | WP-5.8: dedup/ciclos/profundidad 8/media screen |
| `br` (Brotli)/stacked encodings, HTTP/1.0, proxy PAC/SOCKS y CORS completo | **PENDIENTE** | HTTP/2, streaming H1/inflate, PSL real, SameSite, Referrer-Policy y Range ya HECHOS |
| Ventana/eventos/scroll/zoom | **PARCIAL** | Fase 7: ventana+scroll+inputs+HiDPI+worker+navegación+zoom+auto-refresh hechos; formularios/pestañas/event-loop real/eventos DOM pendientes |

## 6. DOM generacional + API de navegador (`crates/dom`, 36 tests)

| Capacidad | Estado | Notas |
|---|:---:|---|
| `NodeId` 4 B (20 bits slot + 12 generación), free list LIFO, reutilización de slots | **HECHO** | Máx. 1.048.575 slots |
| `remove_node`/`remove_subtree`; handles stale/foráneos rechazados | **HECHO** | `try_node`/`owns`; panic claro en `node()` |
| `descendants`/`ancestors` iterativos con guardas de ciclo; `validate_invariants`; `estimate_bytes` | **HECHO** | Probado con 100k nodos |
| API de navegador: `document_element/head/body`, `get_element_by_id`, `get_elements_by_tag_name/class_name`, `text_content`, mutaciones de hijos/atributos, `DomError` | **HECHO** | 28 tests dom |

## 7. Fuera del alcance de la Fase 4

Formularios interactivos, `flex`/`grid`, `@font-face`, bidi/scripts complejos, eventos DOM/foco y formularios del host (Fase 7), animaciones, `border-radius`, `box-shadow`, gradientes y medios paginados. JavaScript está en Fase 6 (ver §8); hit-testing y navegación ya existen (rondas 9–10).

## 8. Fase 6: JavaScript (`crates/js`, 168 tests + scripting E2E en `render`/parser)

| **ES modules (ronda 25)**: `eval_module` con resolver/loader, imports default/named/namespace/side-effect, exports const/let/var/function/class/default/{as}, `import.meta.url`, orden topológico, ciclos `ModuleCycle` y errores tipados | **HECHO** | js 96 tests; subset line-based documentado (sin import() dinámico/top-level await/export-from/live bindings) |
| **SVG filters + SMIL (ronda 26)**: `feColorMatrix`/`feBlend`/`feMerge`/`feComposite` arithmetic con cadenas in/in2/result y `<animate>/<set>/<animateTransform>` determinista (`render_svg_with_time`) | **HECHO** | svg 97 tests; sin animación byte-idéntico |

| Capacidad | Estado | Notas |
|---|:---:|---|
| Runtime **Boa 0.21.1** con feature `js` ON por defecto (stub `--no-default-features`), realm persistente, `eval`, `console.*`, `window`/`document` | **HECHO** | WP-6.1; `#![forbid(unsafe_code)]` |
| Bindings DOM (`document.write`, `getElementById`, `querySelector(All)`, `innerHTML`, `style`, `classList`, árbol, atributos) sobre side-table `NodeId↔handle` generacional | **HECHO** | identidad de wrappers cacheada (WP-B) |
| **WP-A: eventos y timers (ronda 10)**: `EventTarget` (`addEventListener`/`removeEventListener`/`dispatchEvent`) con captura window→document→ancestros→target y burbuja; `Event`/`CustomEvent` (`bubbles`/`cancelable`/`detail`/`defaultPrevented`, prevent/stop/stopImmediate); timers con **reloj inyectable** (`setTimeout`/`setInterval`/clear, orden `(due, id)`, `advance`/`run_until`/`now_ms`/`pending_timers`; `run_tasks` ejecuta vencidos por reloj real) | **HECHO** | 14 tests nuevos (26 js); límites: sin `onclick`/`once`/`passive`, `currentTarget` null en listener, `removeEventListener` sin identidad |
| Anti-DoS: `Budget` → Boa `RuntimeLimits` (bucle 5.000.000, recursión 512, pila VM 20.480; `Budget::SMALL` en páginas/tests), sin APIs de red/FS, side-table cap 1.000.000; scheduler base `setTimeout`/microtasks | **HECHO** | Violaciones → `JsError::Budget`; sin event loop completo |
| **Integración en `render` (WP-6.2 host)**: `ScriptingMode`/`--no-scripting`, `ScriptingLimits`/`ScriptBudget`, descubrimiento y ejecución de scripts post-parseo, caps 8/32 MiB y 64 scripts, microtasks/timers, console/stats y re-render del DOM mutado | **HECHO** | 7 fixtures `js_*`; `js_text.html` 15.876 B `md5 e1da51d4…` vs 15.499 B `md5 53f67e77…` |
| **Eventos avanzados (ronda 12)**: `currentTarget` real y `eventPhase` 1/2/3 (capture en el target), `once`/`passive`, `removeEventListener` por identidad, handlers inline `onclick`/`onload`/`onerror`, `document.close()` no-op, `estimated_heap_bytes()` aproximado, dispatch no-`Event` ⇒ `TypeError` y excepciones de listeners capturadas (`last_event_error()`) | **HECHO** | 58 tests js; Fase 6 ≈80 % |
| **WP-B (ronda 11)**: `CSSStyleDeclaration` completa (`cssText`/`length`/`item`/`getPropertyValue`/`setProperty`/`removeProperty`), `getComputedStyle` con provider inyectable, `NodeList` snapshot, `matches`/`closest`, `insertAdjacentHTML`/`Text` e identidad de wrappers; `document` como `EventTarget`; APIs Rust `eval_on_document`/`dispatch_simple_event`/`take_document` | **HECHO** | Fase 6 ≈80 % |
| **WP-6.4 (ronda 11)**: hook `html::scripting::{ScriptExecutor, ScriptExecution}`; scripts parser-inserted al `</script>`/EOF y `document.write` **re-tokenizado en la posición actual** (scripts anidados incluidos); externos `src` los ejecuta el host; errores no fatales; camino sin scripting zero-cost | **HECHO** | **árbol 3.904/3.904 (100 %), allowlist de tree vacía** |
| **APIs de navegador avanzadas (rondas 27–32)**: `URL`/`URLSearchParams`, `History`/`Web Storage`, `MutationObserver`, `LayoutProvider` (`getBoundingClientRect`/offset/client), `fetch()` con `AbortController`/`ReadableStream`, `window.onerror`/`document.onerror` y Web Workers cooperativos | **HECHO** | Cableados en `render` |
| Pendientes F6: conformance JS/DOM extendida, contabilidad fina de memoria, `fetch` streaming completo y workers con DOM | **PENDIENTE** | Fase 6 ≈98 % |

## 9. Fase 7: Shell y ventana del sistema (`crates/shell`, 238 tests)

| Capacidad | Estado | Notas |
|---|:---:|---|
| Ventana nativa `winit` 0.30 + presentación `softbuffer` 0.4 (`0RGB`, X11/Wayland dlopen, sin GPU) | **HECHO** | Cierre/q/Esc/r, resize con debounce 150 ms, errores a stderr+frame blanco |
| Modo headless `--screenshot` sin `EventLoop` (CI) + CLI `megi` + `tools/megi.sh` | **HECHO** | 640×400 `md5 7324e851…` idéntico a `render` |
| **Scroll vertical** (rueda/teclado/paginación, cap 8192 CSS px `[recortado]`), **entrada** ratón/teclado y **HiDPI** (CSS=physical/scale) | **HECHO** | Foco pendiente |
| **Render en worker** con generación/cancelación/coalescing/debounce (120 ms) y `EventLoopProxy` | **HECHO** | — |
| **Navegación real (WP-7.5)**: clic en enlaces (`Url::join`), historial (`NavigationState`: back/forward/replace/reload, cap 256, truncado forward), Alt+←/Backspace/b, Alt+→/n, `r`, título dinámico `<title>`+URL+% scroll+`[recortado]`, errores y redirects replace; **hit regions nativos** y CLI `--cookies`/`--origin` | **HECHO** | 34 tests nuevos; hit regions y título del mismo render |
| **Zoom / auto-refresh / estado (ronda 13, WP-7.7)**: zoom `Ctrl+=/-/0` y `--zoom` 0.5–3.0 (escalones 50–300 %, viewport CSS = físico/(scale·zoom)), `Ctrl+L` imprime URL, auto-refresh opt-in `--refresh-ms [MS]` (solo con scripts+timers pendientes, worker generacional, scroll conservado, sin bucles) y título con zoom/scroll/warnings/errores | **HECHO** | `--zoom 2` (640×400→320×200) `md5 c11903cfe0c4f7095289052d25750bd3`; basic `7324e851…` intacto |
| **Formularios editables (ronda 17)**: UA styles, `src/forms.rs` (FocusState, TextEdit UTF-8, snapshot `data:` URL + patch/re-render, clic/teclado/Tab/Esc/checkbox/radio, Enter nunca envía) | **HECHO** | Fixture `forms.html` 640×480 `md5 b8b9a20…` |
| **Find-in-page (ronda 19)**: `Ctrl+F` case-insensitive sobre nodos Text, Enter/Shift+Enter con wrap, Esc, scroll centrado al hit, overlays amarillos (0.45/0.75) pintados en framebuffer y `--find QUERY --screenshot`; navegación F5/g reload, Alt+Home y Ctrl+D | **HECHO** | `src/find.rs` puro; golden basic `7324e851…` intacto |
| **Pestañas (ronda 20)**: `tabs.rs` puro con `Tab { navigation,title,url,scroll,zoom,find }` y `TabsState { new,from_urls,open_tab,activate,next,previous,close }`; MAX 32, cerrar la última = New Tab blank; `Ctrl+T/W/Tab/Shift+Tab`, `Alt+1..9`, título `[N/M]`, `--tabs URL` repetible, frames por generación | **HECHO** | shell **140** tests (+17); límites: sin barra visual/reordenar/sesión/aislamiento/reopen |
| Pestañas, widgets nativos/select dropdown, validación/submit y event loop real | **PENDIENTE** | Fase 7 |
| **Hit-testing** (`NodeId`/`LayoutBox` ↔ `DisplayItem`) | **HECHO (paint + render)** | API aditiva `HitRegion`/`hit_test`/`find_link_at`; ronda 10: `RenderOutput.hit_regions` y el shell resuelve enlaces con `find_link_at` (fallback layout+DOM), +0,6 µs |

| Tests de ventana reales en CI (Xvfb) | **PENDIENTE** | El host de desarrollo no tiene servidor gráfico |

## 10. Verificación de la matriz

```bash
./tools/dev.sh all                             # fmt, clippy, 1929 tests, doc y gate PASS
cargo test -p css -p layout -p paint -p fonts -p dom -p html
cargo test -p net -p render -p image -p shell
cargo test -p js
cargo test -p js --no-default-features
cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768
cargo run -p render -- tests/fixtures/render/svg_inline_simple.html /tmp/inline.png --json   # inline_svgs>0
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/gradient.svg /tmp/gradient.png
cargo run -p render -- tests/fixtures/render/background_layers.html /tmp/layers.png --width 240 --height 260   # c357cc2c…
cargo run -p render -- tests/fixtures/render/browser_api.html /tmp/api.png --json
cargo run -p render -- tests/fixtures/render/flex_row.html /tmp/flex_row.png
cargo run -p shell -- tests/fixtures/render/forms.html --screenshot /tmp/forms.png --width 640 --height 480
cargo test -p paint --test effects
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/pattern.svg /tmp/pattern.png
cargo run -p render -- tests/fixtures/render/backgrounds.html /tmp/bg.png
cargo run --release -p bench -- --filter layout --iterations 5     # ~36,3 ms
cargo run --release -p bench -- --filter pipeline --iterations 3   # ~120,5 ms
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_on.png    # JS ejecutado (mutaciones en el PNG)
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_off.png --no-scripting
cargo test -p paint --test hit_testing
cargo test -p svg                              # crate SVG (60 tests)
cargo test -p render --test svg_tests          # SVG E2E en render (11 tests)
cargo run -p render -- tests/fixtures/render/svg_img.html /tmp/svg_img.png --json   # svgs_rendered=1
cargo test -p fonts -p css -p render          # fuentes web @font-face/WOFF2 E2E
cargo run -p render -- tests/fixtures/render/webfont.html /tmp/webfont.png --json   # md5 f38aed03… vs e2d76dee… (fallback)
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/zoom2.png --width 640 --height 400 --zoom 2  # md5 c11903cf…
cargo run -p test_harness -- --summary          # gate: árbol 3904/3904 (100 %), 230/230 serializer, 30/30 CSS; exit 0
cargo test -p render --test event_loop_tests   # event loop presupuestado (6 fixtures js_event_*)
cargo test -p html --lib scripting             # hook WP-6.4 (scripts parser-inserted)
cargo run --release -p bench -- --filter layout   # ronda 12: ~51-53 ms, LayoutBox 176 B
cargo run --release -p bench -- --filter paint    # ronda 12: encoder PNG −30,5 % invariante
cargo run -p render -- https://example.com/ /tmp/page.png --cookies --origin https://example.com --json   # red avanzada E2E
cargo run -p shell -- tests/fixtures/render/showcase/index.html              # navegación (clic, Alt+←/→, b/n, r, q)
cargo run -p render -- tests/fixtures/render/images.html docs/images/images-e2e.png --width 640 --height 400
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/shot.png
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/phase4.png
python3 tools/png_check.py /tmp/phase4.png
./tools/bench.sh
```

Si detectas una discrepancia entre esta matriz y el comportamiento real, es un bug de documentación o de implementación: abre un issue o actualiza el WP correspondiente de [`ROADMAP.md`](ROADMAP.md).
