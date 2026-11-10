# Matriz de Soporte de la Fase 4 (`docs/PHASE4-SUPPORT.md`)

**Objetivo de la Fase 4**: renderizar una página HTML real (autocontenida, sin red ni JavaScript) a una imagen **PNG**, completando para ello el modelo de pintado CSS 2.2 y la tipografía.

**Estado global**: **~97 %**. El pipeline `HTML → CSS → layout → display list → PNG` renderiza **texto real**, bordes por lado, **posicionamiento, floats, `overflow` con clip, stacking contexts/`z-index`, `opacity`**, decoraciones y marcadores, de forma determinista y con golden byte a byte. Solo faltan tablas, `inline-block`/block-in-inline, reemplazados y la compresión PNG.

- Resultado visual: [`images/phase4-basic.png`](images/phase4-basic.png) (800×600, 26 display items; render real de `tests/fixtures/phase4/basic.html`).
- Plan y criterios de aceptación: [`ROADMAP.md`](ROADMAP.md) §«FASE 4» (`WP-4.0`…`WP-4.13`).
- Estado de los crates, comandos y deuda: [`HANDOFF.md`](HANDOFF.md).
- Rendimiento: [`PERFORMANCE.md`](PERFORMANCE.md) · Seguridad/fuzzing: [`SECURITY.md`](../SECURITY.md), [`FUZZING.md`](FUZZING.md).

Leyenda: **HECHO** = implementado y testeado; **PARCIAL** = fundamentos presentes pero falta la integración o casos relevantes; **PENDIENTE** = no implementado (puede existir el parseo de la propiedad sin comportamiento de render).

---

## 1. Modelo de caja y layout

| Capacidad CSS 2.2 | Estado | Notas / WP |
|---|:---:|---|
| Bloques, márgenes, paddings y bordes en BFC (§8, §10.3.3) | **HECHO** | `layout`; 102 tests |
| Cajas anónimas (§9.2.1.1) | **HECHO** | Estilos *inherit-only* (WP-4.2) |
| Colapso de márgenes (§8.3.1) | **HECHO** | Padre-hijo, hermanos y márgenes negativos |
| Bloques de altura cero auto-colapsantes (*self-collapsing*) | **PENDIENTE** | Caso menor restante de §8.3.1 |
| Bloque contenedor inicial con altura del viewport (§10.1) | **HECHO** | WP-4.2 |
| `box-sizing`, `min/max-width/height`, alturas porcentuales, sobre-restringido (§10.4/§10.7) | **HECHO** | WP-4.2 |
| IFC: cajas de línea, strut, fragmentación de inlines (§9.4.2, §10.8) | **HECHO** | WP-4.5 |
| Saltos UAX #14 (no romper entre nodos inline adyacentes sin espacio) | **HECHO** | WP-4.5; `ifc_conformance2` |
| Soft hyphen (ancho cero; guion solo al romper) | **HECHO** | WP-4.5 |
| `line-height` (`normal`, `<number>` heredado como factor) | **HECHO** | WP-4.0/4.5; `line_height_factor` |
| `vertical-align` (baseline/middle/sub/super/text-top/text-bottom/length/top/bottom) | **HECHO** | WP-4.5; *aligned subtree* §10.8.1 |
| `text-align: left/right/center/justify` | **HECHO** | WP-4.5 |
| `position: relative/absolute/fixed` (§9.3/§9.4.3/§9.6) | **HECHO** | WP-4.6; containing blocks, posición estática, shrink-to-fit; `sticky` documentado como relative |
| `z-index` y stacking contexts (§9.9, Apéndice E) | **HECHO** | WP-4.6 (layout marca contextos; paint ordena) |
| `opacity` como grupo compuesto | **HECHO** | WP-4.6; `DisplayItem::OpacityGroup` offscreen |
| `float`/`clear` (§9.5) y BFC que contiene floats (§10.6.7) | **HECHO** | WP-4.7; estrechamiento a granularidad de bloque (limitación documentada) |
| `overflow: hidden/clip/scroll/auto` (§11.1) y clip rect propagado | **HECHO** | WP-4.10; clip efectivo en paint |
| `display: inline-block` (§10.3.9) | **PENDIENTE** | WP-4.9b (layout: sigue inline normal) |
| `display: table/table-row/table-cell`, `border-collapse` (§17) | **PENDIENTE** | WP-4.8 (hoy se apilan como bloques) |
| `display: list-item` y marcadores (§12.5) | **HECHO** | WP-4.9; painter pinta `disc`/`circle`/`square`/`decimal` |
| `visibility: hidden` (§11.2) | **HECHO** | WP-4.9 (paint no pinta caja ni texto) |
| Block-in-inline (inline que contiene bloques) | **PENDIENTE** | Fuera del alcance actual de WP-4.9b |
| Elementos reemplazados (`<img>` con tamaño intrínseco) | **PENDIENTE** | Depende de Fase 5 (red/decodificación) |

## 2. Bordes, fondos y decoración

| Capacidad CSS 2.2 / módulo | Estado | Notas / WP |
|---|:---:|---|
| `border-*` por lado, shorthands y `border-style: none` ⇒ ancho 0 (§8.5) | **HECHO** | CSS (WP-4.0); 41 tests css |
| Bordes por lado con 4 colores/estilos y esquinas a inglete | **HECHO** | WP-4.3; `BorderSide[4]` + mitre |
| `border-style: solid/dashed/dotted/double/none`/`hidden` | **HECHO** | WP-4.3 (`groove`/`ridge`/`inset`/`outset` degradan a sólido) |
| `background-color` bajo el *border box* (§14.2, CSS Backgrounds 3) | **HECHO** | WP-4.3 |
| Canvas: fondo raíz/`body` sobre todo el viewport (§14.2) | **HECHO** | 0 % de píxeles transparentes en el golden |
| `text-decoration: underline/overline/line-through` | **HECHO** | WP-4.9 (paint) |
| `background-image`, gradientes, `background-position/repeat/size` | **PENDIENTE** | Fase 5 (red/decodificación) |
| `border-radius`, `box-shadow` | **PENDIENTE** | Fuera de alcance de Fase 4 |

## 3. Tipografía y texto

| Capacidad | Estado | Notas / WP |
|---|:---:|---|
| Motor de fuentes real (`crates/fonts`): descubrimiento, fallback, métricas OpenType | **HECHO** | WP-4.4; 13 tests + doctest |
| Shaping LTR con kern/ligaduras y rasterización de glifos | **HECHO** | WP-4.4 (`rustybuzz` + `ab_glyph`) |
| Integración en `layout`/`paint` (métricas, glyph runs, baseline) | **HECHO** | WP-4.4; texto real en el PNG |
| `font-weight`/`font-style` en medición y pintado | **HECHO** | `font_weight`/`font_italic` en la display list |
| Glifos reales con antialiasing | **HECHO** | WP-4.4/4.11 |
| Bidi, scripts complejos, vertical text, fuentes variables | **PENDIENTE** | Fuera de alcance de Fase 4 (LTR latino) |

## 4. Pintado y salida de imagen

| Capacidad | Estado | Notas / WP |
|---|:---:|---|
| Orden de pintado CSS 2.2 Apéndice E (caso in-flow) | **HECHO** | WP-4.6; z<0 → bloques → floats → inline/texto/decoraciones/marcadores → z auto/0 → z>0 |
| Stacking contexts, `z-index` y `opacity` por grupos | **HECHO** | WP-4.6; 6 tests de stacking |
| `PushClip`/`PopClip` desde `overflow` | **HECHO** | WP-4.10; no clipea fondo/borde propios |
| Pipeline `HTML → PNG` de extremo a extremo | **HECHO** | `cargo run --release -p paint --example html_to_png -- ...`; paint **63 tests** |
| Codificador PNG propio sin dependencias (RGBA 8 bits, CRC-32, zlib/DEFLATE *stored*) | **HECHO** | Decisión D3, §5 |
| Validación PNG con `tools/png_check.py` | **HECHO** | Firma, IHDR, chunks, CRCs, IDAT, filtros, IEND |
| Golden determinista (`tests/fixtures/phase4/basic.rgba`) | **HECHO** | `basic.rgba` `md5 60f8984e…`; PNG del pipeline `md5 34152025…`; 26 display items |
| Antialiasing exacto y culling/cachés por pasada | **HECHO** | WP-4.11; `raster_quality.rs` (14 tests) |
| Captura comprimida para documentación (`docs/images/phase4-basic.png`, 26 KB) | **HECHO** | Generada desde el PNG del pipeline |
| Compresión PNG dentro del codificador | **PENDIENTE** | WP-4.11b; hoy *stored* (~1,92 MB a 800×600) |
| Alfa premultiplicado interno | **PENDIENTE** (opcional) | Se mantiene alfa straight; revisar con más grupos de `opacity` |
| Salida a ventana (Fase 7) | **PENDIENTE** | `crates/shell`; reutilizará `Pixmap` |

## 5. Decisión D3: codificador PNG propio (sin dependencias)

**Contexto**: la auditoría de Fase 4 encontró que la única salida de imagen era PPM. El objetivo de la fase exige PNG y los golden tests necesitan una salida estándar y binaria.

**Opciones evaluadas**:

1. **Añadir `png`/`flate2`** a `crates/paint`: fuera de la matriz de dependencias autorizadas de `ARCHITECTURE.md` §3.1; exigiría enmienda de política y justificación de licencia/impacto.
2. **Diferir PNG a Fase 5**: inaceptable para el objetivo y los golden de Fase 4.
3. **Implementar el codificador desde cero con `std`** — **elegida**: `crates/paint/src/png.rs` genera PNG válidos con `IHDR` (8 bits/canal, color type 6 RGBA), `IDAT` (zlib RFC 1950 + DEFLATE RFC 1951 con bloques *stored*), CRC-32 IEEE por chunk y `IEND`, más Adler-32. Determinista (requisito de golden tests), sin dependencias y sin tocar el DAG del núcleo.

**Trade-off aceptado**: sin compresión, un PNG de 800×600 RGBA ocupa ~1,92 MB (frente a unos pocos KB con *deflate* real). Es aceptable para tests y goldens; para documentación se publica una captura comprimida aparte (26 KB) generada desde ese PNG. La compresión se planifica en WP-4.11b, bien con implementación propia (Huffman/LZ77), bien mediante una enmienda explícita de política para `flate2`/`png` si el tamaño pasa a ser requisito. Cualquier cambio debe actualizar esta sección y `ARCHITECTURE.md` §3.2.

**Verificación reproducible**:

```bash
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
cargo test -p paint --test end_to_end_html --test png_format --test raster_quality \
    --test stacking_order --test decorations
```

## 6. Fase 5 (red): estado de soporte

| Capacidad | Estado | Notas / WP |
|---|:---:|---|
| URL WHATWG subset (`http`/`https`/`file` + `join`) | **HECHO** | WP-5.0; 12 tests |
| HTTP/1.1 `GET` (timeouts, `Content-Length`, chunked, redirects, caps) | **HECHO** | WP-5.1; 14 tests |
| **TLS 1.2/1.3** (`rustls` 0.23 ring + `webpki-roots`, SNI/validación) | **HECHO** | WP-5.3; feature `tls` default; `rcgen` solo tests |
| **Caché HTTP RFC 9111 subset** y **`data:` URLs** | **HECHO** | WP-5.4; `max-age`/`Expires`/`ETag`/`304`/`Vary`, Base64 propio |
| `file://`, MIME sniffing y `ResourceLoader` con caché | **HECHO** | WP-5.2 |
| Fetch/CORS, `Referrer-Policy` y carga de subrecursos | **PENDIENTE** | WP-5.5 |
| Integración de recursos en el pipeline de render (`<link>`, imágenes) | **PENDIENTE** | WP-5.6 → depende de WP-5.5 |
| Cookies, streaming, compresión gzip/br, HTTP/2 | **PENDIENTE** | WP-5.7 |

Detalle y siguientes pasos: [`ROADMAP.md`](ROADMAP.md) §«FASE 5»; seguridad de red en [`SECURITY.md`](../SECURITY.md).

## 7. Fuera del alcance de la Fase 4

- **Fase 5 (`crates/net`, en curso)**: red, `<img>` real, hojas externas, `background-image`, Fetch/CORS.
- **Fase 6**: JavaScript, `document.write`, DOM bindings.
- **Fase 7**: ventana/eventos, scroll y *hit-testing*.
- **Post-Fase 4**: formularios interactivos, `flex`/`grid`, `@font-face`, bidi/scripts complejos, animaciones/transiciones, `transform`, `border-radius`, `box-shadow`, gradientes y medios paginados.

## 8. Verificación de la matriz

```bash
./tools/dev.sh all                             # fmt, clippy, 362 tests, doc y gate PASS
cargo test -p css -p layout -p paint -p fonts
cargo test -p net                              # Fase 5 (85 tests, offline)
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
./tools/bench.sh                               # baseline en docs/PERFORMANCE.md
```

Si detectas una discrepancia entre esta matriz y el comportamiento real, es un bug de documentación o de implementación: abre un issue o actualiza el WP correspondiente de [`ROADMAP.md`](ROADMAP.md).
