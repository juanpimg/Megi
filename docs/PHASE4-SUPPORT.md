# Matriz de Soporte de la Fase 4 (`docs/PHASE4-SUPPORT.md`)

**Objetivo de la Fase 4**: renderizar una página HTML real (autocontenida, sin red ni JavaScript) a una imagen **PNG**, completando para ello el modelo de pintado CSS 2.2 y la tipografía.

**Estado global**: **~90 %**. El pipeline `HTML → CSS → layout → display list → PNG` ya renderiza **texto real**, **bordes por lado** y **antialiasing**, de forma determinista y con golden byte a byte. Lo que falta es el modelo de posicionamiento/stacking, floats, tablas, `inline-block`, el recorte desde `overflow` y la compresión PNG.

- Resultado visual: [`images/phase4-basic.png`](images/phase4-basic.png) (800×600; render real de `tests/fixtures/phase4/basic.html`).
- Plan y criterios de aceptación: [`ROADMAP.md`](ROADMAP.md) §«FASE 4» (`WP-4.0`…`WP-4.13`).
- Estado de los crates, comandos y deuda: [`HANDOFF.md`](HANDOFF.md).
- Rendimiento del pipeline: [`PERFORMANCE.md`](PERFORMANCE.md).
- Arquitectura, DAG, política de dependencias y decisión PNG: [`ARCHITECTURE.md`](ARCHITECTURE.md) §1 y §3.

Leyenda: **HECHO** = implementado y testeado; **PARCIAL** = fundamentos presentes pero falta la integración o casos relevantes; **PENDIENTE** = no implementado (puede existir el parseo de la propiedad sin comportamiento de render).

---

## 1. Modelo de caja y layout

| Capacidad CSS 2.2 | Estado | Notas / WP |
|---|:---:|---|
| Bloques, márgenes, paddings y bordes en BFC (§8, §10.3.3) | **HECHO** | `layout`; 80 tests |
| Cajas anónimas (§9.2.1.1) | **HECHO** | Estilos *inherit-only* (WP-4.2); fin de márgenes/fondos fantasma |
| Colapso de márgenes (§8.3.1) | **HECHO** | Padre-hijo, hermanos y márgenes negativos (WP-4.2) |
| Márgenes auto-colapsantes de bloques de altura cero (*self-collapsing*) | **PENDIENTE** | Caso menor restante de §8.3.1 |
| Bloque contenedor inicial con altura del viewport (§10.1) | **HECHO** | WP-4.2 |
| `box-sizing`, `min/max-width/height`, alturas porcentuales, sobre-restringido (§10.4/§10.7) | **HECHO** | WP-4.2 |
| IFC: cajas de línea, strut, fragmentación de inlines (§9.4.2, §10.8) | **HECHO** | WP-4.5 |
| `<br>` y `&nbsp;` no colapsable | **HECHO** | WP-4.2/4.5 |
| Oportunidades de salto UAX #14 (no romper entre nodos inline adyacentes sin espacio) | **HECHO** | WP-4.5; `ifc_conformance2` |
| Soft hyphen (ancho cero; el guion solo aparece al romper) | **HECHO** | WP-4.5 |
| `line-height` (`normal`, `<number>` heredado como factor) | **HECHO** | WP-4.0/4.5; `line_height_factor` aditivo |
| `vertical-align: baseline/middle/sub/super/text-top/text-bottom/length/top/bottom` | **HECHO** | WP-4.5; *aligned subtree* §10.8.1 y cálculo unificado |
| `text-align: left/right/center/justify` | **HECHO** | WP-4.5; `justify` en líneas soft-wrap |
| `position: relative/absolute/fixed`, `z-index`, stacking contexts (§9.3/§9.6/§9.9) | **PENDIENTE** | Propiedades calculadas pero no aplicadas; WP-4.6 |
| `float`/`clear` (§9.5) | **PENDIENTE** | Parseadas (WP-4.0); layout/paint pendientes (WP-4.7) |
| `display: inline-block` (§10.3.9) | **PENDIENTE** | WP-4.9 (hoy inline normal, sin shrink-to-fit ni baseline) |
| `display: table/table-row/table-cell`, `border-collapse` (§17) | **PENDIENTE** | Hoy se apilan como bloques; WP-4.8 |
| `display: list-item` y marcadores (§12.5) | **PENDIENTE** | `list-style-type` parseado; marcadores no pintados (WP-4.9) |
| `overflow`/clipping (§11.1) | **PARCIAL** | El rasterizador ya soporta `ClipStack`/`PushClip`/`PopClip`; el painter aún no genera clips desde `overflow` (WP-4.10) |
| `visibility` (§11.2) | **PENDIENTE** | Propiedad parseada; sin efecto de render |
| Elementos reemplazados (`<img>` con tamaño intrínseco) | **PENDIENTE** | Sin decodificador de imágenes; depende de Fase 5 (`crates/net`). Placeholder en WP-4.9 |

## 2. Bordes, fondos y decoración

| Capacidad CSS 2.2 / módulo | Estado | Notas / WP |
|---|:---:|---|
| `border-*` por lado, shorthands y `border-style: none` ⇒ ancho 0 (§8.5) | **HECHO** | CSS (WP-4.0); 41 tests css |
| Pintado de bordes por lado con 4 colores/estilos y esquinas a inglete | **HECHO** | WP-4.3; `BorderSide[4]` + mitre |
| `border-style: solid` / `dashed` / `dotted` / `double` / `none`/`hidden` | **HECHO** | WP-4.3 (`groove`/`ridge`/`inset`/`outset` degradan a sólido) |
| `background-color` bajo el *border box* (§14.2, CSS Backgrounds 3) | **HECHO** | WP-4.3 |
| Canvas: fondo raíz/`body` sobre todo el viewport (§14.2) | **HECHO** | 0 % de píxeles transparentes en el golden (WP-4.1) |
| `background-image`, gradientes, `background-position/repeat/size` | **PENDIENTE** | Fuera de Fase 4 (necesitan red/decodificación, Fase 5) |
| `border-radius` (§4 CSS Backgrounds 3) | **PENDIENTE** | Fuera de alcance de Fase 4 |
| `box-shadow` | **PENDIENTE** | Fuera de alcance de Fase 4 |
| `text-decoration: underline/overline/line-through` | **PENDIENTE** | Parseado (WP-4.0); pintado en WP-4.9 |
| `opacity` como grupo compuesto | **PENDIENTE** | Propiedad calculada; requiere capas offscreen (WP-4.6) |

## 3. Tipografía y texto

| Capacidad | Estado | Notas / WP |
|---|:---:|---|
| Motor de fuentes real (`crates/fonts`): descubrimiento, fallback, métricas OpenType | **HECHO** | WP-4.4; 13 tests + doctest |
| Shaping LTR con kern/ligaduras (`rustybuzz`) y rasterización de glifos (`ab_glyph`) | **HECHO** | WP-4.4 |
| Integración en `layout`/`paint` (`TextMeasurer` real, glyph runs, baseline) | **HECHO** | WP-4.4; texto real en el PNG |
| Métricas tipográficas reales para line boxes (`line-height: normal` desde la fuente) | **HECHO** | WP-4.4/4.5 |
| Glifos reales en el raster con antialiasing | **HECHO** | WP-4.4/4.11 |
| `font-weight`/`font-style` en medición y pintado | **HECHO** | `Text` de la display list: `font_weight`, `font_italic` |
| Bidi, scripts complejos, vertical text, fuentes variables | **PENDIENTE** | Fuera de alcance de Fase 4 (LTR latino) |

## 4. Salida de imagen

| Capacidad | Estado | Notas / WP |
|---|:---:|---|
| Pipeline `HTML → PNG` de extremo a extremo | **HECHO** | `cargo run --release -p paint --example html_to_png -- ...`; `crates/paint`: **52 tests** (13 unit + 17 e2e + 8 formato PNG + 14 calidad de raster) |
| Codificador PNG propio sin dependencias (RGBA 8 bits, CRC-32, zlib/DEFLATE *stored*) | **HECHO** | Decisión D3, §5 |
| Validación PNG con `tools/png_check.py` | **HECHO** | Firma, IHDR, chunks, CRCs, IDAT, filtros, IEND |
| Golden determinista (`tests/fixtures/phase4/basic.rgba`) | **HECHO** | Comparación byte a byte; PNG del pipeline `md5 d97c67dc…`, `basic.rgba` `md5 4139fdd8…` |
| Antialiasing / cobertura por píxel en rectángulos, bordes y glifos | **HECHO** | WP-4.11; `raster_quality.rs` (14 tests) |
| Culling y cachés por pasada (shaped runs, caras, máscaras de glifo) | **HECHO** | WP-4.11; raster 800×600 1598,8 → 0,9 ms |
| Captura comprimida para documentación (`docs/images/phase4-basic.png`, 22,9 KB) | **HECHO** | Generada desde el PNG del pipeline |
| Compresión PNG dentro del codificador | **PENDIENTE** | WP-4.11; hoy *stored* (~1,92 MB a 800×600) |
| Alfa premultiplicado interno | **PENDIENTE** (opcional) | Se mantiene alfa straight justificado; revisar con grupos de `opacity` (WP-4.6) |
| Salida a ventana (Fase 7) | **PENDIENTE** | `crates/shell`; reutilizará `Pixmap` |

## 5. Decisión D3: codificador PNG propio (sin dependencias)

**Contexto**: la auditoría de Fase 4 encontró que la única salida de imagen era PPM. El objetivo de la fase exige PNG y los golden tests necesitan una salida estándar y binaria.

**Opciones evaluadas**:

1. **Añadir `png`/`flate2`** a `crates/paint`: fuera de la matriz de dependencias autorizadas de `ARCHITECTURE.md` §3.1; exigiría enmienda de política y justificación de licencia/impacto.
2. **Diferir PNG a Fase 5**: inaceptable para el objetivo y los golden de Fase 4.
3. **Implementar el codificador desde cero con `std`** — **elegida**: `crates/paint/src/png.rs` genera PNG válidos con `IHDR` (8 bits/canal, color type 6 RGBA), `IDAT` (zlib RFC 1950 + DEFLATE RFC 1951 con bloques *stored*), CRC-32 IEEE por chunk y `IEND`, más Adler-32. Determinista (requisito de golden tests), sin dependencias y sin tocar el DAG del núcleo.

**Trade-off aceptado**: sin compresión, un PNG de 800×600 RGBA ocupa ~1,92 MB (frente a unos pocos KB con *deflate* real). Es aceptable para la fase de pruebas y golden images; para documentación se publica una captura comprimida aparte (22,9 KB) generada desde ese PNG. La compresión se planifica en WP-4.11, bien con implementación propia (Huffman/LZ77), bien mediante una enmienda explícita de política para `flate2`/`png` si el tamaño pasa a ser requisito. Cualquier cambio debe actualizar esta sección y `ARCHITECTURE.md` §3.2.

**Verificación reproducible**:

```bash
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
cargo test -p paint --test end_to_end_html --test png_format --test raster_quality
```

## 6. Fuera del alcance de la Fase 4

- **Fase 5 (`crates/net`, ya en curso)**: red, `<img>` real, hojas externas, `background-image`, Fetch/CORS y TLS.
- **Fase 6**: JavaScript, `document.write`, DOM bindings.
- **Fase 7**: ventana/eventos, scroll y *hit-testing*.
- **Post-Fase 4**: formularios interactivos, `flex`/`grid`, `@font-face`, bidi/scripts complejos, animaciones/transiciones, `transform`, `border-radius`, `box-shadow`, gradientes y medios paginados.

## 7. Verificación de la matriz

Esta matriz se contrasta con:

```bash
./tools/dev.sh all                             # fmt, clippy, 301 tests, doc y gate PASS
cargo test -p css -p layout -p paint -p fonts
cargo test -p net                              # Fase 5 (57 tests, offline)
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
./tools/bench.sh                               # baseline en docs/PERFORMANCE.md
```

Si detectas una discrepancia entre esta matriz y el comportamiento real, es un bug de documentación o de implementación: abre un issue o actualiza el WP correspondiente de [`ROADMAP.md`](ROADMAP.md).
