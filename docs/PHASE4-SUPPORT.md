# Matriz de Soporte de la Fase 4 (`docs/PHASE4-SUPPORT.md`)

**Objetivo de la Fase 4**: renderizar una página HTML real (autocontenida, sin red ni JavaScript) a una imagen **PNG**, completando para ello el modelo de pintado CSS 2.2 y la tipografía.

**Estado global**: **~50 %**. El pipeline `HTML → CSS → layout → display list → PNG` ya funciona de extremo a extremo y está cubierto por tests y un golden determinista. Lo que falta es fidelidad: fuentes reales integradas, bordes por lado en el raster, `position`/stacking, floats, tablas, `inline-block`, overflow y calidad de imagen.

- Plan por paquetes y criterios de aceptación: [`ROADMAP.md`](ROADMAP.md) §«FASE 4» (`WP-4.0`…`WP-4.13`).
- Estado de los crates y comandos de verificación: [`HANDOFF.md`](HANDOFF.md).
- Arquitectura, DAG y política de dependencias (incluida la decisión PNG): [`ARCHITECTURE.md`](ARCHITECTURE.md) §1 y §3.2.

Leyenda: **HECHO** = implementado y testeado; **PARCIAL** = fundamentos presentes pero falta la integración o casos relevantes; **PENDIENTE** = no implementado (puede existir el parseo de la propiedad sin comportamiento de render).

---

## 1. Modelo de caja y layout

| Capacidad CSS 2.2 | Estado | Notas / WP |
|---|:---:|---|
| Bloques, márgenes, paddings y bordes en BFC (§8, §10.3.3) | **HECHO** | `layout`; 61 tests |
| Cajas anónimas (§9.2.1.1) | **HECHO** | Estilos *inherit-only* (WP-4.2); fin de márgenes/fondos fantasma |
| Colapso de márgenes (§8.3.1) | **HECHO** | Padre-hijo, hermanos y márgenes negativos (WP-4.2) |
| Márgenes auto-colapsantes de bloques de altura cero (*self-collapsing*) | **PENDIENTE** | Caso restante de §8.3.1 (seguimiento en HANDOFF) |
| Bloque contenedor inicial con altura del viewport (§10.1) | **HECHO** | WP-4.2 |
| `box-sizing`, `min/max-width/height`, alturas porcentuales, sobre-restringido (§10.4/§10.7) | **HECHO** | WP-4.2 |
| IFC: cajas de línea, strut, fragmentación de inlines (§9.4.2, §10.8) | **PARCIAL** | Base implementada; pendiente WP-4.5 |
| `<br>` y `&nbsp;` no colapsable | **HECHO** | WP-4.2/4.5 |
| Oportunidades de salto UAX #14 (no romper entre nodos inline adyacentes sin espacio) | **PENDIENTE** | WP-4.5 (hallazgo F-09 de la auditoría) |
| Soft hyphen (ancho cero, guion sólo al romper) | **PENDIENTE** | WP-4.5 |
| `line-height` (`normal`, `<number>` por elemento) | **PARCIAL** | `normal` se aproxima a 1,2 em; la herencia de `<number>` se congela en px (WP-4.0/4.5) |
| `vertical-align: baseline/middle/sub/super/text-top/text-bottom/length` | **PARCIAL** | Fórmulas actuales no conformes para `top`/`bottom`/`length` (§10.8.1) |
| `text-align: left/right/center` | **HECHO** | IFC |
| `text-align: justify` | **PENDIENTE** | Se parsea y se pinta como izquierda |
| `position: relative/absolute/fixed`, `z-index`, stacking contexts (§9.3/§9.6/§9.9) | **PENDIENTE** | Propiedades calculadas pero no aplicadas; WP-4.6 |
| `float`/`clear` (§9.5) | **PENDIENTE** | Parseadas (WP-4.0); layout/paint pendientes (WP-4.7) |
| `display: inline-block` (§10.3.9) | **PENDIENTE** | WP-4.9 |
| `display: table/table-row/table-cell`, `border-collapse` (§17) | **PENDIENTE** | Hoy se apilan como bloques; WP-4.8 |
| `display: list-item` y marcadores (§12.5) | **PENDIENTE** | `list-style-type` parseado; marcadores no pintados (WP-4.9) |
| `overflow`/clipping (§11.1) | **PENDIENTE** | Propiedad parseada; el painter no emite `PushClip` (WP-4.10) |
| `visibility` (§11.2) | **PENDIENTE** | Propiedad parseada; sin efecto de render |
| Elementos reemplazados (`<img>` con tamaño intrínseco) | **PENDIENTE** | Sin decodificador de imágenes; depende de Fase 5 (`crates/net`). Placeholder en WP-4.9 |

## 2. Bordes, fondos y decoración

| Capacidad CSS 2.2 / módulo | Estado | Notas / WP |
|---|:---:|---|
| `border-*` por lado, shorthands y `border-style: none` ⇒ ancho 0 (§8.5) | **HECHO** | CSS (WP-4.0); 23 tests |
| Pintado de bordes por lado con 4 colores/estilos y esquinas a inglete | **PARCIAL** | La display list aún usa un solo color por caja; WP-4.3 |
| `border-style: solid` | **PARCIAL** | Se pinta como bandas rectangulares; falta el modelo por lados |
| `dashed`/`dotted`/`double`/`groove`/`ridge`/`inset`/`outset` | **PENDIENTE** | WP-4.3 |
| `background-color` en *padding box* vs. *border box* y `background-clip` (§14.2, CSS Backgrounds 3) | **PARCIAL** | Canvas raíz/`body` propagado (WP-4.1); área de fondo por caja pendiente de confirmar en WP-4.3 |
| Canvas: fondo raíz/`body` sobre todo el viewport (§14.2) | **HECHO** | 0 % de píxeles transparentes en el golden de `basic.html` (WP-4.1) |
| `background-image`, gradientes, `background-position/repeat/size` | **PENDIENTE** | Fuera de Fase 4 (imágenes en Fase 5) |
| `border-radius` (§4 CSS Backgrounds 3) | **PENDIENTE** | Fuera de alcance de Fase 4 |
| `box-shadow` | **PENDIENTE** | Fuera de alcance de Fase 4 |
| `text-decoration: underline/overline/line-through` | **PENDIENTE** | Parseado (WP-4.0); pintado en WP-4.9 |
| `opacity` como grupo compuesto | **PENDIENTE** | Propiedad calculada; requiere capas offscreen (WP-4.6) |

## 3. Tipografía y texto

| Capacidad | Estado | Notas / WP |
|---|:---:|---|
| Motor de fuentes real (`crates/fonts`): descubrimiento, fallback, métricas OpenType | **HECHO** | WP-4.4 fase 1; 13 tests + doctest |
| Shaping LTR con kern/ligaduras (`rustybuzz`) y rasterización de glifos (`ab_glyph`) | **HECHO** | WP-4.4 fase 1 |
| Integración de `crates/fonts` en `layout`/`paint` (`TextMeasurer` real, glyph runs, baseline) | **PENDIENTE** | **Prioridad máxima**; WP-4.4 fase 2 |
| Métricas tipográficas reales para line boxes (`line-height: normal` desde la fuente) | **PENDIENTE** | WP-4.4 fase 2 |
| Glifos reales en el raster (hoy 12 patrones 8×8 + fallback de caja) | **PARCIAL** | `DefaultTextMeasurer` y fuente *stub* hasta la integración; el avance pintado ya coincide con el medido (WP-4.1) |
| `font-weight`/`font-style` en medición y pintado | **PENDIENTE** | `crates/fonts` ya selecciona 400/700 y normal/italic; falta el plumbing de layout/paint |
| Bidi, scripts complejos, vertical text | **PENDIENTE** | Fuera de alcance de Fase 4 (LTR latino) |

## 4. Salida de imagen

| Capacidad | Estado | Notas / WP |
|---|:---:|---|
| Pipeline `HTML → PNG` de extremo a extremo | **HECHO** | Ejemplo `cargo run -p paint --example html_to_png -- ...`; `crates/paint` suma **33 tests** (13 unit + 12 e2e + 8 de formato PNG) |
| Codificador PNG propio sin dependencias (RGBA 8 bits, CRC-32, zlib/DEFLATE *stored*) | **HECHO** | Decisión D3, §5 |
| Validación PNG con `tools/png_check.py` | **HECHO** | Firma, IHDR, chunks, CRCs, IDAT, filtros, IEND |
| Golden determinista (`tests/fixtures/phase4/basic.rgba`) | **HECHO** | Comparación byte a byte; 800×600, 1.920.813 B de PNG, 9 colores, 0 % transparente |
| PPM (`Pixmap::to_ppm`), heredado de la etapa anterior | **HECHO** | Disponible para depuración |
| Compresión PNG | **PENDIENTE** | WP-4.11 (o enmienda de política para `flate2`/`png`, §5) |
| Antialiasing / cobertura por píxel | **PENDIENTE** | `fill_rect` redondea hacia fuera; WP-4.11 |
| Alfa premultiplicado interno | **PENDIENTE** | WP-4.11 |
| Salida a ventana (Fase 7) | **PENDIENTE** | `crates/shell`; requiere `winit`/`softbuffer` |

## 5. Decisión D3: codificador PNG propio (sin dependencias)

**Contexto**: la auditoría de Fase 4 encontró que la única salida de imagen era PPM. El objetivo de la fase exige PNG y los golden tests necesitan una salida estándar y binaria.

**Opciones evaluadas**:

1. **Añadir `png`/`flate2`** a `crates/paint`: fuera de la matriz de dependencias autorizadas de `ARCHITECTURE.md` §3.1; exigiría enmienda de política y justificación de licencia/impacto.
2. **Diferir PNG a Fase 5**: inaceptable para el objetivo y los golden de Fase 4.
3. **Implementar el codificador desde cero con `std`** — **elegida**: `crates/paint/src/png.rs` genera PNG válidos con `IHDR` (8 bits/canal, color type 6 RGBA), `IDAT` (zlib RFC 1950 + DEFLATE RFC 1951 con bloques *stored*), CRC-32 IEEE por chunk y `IEND`, más Adler-32. Determinista (requisito de golden tests), sin dependencias y sin tocar el DAG del núcleo.

**Trade-off aceptado**: sin compresión, un PNG de 800×600 RGBA ocupa ~1,92 MB (frente a unos pocos KB con *deflate* real). Es aceptable durante la fase de pruebas; la compresión se planifica en WP-4.11, bien con implementación propia (Huffman/LZ77), bien mediante una enmienda explícita de política para `flate2`/`png` si el tamaño pasa a ser requisito. Cualquier cambio debe actualizar esta sección y `ARCHITECTURE.md` §3.2.

**Verificación reproducible**:

```bash
cargo run -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png          # OK + estadísticas de canvas
cargo test -p paint --test end_to_end_html --test png_format
```

## 6. Fuera del alcance de la Fase 4

Red y `<img>` real, JavaScript, formularios interactivos, `flex`/`grid`, `@media`, `@font-face`, bidi y scripts complejos, scroll/hit-testing/eventos, animaciones y transiciones, `transform`, `border-radius`, `box-shadow`, `background-image`/gradientes y medios paginados. Su entrada está planificada en las Fases 5–7 (`crates/net`, `crates/js`, `crates/shell`) o queda explícitamente post-Fase 4.

## 7. Verificación de la matriz

Esta matriz se contrasta con:

```bash
./tools/dev.sh all                             # fmt, clippy, 165 tests, doc y gate PASS
cargo test -p css -p layout -p paint -p fonts
cargo run -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
```

Si detectas una discrepancia entre esta matriz y el comportamiento real, es un bug de documentación o de implementación: abre un issue o actualiza el WP correspondiente de [`ROADMAP.md`](ROADMAP.md).
