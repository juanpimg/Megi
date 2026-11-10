# Matriz de Soporte de la Fase 4 (`docs/PHASE4-SUPPORT.md`)

**Objetivo de la Fase 4**: renderizar una página HTML real a una imagen **PNG**, completando el modelo de pintado CSS 2.2 y la tipografía. **Estado global: ~99 %.**

El pipeline `HTML → CSS → layout → display list → PNG` renderiza texto real, tablas, `inline-block`, cajas reemplazadas, posicionamiento, floats, `overflow` con clip, stacking/`z-index`/`opacity`, decoraciones/marcadores y comprime PNG con un DEFLATE propio. **Solo falta pintar el contenido de `<img>`/`background-image`** y algunos casos avanzados de tablas.

- Resultados visuales: [`images/phase4-basic.png`](images/phase4-basic.png) · [`images/render-fixture.png`](images/render-fixture.png).
- Plan: [`ROADMAP.md`](ROADMAP.md) §«FASE 4»/«FASE 5» · Estado y deuda: [`HANDOFF.md`](HANDOFF.md) · Rendimiento: [`PERFORMANCE.md`](PERFORMANCE.md).
- Seguridad/fuzzing: [`SECURITY.md`](../SECURITY.md) · [`FUZZING.md`](FUZZING.md).

Leyenda: **HECHO** = implementado y testeado; **PARCIAL** = fundamentos presentes, falta integración/casos; **PENDIENTE** = no implementado.

---

## 1. Modelo de caja y layout (`crates/layout`, 118 tests)

| Capacidad CSS 2.2 | Estado | Notas |
|---|:---:|---|
| Bloques, márgenes, paddings, bordes; cajas anónimas (§9.2.1.1) | **HECHO** | BFC con colapso de márgenes §8.3.1; ICB con viewport |
| `box-sizing`, `min/max`, alturas porcentuales, sobre-restringido | **HECHO** | §10.1/§10.3.3/§10.4/§10.6/§10.7 |
| Auto-colapso de bloques de altura cero (*self-collapsing*) | **PENDIENTE** | Caso menor de §8.3.1 |
| IFC: saltos UAX #14, SHY de ancho cero, `line-height` factor, `vertical-align` §10.8.1, `justify` | **HECHO** | `ifc_conformance2` (14), `fonts_integration` (4) |
| `position: relative/absolute/fixed`; `z-index`/stacking contexts | **HECHO** | §9.3/§9.4.3/§9.6/§9.9; `sticky` documentado como relative (sin scroll) |
| `float`/`clear` §9.5 y BFC que contiene floats §10.6.7 | **HECHO** | Estrechamiento a granularidad de bloque (limitación) |
| `overflow` → BFC + clip rect §11.1 | **HECHO** | El painter emite `PushClip`/`PopClip` |
| **Tablas CSS 2.1 §17** (auto layout, min/max-content, `border-spacing`, `colspan`, `vertical-align` de celdas, caption, row-groups, fixups anónimos) | **HECHO** | `border-collapse` sin conflicto de bordes, `fixed`→auto, sin `rowspan`/% (pendiente WP-4.8b) |
| `inline-block` (shrink-to-fit, baseline, vertical-align §10.8.1) | **HECHO** | 4 tests |
| Cajas reemplazadas (`BoxType::Replaced`) con tamaños intrínsecos (`img` 300×150, `input` 20×20, `textarea` 160×40) | **HECHO** | API `layout_document_with_replaced_sizes`; 6 tests |
| Pintado del contenido de `<img>` / `background-image` | **PENDIENTE** | Siguiente WP con `crates/image` |
| Block-in-inline | **PENDIENTE** | Fuera del alcance actual |
| `display: list-item` y marcadores (§12.5) | **HECHO** | paint pinta `disc`/`circle`/`square`/`decimal` |
| `visibility: hidden` (§11.2) | **HECHO** | paint no pinta caja ni texto |

## 2. CSS (`crates/css`, 44 tests + 30 WPT)

| Capacidad | Estado | Notas |
|---|:---:|---|
| CSS Syntax L3, Selectores L4 funcionales/estructurales, especificidad, cascada, shorthands, `font`, `@media` básico, índice de reglas (2,1×) | **HECHO** | 30/30 CSS Syntax WPT vendorizado |
| Bordes por lado/shorthands, `border-style: none` ⇒ ancho 0; `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type`; `line_height_factor` | **HECHO** | Bases de Fase 4 |
| Tablas: `display: table-*`/`inline-table`/caption, `border-spacing`, `border-collapse`, `table-layout`, `caption-side`, `empty-cells` + hoja UA §15 | **HECHO** | Propiedades computadas; el layout las consume |

## 3. Pintado (`crates/paint`, 77 tests)

| Capacidad | Estado | Notas |
|---|:---:|---|
| Orden CSS 2.2 Apéndice E (stacking contexts, z<0/auto/>0, floats, inline, opacidad) | **HECHO** | 6 tests de stacking |
| `OpacityGroup` offscreen compuesto una vez; `PushClip`/`PopClip` desde `overflow` | **HECHO** | No clipea fondo/borde propios |
| Bordes por lado (`solid/dashed/dotted/double/none/hidden`, esquinas a mitre), fondo bajo el *border box* | **HECHO** | Groove/ridge/inset/outset → sólido |
| Texto real (fuentes integradas), decoraciones (underline/overline/line-through), `visibility`, marcadores | **HECHO** | 5 tests de decoraciones |
| AA exacto, culling y cachés por pasada | **HECHO** | `raster_quality` (14) |
| **Compresión PNG propia** (DEFLATE LZ77+Huffman fijo, zlib/Adler, filtros adaptativos) | **HECHO** | **1.920.813 → 42.853 B (44,8×)**, `md5 9e957177…`, `basic.rgba` idéntico |
| Painter iterativo con caps (stacking 256, floats 64, items 5M) | **HECHO** | Probado a 20.000 niveles |
| Codificador PNG determinista + PPM de depuración | **HECHO** | Golden byte a byte |

## 4. Tipografía (`crates/fonts`, 13 tests + doctest)

| Capacidad | Estado | Notas |
|---|:---:|---|
| fontdb + ttf-parser + rustybuzz + ab_glyph; fallback, métricas, shaping LTR, máscaras de glifo | **HECHO** | Integrado en layout/paint; fuente DejaVu determinista |
| `font-weight`/`font-style`, baseline en la display list | **HECHO** | — |
| Bidi, scripts complejos, fuentes variables | **PENDIENTE** | Fuera de alcance de Fase 4 |

## 5. Documento, imagen y red (Fase 5, ~60 %)

| Capacidad | Estado | Notas |
|---|:---:|---|
| URL WHATWG subset, HTTP/1.1, **TLS 1.2/1.3** (`rustls`+`webpki-roots`), caché HTTP RFC 9111 subset, `data:`, `file://`, MIME, `ResourceLoader` | **HECHO** | `crates/net`, 85 tests offline |
| **Decoder PNG propio + inflate zlib/DEFLATE** (color types 0/2/3/4/6, bit depth 1–16, filtros 0–4, Adam7, CRC, `PngLimits`) | **HECHO** | `crates/image`, 46 tests; GIF/JPEG/WebP solo detectados |
| **Pipeline de navegador** `URL → PNG` con CSS externo: charset UTF-8/Windows-1252, `<base>`, `<style>`+`<link rel=stylesheet>` (dedup/media/caps), cascada UA+autor, layout con fuentes, PNG | **HECHO** | `crates/render`, 22 tests; fixture 640×400 → 15.498 B `md5 7324e851…` |
| `<img>` / `background-image` de extremo a extremo (fetch + decodificación + pintado) | **PENDIENTE** | WP-5.7 (usa `crates/image`) |
| `@import`, media queries completas, `@font-face` | **PENDIENTE** | WP-5.8 |
| CORS/`Referrer-Policy`, cookies, gzip/br, HTTP/2, streaming | **PENDIENTE** | WP-5.9 |
| Ventana/eventos/scroll/hit-testing | **PENDIENTE** | Fase 7 (`crates/shell`) |

## 6. DOM generacional (`crates/dom`, 20 tests)

| Capacidad | Estado | Notas |
|---|:---:|---|
| `NodeId` 4 B (20 bits slot + 12 generación), free list LIFO, reutilización de slots | **HECHO** | Máx. 1.048.575 slots |
| `remove_node`/`remove_subtree`; handles stale/foráneos rechazados | **HECHO** | `try_node`/`owns`; panic claro en `node()` |
| `descendants`/`ancestors` iterativos con guardas de ciclo; `validate_invariants`; `estimate_bytes` | **HECHO** | Probado con 100k nodos |

## 7. Fuera del alcance de la Fase 4

JavaScript (Fase 6), formularios interactivos, `flex`/`grid`, `@font-face`, bidi/scripts complejos, scroll/hit-testing/eventos (Fase 7), animaciones, `border-radius`, `box-shadow`, gradientes y medios paginados. Las imágenes reales llegan con la Fase 5 (WP-5.7).

## 8. Verificación de la matriz

```bash
./tools/dev.sh all                             # fmt, clippy, 475 tests, doc y gate PASS
cargo test -p css -p layout -p paint -p fonts -p dom
cargo test -p net -p render -p image
cargo run -p render -- tests/fixtures/render/basic.html docs/images/render-fixture.png --width 640 --height 400
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/phase4.png
python3 tools/png_check.py /tmp/phase4.png
./tools/bench.sh
```

Si detectas una discrepancia entre esta matriz y el comportamiento real, es un bug de documentación o de implementación: abre un issue o actualiza el WP correspondiente de [`ROADMAP.md`](ROADMAP.md).
