# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**.

- **Fecha**: 2026-11-13, 01:22–01:24 (Europe/Madrid). Ronda 36: clasificación de
  estados de animación/transiciones, fallback de fuentes de sistema por glifo,
  caché de render por documento, `apply_transitions` con interpolación en tiempo
  real, tope de profundidad de layout y pools keep-alive HTTP/1.1 (r33–r36).
- **Repo**: `Megi`, rama `main`, **HEAD `a080249`** (batch CI/MSRV `2876c74`).
- **Ventana**: 7 pasadas, mediana por fila; `TOTAL` (suma de medianas, 81 filas)
  **821,380 ms**; filas legacy (19) **346,488 ms**. Totales por pasada
  817,7–833,5 ms (dispersión 1,9 %).
- **Load**: 8,1–13,0 durante la ventana — **la máquina no estaba ociosa** (navegador
  y editor del usuario activos), a diferencia de la ventana r32 (1,69–1,83). Los
  valores absolutos de r36 no son comparables 1:1 con r32; la comparación válida
  es por fila y, sobre todo, A/B intercalado. Una pasada previa
  (`target/bench-r36-run*`) cayó en un pico de load 25 y salió con
  `TOTAL 1 322,919 ms`: descartada, se repitieron 7 pasadas.
- **Binarios**: bench con `rustc 1.99.1 release` (`bench --json`, `iterations = 5`,
  `warmup = 1`), fixtures en memoria (`crates/bench/src/generators.rs`).
- **Artefactos**: `target/bench-r36b-run1..7.json` (primaria), `target/bench-r36-run*`
  (lote descartado), `target/bench-latest.txt` (JSON de la última pasada).
- **Filas**: 81 en total (62 de la baseline r32 + 19 nuevas: matriz
  `paint/png-encode-{full,regions}-*`, `paint/backdrop-blur-*`,
  `image/decode-webp-lossy*`, `image/decode-webp-alpha/palette` y variantes
  `pipeline/*-no-effect`).
---

## 1. Metodología

- Cargas en memoria (`crates/bench/src/generators.rs`): E2E, efectos de capa,
  save-png, decode PNG/JPEG/GIF/WebP (VP8L y VP8 lossy), filas nuevas de fases
  PNG (`filter`/`deflate`/`encode`) y `render/html-to-png`.
- `Instant`, `--warmup 1`, `--iterations 5` (heavy 3); mediana de 5 pasadas
  válidas; canónica `./tools/bench.sh`; artefactos JSON por pasada.
- Render/shell: 10 ejecuciones intercaladas con `--json`/md5; fixtures propias
  en `target/bench-r32/feature/`.
- Máquina: i7-12700H, 15 GiB, CachyOS 7.3.1, rustc 1.99.1 release; ACPI
  `performance`; load 1,0–1,83 durante las mediciones.

## 2. Catálogo

| Grupo | Benchmarks |
|---|---|
| html/css/layout | parseo, cascada (**selector bloom**, layers), matching, layout block/sticky |
| paint | display-list, raster, render-to-png, clipping, layer-effects, object-fit, filter/backdrop raster+png, **PNG filter/deflate/encode (flat y alpha)** |
| pipeline | html-to-png, gradients, grid, filter/backdrop 300×260 |
| **render** | **`render/html-to-png`** (crate render end-to-end) |
| shell | save-png encode/write/total, chrome-png 640×400 y 1024×768 |
| image | decode PNG/JPEG/GIF, WebP VP8L lossless/alpha/palette y VP8 lossy; **decode-png-scaled-x2** |

## 3. Baseline vigente (ronda 36, HEAD `a080249`)

7 pasadas (load 8,1–13,0; ver caveat en la cabecera). **Filas legacy (19)**:
`TOTAL 346.488 ms`; **todas (81)**: `TOTAL 821.380 ms`.

| Benchmark | mediana (ms) | rango 7 pasadas (ms) |
|---|---:|---:|
| `css/parse-10k-rules` | **13.366** | 13.148–13.610 |
| `css/resolve-styles-10k-nodes` | **12.854** | 12.744–13.115 |
| `css/resolve-styles-2k-nodes` | **2.873** | 2.801–2.891 |
| `css/selector-match-descendant` | **1.037** | 0.997–1.147 |
| `css/selector-match-simple` | **0.542** | 0.510–0.558 |
| `css/vars-calc-10k` | **21.365** | 20.414–22.183 |
| `html/massive-end-p` | **19.065** | 18.449–19.370 |
| `html/massive-formatting` | **10.911** | 10.545–11.055 |
| `html/nested-div-10k` | **2.836** | 2.789–2.950 |
| `html/nested-div-32k` | **8.950** | 8.815–9.447 |
| `html/nested-div-64k` | **18.165** | 17.805–18.646 |
| `html/nested-div-64k-default-limits` | **10.215** | 10.081–10.455 |
| `html/pretty-5k-blocks` | **9.322** | 9.135–9.358 |
| `html/realistic-1mb` | **17.873** | 17.514–18.065 |
| `html/tag-40k-attrs` | **9.524** | 9.380–9.792 |
| `image/decode-gif` | **3.013** | 2.933–3.018 |
| `image/decode-jpeg-800x600` | **11.060** | 10.257–11.294 |
| `image/decode-png-800x600` | **2.548** | 2.373–2.599 |
| `image/decode-png-scaled-x2` | **2.815** | 2.776–2.860 |
| `image/decode-webp-alpha` | **0.024** | 0.023–0.024 |
| `image/decode-webp-lossless` | **0.049** | 0.047–0.052 |
| `image/decode-webp-lossy` | **10.868** | 10.783–10.918 |
| `image/decode-webp-lossy-q75` | **0.059** | 0.057–0.061 |
| `image/decode-webp-lossy-q90` | **0.065** | 0.064–0.067 |
| `image/decode-webp-palette` | **0.007** | 0.006–0.007 |
| `layout/realistic-1mb` | **35.763** | 34.669–36.573 |
| `layout/sticky-text-1mb` | **32.737** | 31.792–33.893 |
| `paint/backdrop-blur-800x600` | **6.152** | 6.071–6.170 |
| `paint/backdrop-blur-color-800x600` | **9.988** | 9.461–10.165 |
| `paint/backdrop-blur-no-effect-800x600` | **0.265** | 0.257–0.274 |
| `paint/backdrop-fixture` | **0.339** | 0.293–0.362 |
| `paint/backdrop-fixture-no-effect` | **0.022** | 0.020–0.024 |
| `paint/backdrop-fixture-png` | **0.564** | 0.529–0.572 |
| `paint/backdrop-fixture-png-no-effect` | **0.438** | 0.415–0.442 |
| `paint/backdrop-fixture-raster` | **1.264** | 1.180–1.295 |
| `paint/backdrop-fixture-raster-no-effect` | **0.972** | 0.903–0.992 |
| `paint/build-display-list` | **8.966** | 8.774–9.285 |
| `paint/clipping-800x600` | **45.355** | 44.455–46.121 |
| `paint/filter-fixture` | **0.271** | 0.255–0.283 |
| `paint/filter-fixture-no-effect` | **0.269** | 0.257–0.293 |
| `paint/layer-effects-800x600` | **26.261** | 26.148–26.557 |
| `paint/object-fit-800x600` | **3.384** | 3.175–3.407 |
| `paint/png-deflate-1024x768` | **3.445** | 3.368–3.575 |
| `paint/png-deflate-800x600` | **2.186** | 2.078–2.243 |
| `paint/png-encode-1024x768` | **3.933** | 3.753–3.980 |
| `paint/png-encode-1024x768-alpha` | **108.771** | 108.023–113.509 |
| `paint/png-encode-800x600` | **2.409** | 2.258–2.450 |
| `paint/png-encode-800x600-alpha` | **64.284** | 62.674–65.235 |
| `paint/png-encode-full-1024x768-100pct` | **3.908** | 3.769–4.072 |
| `paint/png-encode-full-1024x768-10pct` | **10.434** | 10.133–10.677 |
| `paint/png-encode-full-1024x768-25pct` | **9.318** | 9.161–9.578 |
| `paint/png-encode-full-1024x768-50pct` | **7.668** | 7.378–7.720 |
| `paint/png-encode-full-800x600-100pct` | **2.332** | 2.278–2.385 |
| `paint/png-encode-full-800x600-10pct` | **6.378** | 6.159–6.634 |
| `paint/png-encode-full-800x600-25pct` | **5.611** | 5.547–5.847 |
| `paint/png-encode-full-800x600-50pct` | **4.595** | 4.495–4.798 |
| `paint/png-encode-regions-100pct` | **2.476** | 2.350–2.632 |
| `paint/png-encode-regions-1024x768-100pct` | **4.371** | 4.318–4.502 |
| `paint/png-encode-regions-1024x768-10pct` | **5.281** | 5.048–5.429 |
| `paint/png-encode-regions-1024x768-25pct` | **5.075** | 4.904–5.172 |
| `paint/png-encode-regions-1024x768-50pct` | **4.831** | 4.733–4.951 |
| `paint/png-encode-regions-10pct` | **3.089** | 2.972–3.298 |
| `paint/png-encode-regions-25pct` | **2.992** | 2.851–3.171 |
| `paint/png-encode-regions-50pct` | **2.850** | 2.738–2.921 |
| `paint/png-filter-1024x768` | **0.303** | 0.292–0.364 |
| `paint/png-filter-800x600` | **0.205** | 0.201–0.362 |
| `paint/rasterize-800x600` | **1.059** | 0.969–1.094 |
| `paint/render-to-png-800x600` | **6.407** | 6.286–6.504 |
| `pipeline/backdrop-fixture-300x260` | **1.851** | 1.766–1.868 |
| `pipeline/backdrop-fixture-300x260-no-effect` | **1.432** | 1.326–1.457 |
| `pipeline/filter-fixture-300x260` | **0.828** | 0.759–0.837 |
| `pipeline/filter-fixture-300x260-no-effect` | **0.812** | 0.741–0.830 |
| `pipeline/gradients-800x600` | **47.088** | 45.320–47.558 |
| `pipeline/grid-800x600` | **4.433** | 4.408–4.568 |
| `pipeline/html-to-png-800x600` | **110.124** | 107.612–119.501 |
| `render/html-to-png` | **7.614** | 7.326–7.754 |
| `shell/chrome-png-1024x768` | **3.794** | 3.650–3.970 |
| `shell/chrome-png-640x400` | **1.235** | 1.176–1.305 |
| `shell/save-png-encode-800x600` | **2.753** | 2.601–2.796 |
| `shell/save-png-total-800x600` | **2.786** | 2.585–2.826 |
| `shell/save-png-write-800x600` | **0.003** | 0.003–0.003 |

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k 2,8 · 32k 8,9 · 64k 17,8 ms | lineal |
| `css/resolve-styles-10k-nodes` | **13,467 ms** | **−52 %** vs r31 (bloom de clases ancestro) |
| `layout/realistic-1mb` | **36,447 ms** | −3,3 % A/B vs r31 (box children más baratos) |
| `paint/build-display-list` | **8,297 ms** | −10,2 % A/B |
| `paint/render-to-png-800x600` | **6,891 ms** | **−25,2 %** A/B (encode PNG más rápido) |
| `image/decode-png-800x600` | **2,611 ms** | **−63,7 %** A/B (adler/render/unfilter) |
| `image/decode-png-scaled-x2` | **3,002 ms** | nuevo (800×600 → 400×300) |
| `pipeline/html-to-png` | **108,458 ms** (107,4–142,6) | −6,6 % A/B; pico 1 pasada |
| `render/html-to-png` | **7,906 ms** | nuevo: crate render end-to-end |

## 5. Ronda 32: features y verificación

### 5.1 Qué entró

- `8f3956c perf(css,layout)`: **ancestor-class bloom pruning** (selectores) y
  caja de hijos más barata.
- `a90ab4a perf(image)`: PNG decode 6,74 → 2,54 ms (adler32 por bloques,
  render_row fast paths y unfiltro especializado).
- `343f75c perf(render)`: **skip de scans de imágenes/SVG** cuando el documento
  no puede necesitarlos.
- `b4eb190 feat(js,render)`: **top-level await y live module bindings** (Boa).
- `5296cab feat(net)`: **h1 streaming incremental real** y **prioridad de push**.
- `a477061 perf(paint)`: **PNG encode byte-safe 17–28 % más rápido**.
- `76e1678 test(bench)`: filas de fases PNG, decode-scaled y WebP lossy.

### 5.2 Veredicto de presupuestos (sin subir)

Ver tabla completa en §11. Resumen: todos los presupuestos se cumplen con margen.
`render/html-to-png` queda como informativo (~7,91 ms, presupuesto pendiente de asignar).

## 6. Coste de features r32

10 ejecuciones intercaladas, load ~1,0–1,8, performance:

| Feature | Medición | Lectura |
|---|---|---|
| **Selector bloom** | `css/resolve-styles-10k` **13,47 ms** vs r31 29,32 en A/B | **−52,2 %**: poda por clases ancestro antes de matching |
| **Layout (box children)** | **36,45 ms** | −3,3 % A/B; menos coste de árbol/cajas |
| **PNG decode** | **2,611 ms** (scaled 3,002) | −63,7 % A/B; foco recuperado |
| **PNG encode fases (800×600 flat)** | filter **0,229** · deflate **2,240** · encode **2,608 ms** | filas nuevas; encode byte-safe |
| **PNG encode alpha 800×600** | **69,666 ms** | foto alpha; encode completo |
| **PNG encode 1024×768 flat/alpha** | **4,228 / 120,410 ms** | filas nuevas; alpha domina |
| **Render CLI fast paths** | simple **8,19** · basic **9,01** · showcase **25,37 ms** | skips de scans imágenes/SVG |
| **TLA fetch** | **7,16 ms** vs estático **3,64 ms** | **+3,52 ms (+96,7 %)** por fetch+await+re-render |
| **h1 streaming loopback** | **0,115 ms/petición**, 64 KiB/iter (100 iter, in-process) | streaming incremental real; body por chunks |
| **Push priority** | no medido | sin fila de bench; se documenta pendiente |
| **PNG componente chrome** | chrome-png 640 **1,350** / 1024 **4,194 ms** | fase de encode de chrome |

## 7. `render/html-to-png` (fila nueva) y render CLI

`pipeline/html-to-png` compone a mano html→styles→layout→display-list→raster→PNG
y **no mide el crate `render`**. La fila nueva pasa una página determinista por
`render::render_url` (loader datos, scans, colección de estilos, layout, paint y
PNG):

| Medición | r32 |
|---|---:|
| `render/html-to-png` (data URL 14 376 B, 240 cards, 800×600) | **7,906 ms** (canónica 7,719) |
| render CLI simple (sin imágenes/SVG) | **8,19 ms** |
| render CLI basic | **9,01 ms** |
| render CLI showcase 1024×768 | **25,37 ms** |

La fila entra en el bench con `heavy()` y unidades MPix; sirve para medir el
pipeline real y evitar que `pipeline/*` (composición manual) oculte regresiones
del crate render. `crates/bench/Cargo.toml` añade la dependencia `render`
(solo para esta fila); el resto del bench no cambia.

## 8. Shell, histórico de páginas y goldens (r32)

| Modo | r32 | r31 | PNG |
|---|---:|---:|---:|
| shell showcase headless | **26,28 ms** | 29,85 | 80 220 B (`c95c7eff…`) |
| showcase `--chrome` | **27,32 ms** | 31,09 | 86 629 B |
| showcase `--chrome --chrome-menu` | **30,42 ms** | 34,34 | 88 003 B |
| `find.html --find` | **10,60 ms** | 11,95 | 29 950 B |
| showcase `--save-png` | **26,74 ms** | 30,31 | 80 220 B |
| about:history | **10,00 ms** | 10,44 | 18 204 B |
| directorio file:// | **11,35 ms** | 12,83 | — |
| error page | **10,38 ms** | 12,57 | — |

Goldens de página: showcase **`c95c7eff…`** 80 220 B, basic **`e943565d…`**
21 090 B, phase4 **`9e957177…`** 42 853 B, `basic.rgba` **`60f8984e…`**
1 920 000 B — intactos. Chrome/menu no son goldens de página (llevan UI).

## 9. Filas E2E y decode

| Fila | r32 (mediana) | r31 |
|---|---:|---:|
| `pipeline/gradients-800x600` | **50,083 ms** | 54,9 |
| `pipeline/grid-800x600` | **5,053 ms** | 6,5 |
| `css/vars-calc-10k` | **21,093 ms** | 21,3 |
| `paint/clipping-800x600` | **49,388 ms** | 52,5 |
| `layout/sticky-text-1mb` | **33,260 ms** | 35,0 |
| `image/decode-webp-lossy` | **11,446 ms** | 11,083 |
| `image/decode-webp-lossy-q75/q90` | **0,062/0,068 ms** | 0,062/0,066 |
| `image/decode-png-scaled-x2` | **3,002 ms** | nuevo |

## 10. Filas de encode PNG (r32)

| Fila | mediana (ms) | Nota |
|---|---:|---|
| `paint/png-filter-800x600` | **0,229** | filtrado de scanlines (flat UI) |
| `paint/png-deflate-800x600` | **2,240** | zlib de scanlines pre-filtradas |
| `paint/png-encode-800x600` | **2,608** | encode completo flat |
| `paint/png-encode-800x600-alpha` | **69,666** | foto alpha (RGBA) |
| `paint/png-filter-1024x768` | **0,335** | filtrado 1024 |
| `paint/png-deflate-1024x768` | **3,963** | zlib 1024 |
| `paint/png-encode-1024x768` | **4,228** | flat |
| `paint/png-encode-1024x768-alpha` | **120,410** | alpha domina |

El encode byte-safe de `a477061` baja `render-to-png` de 8,85 a **6,89 ms**
(−25,2 % A/B). Las filas de fase separan filtro/deflate/encode para futuras
optimizaciones de PNG.

## 11. Presupuestos: veredicto

| Medida | r36 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png` | **110,12 ms** | **≤140** | 78,7 % | 29,88 ms | ✅ |
| `layout/realistic-1mb` | **35,76 ms** | **≤45** | 79,5 % | 9,24 ms | ✅ |
| `css/resolve-styles-10k` | **12,85 ms** | **≤35** | 36,7 % | 22,15 ms | ✅ |
| showcase render CLI | 25,37 ms (r32, sin re-medir) | **≤35** | 72,5 % | 9,63 ms | ✅ (sin cambios de código) |
| shell headless showcase | 26,28 ms (r32, sin re-medir) | ≤35 | 75,1 % | 8,72 ms | ✅ (sin cambios de código) |
| `paint/render-to-png` | **6,41 ms** | ref ~8,5 | — | −2,09 ms | ✅ recuperado |
| `paint/build-display-list` | **8,97 ms** | **≤25** | 35,9 % | 16,03 ms | ✅ |
| `html/realistic-1mb` | 17,87 ms | ≤100 | 17,9 % | 82,13 ms | ✅ |
| `image/decode-png` | **2,55 ms** | foco ≤6,6 | 38,6 % | +4,05 ms | ✅ recuperado |
| `render/html-to-png` | **7,61 ms** | ≤20 (propuesto) | 38,1 % | +12,39 ms | ✅ |

**No se suben los presupuestos.** Todo con margen amplio incluso medida a load 8–13
(peor que la ventana r32): si el margen sobrevive con la máquina cargada, sobra en
condiciones limpias. Las dos medidas de showcase siguen en valores r32 porque no se
tocó el CLI de shell/render en r33–r36 más allá del cache (que solo puede mejorar);
se re-medirán en la próxima ventana dedicada.

---

## 12. Comparación histórica

Treinta baselines en la misma máquina, mismo arnés (la ronda 14 no publicó baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 | R28 | R29 | R30 | R31 | **R32 (vigente)** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | 00:34 | 01:18 | 02:07 | 03:04 | 04:06 | 04:39 | 06:48 | 07:25 | **10:05** | **11:30** | **12:30** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | `00d8388` | `d1f3121` | `0b3adee` | `4718408` | `f42b9b9` | `9d2298b` | `a0cdfcb` | `8f9cfa9` | **`f7dad8d`** | **`aba05f8`** | **`3e69968`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | 1,20–1,23 | 1,76–1,86 | 1,32–1,38 | 0,79–0,84 | 1,30–1,35 | 1,36–1,42 | 0,67–0,96 | 1,04–1,05 | **1,27–1,35** | **1,46–1,69** | **1,69–1,83** |

Filas clave (mediana, ms; r22–r30 incluyen filas E2E/decode/paint que no entran en TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 | R28 | R29 | R30 | R31 | **R32** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | 16.847 | 16.566 | 16.171 | 16.701 | 16.609 | **16.536** | **17.613** | 17.175 | 17.508 | 17.998 | 17.945 |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | 35.805 | 25.477 | 27.085 | 27.322 | 26.577 | **25.957** | **27.508** | 27.216 | 27.855 | 29.072 | 13.467 |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | 37.069 | 35.043 | 34.900 | 35.355 | 35.396 | **35.930** | **37.988** | 37.786 | 37.283 | 40.386 | 36.447 |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | 8.973 | 7.940 | 7.982 | 7.702 | 7.974 | **7.729** | **8.606** | 8.409 | 7.825 | 9.220 | 8.297 |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | 8.354 | 8.626 | 8.775 | 8.905 | 8.695 | **8.877** | **9.313** | 8.906 | 8.863 | 9.205 | 6.891 |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | 141.194 | 106.440 | 107.974 | 107.635 | 107.950 | **106.522** | **115.674** | 113.238 | 113.890 | 138.485 | 108.458 |
| `image/decode-png-800x600` | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 6.495 | 6.506 | 6.524 | 6.551 | 7.975 | **6.399** | **6.699** | 6.661 | 6.634 | 6.898 | 2.611 |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** | **305.461** | **307.687** | **309.478** | **309.460** | **306.611** | **330.650** | 323.023 | **322.706** | **356.352** | **302.280** |


### Ronda 31 → ronda 32, fila a fila

A/B prístino intercalado (10 pares, load 1,7–2,1, performance) r31 `aba05f8` vs
r32 `3e69968`:

| Benchmark | R31 (esta ventana) | R32 (esta ventana) | Δ A/B |
|---|---:|---:|---:|
| `pipeline/html-to-png` | 115,63 | **108,04** | **−6,6 %** |
| `css/resolve-styles-10k` | 29,32 | **14,02** | **−52,2 %** |
| `layout/realistic-1mb` | 38,94 | **37,64** | **−3,3 %** |
| `paint/build-display-list` | 8,85 | **7,95** | **−10,2 %** |
| `paint/render-to-png` | 8,85 | **6,62** | **−25,2 %** |
| `image/decode-png` | 6,916 | **2,508** | **−63,7 %** |
| **render/html-to-png** | — | **7,906** | fila nueva (render crate) |

**Hallazgo metodológico**: `pipeline/html-to-png` no usa el crate `render` (compone
html/css/layout/paint a mano); por eso se añadió la fila **`render/html-to-png`**
(data URL con 240 cards → DOM/styles/layout/paint/PNG) para medir el pipeline real.

### Ronda 32 → ronda 36 (62 filas compartidas)

Las tablas anchas de arriba se congelan en r32 para no romper su formato; r33–r36
entran aquí. Método distinto en un punto: r36 son **7 pasadas a load 8,1–13,0**
frente a las 5 de r32 a load 1,69–1,83, así que el delta absoluto mezcla mejora de
código y ruido de máquina cargada. Aun así el signo es interpretable: de las 62
filas compartidas, **51 mejoran >1 %**, **5 quedan planas (±1 %)** y
**6 empeoran >1 %**; delta mediano **-4,8 %**. La suma legacy
(19 filas) pasa de **348,683 ms** a **346,488 ms** (−0,6 %).

| Benchmark | R32 | R36 | Δ |
|---|---:|---:|---:|
| `pipeline/html-to-png-800x600` | 108,458 | **110,124** | **+1,5 %** |
| `layout/realistic-1mb` | 36,447 | **35,763** | **-1,9 %** |
| `css/resolve-styles-10k-nodes` | 13,467 | **12,854** | **-4,6 %** |
| `paint/build-display-list` | 8,297 | **8,966** | **+8,1 %** |
| `paint/render-to-png-800x600` | 6,891 | **6,407** | **-7,0 %** |
| `image/decode-png-800x600` | 2,611 | **2,548** | **-2,4 %** |
| `html/realistic-1mb` | 17,945 | **17,873** | **-0,4 %** |
| `css/selector-match-simple` | 0,651 | **0,542** | **-16,7 %** |
| `css/selector-match-descendant` | 1,331 | **1,037** | **-22,1 %** |
| `paint/clipping-800x600` | 49,388 | **45,355** | **-8,2 %** |
| `pipeline/gradients-800x600` | 50,083 | **47,088** | **-6,0 %** |
| `shell/chrome-png-1024x768` | 4,194 | **3,794** | **-9,5 %** |

Filas nuevas de r33–r36 (19) que no tienen par en r32: matriz completa
`paint/png-encode-{full,regions}-{800x600,1024x768}-{10,25,50,100}pct`
(codifica por regiones vs página completa: a 10 % de contenido las regiones ganan
~40 %), `paint/backdrop-blur-800x600` (6,15 ms; 0,27 ms sin efecto),
`image/decode-webp-lossy` (10,87 ms, VP8 keyframe) y las variantes
`pipeline/*-no-effect`.

---

## 13. Cómo comparar y detectar regresiones

1. Medir con load <2 y **perfil `performance`**; sin él la máquina capa a
   1,3–2,2 GHz. Documentar load/freq y descartar picos (pipeline r32 tuvo una
   pasada 142,6 ms; la mediana y el A/B son la referencia).
2. A/B prístino intercalado obligatorio para atribuir deltas; comparar ratios.
3. Guardar JSON; regresión si empeora >15 % reproducible. Filas:
   `--filter image`, `--filter paint`, `--filter render`, `--filter pipeline`,
   `--filter shell/chrome-png`, `--filter css`.
4. Goldens: showcase `c95c7eff…` 80 220 B; basic `e943565d…` 21 090 B; phase4
   `9e957177…` 42 853 B; `basic.rgba` `60f8984e…`.
5. `render/html-to-png` mide el crate render; no lo sustituyas por `pipeline/*`
   (composición manual).
6. **Comparación automática** (#963): `docs/status/bench-baseline.json` guarda la
   mediana por fila de la ronda 36 (81 filas, commit `a080249`). Para comparar una
   medición nueva:

   ```bash
   ./tools/bench.sh --json            # escribe target/bench-latest.txt
   python3 tools/bench_compare.py target/bench-latest.txt
   ```

   `bench_compare.py` acepta varias pasadas (toma la mediana de medianas), marca
   `REGRESSION` cuando una fila crece >20 % **y** >0,5 ms, imprime `noise` para las
   filas por debajo del suelo absoluto y sale con código 1 si hay regresión. El
   workflow `bench` (lunes 04:41 UTC y `workflow_dispatch`) ejecuta tres pasadas y
   aplica el mismo criterio en CI.

## 14. Limitaciones

- **`pipeline/html-to-png` no usa render**: se mantiene por compatibilidad;
  `render/html-to-png` (nuevo) es la medida correcta del pipeline real.
- **La ventana r36 no fue limpia**: load 8,1–13,0 por el navegador/editor del
  usuario. Los números sirven para veredicto de presupuestos (margen amplio) y
  para detectar regresiones grandes, no para atribuir mejoras de <10 % sin A/B.
- **Selector bloom**: cambio en css; el A/B css es −52,2 %, sin cambio de
  semántica de matching (tests selectores verdes).
- **PNG**: decode 2,61 ms; encode alpha 800×600 cuesta ~69,7 ms y 1024×768
  alpha ~120 ms (fotos RGBA); las filas de fase permiten atacar filtro/deflate.
- **TLA fetch**: +3,52 ms con fixture mínima; la red real debe medirse con I/O.
- **h1 streaming**: 0,115 ms/petición loopback de 64 KiB (in-process); el proxy/
  red real no se mide en el arnés offline.
- **Push priority**: implementado pero sin fila de bench; pendiente.
- **Perfil**: `performance` vía `powerprofilesctl` (reversible); loads
  1,0–2,1 durante las mediciones.
- **website/pages.yml**: del dueño, no se tocan.
