# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 21:37–21:55 (Europe/Madrid). Ronda 17:
  **flexbox MVP** (row/column/wrap), **decoraciones** (`border-radius`,
  `box-shadow`, `text-shadow` con cap de blur 64), **formularios** (estilos UA +
  controles visibles/editables en el shell) y **SVG `<pattern>`/`<text>`**.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `67f7c57`**
  (`feat(shell): visible and editable forms with focus, typing and snapshot re-render`).
- **Estado del árbol**: limpio de código salvo un **comentario rustdoc** en
  `crates/render/src/lib.rs` de otro agente (sin cambio de comportamiento) y
  docs en vuelo (`dirty = 11` en la pasada canónica). `website/` y
  `.github/workflows/pages.yml` son del dueño y **no se tocan**.
- **Binario de bench**: `target/release/bench`, `sha256 = 8cfbb715…`
  (4 467 120 B) en la primera tanda de pasadas y `3115464e…` (4 467 056 B)
  tras la recompilación del script (misma fuente).
- **Artefactos guardados** (gitignored):
  - `target/bench-r17-run*.json`, `target/bench-aggregate-r17.json`
    (`sha256 = 63c38f89…`), `target/bench-latest.txt`.
  - `target/bench-r17/flex_summary.json`, `decorations_summary.json`,
    `features_render_shell.json` y los fixtures temporales `deco_*.html` /
    `block_flex_*.html`.

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §14 (rondas 2–17; la 14 no publicó baseline).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. **El crate `bench` y sus generadores no cambiaron en esta ronda.**

El bench **no usa flexbox, decoraciones, SVG, formularios ni JS**: sus
generadores no contienen `display:flex`, `border-radius`/`box-shadow`/
`text-shadow`, `<svg>`, `<form>` ni `<script>`. Las capacidades nuevas de r17 se
miden aparte con `render`/`shell`/`svg` (§8–§11) y **no cambian el pipeline por
defecto** del bench.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- **La tabla de §3 es la mediana, por fila, de 6 pasadas válidas** tomadas en
  una **ventana final de load 0,44–0,69** (de 54 intentos totales; ver §5.3).
  Criterio: `layout` ≤43 ms, `pipeline` ≤135 ms, `render-to-png` ≤10,2 ms y
  `html/realistic` ≤22,5 ms.
- `setup_ms` de las válidas: 184,2–192,4 ms. La pasada canónica de
  `./tools/bench.sh` dio `TOTAL 356.981 ms`
  (`layout 36.521`, `pipeline 123.769`, `render-to-png 8.332`).
- Render/shell: 11 ejecuciones intercaladas (pared) + 7 `--json` para etapas.
  Flex: 7 pared + 3 `--json` por fixture, con equivalente `display:block`
  temporal. Decoraciones: 7 pared + 3 `--json` por fixture temporal.
  Formularios: 7 ejecuciones shell + 7 render. SVG: 7 ejecuciones por fixture.
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E), governor `powersave` |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` por defecto |
| Carga y térmica | ventana válida: load 0,44–0,69, package 63 °C; primeras ventanas con load 2,5–8,6 y núcleos a 400 MHz (`powersave`) → descartadas |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Bench (6 pasadas válidas en ventana de load < 0,7)
./tools/bench.sh
for i in 1 2 3 4 5 6; do target/release/bench --json > "target/bench-r17-run$i.json"; done

# Flexbox (con equivalente block temporal en target/ para el Δ)
target/release/render tests/fixtures/render/flex_row.html /tmp/f.png --json
target/release/render target/bench-r17/block_flex_row.html /tmp/b.png --json

# Decoraciones (fixtures temporales en target/, no en el repo)
target/release/render target/bench-r17/deco_shadow_64.html /tmp/d.png --json
#   deco_base, deco_radius, deco_shadow_{8,32,64,128}, deco_text_{8,64}, deco_all

# Formularios (shell headless; la interacción no se mide sin ventana)
target/release/megi tests/fixtures/render/forms.html \
    --screenshot /tmp/forms.png --width 640 --height 480
target/release/render tests/fixtures/render/forms.html /tmp/forms.png \
    --width 640 --height 480 --json

# SVG pattern/text (crate svg)
target/release/examples/svg_to_png crates/svg/tests/fixtures/pattern.svg /tmp/p.png
target/release/examples/svg_to_png crates/svg/tests/fixtures/text.svg /tmp/t.png
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
| fonts | (`feature fonts`) | No re-medido en r17 (sin cambios; webfonts medidas en r13). |

**Fuera del bench (medido en §8–§11)**: flexbox, decoraciones, formularios,
SVG pattern/text y la página showcase/basic del render.

---

## 3. Baseline vigente (2026-11-10, HEAD `67f7c57`)

Mediana por fila de **6 pasadas válidas** (5 iteraciones, 1 warm-up; *heavy* 3).
`setup`: 184,2–192,4 ms. Binario `sha256 = 8cfbb715…`. Suma de medianas:
`TOTAL 360.410 ms`; pasada canónica del script: `TOTAL 356.981 ms`.

| Benchmark | Carga | mediana (ms) | rango 6 pasadas (ms) | throughput |
|---|---|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | **20.581** | 20.113–20.769 | 48,6 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | **3.177** | 3.116–3.259 | 33,0 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | **10.244** | 10.014–10.378 | 32,8 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | **20.164** | 19.866–20.725 | 33,3 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | **13.183** | 12.642–13.533 | — |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | **18.559** | 18.180–18.767 | 5,39 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | **11.789** | 11.358–12.032 | 848 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | **12.253** | 12.097–12.466 | 3,26 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | **11.037** | 10.832–11.170 | 453 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | **13.555** | 13.234–14.633 | 44,0 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | **6.879** | 6.435–7.057 | 291 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | **32.774** | 32.279–33.800 | 305 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | **0.663** | 0.625–0.678 | 60,4 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | **1.270** | 1.145–1.286 | 31,5 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | **37.085** | 36.200–37.967 | 760 k elementos/s |
| `paint/build-display-list` | 34 158 items | **13.101** | 12.518–13.246 | — |
| `paint/rasterize-800x600` | 34 158 items → pixmap 800×600 | **0.921** | 0.906–1.245 | 521 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (57 422 B) | **8.604** | 8.391–8.840 | 6,36 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | **124.575** | 121.798–133.930 | — |
| **TOTAL (suma)** | | **360.410** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página.

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.18 · 32k: 10.24 · 64k: 20.16 ms | **Lineal** (3,2× → 3,22×; 2× → 1,97×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 13.56 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 2 005 el: 6.88 · 10 005 el: 32.77 ms | **Lineal** (~305 k el/s; +3,8 % vs r16 por reglas UA de formularios, ver §5.2). |
| `layout/realistic-1mb` | 28 195 elementos → **37,09 ms** | Plano vs r16 (37,42); sin coste de flex/decoraciones en el fixture. |
| `paint/build-display-list` | 34 158 items → **13,10 ms** | +5,3 % vs r16: pequeña sobrecarga de preparación de decoraciones (ver §5.2). |
| `paint/render-to-png` | raster 0,92 + encoder ≈7,7 ms | Encoder PNG de r12, byte-idéntico. |
| `pipeline` | 124,6 ms (121,8–133,9) | Reparto: layout 37,1 + CSS 32,8 + parse 20,6 + display list 13,1 + PNG 8,6. |

---

## 5. Ronda 17: flex, decoraciones, formularios y SVG pattern/text

### 5.1 Qué entró en la ronda

- `8f60cb8 feat(svg)`: **`<pattern>`** (tiling `userSpaceOnUse` con
  `patternTransform`) y **`<text>` básico** (familia/tamaño/anchor/bold).
- `082f52d feat(css,layout,paint)`: **flexbox MVP** (`display:flex`,
  row/column/wrap, `justify-content`, `align-items`, `gap`, `flex: 0 0 N`),
  **`border-radius`**, **`box-shadow`** y **`text-shadow`** (blur con cap 64) y
  estilos UA de formularios.
- `67f7c57 feat(shell)`: **formularios visibles y editables** (foco, tecleo,
  re-render con snapshot).

### 5.2 Verificación: no regresión en el bench

| Métrica | Esperado en la ronda | Nuestra medición (6 pasadas) | Pasada canónica | Veredicto |
|---|---:|---:|---:|---|
| `layout/realistic-1mb` | ~37 ms | **37.085 ms** (−0,9 % vs r16) | 36.521 | ✅ sin regresión |
| `pipeline/html-to-png` | ~120–122 ms | **124.575 ms** (+1,8 %) | 123.769 | ≈ varianza / +1–2 % |
| `paint/render-to-png` | ~9 ms | **8.604 ms** (−4,6 %) | 8.332 | ✅ |
| `html/realistic-1mb` | ~20–21 ms | **20.581 ms** (−2,5 %) | 20.518 | ✅ |
| `css/resolve-styles-10k` | — | **32.774 ms** (+3,8 %) | 32.566 | ⚠️ pequeño coste de reglas UA de formularios |
| `paint/build-display-list` | — | **13.101 ms** (+5,3 %) | 13.202 | ⚠️ pequeña sobrecarga de preparación de decoraciones |
| **TOTAL (suma)** | — | **360.410 ms** (−0,6 % vs r16) | 356.981 | ✅ plano |

**Explicación**: el bench no usa ninguna capacidad nueva, pero r17 añadió
**reglas UA de formularios** (que agrandan ligeramente la cascada:
`css/resolve-*` +3,8 % y `selector-match` +4,7 %) y **preparación de
decoraciones** en la display list (+5,3 %), aunque no haya decoraciones en el
fixture. Son sobrecargas pequeñas (~1–1,5 ms en el pipeline) y **candidatas a
micro-optimizar** (p. ej. saltar el pase de decoraciones si no hay ninguna).
`layout` y `render-to-png` quedan planos, y HTML/CSS parse mejoran con la
ventana más limpia; **no hay regresión de layout/pipeline atribuible a las
features** más allá de ese 1–2 %.

### 5.3 Nota de la ventana de medición

Los primeros 25 intentos se hicieron con load 2,6–8,6 y núcleos a **400 MHz**
(governor `powersave`, package 63 °C): dieron `layout 38–45` y
`pipeline 127–139`, no comparables. Tras dejar enfriar/estabilizar el host, la
ventana final (load **0,44–0,69**) dio `layout 36,2–38,0` y
`pipeline 121,8–133,9`. La tabla publica mediana de las 6 válidas de esa
ventana; el resto se descarta y se documenta para que la cifra sea reproducible.

---

## 6. Codificador PNG (sin cambios en r17)

| Métrica | r16 | r17 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 9.015 ms | **8.604 ms** | −4,6 % (varianza favorable) |
| — rasterizar | 1.003 ms | **0.921 ms** | varianza |
| — codificar PNG | ~8,01 ms | **~7,68 ms** | encoder de r12, sin cambios |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden basic `--zoom 2` (CSS 160×100) | 3 202 B | **3 202 B** | `4792779d…` |

---

## 7. Pipeline `render` y shell headless (ronda 17)

11 ejecuciones intercaladas (ventana de load bajo):

| Fixture / modo | pared r17 | pared r16 | rango r17 | PNG |
|---|---:|---:|---:|---:|
| showcase 1024×768 (render) | **27,53 ms** | 28,24 ms | 26,61–30,03 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,01 ms** | 8,62 ms | 8,07–9,97 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **27,98 ms** | 28,56 ms | 26,65–29,40 | 80 220 B (idéntico) |
| basic (shell headless) | **9,20 ms** | 8,70 ms | 8,69–10,45 | 21 090 B (idéntico) |
| basic `--width 320 --height 200 --zoom 2` (shell) | **3,56 ms** | 3,58 ms | 3,17–4,52 | 3 202 B |
| basic CSS 160×100 (render, referencia) | **3,26 ms** | 3,14 ms | 2,49–3,82 | 3 202 B (`4792779d…`) |

Todo plano frente a r16 dentro de la varianza; goldens intactos. `--zoom 2`
headless sigue equivaliendo a viewport CSS `physical/zoom` con PNG
byte-idéntico; auto-refresh sigue siendo opt-in (no se ejerce en `--screenshot`).

---

## 8. Coste de flexbox (nuevo en r17)

Fixtures del repo de 3–4 cajas + equivalente temporal con `display:block`
(mismo DOM, en `target/bench-r17/block_flex_*.html`). 7 runs + 3 `--json`:

| Fixture | flex pared (ms) | block pared (ms) | Δpared | `layout` flex/block (µs) | `paint` | items | md5 |
|---|---:|---:|---:|---:|---:|---:|---|
| `flex_row.html` (space-between + gap) | **7,83** | 7,44 | **+0,39** | 13 / 12 | 4 | 4 | `c28ce538…` |
| `flex_column.html` | **7,51** | 7,59 | −0,08 | 15 / 14 | 3 | 4 | `8d06d66c…` |
| `flex_wrap.html` (wrap + flex-basis) | **8,02** | 8,79 | −0,77 | 15 / 11 | 4 | 5 | `7fbf998e…` |

- El coste de flexbox en estos fixtures es **de ruido** (<1 ms de pared; etapa
  `layout` 11–15 µs): resolver `justify-content`/`align-items`/`gap`/`wrap`
  para pocos hijos es barato. **Determinismo**: 2 pasadas idénticas por fixture
  (md5 iguales) en los tres.
- El bench no contiene flex, así que las filas de §3 no incluyen este coste; en
  contenedores grandes (decenas/cientos de hijos) el coste escalaría, pero no
  se ha medido aquí (no hay fixture del repo con eso).

---

## 9. Coste de decoraciones (radius y sombras, cap 64)

Fixtures temporales en `target/bench-r17/deco_*.html` (3 cajas 200×100, texto
32 px). 7 runs + 3 `--json`; base = mismo documento sin decoraciones:

| Fixture | pared (ms) | Δpared | `paint` (µs) | `raster` (µs) | Δraster | PNG | items |
|---|---:|---:|---:|---:|---:|---:|---:|
| `deco_base` (sin decoración) | **8,28** | — | 3 | 550 | — | 13 772 B | 5 |
| `deco_radius` (radius 32) | **14,28** | **+6,00** | 5 | 4 836 | **+4 286** | 16 674 B | 11 |
| `deco_shadow_8` (blur 8) | **11,62** | **+3,34** | 4 | 2 280 | +1 730 | 16 596 B | 8 |
| `deco_shadow_32` (blur 32) | **16,58** | **+8,30** | 3 | 2 769 | +2 219 | 23 686 B | 8 |
| `deco_shadow_64` (blur 64) | **19,68** | **+11,40** | 4 | 6 939 | **+6 389** | 31 857 B | 8 |
| `deco_shadow_128` (blur 128 → cap) | **18,72** | **+10,44** | 3 | 4 079 | +3 529 | **31 857 B** | 8 |
| `deco_text_8` (text-shadow 8) | **9,95** | +1,67 | 6 | 1 702 | +1 152 | 29 781 B | 7 |
| `deco_text_64` (text-shadow 64) | **12,68** | +4,40 | 4 | 2 412 | +1 862 | 24 327 B | 7 |
| `deco_all` (radius+shadow 32+text 32) | **18,27** | **+9,99** | 5 | 5 801 | +5 251 | 34 368 B | 13 |

**Lectura**:

- La etapa `paint` apenas cambia (3–6 µs: las decoraciones se expanden a items
  extra de la display list). **El coste está en el rasterizado**: `radius`
  añade **+4,3 ms** (AA de esquinas), `box-shadow` escala con el blur
  (+1,7 ms a 8 → +6,4 ms a 64) y `text-shadow` es más barato (+1,2 → +1,9 ms).
- **El cap de blur se cumple**: `deco_shadow_128` produce **el mismo PNG**
  (31 857 B) que `deco_shadow_64` — el blur se clampa a 64 y el resultado es
  determinista.
- Los `display_items` crecen de 5 a 7–13 según decoración (más capas de
  pintado): eso explica el +5,3 % de `build-display-list` del bench (§5.2),
  aunque el fixture del bench no tenga decoraciones (es la preparación genérica).
- El bench no usa decoraciones: no aparecen en §3. El coste es de raster y
  proporcional al área/blur, no al número de cajas.

---

## 10. Coste de formularios (shell/render, r17)

`tests/fixtures/render/forms.html` (fieldset, input text/textarea/select/
checkbox/radio/submit/button, uno disabled), 640×480:

| Medida | shell headless | render |
|---|---:|---:|
| pared (mediana 7 runs) | **8,17 ms** | **6,97 ms** |
| PNG | 14 355 B (`b8b9a204…`) | 14 355 B (idéntico) |
| `display_items` | 51 | 51 |
| `hit_regions` | 24 | 24 |
| `styles` / `layout` / `raster` | 532 / 143 / 885 µs | 522 / 146 / 880 µs |

**Lectura**: pintar formularios cuesta **~7–8 ms** en total (el PNG+driver es
~2 ms y el resto es estilos UA + layout/paint de controles); el shell headless
añade ~1,2 ms por su arranque. **La interacción (foco, tecleo, cambios de
estado) no se mide en headless** (`--screenshot` renderiza una vez; el
re-render con snapshot y la edición solo ocurren en modo ventana), así que este
número es el coste de **primer pintado**, no el de un formulario en uso.

---

## 11. Coste SVG `<pattern>` y `<text>` (r17)

Ejemplo `svg_to_png` (7 runs; la pared incluye el PNG writer *stored* del
ejemplo):

| Fixture | pared (ms) | rango | PNG | Notas |
|---|---:|---:|---:|---|
| `crates/svg/tests/fixtures/pattern.svg` (120×80; checker + dots tiling) | **3,05** | 2,37–3,50 | **38 548 B** | `<pattern>` `userSpaceOnUse` + transform |
| `crates/svg/tests/fixtures/text.svg` (240×120; 3 textos con anchor/bold) | **3,26** | 2,92–4,49 | **115 393 B** | `<text>` básico; el PNG stored de 240×120 domina el tamaño |

**Lectura**: `<pattern>` y `<text>` añaden un coste pequeño en estos tamaños
(~3 ms de pared total cada uno, con arranque + parse + raster + PNG); el
`<text>` renderiza con la fuente del sistema (DejaVu) y el `PNG stored` del
ejemplo pesa ~115 KB. Siguen sin soporte filtros, máscaras, `textPath`,
justificación avanzada ni `font-face` interno en SVG (próximos pasos del
crate, no del pipeline HTML).

---

## 12. Coste de floats, posicionamiento, stacking y opacidad (referencia r4)

| Variante | Δ vs control | Factor |
|---|---:|---:|
| `position: relative` | +0,93–1,13 ms | 1,02× |
| `position: relative; z-index` | +0,20–0,59 ms | 1,00–1,01× |
| `opacity: 0.85` | +2,96–3,55 ms | 1,05–1,06× |
| `float: left` | +12,54–12,72 ms | **1,22×** |

El documento realista del bench no usa floats ni opacidad; no interviene en §3.

---

## 13. Presupuestos orientativos

Se **mantienen** los presupuestos de r15/r16:

| Benchmark / medida | Baseline r17 | Presupuesto | % | Veredicto |
|---|---:|---:|---:|---|
| `html/realistic-1mb` | 20,6 ms | ≤ 100 ms | 21 % | ✅ |
| `html/tag-40k-attrs` | 12,3 ms | ≤ 50 ms | 25 % | ✅ |
| `html/massive-end-p` | 18,6 ms | ≤ 50 ms | 37 % | ✅ |
| `css/parse-10k-rules` | 13,6 ms | ≤ 30 ms | 45 % | ✅ |
| `css/resolve-styles-10k-nodes` | 32,8 ms | ≤ 75 ms | 44 % | ✅ |
| `layout/realistic-1mb` | **37,1 ms** | **≤ 45 ms** | **82,4 %** (canónica 81,2 %) | ✅ mantener |
| `paint/build-display-list` | 13,1 ms | ≤ 25 ms | 52 % | ✅ |
| `paint/rasterize-800x600` | 0,92 ms | ≤ 25 ms | 4 % | ✅ |
| `paint/render-to-png-800x600` | 8,6 ms | ≤ 50 ms | 17 % | ✅ |
| `pipeline/html-to-png-800x600` | **124,6 ms** | **≤ 140 ms** | **89,0 %** (canónica 88,4 %) | ✅ mantener |
| `render` showcase 1024×768 | **27,5 ms** | **≤ 35 ms** | **78,6 %** | ✅ mantener |
| shell headless showcase | 28,0 ms | ≤ 35 ms | 80 % | ✅ |
| shell basic `--zoom 2` 320×200 | 3,6 ms | — | — | ✅ |
| flex row/column/wrap (3–4 hijos) | ≈ 8 ms (Δ ruido) | — | — | ✅ |
| box-shadow blur 64 (3 cajas) | +11,4 ms (raster) | — | — | ✅ coste de raster, cap 64 |
| border-radius 32 (3 cajas) | +6,0 ms (raster) | — | — | ✅ |
| formularios (primer pintado 640×480) | 7–8 ms | — | — | ✅ interacción no medida |
| SVG pattern/text (fixtures del crate) | ~3 ms c/u (ejemplo) | — | — | ✅ |

**Conclusión**: mantener **pipeline ≤140 ms** (89,0 %; margen ~15 ms),
**layout ≤45 ms** (82,4 %) y **render showcase ≤35 ms** (78,6 %). El coste de
las features nuevas no entra en el bench; sí conviene revisar el +5,3 % de
`build-display-list` y el +3,8 % de `css/resolve-*` (preparación genérica de
decoraciones y reglas UA), que son los dos únicos efectos medibles de r17 en el
pipeline.

---

## 14. Comparación histórica

Quince baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | **21:37** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | **`67f7c57`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | limpio | docs dirty | limpio | limpio | docs dirty | docs dirty | WIP html | limpio | limpio | **rustdoc ajeno + docs** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | `43f7748d…` | `a188a1da…` | `c34ff65c…` | `d655c1f2…` | `153b8aff…` | `1f1dbbdd…` | `39909f01…` | **`8cfbb715…`** |
| Load (bench) | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | **0,44–0,69** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | **20.581** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | 25.658 | 25.084 | 24.772 | 24.022 | 20.396 | 21.469 | 22.466 | 23.148 | 21.433 | 21.266 | **20.164** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | **32.774** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | **37.085** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | **13.101** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | 0.843 | 0.993 | 0.881 | 1.082 | 0.925 | 1.052 | 0.999 | 0.947 | 1.003 | **0.921** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | **8.604** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | **124.575** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** |

### Ronda 16 → ronda 17, fila a fila

| Benchmark | R16 (ms) | R17 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 21.104 | **20.581** | −2,5 % |
| `html/nested-div-10k` | 3.316 | **3.177** | −4,2 % |
| `html/nested-div-32k` | 10.638 | **10.244** | −3,7 % |
| `html/nested-div-64k` | 21.266 | **20.164** | −5,2 % |
| `html/nested-div-64k-default-limits` | 13.779 | **13.183** | −4,3 % |
| `html/massive-end-p` | 18.936 | **18.559** | −2,0 % |
| `html/massive-formatting` | 12.870 | **11.789** | −8,4 % |
| `html/tag-40k-attrs` | 12.644 | **12.253** | −3,1 % |
| `html/pretty-5k-blocks` | 11.611 | **11.037** | −4,9 % |
| `css/parse-10k-rules` | 14.262 | **13.555** | −5,0 % |
| `css/resolve-styles-2k-nodes` | 6.618 | **6.879** | +3,9 % ~ |
| `css/resolve-styles-10k-nodes` | 31.564 | **32.774** | +3,8 % ~ |
| `css/selector-match-simple` | 0.633 | **0.663** | +4,7 % ~ |
| `css/selector-match-descendant` | 1.213 | **1.270** | +4,7 % ~ |
| `layout/realistic-1mb` | 37.418 | **37.085** | −0,9 % ✅ |
| `paint/build-display-list` | 12.441 | **13.101** | +5,3 % ~ |
| `paint/rasterize-800x600` | 1.003 | **0.921** | −8,2 % |
| `paint/render-to-png-800x600` | 9.015 | **8.604** | −4,6 % |
| `pipeline/html-to-png-800x600` | 122.383 | **124.575** | +1,8 % ~ |
| **TOTAL (suma)** | **362.714** | **360.410** | −0,6 % |

~ Efectos pequeños y estructurales de r17 en el bench: reglas UA de formularios
(+3,8 % en cascada/selectores) y preparación genérica de decoraciones en la
display list (+5,3 %). No son regresiones de layout/pipeline (planos ±2 %) y
están flagged para micro-optimizar. El TOTAL baja −0,6 %.

---

## 15. Cómo comparar y detectar regresiones

1. Medir en la misma máquina y con el sistema ocioso (`uptime` < ~1.5). En r17
   hubo que esperar a que el governor/frecuencia se estabilizara: ventana final
   load 0,44–0,69; los 25 primeros intentos (load 2,6–8,6, núcleos a 400 MHz)
   se descartaron.
2. Guardar JSON y comparar medianas:

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
3. Criterio de descarte usado en r17: `layout` ≤43, `pipeline` ≤135,
   `render-to-png` ≤10,2, `html` ≤22,5, y sobre todo **load/frecuencia
   estables** (no medir con núcleos a 400 MHz).
4. Regresión si la mediana empeora >15 % de forma reproducible; dispersión
   típica ~1–6 % en reposo y ±5–10 % en layout/paint con carga.
5. Para complejidad, `--scale 0.5` vs `1.0`: ratios ≈1.
6. Features nuevas: flex (`flex_*.html`), decoraciones (`target/bench-r17/
   deco_*.html`, cap blur 64), formularios (`forms.html`, shell vs render),
   SVG pattern/text (ejemplo `svg_to_png`). Determinismo: 2 pasadas → mismo PNG.
7. Golden/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   phase4 basic `9e957177…` 42 853 B, bench PNG 57 422 B, formularios
   `b8b9a204…` 14 355 B, flex `c28ce538…`/`8d06d66c…`/`7fbf998e…`.

---

## 16. Limitaciones y notas

- **Host y governor**: la primera mitad de la ventana tuvo load 2,6–8,6 y
  núcleos a 400 MHz (`powersave`); las cifras de esa fase (layout 38–45,
  pipeline 127–139) se descartan. La baseline usa 6 pasadas de la ventana en
  calma (load 0,44–0,69) y la pasada canónica.
- **El bench no usa flex/decoraciones/SVG/forms**: las capacidades de r17 no
  cambian el pipeline por defecto. Aun así, se miden dos efectos estructurales
  pequeños en el bench: `css/resolve-*` +3,8 % (reglas UA de formularios) y
  `build-display-list` +5,3 % (preparación genérica de decoraciones); flagged
  para micro-optimizar.
- **Decoraciones**: el coste es de **raster** y escala con área/blur
  (`radius` +4,3 ms; `box-shadow` +1,7/+2,2/+6,4 ms para blur 8/32/64). El
  **cap de blur 64** se cumple (blur 128 → mismo PNG que 64). Los fixtures son
  temporales en `target/` (no versionados).
- **Flexbox**: medido con fixtures de 3–4 hijos (Δpared de ruido). No hay
  fixture de repo con muchos hijos flex; el coste a gran escala no está medido.
- **Formularios**: solo se mide el **primer pintado** en headless (7–8 ms a
  640×480); la interacción (foco/tecleo/re-render) requiere ventana y no se
  mide aquí.
- **SVG pattern/text**: `<pattern>` y `<text>` básicos soportados; siguen sin
  filtros, máscaras, `textPath`, justificación avanzada ni `font-face` interno.
  El ejemplo `svg_to_png` usa PNG *stored* (el tamaño de `text.svg` es casi el
  RGBA sin comprimir).
- **Presupuestos**: mantener pipeline ≤140 ms (124,6 = 89,0 %), layout ≤45 ms
  (37,1 = 82,4 %) y render showcase ≤35 ms (27,5 = 78,6 %).
- **Fast path de tablas (r7)**: sigue la nota histórica; no hay commit
  compilable con el camino avanzado sin fast path.
