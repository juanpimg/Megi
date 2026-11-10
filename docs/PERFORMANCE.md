# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 22:04–22:30 (Europe/Madrid). Cubre las
  **rondas 18 y 19**: fuzz/clipping/grid MVP/vars-calc/net (r18) y **gradientes
  CSS**, **grid con spans/auto-fill**, **render con Referrer-Policy/SameSite y
  estado CSS real**, **find-in-page** (r19).
- **Repo**: `Megi`, rama `sprint/robustez-fase4`. **Código medido `d9e07e0`**
  (`test(render): add the css_state fixture and document the new fixtures`); a
  mitad de ventana aterrizó `8d065b7` (**solo docs**, no cambia el motor). Las
  mediciones de r18 se habían hecho con el binario previo a un **WIP ajeno roto**
  en `crates/css`; esta ronda mide el árbol **limpio**.
- **Estado del árbol**: limpio salvo este `docs/PERFORMANCE.md` (único cambio
  local). `website/` y `.github/workflows/pages.yml` son del dueño y no se tocan.
- **Binarios**: bench `c315ddf3…` (4 651 432 B; el script lo recompiló con el
  commit de docs: `05e92a78…`, 4 652 128 B); render `8fd2f43d…`; shell
  `e53d3633…` (con `--find`).
- **Artefactos guardados** (gitignored): `target/bench-r19-run*.json`,
  `target/bench-aggregate-r19.json` (`sha256 = 1c3f9dc5…`),
  `target/bench-r19/ab_r18_r19.json`, `features.json`, `render_shell.json`,
  fixtures temporales `flat_gradient_*`, `block_grid_*`, `css_state_static.html`;
  y los de r18 en `target/bench-r18/` y `target/bench-r18-clean-run*.json`.

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §15 (rondas 2–19; la 14 no publicó baseline).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. **El crate `bench` no cambió en r18/r19.**

El bench **no usa ninguna feature nueva** (fuzz, grid, clipping, gradientes CSS,
`var()`/`calc()`, net, find-in-page, estado CSS, referrer). Las capacidades se
miden aparte con `render`/`shell`/`svg` (§8–§12); lo que sí afecta al bench es
el **coste estructural** en cascada/layout/display-list (§5).

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- **La tabla de §3 es la mediana, por fila, de 6 pasadas completas** tomadas a
  **load 4,0–5,35** (host con compilaciones ajenas; las pasadas de load 1,4–2,0
  solo se lograron en r18). Por eso los absolutos de r19 están ~5–10 % por
  encima del suelo: se publican tal cual **y** se da la estimación corregida con
  el A/B (§5.2). La pasada canónica de `./tools/bench.sh` (HEAD `8d065b7`, load
  ~2–4) dio `TOTAL 415.663 ms` (`layout 44.367`, `pipeline 155.232`,
  `render-to-png 8.861`).
- **A/B intercalado de archivos prístinos**: `git archive 718d075` (r18,
  `target/r18-clean-src`, binario `5be5f35b…`) vs binario de r19, 5 pares
  alternando orden; el load afecta a ambos y el Δ es fiable (§5.2).
- Features: 7 pares intercalados con/sin la feature + 3 `--json` por fixture.
  Find-in-page: 7 ejecuciones del shell con `--find megi` y sin él.
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E), governor `powersave` |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` por defecto |
| Carga | 4,0–5,35 en las pasadas (antes 1,4–2,0; hubo picos de 7–19 por otros agentes) |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Bench (árbol limpio; fase con load alto: usar A/B para atribuir)
./tools/bench.sh
for i in 1 2 3 4 5 6; do target/release/bench --json > "target/bench-r19-run$i.json"; done

# A/B prístino r18 vs r19 (atribución de regresiones)
git archive 718d075 | tar -x -C target/r18-clean-src
cd target/r18-clean-src && CARGO_TARGET_DIR="$PWD/target" cargo build --release -p bench
# alternar ./target/release/bench (r18) y ../../target/release/bench (r19)

# Gradientes CSS (vs equivalente sin gradiente)
target/release/render tests/fixtures/render/gradient_linear.html /tmp/g.png --json
target/release/render target/bench-r19/flat_gradient_linear.html /tmp/f.png --json

# Grid (vs bloque temporal)
target/release/render tests/fixtures/render/grid_span.html /tmp/gs.png --json
target/release/render target/bench-r19/block_grid_span.html /tmp/b.png --json

# Find-in-page (shell headless)
target/release/megi tests/fixtures/render/find.html --screenshot /tmp/f.png \
    --width 800 --height 600 --find megi

# Render state/referrer
target/release/render tests/fixtures/render/css_state.html /tmp/s.png --json
```

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb` | Parse de documento «realista» de ~1 MiB (1 044 tablas 3×3). |
| html | `html/nested-div-10k/32k/64k` | Parse de `<div>` anidados. |
| html | `html/nested-div-64k-default-limits` | Mismo input con límites anti-DoS (profundidad 4096). |
| html | `html/massive-end-p` | 100 000 `</p>` consecutivos. |
| html | `html/massive-formatting` | 10 000 bloques `<b><i><u>` mal anidados. |
| html | `html/tag-40k-attrs` | Un elemento con 40 000 atributos. |
| html | `html/pretty-5k-blocks` | Documento indentado con 5 000 bloques. |
| css | `css/parse-10k-rules` | Parse de hoja CSS con 10 000 reglas. |
| css | `css/resolve-styles-2k/10k-nodes` | Cascada + `ComputedStyle` sobre ~2 005 / ~10 005 elementos. |
| css | `css/selector-match-simple/descendant` | 4 selectores sobre 10 005 elementos. |
| layout | `layout/realistic-1mb` | `layout_document` (28 195 elementos, 88 155 cajas), 800×600. |
| paint | `paint/build-display-list` | Display list: 34 158 items. |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` DEFLATE (57 422 B). |
| pipeline | `pipeline/html-to-png-800x600` | Parse → styles → layout → display list → raster → PNG. |
| fonts | (`feature fonts`) | No re-medido (sin cambios desde r13). |

**Fuera del bench (medido en §8–§12)**: gradientes CSS, grid, clipping,
`var()`/`calc()`, find-in-page, estado CSS/referrer, net y fuzz.

---

## 3. Baseline vigente (rondas 18+19, HEAD `d9e07e0`)

Mediana por fila de **6 pasadas** (load 4,0–5,35). Binario
`sha256 = c315ddf3…`. Suma de medianas: `TOTAL 417.010 ms`; pasada canónica
`TOTAL 415.663 ms`. **Los absolutos están inflados por el load**: la estimación
en calma (A/B r18→r19 sobre la baseline r18) es ~40 ms de layout y ~149 ms de
pipeline (ver §5.2).

| Benchmark | Carga | mediana (ms) | rango 6 pasadas (ms) | throughput |
|---|---|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | **20.910** | 20.453–21.454 | 47,9 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | **3.311** | 3.200–3.437 | 31,7 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | **10.465** | 9.664–10.958 | 32,1 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | **21.349** | 19.516–22.276 | 31,5 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | **13.813** | 13.538–13.959 | — |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | **19.053** | 18.583–20.789 | 5,25 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | **12.790** | 12.484–14.829 | 782 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | **12.468** | 11.838–13.421 | 3,21 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | **11.560** | 11.270–11.789 | 433 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | **14.494** | 13.480–15.851 | 41,1 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | **8.617** | 7.990–8.755 | 233 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | **40.876** | 40.454–42.339 | 245 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | **0.655** | 0.583–0.700 | 61,1 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | **1.218** | 1.101–1.259 | 32,9 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | **44.406** | 43.497–45.226 | 635 k elementos/s |
| `paint/build-display-list` | 34 158 items | **15.546** | 15.050–16.196 | — |
| `paint/rasterize-800x600` | 34 158 items → pixmap 800×600 | **0.956** | 0.885–1.025 | 503 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (57 422 B) | **8.892** | 8.729–10.279 | 6,16 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | **155.633** | 152.518–159.877 | — |
| **TOTAL (suma)** | | **417.010** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página. **El pipeline supera el presupuesto ≤140 ms** (ver §13).

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.31 · 32k: 10.47 · 64k: 21.35 ms | **Lineal** (3,2× → 3,16×; 2× → 2,04×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 14.49 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 2 005 el: 8.62 · 10 005 el: **40.88 ms** | +19 % vs r17 por custom properties/r18; r19 no lo agrava (A/B +0,6–1,2 %). |
| `layout/realistic-1mb` | 28 195 elementos → **44,41 ms** (est. calma ~40,4) | +5,3–5,8 % vs r18 en A/B (grid placement/gradients añaden código al camino de layout). |
| `paint/build-display-list` | 34 158 items → **15,55 ms** | +4,4–6,0 % vs r18 en A/B (preparación de gradientes/clipping). |
| `paint/render-to-png` | raster 0,96 + encoder ≈7,9 ms | Encoder PNG de r12, byte-idéntico. |
| `pipeline` | **155,6 ms** medido (est. calma ~149) | +3,4–4,7 % vs r18 (A/B), que ya estaba +14,1 % sobre r17. |

---

## 5. Rondas 18 y 19: qué entró y verificación

### 5.1 Cambios

**Ronda 18** (`...718d075`): `a872a86` grid MVP (fr/repeat/gaps/alineación);
`01339a3` clipping a cajas redondeadas (grupo offscreen, cap
`MAX_ROUNDED_GROUP_PIXELS = 16 777 216`) y `background-clip:text` (máscara de
runs, `MAX_TEXT_MASK_RUNS = 4096`); `dc0a056` custom properties/`var()`,
`calc()`, pseudo-clases de estado, parseo de grid y `clip:text`; `718d075` net
(SameSite + PSL ≈150 sufijos, Referrer-Policy 8 políticas, Range/206); `b565ee0`
fuzz targets + job build-only en PRs.

**Ronda 19** (`...d9e07e0`): `3c82cc5` **gradientes CSS** (linear/radial,
repeat, ángulos, cap `MAX_GRADIENT_STOPS = 64` y `MAX_GRADIENTS = 256`);
`3d052b3` **grid placement** con `span`, `minmax`, `auto-fill` y `order`;
`aef6df1` **render instala Referrer-Policy/SameSite en subrecursos y resuelve
estado CSS real**; `abcd8fa` **find-in-page** en el shell con resaltado y teclas
de navegación; `d9e07e0`/`8d065b7` fixture `css_state` y docs.

### 5.2 Verificación y atribución

r18 vs r17 quedó dominada por una **regresión real de la cascada** (documentada
en el intento anterior): `css/resolve-*` +19 % porque `ComputedStyle` gana
`custom_properties: HashMap<String, String>` y **cada** elemento pasa por
`custom::resolve_declarations()`. r19 **no la corrige** (A/B +0,6–1,2 %) y añade
su propio coste estructural:

| Fila (A/B prístino r18 → r19, 5 pares, load 2,5) | r18 | r19 | Δ |
|---|---:|---:|---:|
| `css/resolve-styles-10k-nodes` | 40,36 | 40,60 | **+0,6 %** |
| `css/parse-10k-rules` | 14,19 | 14,30 | +0,7 % |
| `layout/realistic-1mb` | 41,78 | 44,19 | **+5,8 %** |
| `paint/build-display-list` | 14,84 | 15,73 | **+6,0 %** |
| `paint/render-to-png-800x600` | 8,92 | 8,96 | +0,5 % |
| `pipeline/html-to-png-800x600` | 149,33 | 154,41 | **+3,4 %** |
| **TOTAL (suma)** | 405,74 | 416,49 | **+2,6 %** |

**Estimación en calma**: r18 limpio daba 38,4 ms de layout y 142,2 ms de
pipeline; aplicando los factores del A/B (×1,053 / ×1,047) → **~40,4 ms y
~149 ms** para r19. **Sigue por encima de ≤140** (y r18 ya lo estaba). Causas:
(a) la regresión de custom properties (heredada, top prioridad de fix:
asignación perezosa `Option<Box<HashMap>>`, saltar expansión si no hay `--*`,
cachear la colección UA); (b) +4,7 % de r19 en layout/display-list (grid
placement y preparación de gradientes en el camino genérico).

### 5.3 Nota de integridad

El intento de r18 se midió con el árbol roto por WIP ajeno; esa tabla (r18:
TOTAL 387,685; layout 38,389; pipeline 142,178; css resolve 39,300) se conserva
en el histórico de §15 como **R18**. Esta ronda mide el árbol limpio (r19) y
atribuye deltas con A/B prístino. `./tools/bench.sh` funciona ya; el binario se
recompiló al aterrizar `8d065b7` (solo docs) → `05e92a78…`.

---

## 6. Codificador PNG (sin cambios)

| Métrica | r18 | r19 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.437 | **8.892** | +5,4 % (load) |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden `--zoom 2` | 3 202 B | **3 202 B** | `4792779d…` |

---

## 7. Pipeline `render` y shell headless (r19)

9 ejecuciones (load ~2,0; goldens verificados):

| Fixture / modo | pared r19 | pared r18 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **28,79 ms** | 28,32 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,68 ms** | 8,74 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **29,65 ms** | 29,65 | 80 220 B (idéntico) |
| basic (shell headless) | **9,14 ms** | 9,31 | 21 090 B (idéntico) |
| basic `--zoom 2` (320×200) | **3,46 ms** | 3,64 | 3 202 B |
| basic CSS 160×100 (render) | **3,07 ms** | 3,61 | 3 202 B (`4792779d…`) |

Planos dentro de la varianza; goldens byte-idénticos. `--zoom 2` sigue siendo
viewport CSS `physical/zoom`; auto-refresh opt-in no se ejerce en `--screenshot`.

---

## 8. Coste de gradientes CSS (nuevo en r19)

Fixtures `gradient_linear`/`gradient_radial` vs equivalente temporal con los
mismos bloques sin `background-image` y texto sólido (`flat_gradient_*`).
7 pares intercalados + 3 `--json`:

| Fixture | `styles` (µs) | `raster` (µs) | Δpared (pares) | PNG | PNG plano | md5 |
|---|---:|---:|---:|---:|---:|---|
| `gradient_linear.html` (6 gradientes, incl. repeat/sized/clip:text) | 457 | **1 992** | **+3,23 ms** | 22 585 B | 14 636 B | `26f9b4bc…` |
| `gradient_radial.html` (radial + linear) | 454 | **1 963** | **+8,71 ms** | 32 133 B | 13 812 B | `d1b597ca…` |

- La etapa `styles` apenas cambia (±0,06 ms); **el coste está en el rasterizado
  por píxel del gradiente** (linear +0,83 ms; radial +1,26 ms de `raster` en
  estos fixtures) y el Δpared está inflado por el load (pares). **Deterministas**
  (2 pasadas idénticas).
- **Caps**: `MAX_GRADIENT_STOPS = 64` (los stops extra se truncan) y
  `MAX_GRADIENTS = 256` por documento.
- Los PNG cambian de tamaño por el contenido (gradiente y texto recortado); en
  el pipeline real el encoder DEFLATE comprime bastante mejor que el writer
  *stored* del ejemplo SVG.

---

## 9. Coste de grid (r18 MVP + r19 spans/auto-fill)

5 fixtures del repo vs equivalente temporal `display:block`
(`target/bench-r19/block_grid_*`). 7 pares intercalados + 3 `--json`:

| Fixture | `layout` grid/block (µs) | `paint` (µs) | Δpared (mediana) | items | md5 |
|---|---:|---:|---:|---:|---|
| `grid_columns` (`1fr` ×3) | 18 / 16 | 4 | −0,14 ms | 4 | `2d6714bc…` |
| `grid_rows` | 21 / 15 | 6 | −0,48 ms | 5 | `c411ff6d…` |
| `grid_alignment` | 17 / 16 | 4 | +0,59 ms | 3 | `93d1126d…` |
| `grid_span` (`grid-column:span 2`, `grid-row:span 2`) | 26 / 20 | 8 | −0,28 ms | 6 | `0c376406…` |
| `grid_autofill` (`repeat(auto-fill, minmax(60px,1fr))`) | 21 / 18 | 5 | +0,14 ms | 5 | `d2255ca3…` |

- Resolver grid para 3–6 hijos cuesta **+2–6 µs de layout**; el Δpared es ruido
  (pares intercalados, stdev ≤1,8 ms). **Deterministas**. El bench no usa grid.
- `span`/`auto-fill` no muestran coste extra perceptible en estos tamaños.

---

## 10. Coste de find-in-page (nuevo en r19)

`find.html` (3 apariciones de «Megi», una en mayúsculas) con `megi --screenshot
--find megi` vs sin `--find`, 800×600, 7 runs:

| Modo | pared (mediana) | PNG | md5 |
|---|---:|---:|---|
| sin `--find` | **10,89 ms** | 30 056 B | `19c35c18…` |
| con `--find megi` | **16,89 ms** | 29 950 B | `a818f32a…` |

- Δpared **+6,0 ms** (load ~4,5; incluye construir el índice de texto del
  documento y pintar el overlay de resaltado). La búsqueda es **pura y O(texto)**
  (recorrido del DOM + `to_lowercase`), sin I/O; documentada como barata.
- El overlay **no re-renderiza la página** al cambiar de hit: se pinta sobre el
  PNG ya rasterizado (el fixture tiene 3 hits; el PNG cambia solo por el
  resaltado). En modo ventana, `Enter`/`Shift+Enter` mueven el hit sin volver a
  resolver estilos/layout.
- El coste real de un buscador en uso (pulsaciones) es el repintado del overlay,
  no el pipeline; no se mide en headless.

---

## 11. Coste de render con estado CSS y referrer (r19)

`css_state.html` (inputs `:checked`/`:disabled`/`:focus`/`:placeholder-shown`,
`div:checked`, `button:not(:disabled):not(:checked)`) vs equivalente temporal
sin esas reglas (`css_state_static.html`), 7 pares + 3 `--json`:

| Métrica | con estado | estático | Δ |
|---|---:|---:|---:|
| etapa `styles` | 657 µs | 660 µs | **≈0** |
| Δpared (pares) | — | — | +1,58 ms (ruido, stdev 1,68) |
| PNG | 13 920 B (`0f37ba13…`) | — | overlay/estado determinista |
| `styles_pre_script` | 0 µs (sin scripts) | — | — |

**Lectura**: resolver `StateContext` (`:focus`/`:hover`/`:checked`/…) durante la
cascada **no añade coste medible** en este fixture; `aef6df1` instala además
**Referrer-Policy** (derivada de la respuesta o `<meta name=referrer>`) y
**SameSite** en subrecursos: clasificación pura (`psl`/`referrer`, ≈0 en el
bench) **sin peticiones extra**, con la mejora de seguridad de recortar el
referrer y retener cookies cross-site. El bench no usa estado ni red.

---

## 12. Clipping, `var()`/`calc()`, net y fuzz (r18, resumen)

**Clipping / `clip:text`** (fixtures temporales `target/bench-r18/clip_*`):
`clip_base` raster 5 031 µs / 25 137 B; `clip_radius` (200×150 con
`overflow:hidden;border-radius:32px`) raster **16 124 µs** (+11,1 ms) y PNG
24 078 B (`4ae46680…`); `clip_radius_text` raster **21 767 µs** (+16,7 ms) y
19 402 B (`35d3f69c…`); `clip_text` raster 2 845 µs (−5,3 ms de pared) y
18 790 B (`72f72fa1…`). El coste es de **raster por área**; caps:
`MAX_ROUNDED_GROUP_PIXELS = 16 777 216` (16,7 Mpx), `MAX_TEXT_MASK_RUNS = 4096`,
`MAX_SHADOWS = 32`, `MAX_SHADOW_BLUR = 64`, `MAX_BACKGROUND_TILES = 4096`,
`MAX_STACKING_CONTEXT_DEPTH = 256`, `MAX_FLOAT_DEPTH = 64`.

**`var()`/`calc()`**: documento con `:root` + `var()` en colores/tamaños y
`calc()` en márgenes vs literales → etapa `styles` **813 → 959 µs (+146 µs,
+18 % de esa etapa)**; Δpared −0,30 ms (ruido). El coste estructural real es el
registro por elemento (§5.2), no la expansión de estos fixtures.

**Net**: r18 añadió `psl.rs` (355 LOC, ≈150 sufijos + fallback), `referrer.rs`
(270 LOC, 8 políticas), `range.rs` (239 LOC, 206) y SameSite en cookies
(~1 020 LOC); r19 **integra** referrer/SameSite en `render`. Son módulos puros
sin I/O: **≈0 en el bench** y despreciables por petición; el coste de CI es
compilarlos (ya incluido).

**Fuzz**: no se ejecuta en el bench. **7 targets** (`html_parse`, `css_parse`,
`selectors`, `net_url`, `render_pipeline`, `js_eval`, `svg_parse`); caché local
`fuzz/target` 1,6 GB; referencia del carril **~15 s incremental / 3–5 min el
primer build**. CI: job **build-only en PRs** (`cargo +nightly fuzz build`,
`rust-cache`, timeout 20 min, `cargo-fuzz`) → **minutos por PR**, **0 en el
bench**.

---

## 13. Presupuestos orientativos

| Benchmark / medida | Baseline r19 (medida) | Est. calma | Presupuesto | % | Veredicto |
|---|---:|---:|---:|---:|---|
| `html/realistic-1mb` | 20,9 ms | ~20,4 | ≤ 100 ms | 21 % | ✅ |
| `css/parse-10k-rules` | 14,5 ms | ~14,0 | ≤ 30 ms | 48 % | ✅ |
| `css/resolve-styles-10k-nodes` | 40,9 ms | ~39,9 | ≤ 75 ms | 55 % | ⚠️ +19 % heredado r18 |
| `layout/realistic-1mb` | 44,4 ms | **~40,4** | **≤ 45 ms** | **98,7 % medida** (89,8 % est.) | ⚠️ justo |
| `paint/build-display-list` | 15,5 ms | ~14,8 | ≤ 25 ms | 62 % | ✅ |
| `paint/rasterize-800x600` | 0,96 ms | ~0,93 | ≤ 25 ms | 4 % | ✅ |
| `paint/render-to-png-800x600` | 8,9 ms | ~8,5 | ≤ 50 ms | 18 % | ✅ |
| `pipeline/html-to-png-800x600` | **155,6 ms** | **~149** | **≤ 140 ms** | **111,2 % medida** (106,4 % est.) | ❌ **EXCEDIDO** |
| `render` showcase 1024×768 | 28,8 ms | — | **≤ 35 ms** | 82,3 % | ✅ |
| shell headless showcase | 29,7 ms | — | ≤ 35 ms | 85 % | ✅ |
| shell basic `--zoom 2` | 3,5 ms | — | — | — | ✅ |
| gradientes CSS (fixtures 6 bloques) | raster +0,8–1,3 ms | — | — | — | ✅ caps 64/256 |
| grid (3–6 hijos, spans/auto-fill) | layout +2–6 µs | — | — | — | ✅ |
| find-in-page (3 hits) | +6,0 ms (load) | — | — | — | ✅ overlay no re-render |
| render state/referrer | styles ≈0 | — | — | — | ✅ seguridad mejorada |
| clipping redondeado 200×150 | raster +11–17 ms | — | — | — | ✅ coste de área |
| fuzz build-only PR | ~minutos CI | — | — | — | ✅ 0 en bench |

**Conclusión**: se mantienen los presupuestos (pipeline ≤140, layout ≤45,
showcase ≤35), pero **el pipeline los excede** (est. calma ~149 ms = 106 %;
medido 155,6 = 111 %). Es **bloqueante** y viene de dos causas acumuladas:
1. **Regresión r18 de la cascada** (custom properties por elemento, +19 % en
   `css/resolve-*`): fix prioritario (mapa perezoso/skip si no hay `--*`).
2. **Coste estructural r19** (+4,7 % pipeline; layout +5,3–5,8 %, display-list
   +4,4–6,0 %) por grid placement y preparación de gradientes en el camino
   genérico.
No se sube el presupuesto para no ocultar la regresión. Con el fix de (1) se
espera volver a ~124–130 ms y cumplir.

---

## 14. Cómo comparar y detectar regresiones

1. Medir con el sistema ocioso (`uptime` < ~1,5). En r19 no se logró load bajo:
   se publica la tabla medida con load 4–5,35 **y** la estimación corregida por
   A/B. No comparar absolutos de esta ventana sin corregir.
2. Si el árbol no compila por WIP ajeno, medir desde `git archive <commit>` en
   `target/` (r13/r18).
3. Para atribuir un delta a un commit, compilar **dos archivos prístinos**
   (antes/después) y ejecutar los binarios **intercalados** (load igual para
   ambos): así se obtuvo +4,7 % pipeline r18→r19 y se confirmó que css resolve
   no cambia (+0,6 %).
4. Guardar JSON y comparar medianas:

   ```bash
   python3 - <<'EOF'
   import json
   a = {r["name"]: r["median_ms"] for r in json.load(open("target/bench-before.json"))["results"]}
   b = {r["name"]: r["median_ms"] for r in json.load(open("target/bench-latest.json"))["results"]}
   for name in sorted(a):
       delta = (b[name] - a[name]) / a[name] * 100.0
       flag = "  <-- >15%" if abs(delta) > 15.0 else ""
       print(f"{name:<38} {a[name]:>10.2f} -> {b[name]:>10.2f} ms ({delta:+.1f}%){flag}")
   EOF
   ```
5. Regresión si la mediana empeora >15 % reproducible. En r18/r19 el pipeline
   **excede** ≤140 → bloqueante.
6. Features nuevas: gradientes (`gradient_*` vs `flat_gradient_*`), grid
   (`grid_*` vs `block_grid_*`), find (`--find`), estado (`css_state` vs
   `css_state_static`), clipping (`clip_*`), var/calc (`vars_*`).
7. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   phase4 basic `9e957177…` 42 853 B, bench PNG 57 422 B, zoom `4792779d…`
   3 202 B, find `a818f32a…` 29 950 B, gradientes `26f9b4bc…`/`d1b597ca…`.

---

## 15. Comparación histórica

Diecisiete baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | **22:27** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | **`d9e07e0`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | limpio | docs dirty | limpio | limpio | docs dirty | docs dirty | WIP html | limpio | limpio | rustdoc ajeno | WIP css roto | **limpio** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | `43f7748d…` | `a188a1da…` | `c34ff65c…` | `d655c1f2…` | `153b8aff…` | `1f1dbbdd…` | `39909f01…` | `8cfbb715…` | `4185c2de…` | **`c315ddf3…`** |
| Load (bench) | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | **4,0–5,35** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | **20.910** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | **40.876** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | **44.406** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | **15.546** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | 0.843 | 0.993 | 0.881 | 1.082 | 0.925 | 1.052 | 0.999 | 0.947 | 1.003 | 0.921 | 0.927 | **0.956** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | **8.892** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | **155.633** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** |

### Ronda 18 → ronda 19, fila a fila (medido; la subida es load + coste estructural)

| Benchmark | R18 (ms) | R19 (ms) | Δ | Est. calma |
|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 20.440 | **20.910** | +2,3 % | ~20,4 |
| `html/nested-div-64k` | 20.111 | **21.349** | +6,2 % | ~20,1 |
| `html/massive-end-p` | 18.425 | **19.053** | +3,4 % | ~18,4 |
| `css/parse-10k-rules` | 13.957 | **14.494** | +3,8 % | ~14,2 |
| `css/resolve-styles-2k-nodes` | 8.174 | **8.617** | +5,4 % | ~8,5 (A/B +0,6 %) |
| `css/resolve-styles-10k-nodes` | 39.300 | **40.876** | +4,0 % | ~40,4 (A/B +0,6 %) |
| `layout/realistic-1mb` | 38.389 | **44.406** | +15,7 % | **~40,4** (A/B +5,3–5,8 %) |
| `paint/build-display-list` | 13.848 | **15.546** | +12,3 % | ~14,8 (A/B +4,4–6,0 %) |
| `paint/render-to-png-800x600` | 8.437 | **8.892** | +5,4 % | ~8,5 (A/B +0,5 %) |
| `pipeline/html-to-png-800x600` | 142.178 | **155.633** | +9,5 % | **~149** (A/B +3,4–4,7 %) |
| **TOTAL (suma)** | **387.685** | **417.010** | +7,6 % | ~405 (A/B +2,6 %) |

Factores A/B (5 pares, load 2,5): css resolve +0,6–1,2 %, layout +5,3–5,8 %,
display-list +4,4–6,0 %, pipeline +3,4–4,7 %, TOTAL +2,6 %. La diferencia entre
el Δ medido y el A/B es el efecto del load (4–5,35 vs 1,4–2,0 en r18).

---

## 16. Limitaciones y notas

- **Ventana con load alto** (4,0–5,35; picos 7–19 antes): los absolutos de r19
  no son comparables directamente; se dan la tabla medida, la pasada canónica
  (`TOTAL 415.663`) y la **estimación corregida por A/B** (~149 ms pipeline).
  Re-baseline en host quieto pendiente.
- **Regresión heredada de r18** en cascada (+19 % `css/resolve-*`): custom
  properties por elemento (`ComputedStyle.custom_properties: HashMap` +
  `resolve_declarations` en cada nodo). **Bloqueante**: pipeline >140. Fix
  propuesto: mapa perezoso/`Option<Box<…>>`, saltar expansión si la hoja no
  declara `--*`, cachear la colección UA.
- **Coste estructural r19**: +4,7 % pipeline (layout +5,3–5,8 %, display-list
  +4,4–6,0 %) por grid placement/gradientes en caminos genéricos. A vigilar.
- **Tree/HEAD**: medición sobre árbol limpio de `d9e07e0`; `8d065b7` (docs) llegó
  a mitad y solo recompiló el binario. El intento de r18 con árbol roto se
  conserva como R18 en el histórico.
- **Gradientes CSS**: coste por píxel en raster; caps `MAX_GRADIENT_STOPS=64` y
  `MAX_GRADIENTS=256`; no soportados `conic-gradient` ni `calc()` mixto en stops
  (próximo foco).
- **Grid**: medido con 3–6 hijos, incluidos `span`/`auto-fill`/`minmax`; sin
  `grid-template-areas`/dense/`order` complejo medido; el bench no usa grid.
- **Find-in-page**: coste medido en headless (3 hits, +6 ms con load); el
  overlay no re-renderiza al cambiar de hit; el coste en uso real es el repintado
  del overlay, no el pipeline.
- **Estado/referrer**: `StateContext` sin coste medible; Referrer-Policy/SameSite
  instalados en subrecursos (seguridad), clasificación pura ≈0.
- **Fuzz/net**: fuzz no entra en el bench (7 targets, job PR build-only ~minutos
  de CI); net puro ≈0 por petición.
- **`website/` y `pages.yml`**: del dueño; no se tocan.
