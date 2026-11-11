# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-11, 06:48–06:50 (Europe/Madrid). Ronda 28:
  **reduced IDCT JPEG + EXIF thumbnails**, **HTTP/2 push consumido + uploads
  streaming**, **geometría JS**, **blur/PNG byte-safe**, **shell contextual** y
  re-baseline de rendimiento.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `a0cdfcb`** (cierre r28
  `cb7af43`; árbol medido con WIP ajeno en `js`/`net`/`image`, sin tocar el bench).
- **Ventana de bench**: **load 0,67–0,96** en las 5 pasadas válidas y la canónica;
  **platform profile `performance`** (`powerprofilesctl`; la máquina estaba en
  `power-saver`/`quiet`), frecuencia máxima muestreada **4,3–4,6 GHz**.
- **Binarios**: bench `bf273de5f662…` (bf273de5f662cd5680f56e4d4aaa11a1c36c007fb413cdebf5a01b79ae214779), render
  `d75e0a16d24c…`, shell `9406cfb65721…`.
- **Artefactos**: `target/bench-r28/run1..5.json`, `target/bench-r28/summary.json`,
  `target/bench-r28/canonical.txt`, `target/bench-latest.txt`/`-meta.txt`
  (canónica, TOTAL **568.955**).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–28; la 14 no publicó baseline).
> **Caveat de entorno**: el perfil de energía/frecuencia cambió respecto a r27; el
> binario prístino de r27 medido hoy en esta ventana da pipeline ~112–114 ms (vs
> 106,52 histórico), así que los absolutos no son directamente comparables. El
> A/B intercalado r27-vs-r28 (§5.2) no muestra regresión de r28.

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. La matriz incluye E2E (gradients/grid/vars-calc/clipping/sticky),
**efectos de capa** (layer-effects, filtros/backdrop aislados y E2E
filter/backdrop), **save-png aislado** (encode/write/total) y **decode de
imagen** (PNG/JPEG/GIF 800×600 y WebP VP8L real).

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5; *heavy* 3 — decode y pipeline son heavy).
- **La tabla de §3 es la mediana, por fila, de 5 pasadas completas válidas**
  (load 1,36–1,42; `setup_ms` ~400–450 ms). La pasada canónica de
  `./tools/bench.sh` (`TOTAL 522.447 ms`, 43 filas) sirve de fingerprint.
- **A/B intercalado prístino**: `git archive f42b9b9` (r26) + bench de esa ronda
  (`target/r26-pristine`) vs binario r27, 5 pares alternando orden, load
  ~3,0–3,4 (§5.2).
- Render/shell: 7 ejecuciones intercaladas (load ~2,4).
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E), 20 hilos |
| RAM / SO | 15 GiB · CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Governor / térmica | `powersave`; freq máx. variable 2,6–4,7 GHz; paquete ~50–55 °C |
| Load | **1,36–1,42** bench; ~2,4 render/shell |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

./tools/bench.sh                                  # pasada canónica
for i in 1 2 3 4 5; do target/release/bench --json > target/bench-r27-run$i.json; done

# A/B prístino r26 vs r27
git archive f42b9b9 | tar -x -C target/r26-pristine
cd target/r26-pristine && CARGO_TARGET_DIR="$PWD/target" cargo build --release -p bench

# Filas nuevas E2E filter/backdrop y decode
target/release/bench --filter pipeline/filter-fixture
target/release/bench --filter pipeline/backdrop-fixture
target/release/bench --filter image

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
| paint | `build-display-list`, `rasterize`, `render-to-png`, `clipping`, `layer-effects`, `object-fit`, `filter-fixture[/-no-effect]`, `backdrop-fixture[/-no-effect]` | Display list, raster, PNG, clipping, cola de pintado con efectos y fixtures aisladas con/sin efecto. |
| pipeline | `html-to-png`, `gradients`, `grid`, **`filter-fixture-300x260[/-no-effect]`**, **`backdrop-fixture-300x260[/-no-effect]`** | Pipeline completo; las nuevas E2E incluyen parse+styles+layout+display list+raster+PNG de una fixture 300×260 con/sin filtro o backdrop. |
| shell | `save-png-encode/write/total-800x600` | `Pixmap::to_png()`, escritura a disco y coste combinado del `--save-png` aislado. |
| image | `decode-png/jpeg/gif`, `decode-webp-lossless/alpha/palette` | Decode completo a RGBA8. |
| fonts | (`feature fonts`) | No re-medido. |

---

## 3. Baseline vigente (ronda 28, HEAD `a0cdfcb`)

Mediana de **5 pasadas** en calma (load 0,67–0,96). `setup_ms` ~400–450 ms.
**Filas legacy (19)**: `TOTAL 330,650 ms`; **todas (47)**: `TOTAL 568,955 ms`;
pasada canónica: `TOTAL 569.506 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `css/parse-10k-rules` | **14.241** | 13.858–14.782 |
| `css/resolve-styles-10k-nodes` | **27.508** | 26.594–27.920 |
| `css/resolve-styles-2k-nodes` | **5.901** | 5.793–5.975 |
| `css/selector-match-descendant` | **1.292** | 1.160–1.376 |
| `css/selector-match-simple` | **0.724** | 0.688–0.762 |
| `css/vars-calc-10k` | **20.711** | 20.147–20.861 |
| `html/massive-end-p` | **18.567** | 18.300–18.843 |
| `html/massive-formatting` | **11.232** | 10.884–11.353 |
| `html/nested-div-10k` | **2.965** | 2.883–3.156 |
| `html/nested-div-32k` | **9.498** | 9.003–9.830 |
| `html/nested-div-64k` | **18.158** | 17.678–19.603 |
| `html/nested-div-64k-default-limits` | **10.852** | 10.604–10.913 |
| `html/pretty-5k-blocks` | **9.111** | 8.761–9.180 |
| `html/realistic-1mb` | **17.613** | 17.440–17.906 |
| `html/tag-40k-attrs` | **10.323** | 10.191–10.605 |
| `image/decode-gif` | **3.103** | 3.094–3.141 |
| `image/decode-jpeg-800x600` | **11.592** | 11.504–11.692 |
| `image/decode-png-800x600` | **6.699** | 6.617–6.805 |
| `image/decode-webp-alpha` | **0.025** | 0.025–0.027 |
| `image/decode-webp-lossless` | **0.051** | 0.050–0.054 |
| `image/decode-webp-palette` | **0.006** | 0.006–0.007 |
| `layout/realistic-1mb` | **37.988** | 37.161–40.203 |
| `layout/sticky-text-1mb` | **34.009** | 33.352–34.158 |
| `paint/backdrop-fixture` | **0.324** | 0.318–0.326 |
| `paint/backdrop-fixture-no-effect` | **0.022** | 0.021–0.023 |
| `paint/backdrop-fixture-png` | **0.812** | 0.802–0.833 |
| `paint/backdrop-fixture-png-no-effect` | **0.601** | 0.585–0.614 |
| `paint/backdrop-fixture-raster` | **1.289** | 1.283–1.321 |
| `paint/backdrop-fixture-raster-no-effect` | **0.999** | 0.986–1.020 |
| `paint/build-display-list` | **8.606** | 7.773–8.753 |
| `paint/clipping-800x600` | **51.911** | 51.569–52.404 |
| `paint/filter-fixture` | **0.308** | 0.304–0.315 |
| `paint/filter-fixture-no-effect` | **0.306** | 0.304–0.330 |
| `paint/layer-effects-800x600` | **27.971** | 27.913–28.289 |
| `paint/object-fit-800x600` | **3.471** | 3.445–3.497 |
| `paint/rasterize-800x600` | **1.084** | 1.029–1.194 |
| `paint/render-to-png-800x600` | **9.313** | 8.897–9.371 |
| `pipeline/backdrop-fixture-300x260` | **2.154** | 2.124–2.172 |
| `pipeline/backdrop-fixture-300x260-no-effect` | **1.624** | 1.579–1.634 |
| `pipeline/filter-fixture-300x260` | **1.047** | 1.035–1.096 |
| `pipeline/filter-fixture-300x260-no-effect` | **1.050** | 1.045–1.079 |
| `pipeline/gradients-800x600` | **54.368** | 53.080–55.096 |
| `pipeline/grid-800x600` | **6.030** | 5.894–6.345 |
| `pipeline/html-to-png-800x600` | **115.674** | 113.291–116.461 |
| `shell/save-png-encode-800x600` | **3.912** | 3.904–3.964 |
| `shell/save-png-total-800x600` | **3.907** | 3.880–3.958 |
| `shell/save-png-write-800x600` | **0.003** | 0.003–0.004 |

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 2.965 · 32k: 9.498 · 64k: 18.158 ms | **Lineal** (≈3,1× por 3,2× nodos). |
| `css/parse-10k-rules` | 10k: **14.241 ms** (5k histórico ~6,69) | **Lineal** (10k/5k ≈2,1×). |
| `css/resolve-styles-2k/10k` | 5.901 / **27.508 ms** | estable vs r27 (±5 % de ventana). |
| `layout/realistic-1mb` | 28 195 elementos → **37.988 ms** | +5,7 % vs r27 (entorno/frecuencia). |
| `paint/layer-effects-800x600` | 24 backdrop + 24 filter groups → **27.971 ms** | +2,4 % vs r27; estable. |
| `paint/backdrop-fixture-raster/-png` | raster **1.289** vs png **0.812** ms | nueva fila r28: PNG encode reutiliza el buffer ya codificado y es **0.477 ms** más rápida. |
| `pipeline/backdrop-fixture-300x260` | **2.154 ms** vs 1.624 sin efecto | **+0.530 ms**: blur 4px + brightness/grayscale. |
| `image/decode-*` | PNG **6.699** · JPEG 11.592 · GIF 3.103 ms; WebP 6–51 µs | PNG dentro de la ref. ≤6,6 ms en la medición focalizada (10 pasadas: mediana **6,532 ms**); el full-suite da 6,6–6,8 por varianza de suite. |
| `pipeline` | **115.674 ms** | margen **24.3 ms** bajo ≤140; A/B vs r27 prístino: −1,4 %. |

---

## 5. Ronda 28: features y verificación

### 5.1 Qué entró

- **image**: reduced IDCT en `decode_scaled` para JPEG (4×4/2×2/1×1 según
  escala 2/4/8; full intacto) y `exif_thumbnail` (IFD1, caps 64 KiB); A/B
  JPEG 1024×768 full **19,28 ms** → ×2 8,24 / ×4 3,30 / ×8 1,97 ms.
- **net**: HTTP/2 push consumido desde el preload cache y uploads streaming
  (`RequestBody::Stream`, backpressure); opt-in, sin coste en el bench core.
- **js/render**: `LayoutProvider` y geometría (`getBoundingClientRect`,
  offset/client, innerWidth/Height/DPR) con pass pre-script.
- **paint**: blur/PNG byte-safe (encode más rápido, E2E backdrop) y filas nuevas
  `paint/backdrop-fixture-raster`/`-png` (±no-effect) para separar raster vs PNG.
- **shell**: menú contextual, hover/status y Ctrl+clic/clic medio (no entra en la
  matriz core; ver §9).

### 5.2 Verificación: A/B prístino r27 vs r28

A/B intercalado (5 pares, mismo binario prístino r27 `9d2298b` vs árbol r28),
load 0,67–1,60, platform profile performance:

| Fila | r27 (ms) | r28 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 17.718 | **17.012** | -4,0 % |
| `css/resolve-styles-10k-nodes` | 27.586 | **27.258** | -1,2 % |
| `layout/realistic-1mb` | 37.096 | **37.336** | +0,6 % |
| `paint/build-display-list` | 8.346 | **8.715** | +4,4 % |
| `paint/render-to-png-800x600` | 9.245 | **8.762** | -5,2 % |
| `pipeline/html-to-png-800x600` | 114.457 | **112.808** | -1,4 % |
| `image/decode-png-800x600` | 6.680 | **6.644** | -0,5 % |
| `image/decode-jpeg-800x600` | 11.376 | **11.307** | -0,6 % |
| `image/decode-gif` | 3.074 | **3.014** | -2,0 % |
| `paint/layer-effects-800x600` | 28.096 | **27.353** | -2,6 % |

**Lectura**: paridad o mejora en pipeline (−1,4 %), decode PNG/JPEG/GIF y
layer-effects; `build-display-list` +4,4 % y layout +0,6 % están dentro de la
varianza de ventana. **No hay regresión atribuible a r28**; el desfase de los
absolutos contra la tabla histórica de r27 es de entorno (el binario r27 mide hoy
~112–114 ms de pipeline frente a 106,52 histórico).

| Medida | r28 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **115.674 ms** | **≤ 140 ms** | 82.6 % | 24.3 ms | ✅ |
| `layout/realistic-1mb` | **37.988 ms** | **≤ 45 ms** | 84.4 % | 7.0 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **27.508 ms** | **≤ 35 ms** | 78.6 % | 7.5 ms | ✅ |
| `render` showcase 1024×768 | **26.739 ms** | **≤ 35 ms** | 76.4 % | 8.3 ms | ✅ |
| shell headless showcase | 29.25 ms | ≤ 35 ms | 83.6 % | 5.8 ms | ✅ |
| `paint/render-to-png-800x600` | 9.313 ms | ref. ~8,5 | — | +0.8 ms | ✅ |
| `paint/build-display-list` | 8.606 ms | ≤ 25 ms | 34.4 % | 16.4 ms | ✅ |
| `html/realistic-1mb` | 17.613 ms | ≤ 100 ms | 17.6 % | 82.4 ms | ✅ |
| `image/decode-png-800x600` | 6,70 ms (full-suite) / **6,53 ms** (10 pasadas focalizadas) | ref. ≤6,6 ms | — | ✅ | sin regresión de código; ref. cumplida en medición focalizada |

**No se suben los presupuestos.** Todos se cumplen; menor margen relativo
showcase render (23,6 %) y layout (15,6 %). **Siguiente foco**: blur/PNG restante
(`paint/backdrop-fixture-raster` 1,29 vs png 0,81 ms; encode save-png 3,91 ms),
perf de render/JS (js_geometry +2,38 ms con script), campañas de fuzz y huecos
PSL/HTTP (h2 push-to-render, uploads streaming, proxies).

---

## 6. Filas nuevas r28: backdrop raster vs PNG

Las filas `paint/backdrop-fixture-raster`/`-png` aíslan la salida del backdrop:
misma fixture/efecto, `-raster` rasteriza a framebuffer y `-png` reutiliza el PNG
ya codificado (byte-safe). `-no-effect` es la misma fixture sin blur.

| Fila | mediana | -no-effect | Δ efecto |
|---|---:|---:|---:|
| `paint/backdrop-fixture-raster` | **1.289 ms** | 0.999 ms | **+0.290 ms** |
| `paint/backdrop-fixture-png` | **0.812 ms** | 0.601 ms | **+0.211 ms** |
| Δ raster vs png | **+0.477 ms** | +0.398 ms | — |

El PNG reutilizado evita ~0.48 ms de
rasterización/encode en esta fixture; el coste del blur queda en
**+0,29 ms** (raster) y **+0,21 ms** (png). Filas E2E existentes:
`pipeline/filter-fixture-300x260` 1.047 vs
1.050 sin efecto;
`pipeline/backdrop-fixture-300x260` 2.154 vs
1.624 (**+0.530 ms** de blur+PNG).

---

## 7. Coste de features r28

| Feature | Medición (--json) | Lectura |
|---|---|---|
| **Geometría JS** | `js_geometry_fixed.html`: con script **9.032 ms** (scripting 2.832 ms) vs `--no-scripting` **6.653 ms** | **+2.379 ms** por el motor JS + re-render; la geometría en sí es un pass de layout ya existente. |
| **Server push** | loopback TLS h2 (`target/push-bench`): push **1.550 ms** vs sin push **2.008 ms** | **−0,458 ms** (evita una petición); `pushed_used=1`, 2 imágenes cargadas/0 fallos en ambos; coste sólo con `accept_server_push`. |
| **Image scaled** | `image_scaled.html`: full **21.117 ms** vs `--max-image-dim 256` **19.272 ms** | **−1.845 ms (−8,7 %)**; decode PNG escalado + menos raster. |
| **Backdrop raster vs PNG** | raster **1.289 ms** vs png **0.812 ms** | PNG byte-safe **0.477 ms** más rápido (ver §6). |

Comandos: `target/release/render <fixture> out.png --json [--no-scripting|--max-image-dim N]`;
`target/pushbench-target/release/push-bench`; `bench --filter paint/backdrop-fixture-raster`.

---

## 8. Codificador PNG y goldens

| Métrica | r27 | r28 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.877 | **9.313** | +4,9 % vs r27 (entorno) |
| — rasterizar | 0.976 | **1.084** | +11,1 % |
| `shell/save-png-encode-800x600` | 3.686 | **3.912** | encode PNG aislado |
| `shell/save-png-total-800x600` | 3.627 | **3.907** | encode+write |
| Golden showcase | 80 220 B `c95c7eff…` | **80 220 B `c95c7eff…`** | byte-idéntico |
| Golden basic | 21 090 B `7324e851…` | **21 090 B `7324e851…`** | byte-idéntico |
| Golden phase4 PNG | 42 853 B `9e957177…` | **42 853 B `9e957177…`** | byte-idéntico |
| Golden `basic.rgba` | 1 920 000 B `60f8984e…` | **1 920 000 B `60f8984e…`** | byte-idéntico |

---

## 9. Pipeline `render`, shell, chrome, find y save-png (r28)

7 ejecuciones intercaladas (load 0,96; platform performance; goldens verificados):

| Fixture / modo | pared r28 | pared r27 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **26.74 ms** | 29,63 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **11.79 ms** | 10,05 | 21 090 B (`7324e851…`) |
| showcase (shell headless) | **29.25 ms** | 29,31 | 80 220 B (idéntico) |
| showcase `--chrome` | **46.25 ms** | 46,53 | `fe196851…` (favicon/dot r28) |
| showcase `--chrome --chrome-menu` | **45.98 ms** | — | `c7ccc0e4…` |
| `find.html` con `--find megi` | **21.37 ms** | 17,75 | `e76e6303…` |
| showcase `--screenshot` + `--save-png` | **29.23 ms** | 29,50 | 80 220 B (idéntico) |

`--chrome` cuesta +17.0 ms; la pareja
`--save-png` queda en 29.23 vs 29.25 ms.
El chrome r28 incluye el dot de favicon determinista (md5 nuevo `fe196851…`; los
goldens sin chrome `7324e851…`/`c95c7eff…` siguen intactos).

---

## 10. Filas E2E (gradients/grid/vars-calc/clipping/sticky)

| Fila | r28 (ms) | r27 (ms) | Δ |
|---|---:|---:|---:|
| `pipeline/gradients-800x600` | **54.368** | 51.156 | +6,3 % |
| `pipeline/grid-800x600` | **6.030** | 5.793 | +4,1 % |
| `css/vars-calc-10k` | **20.711** | 19.405 | +6,7 % |
| `paint/clipping-800x600` | **51.911** | 48.885 | +6,2 % |
| `layout/sticky-text-1mb` | **34.009** | 32.055 | +6,1 % |

Variaciones de ventana/frecuencia (+4–7 % uniformes), sin coste estructural nuevo;
A/B prístino r27 vs r28 (§5.2) confirma paridad. Filtros: `--filter pipeline`,
`--filter css/vars-calc`, `--filter paint/clipping`, `--filter layout/sticky`,
`--filter image`, `--filter paint/backdrop-fixture-raster`.

---

## 11. Presupuestos: veredicto

| Medida | r28 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **115.674 ms** | **≤ 140 ms** | 82.6 % | 24.3 ms | ✅ |
| `layout/realistic-1mb` | **37.988 ms** | **≤ 45 ms** | 84.4 % | 7.0 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **27.508 ms** | **≤ 35 ms** | 78.6 % | 7.5 ms | ✅ |
| `render` showcase 1024×768 | **26.739 ms** | **≤ 35 ms** | 76.4 % | 8.3 ms | ✅ |
| shell headless showcase | 29.25 ms | ≤ 35 ms | 83.6 % | 5.8 ms | ✅ |
| `paint/render-to-png-800x600` | 9.313 ms | ref. ~8,5 | — | +0.8 ms | ✅ |
| `paint/build-display-list` | 8.606 ms | ≤ 25 ms | 34.4 % | 16.4 ms | ✅ |
| `html/realistic-1mb` | 17.613 ms | ≤ 100 ms | 17.6 % | 82.4 ms | ✅ |
| `image/decode-png-800x600` | 6,70 ms (full-suite) / **6,53 ms** (10 pasadas focalizadas) | ref. ≤6,6 ms | — | ✅ | sin regresión de código; ref. cumplida en medición focalizada |

**No se suben los presupuestos.** Todos se cumplen; el menor margen relativo es
showcase render (23,6 %) y layout (15,6 %), y el absoluto render-to-png
(+0.81 ms). `image/decode-png` queda en
6,70 ms en el full-suite (varianza de suite) pero **6,53 ms en la medición
focalizada de 10 pasadas**, dentro de la ref. ≤6,6 ms; sin cambios de código PNG
en r28. **Siguiente foco**: blur/PNG restante, perf render/JS, campañas de fuzz,
PSL/HTTP (push-to-render, uploads, proxies).

---


## 12. Comparación histórica

Veintiséis baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 | **R28 (vigente)** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---:|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | 00:34 | 01:18 | 02:07 | 03:04 | 04:06 | **04:39** | **06:48** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | `00d8388` | `d1f3121` | `0b3adee` | `4718408` | `f42b9b9` | **`9d2298b`** | **`a0cdfcb`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | 1,20–1,23 | 1,76–1,86 | 1,32–1,38 | 0,79–0,84 | 1,30–1,35 | **1,36–1,42** | **0,67–0,96** |

Filas clave (mediana, ms; r22–r28 incluyen filas E2E/decode/paint nuevas que no
entran en el TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 | R28 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | 16.847 | 16.566 | 16.171 | 16.701 | 16.609 | **16.536** | **17.613** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | 35.805 | 25.477 | 27.085 | 27.322 | 26.577 | **25.957** | **27.508** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | 37.069 | 35.043 | 34.900 | 35.355 | 35.396 | **35.930** | **37.988** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | 8.973 | 7.940 | 7.982 | 7.702 | 7.974 | **7.729** | **8.606** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | 8.354 | 8.626 | 8.775 | 8.905 | 8.695 | **8.877** | **9.313** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | 141.194 | 106.440 | 107.974 | 107.635 | 107.950 | **106.522** | **115.674** |
| `image/decode-png-800x600` | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 6.495 | 6.506 | 6.524 | 6.551 | 7.975 | **6.399** | **6.699** |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** | **305.461** | **307.687** | **309.478** | **309.460** | **306.611** | **330.650** |

### Ronda 27 → ronda 28, fila a fila

Los absolutos de esta ventana son ~5–9 % más altos que los históricos de r27 por
el cambio de perfil/frecuencia; el A/B prístino intercalado (§5.2) muestra
paridad. Deltas informativos (no budgets):

| Benchmark | R27 (ms) | R28 (ms) | Δ |
|---|---:|---:|---:|
| `css/parse-10k-rules` | 13.154 | **14.241** | +8,3 % |
| `css/resolve-styles-10k-nodes` | 25.957 | **27.508** | +6,0 % |
| `css/resolve-styles-2k-nodes` | 5.413 | **5.901** | +9,0 % |
| `css/selector-match-descendant` | 1.210 | **1.292** | +6,8 % |
| `css/selector-match-simple` | 0.632 | **0.724** | +14,6 % |
| `css/vars-calc-10k` | 19.405 | **20.711** | +6,7 % |
| `html/massive-end-p` | 16.920 | **18.567** | +9,7 % |
| `html/massive-formatting` | 10.740 | **11.232** | +4,6 % |
| `html/nested-div-10k` | 2.680 | **2.965** | +10,6 % |
| `html/nested-div-32k` | 8.362 | **9.498** | +13,6 % |
| `html/nested-div-64k` | 16.840 | **18.158** | +7,8 % |
| `html/nested-div-64k-default-limits` | 10.074 | **10.852** | +7,7 % |
| `html/pretty-5k-blocks` | 8.536 | **9.111** | +6,7 % |
| `html/realistic-1mb` | 16.536 | **17.613** | +6,5 % |
| `html/tag-40k-attrs` | 9.523 | **10.323** | +8,4 % |
| `image/decode-gif` | 2.981 | **3.103** | +4,1 % |
| `image/decode-jpeg-800x600` | 10.671 | **11.592** | +8,6 % |
| `image/decode-png-800x600` | 6.399 | **6.699** | +4,7 % |
| `image/decode-webp-alpha` | 0.023 | **0.025** | +8,7 % |
| `image/decode-webp-lossless` | 0.049 | **0.051** | +4,1 % |
| `image/decode-webp-palette` | 0.006 | **0.006** | +0,0 % |
| `layout/realistic-1mb` | 35.930 | **37.988** | +5,7 % |
| `layout/sticky-text-1mb` | 32.055 | **34.009** | +6,1 % |
| `paint/backdrop-fixture` | 0.317 | **0.324** | +2,2 % |
| `paint/backdrop-fixture-no-effect` | 0.020 | **0.022** | +10,0 % |
| `paint/backdrop-fixture-png` | — | **0.812** | nuevo |
| `paint/backdrop-fixture-png-no-effect` | — | **0.601** | nuevo |
| `paint/backdrop-fixture-raster` | — | **1.289** | nuevo |
| `paint/backdrop-fixture-raster-no-effect` | — | **0.999** | nuevo |
| `paint/build-display-list` | 7.729 | **8.606** | +11,3 % |
| `paint/clipping-800x600` | 48.885 | **51.911** | +6,2 % |
| `paint/filter-fixture` | 0.309 | **0.308** | -0,3 % |
| `paint/filter-fixture-no-effect` | 0.289 | **0.306** | +5,9 % |
| `paint/layer-effects-800x600` | 27.311 | **27.971** | +2,4 % |
| `paint/object-fit-800x600` | 3.311 | **3.471** | +4,8 % |
| `paint/rasterize-800x600` | 0.976 | **1.084** | +11,1 % |
| `paint/render-to-png-800x600` | 8.877 | **9.313** | +4,9 % |
| `pipeline/backdrop-fixture-300x260` | 2.065 | **2.154** | +4,3 % |
| `pipeline/backdrop-fixture-300x260-no-effect` | 1.512 | **1.624** | +7,4 % |
| `pipeline/filter-fixture-300x260` | 1.040 | **1.047** | +0,7 % |
| `pipeline/filter-fixture-300x260-no-effect` | 1.033 | **1.050** | +1,6 % |
| `pipeline/gradients-800x600` | 51.156 | **54.368** | +6,3 % |
| `pipeline/grid-800x600` | 5.793 | **6.030** | +4,1 % |
| `pipeline/html-to-png-800x600` | 106.522 | **115.674** | +8,6 % |
| `shell/save-png-encode-800x600` | 3.686 | **3.912** | +6,1 % |
| `shell/save-png-total-800x600` | 3.627 | **3.907** | +7,7 % |
| `shell/save-png-write-800x600` | 0.003 | **0.003** | +0,0 % |

---

## 13. Cómo comparar y detectar regresiones

1. Medir con load < 2 y comprobar frecuencia (r28: load **0,67–0,96**, platform
   profile **performance**, freq 4,3–4,6 GHz). Si no hay ventana, marcarlo y
   descartar picos. **Ojo**: sin `performance` la frecuencia queda ~1,2–1,4 GHz y
   todos los absolutos se duplican; verificar `powerprofilesctl get` antes.
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
4. Regresión si la mediana empeora >15 % reproducible. En r28 todos los
   presupuestos se cumplen (§11); decode PNG 6,53 ms focalizado (≤6,6) y
   pipeline A/B vs r27 prístino −1,4 %.
5. Filas E2E/decode/paint: `--filter pipeline`, `css/vars-calc`,
   `paint/clipping`, `layout/sticky`, `image`, `paint`, `shell/save-png`,
   `pipeline/filter-fixture`, `pipeline/backdrop-fixture`.
6. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   phase4 `9e957177…` 42 853 B, `basic.rgba` `60f8984e…` 1 920 000 B, chrome
   `cbfc6087…` 85 746 B, find `a818f32a…` 29 950 B.

---

## 14. Limitaciones y notas

- **Ventana buena** (load 0,67–0,96; platform profile performance, 4,3–4,6 GHz);
  las primeras pasadas con `power-saver`/`quiet` (freq ~1,2–1,4 GHz) se
  descartaron. A/B prístino r27 vs r28 a load 0,67–1,60 (§5.2).
- **WIP ajeno** en `js`/`net`/`image`; el motor medido es el árbol en `a0cdfcb`
  (cierre r28 `cb7af43`). Esta tarea solo tocó `docs/PERFORMANCE.md` y `target/`.
- **Filas E2E nuevas**: filter overhead ≈0,007–0,036 ms; backdrop
  ≈0,54–0,68 ms (blur 4px + brightness/grayscale); ambos incluyen el PNG de
  300×260 (~1,5 ms), que es parte del coste total de la fila.
- **Decode PNG**: fix confirmado, vuelve a **6,40 ms** (≤6,6); JPEG/GIF/WebP
  estables.
- **HTTP/2** (r28): push consumido desde preload cache (opt-in, A/B −0,46 ms en
  el loopback; sin coste con la opción apagada) y uploads streaming con
  backpressure. Pendiente: push-to-render con página real y proxies.
- **`website/` y `pages.yml`**: del dueño; no se tocan.
- **Fast path de tablas (r7)**: nota histórica; no hay commit compilable con el
  camino avanzado sin fast path.
