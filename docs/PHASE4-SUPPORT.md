# Matriz de Soporte de la Fase 4 (`docs/PHASE4-SUPPORT.md`)

**Objetivo de la Fase 4**: renderizar una página HTML real a una imagen **PNG**, completando el modelo de pintado CSS 2.2 y la tipografía. **Estado global: ~100 % (funcionalmente cerrada; ronda 8).**

El pipeline `HTML → CSS → layout → display list → PNG` renderiza texto real, tablas (incluido el texto de sus celdas), `inline-block`, cajas reemplazadas, posicionamiento, floats, `overflow` con clip, stacking/`z-index`/`opacity`, decoraciones/marcadores, **imágenes `<img>` con muestreo bilineal**, **`background-image`/`list-style-image`** (bitmap estirado al *padding box*; sin `repeat`/`size`/`position`) y comprime PNG con un DEFLATE propio. Quedan `background-repeat`/`size`/`position`, colores en conflictos de `border-collapse` y block-in-inline.

- Resultados visuales: [`images/phase4-basic.png`](images/phase4-basic.png) · [`images/render-fixture.png`](images/render-fixture.png).
- Plan: [`ROADMAP.md`](ROADMAP.md) §«FASE 4»/«FASE 5» · Estado y deuda: [`HANDOFF.md`](HANDOFF.md) · Rendimiento: [`PERFORMANCE.md`](PERFORMANCE.md).
- Seguridad/fuzzing: [`SECURITY.md`](../SECURITY.md) · [`FUZZING.md`](FUZZING.md).

Leyenda: **HECHO** = implementado y testeado; **PARCIAL** = fundamentos presentes, falta integración/casos; **PENDIENTE** = no implementado.

---

## 1. Modelo de caja y layout (`crates/layout`, 133 tests)

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

## 2. CSS (`crates/css`, 57 tests + 30 WPT)

| Capacidad | Estado | Notas |
|---|:---:|---|
| CSS Syntax L3, Selectores L4 funcionales/estructurales, especificidad, cascada, shorthands, `font`, `@media` básico, índice de reglas (2,1×) | **HECHO** | 30/30 CSS Syntax WPT vendorizado |
| Bordes por lado/shorthands, `border-style: none` ⇒ ancho 0; `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type`; `line_height_factor` | **HECHO** | Bases de Fase 4 |
| Tablas: `display: table-*`/`inline-table`/caption, `border-spacing`, `border-collapse`, `table-layout`, `caption-side`, `empty-cells` + hoja UA §15 | **HECHO** | Propiedades computadas; el layout las consume |
| `@import` (`Stylesheet.imports`) y **Media Queries completas** (parser/evaluador) | **HECHO** | Integrado en `render` con recursión/dedup/ciclos/media real (WP-5.8) |
| Query API (`css::query::{query_selector, query_selector_all, matches}` + `try_*`) y `background-image`/`list-style-image` en `ComputedStyle` | **HECHO** | Pintado en paint+render (ronda 8), sin repeat/size/position |

## 3. Pintado (`crates/paint`, 97 tests)

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
| **Compresión PNG propia** (DEFLATE LZ77+Huffman fijo, zlib/Adler, filtros adaptativos) | **HECHO** | **1.920.813 → 15.498 B (123,9×)**, `md5 7324e851…`, `basic.rgba` idéntico |
| Painter iterativo con caps (stacking 256, floats 64, items 5M) | **HECHO** | Probado a 20.000 niveles |
| Codificador PNG determinista + PPM de depuración | **HECHO** | Golden byte a byte |
| **Imágenes `<img>`** (`ImageStore = HashMap<NodeId, Arc<RgbaImage>>`, `DisplayItem::Image`, `build_display_list_with_images`) con raster bilineal clamp-to-edge, AA/alfa/clip y placeholder gris | **HECHO** | 7 tests de imágenes |
| API de navegador en el DOM (`getElementById`, `innerHTML`, `document.write`, mutaciones con `DomError`) | **HECHO** | `dom` 28 + `html` 53 tests |

## 4. Tipografía (`crates/fonts`, 13 tests + doctest)

| Capacidad | Estado | Notas |
|---|:---:|---|
| fontdb + ttf-parser + rustybuzz + ab_glyph; fallback, métricas, shaping LTR, máscaras de glifo | **HECHO** | Integrado en layout/paint; fuente DejaVu determinista |
| `font-weight`/`font-style`, baseline en la display list | **HECHO** | — |
| Bidi, scripts complejos, fuentes variables | **PENDIENTE** | Fuera de alcance de Fase 4 |

## 5. Documento, imagen y red (Fase 5, ~90 %)

| Capacidad | Estado | Notas |
|---|:---:|---|
| URL WHATWG subset, HTTP/1.1, **TLS 1.2/1.3** (`rustls`+`webpki-roots`), caché HTTP RFC 9111 subset, `data:`, `file://`, MIME, `ResourceLoader` | **HECHO** | `crates/net`, 180 tests offline |
| **Cookies RFC 6265** (`CookieJar`, cap 4.096, `FetchOptions.cookie_jar`), **CORS básico** (Origin/ACAO/credentials/validación) y **gzip/deflate** (`Accept-Encoding: gzip` por defecto, CRC32/ISIZE, `CompressionLimits`: 64 MiB + ratio 1000:1) | **HECHO** | Ronda 8; SameSite/PSL, CORS completo y HTTP/2 pendientes |
| **Decoder PNG propio + inflate zlib/DEFLATE** (color types 0/2/3/4/6, bit depth 1–16, filtros 0–4, Adam7, CRC, `PngLimits`) | **HECHO** | `crates/image`, 45 tests + 1 doctest; GIF/JPEG/WebP solo detectados |
| **Pipeline de navegador** `URL → PNG` con CSS externo: charset UTF-8/Windows-1252, `<base>`, `<style>`+`<link rel=stylesheet>` (dedup/media/caps), cascada UA+autor, layout con fuentes y backgrounds, PNG | **HECHO** | `crates/render`, 53 tests; fixture 640×400 → 15.498 B `md5 7324e851…`; showcase 1024×768 → 80.220 B `md5 c95c7eff…` (ronda 8, con texto de tablas y backgrounds) |
| `<img>` de extremo a extremo (fetch + decodificación + pintado + placeholder) | **HECHO** | WP-5.7; fixture `images.html` 800×600 → 15.625 B `md5 1de31699…` |
| `background-image` de extremo a extremo | **HECHO** | WP-5.9: fetch+decode+pintado offline/HTTP, caps 32 imágenes / 8 MiB / 16 MiB y stats/warnings; `repeat`/`size`/`position` pendientes |
| `@import` recursivo + media real en `render` | **HECHO** | WP-5.8: dedup/ciclos/profundidad 8/media screen; faltan `layer()`/`supports()`/`@font-face` |
| `Referrer-Policy`, `br`/stacked encodings, HTTP/2 y streaming; SameSite/PSL y CORS completo | **PENDIENTE** | WP-5.9 (cookies/CORS/gzip básicos ya HECHO) |
| Ventana/eventos/scroll/hit-testing | **PENDIENTE** | Fase 7 (`crates/shell`) |

## 6. DOM generacional + API de navegador (`crates/dom`, 28 tests)

| Capacidad | Estado | Notas |
|---|:---:|---|
| `NodeId` 4 B (20 bits slot + 12 generación), free list LIFO, reutilización de slots | **HECHO** | Máx. 1.048.575 slots |
| `remove_node`/`remove_subtree`; handles stale/foráneos rechazados | **HECHO** | `try_node`/`owns`; panic claro en `node()` |
| `descendants`/`ancestors` iterativos con guardas de ciclo; `validate_invariants`; `estimate_bytes` | **HECHO** | Probado con 100k nodos |
| API de navegador: `document_element/head/body`, `get_element_by_id`, `get_elements_by_tag_name/class_name`, `text_content`, mutaciones de hijos/atributos, `DomError` | **HECHO** | 28 tests dom |

## 7. Fuera del alcance de la Fase 4

Formularios interactivos, `flex`/`grid`, `@font-face`, bidi/scripts complejos, scroll/hit-testing/eventos (Fase 7), animaciones, `border-radius`, `box-shadow`, gradientes y medios paginados. JavaScript ya está iniciado en Fase 6 (ver §8).

## 8. Fase 6: JavaScript (`crates/js`, 12 tests)

| Capacidad | Estado | Notas |
|---|:---:|---|
| Runtime **Boa 0.21.1** con feature `js` ON por defecto (stub `--no-default-features`), realm persistente, `eval`, `console.*`, `window`/`document` | **HECHO** | WP-6.1; `#![forbid(unsafe_code)]` |
| Bindings DOM (`document.write`, `getElementById`, `querySelector(All)`, `innerHTML`, `style`, `classList`, árbol, atributos) sobre side-table `NodeId↔handle` generacional | **HECHO** | 12 tests; sin caché de identidad de wrappers |
| Anti-DoS: `Budget` → Boa `RuntimeLimits` (bucle 5.000.000, recursión 512, pila VM 20.480; `Budget::SMALL` en páginas/tests), sin APIs de red/FS, side-table cap 1.000.000; scheduler base `setTimeout`/microtasks | **HECHO** | Violaciones → `JsError::Budget`; sin event loop completo |
| Integración en `render` (`<script>`), eventos, timers de reloj, CSSOM/`getComputedStyle` | **PENDIENTE** | WP-6.2…6.7; Fase 6 ≈10 % |

## 9. Fase 7: Shell y ventana del sistema (`crates/shell`, 46 tests)

| Capacidad | Estado | Notas |
|---|:---:|---|
| Ventana nativa `winit` 0.30 + presentación `softbuffer` 0.4 (`0RGB`, X11/Wayland dlopen, sin GPU) | **HECHO** | Cierre/q/Esc/r, resize con debounce 150 ms, errores a stderr+frame blanco |
| Modo headless `--screenshot` sin `EventLoop` (CI) + CLI `megi` + `tools/megi.sh` | **HECHO** | 640×400 `md5 7324e851…` idéntico a `render` |
| **Scroll vertical** (rueda/teclado/paginación, cap 8192 CSS px `[recortado]`), **entrada** ratón/teclado y **HiDPI** (CSS=physical/scale) | **HECHO** | Sin hit-testing/foco |
| **Render en worker** con generación/cancelación/coalescing/debounce (120 ms) y `EventLoopProxy` | **HECHO** | Sin zoom |
| Navegación/historial, foco y eventos DOM | **PENDIENTE** | Fase 7 |
| **Hit-testing** (`NodeId`/`LayoutBox` ↔ `DisplayItem`) | **PENDIENTE** | Fase 7 |
| Zoom y pulido de HiDPI | **PENDIENTE** | Fase 7 |
| Tests de ventana reales en CI (Xvfb) | **PENDIENTE** | El host de desarrollo no tiene servidor gráfico |

## 10. Verificación de la matriz

```bash
./tools/dev.sh all                             # fmt, clippy, 734 tests, doc y gate PASS
cargo test -p css -p layout -p paint -p fonts -p dom -p html
cargo test -p net -p render -p image -p shell
cargo test -p js
cargo test -p js --no-default-features
cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768
cargo run -p render -- tests/fixtures/render/images.html docs/images/images-e2e.png --width 640 --height 400
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/shot.png
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/phase4.png
python3 tools/png_check.py /tmp/phase4.png
./tools/bench.sh
```

Si detectas una discrepancia entre esta matriz y el comportamiento real, es un bug de documentación o de implementación: abre un issue o actualiza el WP correspondiente de [`ROADMAP.md`](ROADMAP.md).
