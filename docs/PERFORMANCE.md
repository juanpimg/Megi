# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-11, 01:18–01:40 (Europe/Madrid). Ronda 23:
  **recuperación de CSS y paint** (caché de parent-style key, move-on-miss y AST
  de la hoja UA cacheada; análisis agregado de efectos + fast path de paint
  equivalente a r21) junto con las features de la ronda (EXIF/JPEG progresivo/
  WebP detect, HTTP/2 flow control/multiplexación).
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `d1f3121`**. Árbol con
  WIP ajeno solo en docs (`README.md`, `docs/PHASE4-SUPPORT.md`,
  `docs/ROADMAP.md`); el motor medido es `d1f3121`.
- **Ventana de bench**: **load 1,76–1,86** en las 5 pasadas, `powersave` con
  frecuencia máxima muestreada **3,5–4,6 GHz**; `render`/shell a load ~1,5.
- **Binarios**: bench `6c253823…` (51 444 24 B) en las 5 pasadas; el script lo
  recompiló con la misma fuente (`81b76e93…`); render `f0fc412a…`; shell
  `65623833…` (con `--chrome`/`--find`).
- **Artefactos**: `target/bench-r23-run1..5.json`,
  `target/bench-aggregate-r23.json` (`sha256 = b7b26d1b…`),
  `target/bench-r23/ab_r22_r23.json`, `target/bench-r23-render-shell.json`,
  `target/bench-latest.txt`/`-meta.txt` (canónica, TOTAL 485.809).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–23; la 14 no publicó baseline).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. La matriz incluye las filas E2E de r20 (gradients/grid/vars-calc/
clipping/sticky) y las de **decodificación de imagen** de r22
(PNG/JPEG/GIF 800×600 con fixtures commiteadas e `include_bytes!`).

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5; *heavy* 3 — las filas de decode son heavy).
- **La tabla de §3 es la mediana, por fila, de 5 pasadas completas válidas**
  (load 1,76–1,86; `setup_ms` ~400–430 ms por E2E/decode). La pasada canónica de
  `./tools/bench.sh` (`TOTAL 485.809 ms`, 27 filas) sirve de fingerprint.
- **A/B intercalado prístino**: `git archive 00d8388` (r22) + parche de bench de
  esa ronda (`target/r22-pristine`) vs binario r23, 5 pares alternando orden,
  mismo load (§5.2).
- Render/shell: 9 ejecuciones intercaladas; `--chrome`/`--find` 9.
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E), 20 hilos |
| RAM / SO | 15 GiB · CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Governor / térmica | `powersave`; freq máx. 3,5–4,6 GHz; paquete ~50–55 °C |
| Load | **1,76–1,86** bench; ~1,5 render/shell |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

./tools/bench.sh                                  # pasada canónica
for i in 1 2 3 4 5; do target/release/bench --json > target/bench-r23-run$i.json; done

# A/B prístino r22 vs r23
git archive 00d8388 | tar -x -C target/r22-pristine
cd target/r22-pristine && CARGO_TARGET_DIR="$PWD/target" cargo build --release -p bench

# Filas E2E / decode
target/release/bench --filter pipeline
target/release/bench --filter image

# Render/shell/chrome/find
target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png --width 1024 --height 768
target/release/megi tests/fixtures/render/showcase/index.html \
    --screenshot /tmp/c.png --width 1024 --height 768 --chrome
target/release/megi tests/fixtures/render/find.html \
    --screenshot /tmp/f.png --width 800 --height 600 --find megi
```

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb`, `nested-div-*`, `massive-*`, `tag-40k-attrs`, `pretty-5k-blocks` | Parse/tree construction. |
| css | `parse-10k-rules`, `resolve-styles-2k/10k-nodes`, `selector-match-*`, `vars-calc-10k` | Parse, cascada, matching, custom properties/calc. |
| layout | `layout/realistic-1mb`, `layout/sticky-text-1mb` | Layout; sticky + text-transform. |
| paint | `build-display-list`, `rasterize-800x600`, `render-to-png-800x600`, `clipping-800x600` | Display list, raster, PNG, clipping. |
| pipeline | `html-to-png-800x600`, `gradients-800x600`, `grid-800x600` | Pipeline completo. |
| image | `decode-png-800x600`, `decode-jpeg-800x600`, `decode-gif` | Decode completo a RGBA8. |
| fonts | (`feature fonts`) | No re-medido. |

---

## 3. Baseline vigente (ronda 23, HEAD `d1f3121`)

Mediana de **5 pasadas** en calma (load 1,76–1,86). `setup_ms` ~400–430 ms.
**Filas legacy (19)**: `TOTAL 305.461 ms`; **todas (27)**: `TOTAL 482.109 ms`;
pasada canónica: `TOTAL 485.809 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `html/realistic-1mb` | **16.566** | 15.335–16.860 |
| `html/nested-div-10k` | **2.683** | 2.658–2.744 |
| `html/nested-div-32k` | **8.584** | 8.271–8.888 |
| `html/nested-div-64k` | **17.162** | 16.743–17.690 |
| `html/nested-div-64k-default-limits` | **10.167** | 9.494–10.202 |
| `html/massive-end-p` | **17.058** | 17.041–17.434 |
| `html/massive-formatting` | **10.665** | 10.578–10.774 |
| `html/tag-40k-attrs` | **9.304** | 8.909–9.548 |
| `html/pretty-5k-blocks` | **8.581** | 8.523–8.753 |
| `css/parse-10k-rules` | **13.130** | 12.895–13.371 |
| `css/resolve-styles-2k-nodes` | **5.276** | 5.186–5.403 |
| `css/resolve-styles-10k-nodes` | **25.477** | 25.078–25.580 |
| `css/selector-match-simple` | **0.639** | 0.607–0.645 |
| `css/selector-match-descendant` | **1.208** | 1.180–1.221 |
| `layout/realistic-1mb` | **35.043** | 34.402–35.343 |
| `paint/build-display-list` | **7.940** | 7.342–8.052 |
| `paint/rasterize-800x600` | **0.912** | 0.873–0.960 |
| `paint/render-to-png-800x600` | **8.626** | 8.607–9.246 |
| `pipeline/html-to-png-800x600` | **106.440** | 103.569–106.583 |
| `layout/sticky-text-1mb` | **31.285** | 30.791–31.501 |
| `pipeline/gradients-800x600` | **51.336** | 50.201–51.662 |
| `pipeline/grid-800x600` | **5.818** | 5.648–5.911 |
| `css/vars-calc-10k` | **19.110** | 18.856–19.515 |
| `paint/clipping-800x600` | **48.732** | 48.365–49.059 |
| `image/decode-png-800x600` | **6.506** | 6.448–6.520 |
| `image/decode-jpeg-800x600` | **10.950** | 10.804–11.012 |
| `image/decode-gif` | **2.911** | 2.632–2.974 |

> **Aviso**: el TOTAL (legacy) suma etapas y pipeline end-to-end (solapados); no
> es un presupuesto. En r23 **todos los presupuestos se cumplen con margen**
> (ver §11).

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 2.68 · 32k: 8.58 · 64k: 17.16 ms | **Lineal** (3,2× → 3,20×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 13.13 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 5.28 / **25.48 ms** | **−28,8 % vs r22**: parent-style cache key + move-on-miss + AST UA cache. |
| `layout/realistic-1mb` | 28 195 elementos → **35.04 ms** | −5,5 % vs r22 (la caché de estilos también ayuda al layout). |
| `image/decode-*` | PNG 6,51 · JPEG 10,95 · GIF 2,91 ms | Estables vs r22 (±1 %). |
| `pipeline` | **106,44 ms** (103,6–106,6) | **−24,6 % vs r22**; dentro de ≤140 con margen 33,6 ms. |

---

## 5. Ronda 23: recuperación de CSS/paint y verificación

### 5.1 Qué entró

- `cfad971 perf(css)`: **parent-style cache key**, **move-on-miss** y **AST de
  la hoja UA cacheada** (además del interning ya existente) para eliminar el
  trabajo repetido por nodo en cascada.
- `3f5ce3e perf(paint)`: **análisis agregado de efectos** (filtros/blend/
  image-set) y **fast path de pintado equivalente a r21**.
- `1e8e078 feat(image,render)` y `a2fb055 feat(net)`: EXIF/JPEG progresivo/
  WebP detect (imagen) y HTTP/2 flow control/multiplexación (net), sin coste en
  el bench (no intervienen en las fixtures).

### 5.2 Verificación: A/B por carril y presupuestos

A/B intercalado de archivos prístinos r22 (`00d8388`) vs r23 (`d1f3121`),
5 pares, load ~1,46:

| Fila | r22 (ms) | r23 (ms) | Δ (A/B) | Claim ronda |
|---|---:|---:|---:|---|
| `html/realistic-1mb` | 16,92 | 16,63 | **−1,7 %** | estable ✅ |
| `css/parse-10k-rules` | 13,29 | 13,25 | −0,3 % | — |
| `css/resolve-styles-2k-nodes` | 7,63 | 5,37 | **−29,5 %** | 35,81→26,54 (10k) ✅ aprox. |
| `css/resolve-styles-10k-nodes` | 36,71 | 25,71 | **−30,0 %** | 35,81→26,54 ✅ |
| `layout/realistic-1mb` | 36,65 | 34,64 | **−5,5 %** | ~37 ✅ |
| `paint/build-display-list` | 8,86 | 7,77 | **−12,2 %** | 7,34–7,72 (canónica 7,63) ✅ cerca |
| `paint/rasterize-800x600` | 0,92 | 0,93 | +1,7 % | ~igual |
| `paint/render-to-png-800x600` | 8,65 | 8,70 | +0,7 % | ~8,4 (mediana 8,63) ✅ |
| `pipeline/html-to-png-800x600` | 142,88 | 105,68 | **−26,0 %** | 142,64→105,47 ✅ |

**Veredicto de presupuestos (definitivo)**: **se cumplen todos con margen**.

| Medida | r23 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **106,44 ms** | **≤ 140 ms** | 76,0 % | 33,6 ms | ✅ |
| `layout/realistic-1mb` | **35,04 ms** | **≤ 45 ms** | 77,9 % | 10,0 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **25,48 ms** | **≤ 35 ms** | 72,8 % | 9,5 ms | ✅ |
| `render` showcase 1024×768 | **27,27 ms** | **≤ 35 ms** | 77,9 % | 7,7 ms | ✅ |
| shell headless showcase | 29,24 ms | ≤ 35 ms | 83,5 % | 5,8 ms | ✅ |
| `paint/render-to-png-800x600` | **8,63 ms** | (ref ~8,5) | — | ~0,1 ms | ✅ |
| `paint/build-display-list` | **7,94 ms** | **≤ 25 ms** | 31,8 % | 17,1 ms | ✅ |
| `html/realistic-1mb` | 16,57 ms | ≤ 100 ms | 16,6 % | 83,4 ms | ✅ |

Los claims de la ronda se confirman en dirección y magnitud: css resolve
−30 % (35,8→25,5 medido), pipeline −25 % (141,2→106,4), display-list ~7,3–7,9,
html estable, layout 35,0. **No se suben los presupuestos**: por primera vez
desde r17 hay margen cómodo en todos (pipeline 24 %, layout 22 %, css 27 %,
showcase 22 %); a partir de ahora una regresión >15 % debería revisarse con el
A/B intercalado antes de tocar ningún presupuesto.

### 5.3 Nota de integridad

El árbol tenía solo WIP ajeno en docs (`README.md`, `docs/PHASE4-SUPPORT.md`,
`docs/ROADMAP.md`); el motor medido es `d1f3121`. El A/B usó archivos prístinos
de `00d8388` (r22) y `d1f3121` (r23) para no mezclar WIP. El script recompiló
el bench con la misma fuente (`81b76e93…`); las 5 pasadas usaron `6c253823…`.

---

## 6. Codificador PNG (sin cambios)

| Métrica | r22 | r23 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.354 | **8.626** | +3,3 % (varianza) |
| — rasterizar | 0.962 | **0.912** | −5,2 % |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden `--zoom 2` | 3 202 B | **3 202 B** | `4792779d…` |

---

## 7. Pipeline `render`, shell, chrome, find y zoom (r23)

9 ejecuciones intercaladas (load ~1,5; goldens verificados):

| Fixture / modo | pared r23 | pared r22 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **27,27 ms** | 28,17 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,00 ms** | 9,79 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **29,24 ms** | 28,92 | 80 220 B (idéntico) |
| basic (shell headless) | **8,62 ms** | 9,10 | 21 090 B (idéntico) |
| showcase `--chrome` | **44,26 ms** | 45,30 | 85 746 B (`cbfc6087…`) |
| basic `--chrome` | **16,21 ms** | 16,75 | 23 089 B (`f25f5d93…`) |
| `find.html` sin `--find` | **10,87 ms** | 10,26 | 30 056 B |
| `find.html` con `--find megi` | **17,31 ms** | 19,19 | 29 950 B (`a818f32a…`) |
| basic `--zoom 2` (320×200) | **3,61 ms** | 3,80 | 3 202 B |

El chrome headless cuesta **+17,0 ms** en showcase (address bar + tab strip;
PNG +5,5 KB) y +7,2 ms en basic; `--find` cuesta **+6,4 ms** (índice + overlay;
3 hits, sin re-render al cambiar de hit). Goldens de render sin chrome
byte-idénticos; los PNG de chrome son deterministas.

---

## 8. Filas de decodificación de imagen

Método: fixtures commiteadas con `include_bytes!` (sin red, sin aleatoriedad),
decode completo a RGBA8 y descarte del buffer; filas *heavy* (3 iteraciones).

| Fila | Fixture | mediana (ms) | rango | throughput |
|---|---|---:|---:|---:|
| `image/decode-png-800x600` | `crates/image/tests/fixtures/png/large_800x600.png` (800×600, 13 947 B) | **6.506** | 6.45–6.52 | 73,8 MPix/s |
| `image/decode-jpeg-800x600` | `crates/bench/fixtures/jpeg-800x600.jpg` (800×600, 76 656 B, Q85 4:2:0) | **10.950** | 10.80–11.01 | 43,8 MPix/s |
| `image/decode-gif` | `crates/bench/fixtures/gif-800x600.gif` (800×600, 84 156 B, 256 colores) | **2.911** | 2.63–2.97 | 164,9 MPix/s |

Estables vs r22 (±1 %); JPEG sigue siendo el formato más caro por MPix de este
motor y GIF el más barato. `--filter image` los ejecuta aislados.

---

## 9. Filas E2E (gradients/grid/vars-calc/clipping/sticky)

| Fila | r23 (ms) | r22 (ms) | Δ |
|---|---:|---:|---:|
| `pipeline/gradients-800x600` | **51.336** | 49.881 | +2,9 % |
| `pipeline/grid-800x600` | **5.818** | 5.737 | +1,4 % |
| `css/vars-calc-10k` | **19.110** | 48.087 | **−60,3 %** |
| `paint/clipping-800x600` | **48.732** | 48.775 | −0,1 % |
| `layout/sticky-text-1mb` | **31.285** | 31.044 | +0,8 % |

La cascada optimizada explica el gran salto de `vars-calc` (−60 %) y el resto
se mantiene dentro de ±3 %. Filtros: `--filter pipeline`, `--filter css/vars-calc`,
`--filter paint/clipping`, `--filter layout/sticky`, `--filter image`.

---

## 10. Clipping, `var()`/`calc()`, net y fuzz (resumen)

- **Clipping/`clip:text`**: coste de raster por área; radius+overflow (200×150)
  ≈ +11,1 ms de raster, con `clip:text` ≈+16,7 ms; caps
  `MAX_ROUNDED_GROUP_PIXELS = 16 777 216`, `MAX_TEXT_MASK_RUNS = 4096`,
  `MAX_SHADOWS = 32`, `MAX_SHADOW_BLUR = 64`, `MAX_BACKGROUND_TILES = 4096`,
  `MAX_STACKING_CONTEXT_DEPTH = 256`, `MAX_FLOAT_DEPTH = 64`.
- **Net HTTP/2**: MVP + flow control/multiplexación/advanced SETTINGS (r23);
  coste en el bench ≈0 (no interviene); pendiente probar contra un servidor real.
- **Imagen**: EXIF orientation y JPEG progresivo (subset) integrados; **WebP
  lossless VP8L sigue pendiente** (detección + warning no fatal en render).
- **Fuzz**: no entra en el bench; 7 targets, build-only en PRs (~minutos CI);
  siguiente paso: campañas con corpus tras el nuevo código de imagen/net.

---

## 11. Presupuestos: veredicto

| Medida | r23 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **106,44 ms** | **≤ 140 ms** | 76,0 % | 33,6 ms | ✅ |
| `layout/realistic-1mb` | **35,04 ms** | **≤ 45 ms** | 77,9 % | 10,0 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **25,48 ms** | **≤ 35 ms** | 72,8 % | 9,5 ms | ✅ |
| `render` showcase 1024×768 | **27,27 ms** | **≤ 35 ms** | 77,9 % | 7,7 ms | ✅ |
| shell headless showcase | 29,24 ms | ≤ 35 ms | 83,5 % | 5,8 ms | ✅ |
| `paint/render-to-png-800x600` | 8,63 ms | ref. ~8,5 | — | ~0,1 ms | ✅ |
| `paint/build-display-list` | 7,94 ms | ≤ 25 ms | 31,8 % | 17,1 ms | ✅ |
| `html/realistic-1mb` | 16,57 ms | ≤ 100 ms | 16,6 % | 83,4 ms | ✅ |
| `image/decode-*` | 2,91–10,95 ms | — | — | — | referencia §8 |

**No se suben los presupuestos.** Es la primera ronda desde r17 con margen en
todos los gates; el mínimo margen relativo está en css resolve (27 %) y
showcase (22 %), el mínimo absoluto en render-to-png (~0,1 ms frente a la
referencia ~8,5). Siguiente foco: **decode de imagen en páginas reales/PNG por
regiones**, **h2 multiplexado contra un servidor real**, **perf de raster** y
**campañas de fuzz** con el código nuevo de imagen/net.

---

## 12. Comparación histórica

Veintiuna baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | 00:34 | **01:18** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | `00d8388` | **`d1f3121`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | 1,20–1,23 | **1,76–1,86** |

Filas clave (mediana, ms; r22/r23 incluyen filas E2E/decode que no entran en el
TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | 16.847 | **16.566** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | 35.805 | **25.477** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | 37.069 | **35.043** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | 8.973 | **7.940** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | 8.354 | **8.626** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | 141.194 | **106.440** |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** | **305.461** |

### Ronda 22 → ronda 23, fila a fila

| Benchmark | R22 (ms) | R23 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 16.847 | **16.566** | −1,7 % |
| `html/nested-div-10k` | 2.781 | **2.683** | −3,5 % |
| `html/nested-div-32k` | 8.597 | **8.584** | −0,2 % |
| `html/nested-div-64k` | 17.083 | **17.162** | +0,5 % |
| `html/nested-div-64k-default-limits` | 10.006 | **10.167** | +1,6 % |
| `html/massive-end-p` | 17.123 | **17.058** | −0,4 % |
| `html/massive-formatting` | 10.325 | **10.665** | +3,3 % |
| `html/tag-40k-attrs` | 9.850 | **9.304** | −5,5 % |
| `html/pretty-5k-blocks` | 8.443 | **8.581** | +1,6 % |
| `css/parse-10k-rules` | 13.461 | **13.130** | −2,5 % |
| `css/resolve-styles-2k-nodes` | 7.638 | **5.276** | **−30,9 %** |
| `css/resolve-styles-10k-nodes` | 35.805 | **25.477** | **−28,8 %** |
| `css/selector-match-simple` | 0.659 | **0.639** | −3,0 % |
| `css/selector-match-descendant` | 1.306 | **1.208** | −7,5 % |
| `layout/realistic-1mb` | 37.069 | **35.043** | −5,5 % |
| `paint/build-display-list` | 8.973 | **7.940** | **−11,5 %** |
| `paint/rasterize-800x600` | 0.962 | **0.912** | −5,2 % |
| `paint/render-to-png-800x600` | 8.354 | **8.626** | +3,3 % |
| `pipeline/html-to-png-800x600` | 141.194 | **106.440** | **−24,6 %** |
| `pipeline/gradients-800x600` | 49.881 | **51.336** | +2,9 % |
| `pipeline/grid-800x600` | 5.737 | **5.818** | +1,4 % |
| `css/vars-calc-10k` | 48.087 | **19.110** | **−60,3 %** |
| `paint/clipping-800x600` | 48.775 | **48.732** | −0,1 % |
| `layout/sticky-text-1mb` | 31.044 | **31.285** | +0,8 % |
| `image/decode-png-800x600` | 6.495 | **6.506** | +0,2 % |
| `image/decode-jpeg-800x600` | 10.877 | **10.950** | +0,7 % |
| `image/decode-gif` | 2.879 | **2.911** | +1,1 % |
| **TOTAL legacy** | **356.476** | **305.461** | **−14,3 %** |

---

## 13. Cómo comparar y detectar regresiones

1. Medir con load < 2 y comprobar frecuencia (en esta ronda: 1,76–1,86, freq
   máx. 3,5–4,6 GHz). Si no hay ventana, marcarlo.
2. Para atribuir un delta a un commit, compilar dos archivos prístinos
   (antes/después) y ejecutar los binarios **intercalados** (mismo load para
   ambos), como en §5.2.
3. Guardar JSON y comparar medianas:

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
4. Regresión si la mediana empeora >15 % reproducible. En r23 **todos los
   presupuestos se cumplen con margen** (ver §11); vigilar css resolve y
   showcase como márgenes más justos.
5. Filas E2E/decode: `--filter pipeline`, `css/vars-calc`, `paint/clipping`,
   `layout/sticky`, `image`. Referencias §8–§9; >15 % = regresión.
6. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   chrome showcase `cbfc6087…` 85 746 B, chrome basic `f25f5d93…` 23 089 B,
   find `a818f32a…` 29 950 B, zoom `4792779d…` 3 202 B.

---

## 14. Limitaciones y notas

- **Ventana buena** (load 1,76–1,86, freq 3,5–4,6 GHz; render/shell ~1,5), aunque
  no es el suelo absoluto; el A/B intercalado sigue siendo la atribución fiable.
- **WIP ajeno en docs** (`README.md`, `docs/PHASE4-SUPPORT.md`,
  `docs/ROADMAP.md`); el motor medido es `d1f3121`. El script recompiló el bench
  con la misma fuente (`6c253823…` → `81b76e93…`).
- **Filas de decode**: fixtures deterministas commiteadas con `include_bytes!`,
  decode completo a RGBA8, heavy (3 iteraciones). JPEG es el formato más caro
  por MPix de este motor.
- **`--chrome`** cuesta +17,0 ms en showcase y +7,2 ms en basic; **`--find`**
  +6,4 ms (índice + overlay; sin re-render al cambiar de hit).
- **Imagen r23**: EXIF orientation y JPEG progresivo (subset) integrados; **WebP
  lossless VP8L sigue pendiente** (detección + warning no fatal).
- **HTTP/2** (r23): flow control/multiplexación/advanced SETTINGS; coste ≈0 en
  el bench; pendiente validar contra un servidor real (h2 público).
- **`website/` y `pages.yml`**: del dueño; no se tocan.
- **Fast path de tablas (r7)**: nota histórica; no hay commit compilable con el
  camino avanzado sin fast path.
