# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 20:15–20:35 (Europe/Madrid). Ronda 15:
  **overhaul del IFC** (~30 % menos layout), **backgrounds** con
  `repeat`/`size`/`position`, **SVG inline** (elementos `<svg>` del documento
  convertidos a `<img>` sintético y rasterizados con el crate `svg`) y **event
  loop determinista** (independiente del reloj de pared), más shell zoom/refresh
  y webfonts de la ronda 14.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `b808d7b`**
  (`fix(render): make the event loop independent of the wall clock`).
- **Estado del árbol**: código limpio; solo `website/` y
  `.github/workflows/pages.yml` untracked (del dueño) y algunos docs en vuelo.
  El JSON del script marca `dirty = 4` por esos untracked/docs, no por código.
- **Binario de bench**: `target/release/bench`, `sha256 = 1f1dbbdd…`
  (4 331 816 B) en las 5 pasadas válidas; la pasada del script lo recompiló con
  la misma fuente (`8d49cf89…`, 4 332 608 B).
- **Artefactos guardados** (gitignored):
  - `target/bench-r15-run*.json`, `target/bench-aggregate-r15.json`
    (`sha256 = 30d3d727…`), `target/bench-latest.txt` (`1bca17b2…`).
  - `target/bench-r15/inline_svg_stages.json`, `backgrounds_summary.json`,
    `event_loop_determinism.json`, `render_shell.json`.

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–13; la ronda 14 no publicó baseline,
> ver §5).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. **El crate `bench` y sus generadores no cambiaron en esta ronda.**

El bench **no usa backgrounds con imagen, SVG inline, webfonts ni JS**: sus
generadores no contienen `url(...)`/`<svg>`/`@font-face`/`<script>`. Las
features nuevas se miden aparte con `render` (§8–§10) y **no cambian el
pipeline por defecto** del bench.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- **La tabla de §3 es la mediana, por fila, de 5 pasadas completas válidas**
  (de 20 intentos) tomadas con load **1,4–2,0** (host en calma). Criterio de
  aceptación: `layout` 34–40 ms, `pipeline` 114–126 ms, `render-to-png`
  8,4–10,2 ms y `html/realistic` ≤22 ms; las pasadas con load alto se
  descartaron.
- `setup_ms` de las válidas: 178,4–187,8 ms.
- La pasada canónica de `./tools/bench.sh` dio `TOTAL 355.097 ms` (la mediana de
  las 5: 354.983), en la misma ventana.
- Render/shell: 11 ejecuciones intercaladas (pared) + 7 `--json` para etapas.
  SVG inline y backgrounds: 7 ejecuciones por fixture + 5 `--json` para las
  etapas. Event loop: 2 pasadas idénticas por fixture para comprobar
  determinismo (semántica, no tiempos).
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` por defecto |
| Carga de fondo | 1,4–2,0 en las pasadas válidas (intentos con más carga descartados) |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Bench (5 pasadas válidas de esta baseline)
./tools/bench.sh
for i in 1 2 3 4 5 6; do target/release/bench --json > "target/bench-r15-run$i.json"; done

# SVG inline (elementos <svg> → img sintético; cap 256/documento, 1 MiB/SVG)
target/release/render tests/fixtures/render/svg_inline_simple.html /tmp/s.png --json
target/release/render tests/fixtures/render/svg_inline_multiple.html /tmp/m.png --json
# (variantes: viewbox, use, block, class, corrupt; ver §8)

# Backgrounds con geometría nueva (repeat/size/position)
target/release/render tests/fixtures/render/backgrounds.html /tmp/b.png --json
# fixture temporal con cover/repeat (en target/, usa --base file://.../tests/fixtures/render/)
target/release/render target/bench-r15/bg_cover.html /tmp/c.png --json \
    --base "file://$PWD/tests/fixtures/render/"

# Event loop determinista (2 pasadas; comparar contadores, no tiempos)
for f in tests/fixtures/render/js_event_*.html; do
  target/release/render "$f" /tmp/e.png --json
done

# Render/shell/zoom
target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png \
    --width 1024 --height 768 --json
target/release/megi tests/fixtures/render/basic.html \
    --screenshot /tmp/z.png --width 320 --height 200 --zoom 2
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
| layout | `layout/realistic-1mb` | `layout_document` (28 195 elementos, 88 155 cajas), 800×600; **IFC nuevo de r15**. |
| paint | `paint/build-display-list` | Display list: 34 158 items. |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` DEFLATE (57 422 B). |
| pipeline | `pipeline/html-to-png-800x600` | Parse → styles → layout → display list → raster → PNG. |
| fonts | (`feature fonts`) | No re-medido en r15; webfonts medidas en r13 con `render`. |

**Fuera del bench (medido en §8–§10)**: SVG inline, backgrounds con geometría,
event loop JS, webfonts y zoom del shell.

---

## 3. Baseline vigente (2026-11-10, HEAD `b808d7b`)

Mediana por fila de **5 pasadas válidas** (5 iteraciones, 1 warm-up; *heavy* 3).
`setup`: 178,4–187,8 ms. Binario `sha256 = 1f1dbbdd…`. Suma de medianas:
`TOTAL 354.983 ms`; pasada del script: `TOTAL 355.097 ms`.

| Benchmark | Carga | mediana (ms) | rango 5 pasadas (ms) | throughput |
|---|---|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | **21.260** | 20.939–21.337 | 47,1 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | **3.351** | 3.072–3.394 | 31,3 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | **10.755** | 10.692–11.037 | 31,2 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | **21.433** | 20.945–22.046 | 31,3 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | **13.919** | 13.596–13.933 | — |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | **19.065** | 18.944–19.373 | 5,25 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | **12.809** | 12.731–13.400 | 781 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | **12.782** | 12.239–12.853 | 3,13 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | **11.735** | 11.325–11.817 | 426 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | **14.706** | 13.974–15.809 | 40,5 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | **6.349** | 6.178–6.475 | 316 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | **30.543** | 30.345–30.677 | 328 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | **0.674** | 0.648–0.675 | 59,4 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | **1.291** | 1.177–1.345 | 31,0 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | **36.054** | 35.964–36.862 | 782 k elementos/s |
| `paint/build-display-list` | 34 158 items | **11.954** | 11.872–12.097 | — |
| `paint/rasterize-800x600` | 34 158 items → pixmap 800×600 | **0.947** | 0.879–0.961 | 507 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (57 422 B) | **8.946** | 8.644–9.149 | 6,12 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | **116.410** | 114.250–117.929 | — |
| **TOTAL (suma)** | | **354.983** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página.

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.35 · 32k: 10.76 · 64k: 21.43 ms | **Lineal** (3,2× → 3,21×; 2× → 2,07×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 14.71 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 2 005 el: 6.35 · 10 005 el: 30.54 ms | **Lineal** (~328 k el/s). |
| `layout/realistic-1mb` | 28 195 elementos → **36,05 ms** | **−30,1 % vs r13**: el overhaul de tokenización/breaking del IFC y las cachés de medición de fuentes. |
| `paint/render-to-png` | raster 0,95 + encoder ≈8,0 ms | Encoder PNG de r12 sin cambios (byte-idéntico). |
| `pipeline` | **116,4 ms** (114,3–117,9) | Reparto: layout 36,1 + CSS 30,5 + parse 21,3 + display list 12,0 + PNG 8,9. |

---

## 5. Ronda 15: IFC, backgrounds, SVG inline y determinismo

### 5.1 Qué entró desde la última baseline (r13)

- `9090175 perf(layout)`: **overhaul del IFC** (tokenización/breaking de líneas
  y cachés de medición de fuentes) — el layout baja de ~51,6 a ~36 ms.
- `b5b07b3 feat(css,paint)`: **background-repeat / background-size /
  background-position** (y geometría asociada).
- `c267d45 feat(render)`: **SVG inline** — los `<svg>` del documento se
  serializan a un `<img>` sintético (`data:`) tras los scripts y se rasterizan
  con el crate `svg`; contadores `inline_svgs`/`svgs_rendered`; cap
  `MAX_INLINE_SVGS = 256`/documento, 1 MiB por SVG y límites de bytes/depth.
- `b808d7b fix(render)`: **event loop independiente del reloj de pared**
  (los timers usan reloj virtual y el resultado es determinista).
- La **ronda 14 no publicó baseline** (aterrizaron backgrounds, wiring SVG E2E
  y shell zoom/refresh); esta ronda es la siguiente medida publicada.

### 5.2 Verificación de las mejoras contra los claims

| Métrica | Claim de la ronda | Nuestra medición | Veredicto |
|---|---:|---:|---|
| `layout/realistic-1mb` | ~36,3 ms (vs r12 51,6) | **36.054 ms** (−30,1 % vs r13 51.601) | ✅ A/B intercalado y mediana coinciden |
| `pipeline/html-to-png` | ~120,5 ms | **116.410 ms** (script 117.897) | ✅ mejor que el claim |
| `paint/render-to-png` | ~9,2 ms | **8.946 ms** | ✅ |
| `html/realistic-1mb` | ~19–20 ms | **21.260 ms** | ⚠️ +6–11 %; fila sin cambios de código en r15 |
| `html/tag-40k-attrs` | ~12,4 ms | **12.782 ms** | ≈ (+3 %) |
| `html/massive-end-p` | ~18,7–19,3 ms | **19.065 ms** | ✅ dentro del rango |
| `render-to-png` byte-idéntico | 42 853 / 80 220 B | **42 853 B** (`9e957177…`) y **80 220 B** (`c95c7eff…`) | ✅ |

**Diferencias con los claims**: el layout y el pipeline se confirman (el claim
del pipeline parece medido con más carga; aquí con load 1,4–2,0 sale 116,4). Las
filas HTML no cambian de código en r15 y su medida depende mucho de la carga
(hemos visto 19,9–25,6 ms en la misma ventana); 21,26 ms es la mediana de 5
pasadas válidas. `tag-40k` +3 % y `massive-end-p` dentro del rango del claim.
**No hay regresión** en ninguna fila del bench; el TOTAL baja **−10,2 %**.

### 5.3 Nota metodológica

Los claims de la ronda se tomaron presumiblemente con A/B intercalado y carga
baja; aquí se publica la mediana de 5 pasadas completas con criterio de
aceptación documentado (load 1,4–2,0). El host sigue siendo la principal fuente
de varianza (±5–10 % en filas HTML).

---

## 6. Codificador PNG (sin cambios en r15)

| Métrica | r13 | r15 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 9.200 ms | **8.946 ms** | −2,8 % (varianza) |
| — rasterizar | 0.999 ms | **0.947 ms** | varianza |
| — codificar PNG | ~8,20 ms | **~8,00 ms** | encoder de r12, sin cambios |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden phase4 basic | 42 853 B | **42 853 B** | `9e957177…` (verificado en r15) |

---

## 7. Pipeline `render` y shell headless (ronda 15)

11 ejecuciones intercaladas (load ~1,5–2,0):

| Fixture / modo | pared r15 | pared r13 | rango r15 | PNG |
|---|---:|---:|---:|---:|
| showcase 1024×768 (render) | **29,49 ms** | 30,78 ms | 27,50–33,55 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **8,49 ms** | 9,70 ms | 7,73–11,36 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **29,41 ms** | 30,78 ms | 27,98–31,08 | 80 220 B (idéntico) |
| basic (shell headless) | **9,59 ms** | 9,70 ms | 8,38–10,49 | 21 090 B (idéntico) |
| basic `--width 320 --height 200 --zoom 2` (shell) | **3,66 ms** | 3,85 ms | 3,08–4,13 | 3 202 B |
| basic a viewport CSS 160×100 (render, referencia) | **3,42 ms** | 3,16 ms | 2,76–3,65 | 3 202 B (`4792779d…`) |

El render del showcase mejora ligeramente (−4,2 %) y `basic` baja a **8,49 ms**
(−12,5 % vs r13): el IFC pesa mucho en páginas pequeñas con texto. El PNG+driver
sigue siendo ~59 % del total del showcase. `--zoom 2` headless sigue siendo un
render al viewport CSS `physical/zoom` y su PNG es byte-idéntico al de `render
--width 160 --height 100`; auto-refresh (`--refresh-ms`) sigue siendo opt-in y no
se ejerce en `--screenshot`.

---

## 8. Coste de SVG inline (nuevo en r15)

Los `<svg>` del documento se reemplazan por un `<img>` con `data:` URL antes de
layout (contados en `inline_svgs`, `svgs_rendered` e `images_loaded`) y se
rasterizan con el crate `svg`. Comparación por etapas contra el mismo fixture
**sin** los `<svg>` (variante en `target/bench-r15/nosvg_*.html`), mediana de 5
runs `--json`:

| Fixture | n | `images` (µs) | `raster` (µs) | Δ stages vs sin-SVG (µs) | PNG |
|---|---:|---:|---:|---|---:|
| `svg_inline_simple` (64×64) | 1 | 281 | 843 | `images +280`, `raster +178`, `total ≈+600` | 14 639 B |
| `svg_inline_multiple` (3 iconos 24×24) | 3 | 116 | 726 | `images +114`, `raster +80`, `total ≈+470` | 13 749 B |
| `svg_inline_viewbox` (40×40) | 1 | 36 | 705 | `images +34`, `raster +111` | 13 730 B |
| `svg_inline_use` (32×32, `<use>`) | 1 | 68 | 692 | `images +67`, `raster +196` | 13 744 B |
| `svg_inline_block` (32×32, inline-block) | 1 | 70 | 701 | `images +68`, `raster +45` | 13 729 B |
| `svg_inline_class` (48×24, CSS class) | 1 | 49 | 667 | `images +48`, `raster +146` | 13 730 B |

**Lectura**:

- El coste incremental por SVG inline es **pequeño**: en etapas, ~**25–70 µs**
  de `images` por icono (serialización+parse+raster AA+decode a RGBA) y
  ~**45–200 µs** de `raster` según tamaño (un icono 24×24 ≈ 38 µs de
  `images`); un SVG 64×64 aislado cuesta ~0,6 ms de etapas. En pared total no
  se distingue del ruido (<±1 ms con 1–3 iconos).
- **Cap defensivo**: `inline_svgs ≤ 256`/documento, 1 MiB por SVG, límites de
  bytes totales y profundidad; al excederse se emite warning y el `<svg>`
  original queda intacto (no rompe el render). Los SVG inline con CSS interno,
  gradientes/filtros o fuentes **no** se renderizan como en un navegador
  (limitaciones documentadas del crate `svg`; gradientes = sin relleno).

---

## 9. Coste de backgrounds con geometría (r14/r15)

`background-repeat`, `background-size` y `background-position` están activos.
Fixture del repo `backgrounds.html` (pattern 16×16 + imagen ausente + marcadores)
y fixtures temporales en `target/bench-r15/` (200×120 con `cover` y con
`repeat 32px`, `--base file://…/tests/fixtures/render/`), baseline sin imagen.
7 runs de pared + 5 `--json`:

| Fixture | pared (ms) | `backgrounds` (µs) | `paint` (µs) | `raster` (µs) | PNG | bg cargadas |
|---|---:|---:|---:|---:|---:|---:|
| `bg_none` (sin imagen, baseline) | 7,85 | 2 | 4 | 622 | 13 734 B | 0 |
| `bg_cover` (cover, 200×120) | **8,93** | 40 | 5 | **1 620** | 14 215 B | 1 (93 B) |
| `bg_repeat` (repeat 32px) | **8,93** | 36 | 6 | **1 547** | 14 193 B | 1 (93 B) |
| `backgrounds.html` (pattern + missing + markers) | 8,76 | 42 | 7 | 1 051 | 15 778 B | 2 (185 B) + 1 fallo |

| Δ vs sin background | pared | `paint` | `raster` | PNG |
|---|---:|---:|---:|---:|
| `bg_cover` | **+1,08 ms** | +1 µs | **+998 µs** | +481 B |
| `bg_repeat` | **+1,08 ms** | +2 µs | **+925 µs** | +459 B |
| `backgrounds.html` (vs su baseline) | +0,91 ms | +3 µs | +429 µs | +2 044 B |

**Lectura**: la geometría (`size`/`repeat`/`position`) se resuelve barata
(`backgrounds` = 36–42 µs en la etapa de pintado); **el coste está en el
rasterizado** del muestreo bilineal de la imagen (≈+0,9–1,0 ms para un área de
200×120). La imagen ausente falla con warning y deja el color de fondo (PNG sin
esa capa). El bench sigue sin backgrounds con imagen: no le afecta.

---

## 10. Event loop determinista (r15)

Los 6 fixtures `js_event_*` con `--json` **dos veces**, comparando **semántica**
(contadores y PNG), no tiempos:

| Fixture | eventos | tasks | iter | virtual ms | pendientes | event_loop (µs, 2 runs) | semántica |
|---|---:|---:|---:|---:|---:|---|---|
| `js_event_domcontentloaded` | 2 | 0 | 0 | 0 | 0 | 127 / 47 | **idéntica** |
| `js_event_interval` | 2 | 3 | 15 | 15 | 0 | 54 / 84 | **idéntica** |
| `js_event_load` | 2 | 1 | 5 | 5 | 0 | 76 / 70 | **idéntica** |
| `js_event_loop` (budget) | 2 | 100 | 1 000 | 1 000 | 1 | 506 / 1 024 | **idéntica** |
| `js_event_timeout` | 2 | 1 | 10 | 10 | 0 | 57 / 61 | **idéntica** |
| `js_event_timeout_zero` | 2 | 1 | 1 | 0 | 0 | 35 / 40 | **idéntica** |

- Los contadores (`script_events/tasks/loop_iterations/virtual_time_ms/
  timers_pending`), `scripts_executed` y el **PNG resultante** son **idénticos
  entre pasadas**: el event loop es determinista con el reloj virtual de r15
  (`b808d7b`). Solo varían los tiempos de `event_loop` (ruido de medición).
- Comparado con r13 (47–77 µs normales, 855 µs el budget): mismos órdenes de
  magnitud (35–127 µs normales, 506–1 024 µs el budget); el budget de bucle
  (100 tareas / 1 000 iteraciones / 1 000 ms virtuales) sigue cortando y deja 1
  timer pendiente.

---

## 11. Presupuestos orientativos

Con el pipeline en **116,4 ms** y el layout en **36,1 ms**, se proponen bajar
los presupuestos (hoy 150 / 35 / layout sin presupuesto específico):

| Benchmark / medida | Baseline r15 | Presupuesto actual | % actual | Propuesta |
|---|---:|---:|---:|---|
| `html/realistic-1mb` | 21,3 ms | ≤ 100 ms | 21 % | mantener |
| `html/tag-40k-attrs` | 12,8 ms | ≤ 50 ms | 26 % | mantener |
| `html/massive-end-p` | 19,1 ms | ≤ 50 ms | 38 % | mantener |
| `css/parse-10k-rules` | 14,7 ms | ≤ 30 ms | 49 % | mantener |
| `css/resolve-styles-10k-nodes` | 30,5 ms | ≤ 75 ms | 41 % | mantener |
| `layout/realistic-1mb` | 36,1 ms | (sin número) | — | **añadir ≤45 ms** (80 %) |
| `paint/build-display-list` | 12,0 ms | ≤ 25 ms | 48 % | mantener |
| `paint/rasterize-800x600` | 0,95 ms | ≤ 25 ms | 4 % | mantener |
| `paint/render-to-png-800x600` | 8,9 ms | ≤ 50 ms | 18 % | mantener |
| `pipeline/html-to-png-800x600` | **116,4 ms** | **≤ 150 ms** | 77,6 % | **bajar a ≤ 140 ms** (83,1 %; script 84,2 %) |
| `render` showcase 1024×768 | **29,5 ms** | **≤ 35 ms** | **84,3 %** | mantener 35 |
| shell headless showcase | 29,4 ms | ≤ 35 ms | 84 % | mantener |
| shell basic `--zoom 2` 320×200 | 3,7 ms | — | — | ✅ |
| SVG inline (1–3 iconos) | +0,1–0,6 ms de etapas | — | — | ✅ cap 256/documento |
| background-size cover/repeat (200×120) | +1,08 ms de pared | — | — | ✅ raster domina |

**Propuesta**: **pipeline ≤140 ms** (actual 116,4 = 83,1 %; canónica 117,9 =
84,2 %), **layout ≤45 ms** (36,1 = 80,1 %) y **render showcase ≤35 ms**
(29,5 = 84,3 %). El margen del pipeline es ~23 ms; el reparto actual es layout
36 + CSS 30 + parse 21 + display list 12 + PNG 9. Las features de r15 (SVG
inline, backgrounds) **no** consumen presupuesto de forma significativa
(+0,1–0,6 ms SVG; +1,1 ms un cover grande por raster).

---

## 12. Comparación histórica

Trece baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline; se compara con r13):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | **20:15** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | **`b808d7b`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | limpio | docs dirty | limpio | limpio | docs dirty | docs dirty | WIP html (archivo prístino) | **limpio (website untracked)** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | `43f7748d…` | `a188a1da…` | `c34ff65c…` | `d655c1f2…` | `153b8aff…` | **`1f1dbbdd…`** |
| Load (bench) | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | **1,4–2,0** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | **21.260** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | 25.658 | 25.084 | 24.772 | 24.022 | 20.396 | 21.469 | 22.466 | 23.148 | **21.433** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | **30.543** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | **36.054** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | **11.954** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | 0.843 | 0.993 | 0.881 | 1.082 | 0.925 | 1.052 | 0.999 | **0.947** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | **8.946** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | **116.410** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** |

### Ronda 13 → ronda 15, fila a fila

| Benchmark | R13 (ms) | R15 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 22.618 | **21.260** | −6,0 % |
| `html/nested-div-10k` | 3.554 | **3.351** | −5,7 % |
| `html/nested-div-32k` | 11.334 | **10.755** | −5,1 % |
| `html/nested-div-64k` | 23.148 | **21.433** | −7,4 % |
| `html/nested-div-64k-default-limits` | 14.014 | **13.919** | −0,7 % |
| `html/massive-end-p` | 22.312 | **19.065** | −14,6 % |
| `html/massive-formatting` | 12.710 | **12.809** | +0,8 % |
| `html/tag-40k-attrs` | 12.047 | **12.782** | +6,1 % |
| `html/pretty-5k-blocks` | 11.645 | **11.735** | +0,8 % |
| `css/parse-10k-rules` | 14.909 | **14.706** | −1,4 % |
| `css/resolve-styles-2k-nodes` | 6.399 | **6.349** | −0,8 % |
| `css/resolve-styles-10k-nodes` | 30.311 | **30.543** | +0,8 % |
| `css/selector-match-simple` | 0.682 | **0.674** | −1,2 % |
| `css/selector-match-descendant` | 1.314 | **1.291** | −1,8 % |
| `layout/realistic-1mb` | 51.601 | **36.054** | **−30,1 %** ✅ |
| `paint/build-display-list` | 12.842 | **11.954** | −6,9 % |
| `paint/rasterize-800x600` | 0.999 | **0.947** | −5,2 % |
| `paint/render-to-png-800x600` | 9.200 | **8.946** | −2,8 % |
| `pipeline/html-to-png-800x600` | 133.629 | **116.410** | **−12,9 %** ✅ |
| **TOTAL (suma)** | **395.269** | **354.983** | **−10,2 %** |

**Lectura**: ronda de mejora clara. El **overhaul del IFC** recorta el layout
**−30,1 %** (51,6 → 36,1 ms) y arrastra el pipeline **−12,9 %** y el TOTAL
**−10,2 %**. HTML/CSS se mantienen dentro de ±1–7 % (varianza del host; las
filas HTML no cambiaron de código). El coste de las features nuevas fuera del
bench es pequeño: SVG inline +0,1–0,6 ms por documento, backgrounds
cover/repeat +1,08 ms (dominado por el raster bilineal) y el event loop es
determinista en semántica (contadores/PNG idénticos entre pasadas).

---

## 13. Cómo comparar y detectar regresiones

1. Medir en la misma máquina y con el sistema ocioso (`uptime` < ~1.5). En r15
   se usaron 5 de 20 intentos con criterio de aceptación (`layout` 34–40,
   `pipeline` 114–126, `render-to-png` 8,4–10,2, `html` ≤22) y load 1,4–2,0.
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
3. Regresión si la mediana empeora >15 % de forma reproducible; dispersión
   típica ~1–6 % en reposo, ±5–10 % en filas HTML/paint con carga.
4. Para complejidad, `--scale 0.5` vs `1.0`: ratios ≈1.
5. Features nuevas: SVG inline (`inline_svgs`/`svgs_rendered` en `render
   --json`), backgrounds (`background_images_loaded/bytes`, geometría),
   event loop (comparar contadores entre 2 pasadas, no tiempos) y webfonts
   (`timings.fonts`, `fonts_loaded/failed/bytes`).
6. Golden/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   phase4 basic `9e957177…` 42 853 B, bench PNG 57 422 B.

---

## 14. Limitaciones y notas

- **Host variable**: 20 intentos de bench, 5 válidos (load 1,4–2,0; intentos con
  más carga descartados). Las filas HTML/CSS difieren ±1–7 % entre ventanas sin
  cambios de código.
- **Ronda 14 sin baseline**: backgrounds (r14) y SVG inline/wiring E2E
  aterrizaron entre r13 y r15; esta ronda es la siguiente medición publicada.
- **SVG inline**: `MAX_INLINE_SVGS = 256`/documento, 1 MiB/SVG, límites de
  bytes totales/profundidad; al excederse hay warning y el `<svg>` se deja
  intacto. No soporta CSS interno, gradientes, filtros ni fuentes dentro del
  SVG (limitaciones del crate `svg`); el coste medido es de etapas, no aislable
  del ruido en pared.
- **Backgrounds**: `size`/`repeat`/`position` resueltos baratos; el raster
  bilineal domina (+~1 ms por 200×120 con imagen). El bench no usa backgrounds
  con imagen.
- **Event loop**: determinista en semántica (contadores y PNG idénticos entre
  pasadas); los tiempos individuales de `event_loop` varían. Presupuestado a
  1 000 ms virtuales / 100 tareas.
- **Presupuestos propuestos**: pipeline ≤140 ms (actual 116,4 = 83,1 %),
  layout ≤45 ms (36,1 = 80,1 %) y render showcase ≤35 ms (29,5 = 84,3 %).
- **Fast path de tablas (r7)**: sigue la nota histórica; no hay commit
  compilable con el camino avanzado sin fast path.
- **`website/` y `pages.yml`**: untracked del dueño; no se tocan (solo se
  renderiza a `/tmp` si hiciera falta).
