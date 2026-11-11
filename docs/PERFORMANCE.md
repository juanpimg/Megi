# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**.

- **Fecha**: 2026-11-11, 12:29–12:31 (Europe/Madrid). Ronda 32: **selector bloom
  CSS**, **layout más barato**, **PNG decode/encode** (fila nueva de fases),
  **render fast paths (skip de scans)**, **top-level await/live bindings**,
  **h1 streaming real + prioridad de push** y **fila `render/html-to-png`**.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `3e69968`** (cierre r32
  `76e1678`; WIP ajeno en docs, sin tocar el bench).
- **Ventana primaria**: **load 1,69–1,83** en las 5 pasadas válidas; **platform
  profile `performance`** (`powerprofilesctl`; la máquina estaba `quiet` y capaba
  1,3–2,2 GHz), freq 3,2–4,35 GHz (alguna pasada con 3,2); canónica
  **TOTAL 745.353**.
- **Binarios**: bench `a3ac9fd1…` (con `render/html-to-png` y filas PNG), render
  `d75e0a16…`, shell `9406cfb6…`; r31 prístino `aba05f8` en `target/r31-pristine/`
  para A/B.
- **Artefactos**: `target/bench-r32b-run1..5.json` (primaria),
  `target/bench-aggregate-r32.json` (`sha256 e4490d04…`),
  `target/bench-r32-ab-r31.json`, `target/bench-r32-feature-costs.json`,
  `target/bench-r32/measure-net/` (h1 streaming), `target/bench-latest.txt`.
- **Hallazgo metodológico**: la fila histórica `pipeline/html-to-png` **no usa el
  crate `render`** (compone html/css/layout/paint a mano). Se añadió
  **`render/html-to-png`** al bench (data URL 14 376 B, 240 cards, render crate
  completo) y entra en el TOTAL; el resto de filas `pipeline/*` siguen igual.
- **Caveat**: la pasada 1 de pipeline fue un pico (142,6 ms); la mediana de las 5
  pasadas es 108,46 ms y el A/B confirma la mejora. No se suben presupuestos.

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

## 3. Baseline vigente (ronda 32, HEAD `3e69968`)

5 pasadas (load 1,69–1,83). **Filas legacy (19)**: `TOTAL 302,280 ms`; **todas
(62)**: `TOTAL 758,386 ms`; canónica: `TOTAL 745.353 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `css/parse-10k-rules` | **13.936** | 13.510–15.010 |
| `css/resolve-styles-10k-nodes` | **13.467** | 13.103–16.147 |
| `css/resolve-styles-2k-nodes` | **3.047** | 2.893–3.729 |
| `css/selector-match-descendant` | **1.331** | 1.169–1.485 |
| `css/selector-match-simple` | **0.651** | 0.603–0.868 |
| `css/vars-calc-10k` | **21.093** | 20.351–22.265 |
| `html/massive-end-p` | **18.742** | 18.205–20.604 |
| `html/massive-formatting` | **11.263** | 10.775–12.611 |
| `html/nested-div-10k` | **2.970** | 2.850–3.242 |
| `html/nested-div-32k` | **9.140** | 8.565–10.196 |
| `html/nested-div-64k` | **18.388** | 17.545–19.390 |
| `html/nested-div-64k-default-limits` | **10.872** | 10.720–12.430 |
| `html/pretty-5k-blocks` | **9.094** | 8.656–10.692 |
| `html/realistic-1mb` | **17.945** | 17.475–19.434 |
| `html/tag-40k-attrs` | **10.282** | 10.005–11.136 |
| `image/decode-gif` | **3.160** | 3.035–3.396 |
| `image/decode-jpeg-800x600` | **11.560** | 11.119–12.621 |
| `image/decode-png-800x600` | **2.611** | 2.464–2.793 |
| `image/decode-png-scaled-x2` | **3.002** | 2.896–3.437 |
| `image/decode-webp-alpha` | **0.026** | 0.025–0.035 |
| `image/decode-webp-lossless` | **0.053** | 0.050–0.057 |
| `image/decode-webp-lossy` | **11.446** | 11.121–12.308 |
| `image/decode-webp-lossy-q75` | **0.062** | 0.060–0.077 |
| `image/decode-webp-lossy-q90` | **0.068** | 0.062–0.078 |
| `image/decode-webp-palette` | **0.007** | 0.006–0.009 |
| `layout/realistic-1mb` | **36.447** | 35.187–39.652 |
| `layout/sticky-text-1mb` | **33.260** | 32.064–35.812 |
| `paint/backdrop-fixture` | **0.321** | 0.306–0.348 |
| `paint/backdrop-fixture-no-effect` | **0.022** | 0.021–0.022 |
| `paint/backdrop-fixture-png` | **0.598** | 0.591–0.607 |
| `paint/backdrop-fixture-png-no-effect` | **0.471** | 0.456–0.513 |
| `paint/backdrop-fixture-raster` | **1.326** | 1.225–1.402 |
| `paint/backdrop-fixture-raster-no-effect` | **1.019** | 0.975–1.062 |
| `paint/build-display-list` | **8.297** | 7.914–11.248 |
| `paint/clipping-800x600` | **49.388** | 47.104–52.446 |
| `paint/filter-fixture` | **0.310** | 0.298–0.323 |
| `paint/filter-fixture-no-effect` | **0.297** | 0.296–0.316 |
| `paint/layer-effects-800x600` | **29.276** | 27.966–29.974 |
| `paint/object-fit-800x600` | **3.466** | 3.421–3.912 |
| `paint/png-deflate-1024x768` | **3.963** | 3.506–4.287 |
| `paint/png-deflate-800x600` | **2.240** | 2.151–2.401 |
| `paint/png-encode-1024x768` | **4.228** | 3.952–5.092 |
| `paint/png-encode-1024x768-alpha` | **120.410** | 114.466–122.952 |
| `paint/png-encode-800x600` | **2.608** | 2.515–2.769 |
| `paint/png-encode-800x600-alpha` | **69.666** | 66.738–71.920 |
| `paint/png-filter-1024x768` | **0.335** | 0.319–0.586 |
| `paint/png-filter-800x600` | **0.229** | 0.197–0.233 |
| `paint/rasterize-800x600` | **1.059** | 0.960–1.165 |
| `paint/render-to-png-800x600` | **6.891** | 6.476–7.039 |
| `pipeline/backdrop-fixture-300x260` | **1.931** | 1.900–2.010 |
| `pipeline/backdrop-fixture-300x260-no-effect` | **1.517** | 1.457–1.524 |
| `pipeline/filter-fixture-300x260` | **0.847** | 0.840–0.922 |
| `pipeline/filter-fixture-300x260-no-effect` | **0.866** | 0.836–0.924 |
| `pipeline/gradients-800x600` | **50.083** | 48.300–52.245 |
| `pipeline/grid-800x600` | **5.053** | 4.645–5.208 |
| `pipeline/html-to-png-800x600` | **108.458** | 107.449–142.607 |
| `render/html-to-png` | **7.906** | 7.650–9.448 |
| `shell/chrome-png-1024x768` | **4.194** | 4.057–4.762 |
| `shell/chrome-png-640x400` | **1.350** | 1.262–1.877 |
| `shell/save-png-encode-800x600` | **2.976** | 2.846–3.164 |
| `shell/save-png-total-800x600` | **2.859** | 2.786–3.154 |
| `shell/save-png-write-800x600` | **0.003** | 0.003–0.004 |

> El TOTAL legacy suma etapas solapadas; no es presupuesto. Las filas de fase
> PNG, `render/html-to-png` y `image/decode-png-scaled-x2` son nuevas de r32.

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

| Medida | r32 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png` | **108,46 ms** | ≤140 | 77,5 % | 31,54 ms | ✅ |
| `layout/realistic-1mb` | **36,45 ms** | ≤45 | 81,0 % | 8,55 ms | ✅ |
| `css/resolve-styles-10k` | **13,47 ms** | ≤35 | 38,5 % | 21,53 ms | ✅ |
| showcase render CLI | **25,37 ms** | ≤35 | 72,5 % | 9,63 ms | ✅ |
| shell headless showcase | 26,28 ms | ≤35 | 75,1 % | 8,72 ms | ✅ |
| `paint/render-to-png` | **6,89 ms** | ref ~8,5 | — | −1,61 ms | ✅ recuperado |
| `paint/build-display-list` | **8,30 ms** | ≤25 | 33,2 % | 16,70 ms | ✅ |
| `html/realistic-1mb` | 17,95 ms | ≤100 | 17,9 % | 82,05 ms | ✅ |
| `image/decode-png` | **2,61 ms** | foco ≤6,6 | 39,6 % | **+3,99 ms** | ✅ **recuperado** |
| `render/html-to-png` | **7,91 ms** | — | — | — | nuevo, informativo |

**No se suben los presupuestos.** El foco decode-png queda holgadamente
recuperado (2,61 vs 6,6) y render-to-png baja a 6,89 (ref ~8,5). El pipeline
recupera margen (77,5 %) pese a la pasada pico; css/layout/display mejoran por
el bloom y los cambios de layout. A/B en §12.

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

| Medida | r32 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png` | **108,46 ms** | **≤140** | 77,5 % | 31,54 ms | ✅ |
| `layout/realistic-1mb` | **36,45 ms** | **≤45** | 81,0 % | 8,55 ms | ✅ |
| `css/resolve-styles-10k` | **13,47 ms** | **≤35** | 38,5 % | 21,53 ms | ✅ |
| showcase render CLI | **25,37 ms** | **≤35** | 72,5 % | 9,63 ms | ✅ |
| shell headless showcase | 26,28 ms | ≤35 | 75,1 % | 8,72 ms | ✅ |
| `paint/render-to-png` | **6,89 ms** | ref ~8,5 | — | −1,61 ms | ✅ recuperado |
| `paint/build-display-list` | **8,30 ms** | **≤25** | 33,2 % | 16,70 ms | ✅ |
| `html/realistic-1mb` | 17,95 ms | ≤100 | 17,9 % | 82,05 ms | ✅ |
| `image/decode-png` | **2,61 ms** | foco ≤6,6 | 39,6 % | +3,99 ms | ✅ recuperado |
| `render/html-to-png` | **7,91 ms** | — | — | — | nuevo |

**No se suben los presupuestos.** Todos con margen amplio; decode-png recuperado
y render-to-png por debajo de referencia. Próximo foco: **asignar presupuesto a
`render/html-to-png`**, **backdrop/PNG regions**, **fingerprint layout cache**,
**HTTP/1.0/gzip streaming** y **fuzz de larga duración**.

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

## 14. Limitaciones

- **`pipeline/html-to-png` no usa render**: se mantiene por compatibilidad;
  `render/html-to-png` (nuevo) es la medida correcta del pipeline real.
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
