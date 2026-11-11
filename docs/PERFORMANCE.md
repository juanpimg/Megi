# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-11, 03:04–03:40 (Europe/Madrid). Ronda 25:
  CSS Color 4/5 (color spaces + `color-mix()`), **ES modules + providers JS**,
  **WebP VP8L lossless completo**, providers JS en render/MutationObserver y
  escritura directa del PNG del shell.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `4718408`**. Árbol con
  WIP ajeno solo en docs (`CHANGELOG`, `CONTRIBUTING`, `README`, `SECURITY`,
  `ARCHITECTURE`, `HANDOFF`, `JAVASCRIPT-STRATEGY`, `PHASE4-SUPPORT`, `ROADMAP`,
  `TOOLING`, `status/progress.toml`); motor medido `4718408`.
- **Ventana de bench**: **load 0,79–0,84** en las 5 pasadas válidas, `powersave`
  con frecuencia máxima muestreada **~4,0–4,6 GHz**; render/shell a load ~0,8–1,6.
- **Binarios**: bench `9d0b862e…` (5 224 264 B, con las 3 filas WebP nuevas),
  render `6d68045f…` (20 748 712 B), shell `420c4bc4…` (24 929 576 B).
- **Artefactos**: `target/bench-r25-run1..5.json`,
  `target/bench-aggregate-r25.json` (`sha256 = b7e8ac02…`),
  `target/bench-r25/ab_r24_r25.json`, `target/bench-r25-feature-costs.json`,
  `target/bench-latest.txt`/`-meta.txt` (canónica, TOTAL 487.074).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–25; la 14 no publicó baseline).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. La matriz incluye E2E (gradients/grid/vars-calc/clipping/sticky) y
**decodificación de imagen**: PNG/JPEG/GIF 800×600 (r22) y **WebP VP8L real**
(r25: foto con transforms, alpha y paleta; fixtures de `crates/image`
pixel-validadas contra PIL).

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5; *heavy* 3 — las filas de decode son heavy).
- **La tabla de §3 es la mediana, por fila, de 5 pasadas completas válidas**
  (load 0,79–0,84; `setup_ms` ~400–430 ms por E2E/decode). La pasada canónica de
  `./tools/bench.sh` (`TOTAL 487.074 ms`, 30 filas) sirve de fingerprint.
- **A/B intercalado prístino**: `git archive 0b3adee` (r24) + bench de esa ronda
  (`target/r24-pristine`) vs binario r25, 5 pares alternando orden, load
  1,52–1,66 (§5.2).
- Render/shell: 7 ejecuciones intercaladas; también fixtures r25
  (`color_spaces`, `js_provider_actions`, `js_external`, `webp_real`) frente a
  equivalentes sin la feature (temporales en `target/bench-r25/`).
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E), 20 hilos |
| RAM / SO | 15 GiB · CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Governor / térmica | `powersave`; freq máx. ~4,0–4,6 GHz; paquete ~50–55 °C |
| Load | **0,79–0,84** bench; 0,8–1,6 render/shell |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

./tools/bench.sh                                  # pasada canónica
for i in 1 2 3 4 5; do target/release/bench --json > target/bench-r25-run$i.json; done

# A/B prístino r24 vs r25
git archive 0b3adee | tar -x -C target/r24-pristine
cd target/r24-pristine && CARGO_TARGET_DIR="$PWD/target" cargo build --release -p bench

# Filas decode (incluye WebP VP8L)
target/release/bench --filter image

# Rendimiento de fixtures r25
target/release/render tests/fixtures/render/color_spaces.html /tmp/c.png --width 800 --height 600
target/release/render target/bench-r25/color_spaces_noprops.html /tmp/n.png --width 800 --height 600

# Render/shell/chrome/find/save-png
target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png --width 1024 --height 768
target/release/megi tests/fixtures/render/showcase/index.html \
    --screenshot /tmp/c.png --save-png /tmp/saved.png --width 1024 --height 768
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
| image | `decode-png-800x600`, `decode-jpeg-800x600`, `decode-gif`, `decode-webp-lossless`, `decode-webp-alpha`, `decode-webp-palette` | Decode completo a RGBA8 (WebP VP8L desde r25). |
| fonts | (`feature fonts`) | No re-medido. |

---

## 3. Baseline vigente (ronda 25, HEAD `4718408`)

Mediana de **5 pasadas** en calma (load 0,79–0,84). `setup_ms` ~400–430 ms.
**Filas legacy (19)**: `TOTAL 309.478 ms`; **todas (30)**: `TOTAL 488.636 ms`;
pasada canónica: `TOTAL 487.074 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `html/realistic-1mb` | **16.701** | 16.496–16.862 |
| `html/nested-div-10k` | **2.698** | 2.692–2.743 |
| `html/nested-div-32k` | **8.470** | 8.356–8.551 |
| `html/nested-div-64k` | **16.810** | 16.640–17.138 |
| `html/nested-div-64k-default-limits` | **9.978** | 9.862–10.032 |
| `html/massive-end-p` | **17.529** | 17.169–17.812 |
| `html/massive-formatting` | **10.507** | 10.439–10.630 |
| `html/tag-40k-attrs` | **9.380** | 9.182–9.396 |
| `html/pretty-5k-blocks` | **8.755** | 8.522–8.859 |
| `css/parse-10k-rules` | **13.162** | 12.688–13.404 |
| `css/resolve-styles-2k-nodes` | **5.703** | 5.440–5.720 |
| `css/resolve-styles-10k-nodes` | **27.322** | 26.365–27.482 |
| `css/selector-match-simple` | **0.659** | 0.650–0.668 |
| `css/selector-match-descendant` | **1.242** | 1.110–1.279 |
| `layout/realistic-1mb` | **35.355** | 34.575–35.687 |
| `paint/build-display-list` | **7.702** | 7.589–7.855 |
| `paint/rasterize-800x600` | **0.965** | 0.901–0.999 |
| `paint/render-to-png-800x600` | **8.905** | 8.523–8.944 |
| `pipeline/html-to-png-800x600` | **107.635** | 106.277–109.844 |
| `layout/sticky-text-1mb` | **31.006** | 30.659–32.066 |
| `pipeline/gradients-800x600` | **52.591** | 51.860–53.067 |
| `pipeline/grid-800x600` | **5.818** | 5.656–6.096 |
| `css/vars-calc-10k` | **19.932** | 19.263–20.078 |
| `paint/clipping-800x600` | **49.295** | 48.989–49.870 |
| `image/decode-png-800x600` | **6.551** | 6.536–6.765 |
| `image/decode-jpeg-800x600` | **10.978** | 10.513–11.212 |
| `image/decode-gif` | **2.912** | 2.719–2.956 |
| `image/decode-webp-lossless` | **0.046** | 0.042–0.051 |
| `image/decode-webp-alpha` | **0.023** | 0.021–0.025 |
| `image/decode-webp-palette` | **0.006** | 0.006–0.006 |

> **Aviso**: el TOTAL (legacy) suma etapas y pipeline end-to-end (solapados); no
> es un presupuesto. En r25 **todos los presupuestos se cumplen con margen**
> (ver §11). Las filas WebP usan fixtures pequeñas (64×48/40×32/32×24) y se
> miden en **microsegundos**: no son comparables en MPix con las de 800×600.

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 2.70 · 32k: 8.47 · 64k: 16.81 ms | **Lineal** (3,14× → 3,15×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 13.16 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 5.70 / **27.32 ms** | +0,9 % vs r24 (Color 4/5 añade parseo por propiedad). |
| `layout/realistic-1mb` | 28 195 elementos → **35.36 ms** | +1,3 % vs r24. |
| `image/decode-*` | PNG 6,55 · JPEG 10,98 · GIF 2,91 ms; WebP 6–46 µs | Decode WebP real (foto transforms) **~0,046 ms para 64×48**. |
| `pipeline` | **107,64 ms** (106,3–109,8) | −0,3 % vs r24; margen 32,4 ms bajo ≤140. |

---

## 5. Ronda 25: features y verificación

### 5.1 Qué entró

- `bd3b317 feat(css,paint)`: **CSS Color 4/5** (display-p3/lab/oklab/oklch/lch/
  hwb y `color-mix()` en srgb/srgb-linear/oklab/oklch).
- `9522cb1 feat(js)`: **ES modules subset** con resolver y loader providers.
- `6d80b20 feat(image,render)`: **WebP VP8L lossless completo** (transforms,
  Huffman complejo/meta, LZ77 y colour cache) y wiring en render.
- `c90aab4 feat(render)`: providers JS + cierre del bucle MutationObserver.
- `560f036 perf(shell)`: escritura directa del PNG del render.

### 5.2 Verificación: A/B y presupuestos

A/B intercalado de archivos prístinos r24 (`0b3adee`) vs r25 (`4718408`),
5 pares, load 1,52–1,66:

| Fila | r24 (ms) | r25 (ms) | Δ (A/B) |
|---|---:|---:|---:|
| `html/realistic-1mb` | 16,64 | 16,56 | **−0,5 %** |
| `css/resolve-styles-10k-nodes` | 26,27 | 27,05 | **+3,0 %** |
| `layout/realistic-1mb` | 34,66 | 34,97 | +0,9 % |
| `paint/build-display-list` | 8,01 | 7,61 | **−5,0 %** |
| `paint/render-to-png-800x600` | 8,57 | 8,62 | +0,5 % |
| `pipeline/html-to-png-800x600` | 107,69 | 108,09 | **+0,4 %** |

**Lectura**: la ronda 25 es prácticamente neutra en el camino caliente
(pipeline +0,4 %, display list −5 %); el único coste estructural medible es
**css resolve +3 %** (parseo de color spaces/`color-mix()` por declaración). Los
presupuestos conservan márgenes amplios. **No se suben los presupuestos.**

| Medida | r25 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **107,64 ms** | **≤ 140 ms** | 76,9 % | 32,4 ms | ✅ |
| `layout/realistic-1mb` | **35,36 ms** | **≤ 45 ms** | 78,6 % | 9,6 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **27,32 ms** | **≤ 35 ms** | 78,1 % | 7,7 ms | ✅ |
| `render` showcase 1024×768 | **28,34 ms** | **≤ 35 ms** | 81,0 % | 6,7 ms | ✅ |
| shell headless showcase | 28,50 ms | ≤ 35 ms | 81,4 % | 6,5 ms | ✅ |
| `paint/render-to-png-800x600` | **8,91 ms** | ref. ~8,5 | — | +0,4 ms | ✅ |
| `paint/build-display-list` | **7,70 ms** | **≤ 25 ms** | 30,8 % | 17,3 ms | ✅ |
| `html/realistic-1mb` | 16,70 ms | ≤ 100 ms | 16,7 % | 83,3 ms | ✅ |

### 5.3 Nota de integridad

La pasada canónica inicial se ejecutó con load ajeno alto y se descartó (TOTAL
1 301 ms); la publicada es de la ventana 0,79–0,84 / 0,8–1,6. Árbol con WIP
ajeno solo en docs; motor medido `4718408`. Bench recompilado tras añadir las
filas WebP (`9d0b862e…`).

---

## 6. Filas de decodificación de imagen (incluye WebP VP8L)

Método: fixtures commiteadas con `include_bytes!` (sin red, sin aleatoriedad),
decode completo a RGBA8 y descarte del buffer; filas *heavy* (3 iteraciones).

| Fila | Fixture | mediana | rango | throughput |
|---|---|---:|---:|---:|
| `image/decode-png-800x600` | `crates/image/tests/fixtures/png/large_800x600.png` (800×600, 13 947 B) | **6.551 ms** | 6.54–6.77 ms | 73,2 MPix/s |
| `image/decode-jpeg-800x600` | `crates/bench/fixtures/jpeg-800x600.jpg` (800×600, 76 656 B) | **10.978 ms** | 10.51–11.21 ms | 43,7 MPix/s |
| `image/decode-gif` | `crates/bench/fixtures/gif-800x600.gif` (800×600, 84 156 B) | **2.912 ms** | 2.72–2.96 ms | 164,8 MPix/s |
| `image/decode-webp-lossless` | `crates/image/tests/fixtures/webp/photo_transforms.webp` (64×48, 488 B) | **0.046 ms** | 0.042–0.051 ms | 66,8 MPix/s |
| `image/decode-webp-alpha` | `.../webp/alpha_real.webp` (40×32, 96 B) | **0.023 ms** | 0.021–0.025 ms | 55,7 MPix/s |
| `image/decode-webp-palette` | `.../webp/palette_real.webp` (32×24, 106 B) | **0.006 ms** | 0.006–0.006 ms | 128 MPix/s |

Las fixtures WebP son **reales de libwebp/PIL** (foto con transforms
predictor/colour, alpha y paleta) y están validadas **píxel a píxel** contra el
RGBA de PIL en `crates/image/tests/webp_tests.rs`. La fila de foto (transforms +
Huffman complejo + LZ77/cache) tarda **~46 µs**; las de alpha/paleta son más
pequeñas y no comparables en MPix con las de 800×600. El bench incluye un test
de determinismo que decodifica las seis fixtures y comprueba dimensiones/RGBA.

---

## 7. Coste de las features r25 (fixtures propias)

Wall time de `render` (7 ejecuciones, load ~0,8), contra la misma fixture sin la
feature (temporales en `target/bench-r25/`):

| Fixture | con feature | sin feature | Δ | notas |
|---|---:|---:|---:|---|
| `color_spaces.html` (Color 4/5 + color-mix) | **9,83 ms** | 8,48 ms | **+1,35 ms (+15,9 %)** | 13 swatches/gradientes con color spaces. |
| `js_provider_actions.html` (history/storage/location) | **9,33 ms** | 8,55 ms | **+0,78 ms (+9,1 %)** | PNG byte-idéntico: el efecto es de script, no visual. |
| `js_external.html` (provider externo) | **9,95 ms** | — | — | incluye resolución/carga de módulo JS. |
| `webp_real.html` (foto VP8L 64×48 real) | **9,77 ms** | 8,67 ms (sin `<img>`) | **+1,10 ms (+12,7 %)** | decode WebP + pintado de la imagen. |
| `--save-png` (shell showcase) | **67,33 ms** | 28,50 ms | **+38,8 ms** | r24: 66,05 ms (+37,8) → **sin mejora medible**; se escriben dos PNG (`--screenshot` + `--save-png`). |

Coste absoluto pequeño (<1,5 ms por fixture) y fuera del camino caliente de los
benchmarks core; no afecta a los presupuestos. El commit de escritura directa
del PNG no reduce la pareja medida (dos ficheros): pendiente revisar el flujo
`--save-png` aislado, que hoy requiere `--screenshot` para entrar en headless.

---

## 8. Codificador PNG y goldens

| Métrica | r24 | r25 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.775 | **8.905** | +1,5 % (varianza) |
| — rasterizar | 0.949 | **0.965** | +1,7 % |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden phase4 PNG | 42 853 B | **42 853 B** | `9e957177…` |
| Golden `basic.rgba` | 1 920 000 B | **1 920 000 B** | `60f8984e…` |

---

## 9. Pipeline `render`, shell, chrome, find y save-png (r25)

7 ejecuciones intercaladas (load ~0,8; goldens verificados):

| Fixture / modo | pared r25 | pared r24 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **28,34 ms** | 27,72 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,38 ms** | 9,59 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **28,50 ms** | 28,22 | 80 220 B (idéntico) |
| showcase `--chrome` | **45,80 ms** | 47,28 | 85 746 B (`cbfc6087…`) |
| `find.html` con `--find megi` | **17,23 ms** | 18,04 | 29 950 B (`a818f32a…`) |
| showcase `--screenshot` + `--save-png` | **67,33 ms** | 66,05 | 80 220 B (idéntico) |

`--chrome` cuesta +17,3 ms en showcase; `--find` queda en 17,23 ms;
`--save-png` añade +38,8 ms en la pareja con `--screenshot` (dos PNG). Goldens
sin chrome byte-idénticos.

---

## 10. Filas E2E (gradients/grid/vars-calc/clipping/sticky)

| Fila | r25 (ms) | r24 (ms) | Δ |
|---|---:|---:|---:|
| `pipeline/gradients-800x600` | **52.591** | 51.455 | +2,2 % |
| `pipeline/grid-800x600` | **5.818** | 5.731 | +1,5 % |
| `css/vars-calc-10k` | **19.932** | 20.559 | −3,0 % |
| `paint/clipping-800x600` | **49.295** | 48.512 | +1,6 % |
| `layout/sticky-text-1mb` | **31.006** | 31.387 | −1,2 % |

Variaciones ≤±3 % (ruido/ventana); sin coste estructural nuevo. Filtros:
`--filter pipeline`, `--filter css/vars-calc`, `--filter paint/clipping`,
`--filter layout/sticky`, `--filter image`.

---

## 11. Presupuestos: veredicto

| Medida | r25 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **107,64 ms** | **≤ 140 ms** | 76,9 % | 32,4 ms | ✅ |
| `layout/realistic-1mb` | **35,36 ms** | **≤ 45 ms** | 78,6 % | 9,6 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **27,32 ms** | **≤ 35 ms** | 78,1 % | 7,7 ms | ✅ |
| `render` showcase 1024×768 | **28,34 ms** | **≤ 35 ms** | 81,0 % | 6,7 ms | ✅ |
| shell headless showcase | 28,50 ms | ≤ 35 ms | 81,4 % | 6,5 ms | ✅ |
| `paint/render-to-png-800x600` | 8,91 ms | ref. ~8,5 | — | +0,4 ms | ✅ |
| `paint/build-display-list` | **7,70 ms** | **≤ 25 ms** | 30,8 % | 17,3 ms | ✅ |
| `html/realistic-1mb` | 16,70 ms | ≤ 100 ms | 16,7 % | 83,3 ms | ✅ |
| `image/decode-*` | 0,006–10,98 ms | — | — | — | referencia §6 |

**No se suben los presupuestos.** Todos se cumplen con márgenes cómodos; el
menor margen relativo es showcase (19,0 %) y css resolve (21,9 %), el absoluto
render-to-png (+0,4 ms frente a ~8,5). Las features r25 añaden coste pequeño y
cuantificado (§5.2 y §7). Siguiente foco: **h2 push/priorities contra servidor
real**, **animación SVG**, **PNG por regiones**, **perf de decode en páginas
reales** y **campañas de fuzz**; mantener A/B intercalado antes de tocar
presupuestos.

---

## 12. Comparación histórica

Veintitrés baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | 00:34 | 01:18 | 02:07 | **03:04** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | `00d8388` | `d1f3121` | `0b3adee` | **`4718408`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | 1,20–1,23 | 1,76–1,86 | 1,32–1,38 | **0,79–0,84** |

Filas clave (mediana, ms; r22–r25 incluyen filas E2E/decode que no entran en el
TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | 16.847 | 16.566 | 16.171 | **16.701** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | 35.805 | 25.477 | 27.085 | **27.322** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | 37.069 | 35.043 | 34.900 | **35.355** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | 8.973 | 7.940 | 7.982 | **7.702** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | 8.354 | 8.626 | 8.775 | **8.905** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | 141.194 | 106.440 | 107.974 | **107.635** |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** | **305.461** | **307.687** | **309.478** |

### Ronda 24 → ronda 25, fila a fila

| Benchmark | R24 (ms) | R25 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 16.171 | **16.701** | +3,3 % |
| `html/nested-div-10k` | 2.653 | **2.698** | +1,7 % |
| `html/nested-div-32k` | 8.405 | **8.470** | +0,8 % |
| `html/nested-div-64k` | 16.666 | **16.810** | +0,9 % |
| `html/nested-div-64k-default-limits` | 9.891 | **9.978** | +0,9 % |
| `html/massive-end-p` | 17.123 | **17.529** | +2,4 % |
| `html/massive-formatting` | 10.335 | **10.507** | +1,7 % |
| `html/tag-40k-attrs` | 9.338 | **9.380** | +0,4 % |
| `html/pretty-5k-blocks` | 8.628 | **8.755** | +1,5 % |
| `css/parse-10k-rules` | 13.258 | **13.162** | −0,7 % |
| `css/resolve-styles-2k-nodes` | 5.602 | **5.703** | +1,8 % |
| `css/resolve-styles-10k-nodes` | 27.085 | **27.322** | +0,9 % |
| `css/selector-match-simple` | 0.674 | **0.659** | −2,2 % |
| `css/selector-match-descendant` | 1.278 | **1.242** | −2,8 % |
| `layout/realistic-1mb` | 34.900 | **35.355** | +1,3 % |
| `paint/build-display-list` | 7.982 | **7.702** | −3,5 % |
| `paint/rasterize-800x600` | 0.949 | **0.965** | +1,7 % |
| `paint/render-to-png-800x600` | 8.775 | **8.905** | +1,5 % |
| `pipeline/html-to-png-800x600` | 107.974 | **107.635** | −0,3 % |
| `pipeline/gradients-800x600` | 51.455 | **52.591** | +2,2 % |
| `pipeline/grid-800x600` | 5.731 | **5.818** | +1,5 % |
| `css/vars-calc-10k` | 20.559 | **19.932** | −3,0 % |
| `paint/clipping-800x600` | 48.512 | **49.295** | +1,6 % |
| `layout/sticky-text-1mb` | 31.387 | **31.006** | −1,2 % |
| `image/decode-png-800x600` | 6.524 | **6.551** | +0,4 % |
| `image/decode-jpeg-800x600` | 11.014 | **10.978** | −0,3 % |
| `image/decode-gif` | 2.968 | **2.912** | −1,9 % |
| `image/decode-webp-lossless` | — | **0.046** | nuevo |
| `image/decode-webp-alpha` | — | **0.023** | nuevo |
| `image/decode-webp-palette` | — | **0.006** | nuevo |
| **TOTAL legacy** | **307.687** | **309.478** | **+0,6 %** |

---

## 13. Cómo comparar y detectar regresiones

1. Medir con load < 2 y comprobar frecuencia (en esta ronda: 0,79–0,84, freq
   máx. ~4,0–4,6 GHz). Si no hay ventana, marcarlo.
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
4. Regresión si la mediana empeora >15 % reproducible. En r25 todos los
   presupuestos se cumplen con margen (§11); el coste nuevo está en §5–§7.
5. Filas E2E/decode: `--filter pipeline`, `css/vars-calc`, `paint/clipping`,
   `layout/sticky`, `image` (incluye WebP VP8L).
6. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   phase4 `9e957177…` 42 853 B, `basic.rgba` `60f8984e…` 1 920 000 B, chrome
   `cbfc6087…` 85 746 B, find `a818f32a…` 29 950 B.

---

## 14. Limitaciones y notas

- **Ventana muy buena** (load 0,79–0,84, freq ~4,0–4,6 GHz); A/B a load 1,52–1,66.
  La pasada canónica inicial se descartó por load ajeno (TOTAL 1 301 ms).
- **WIP ajeno en docs**; el motor medido es `4718408` y solo se tocaron
  `crates/bench`/`tools/bench.sh`/`docs/PERFORMANCE.md`.
- **Filas WebP**: fixtures reales de libwebp/PIL (64×48 foto, 40×32 alpha,
  32×24 paleta), validadas píxel a píxel en `crates/image`; se miden en **µs**,
  no comparables en MPix con 800×600. VP8 lossy sigue sin decoder (warning).
- **Coste de features r25**: cuantificado en §7 (color spaces +1,35 ms, providers
  JS +0,78 ms, WebP real +1,10 ms, `--save-png` +38,8 ms en la pareja con
  `--screenshot`).
- **`--chrome`** del shell +17,3 ms en showcase; `--find` 17,23 ms.
- **`website/` y `pages.yml`**: del dueño; no se tocan.
- **Fast path de tablas (r7)**: nota histórica; no hay commit compilable con el
  camino avanzado sin fast path.
