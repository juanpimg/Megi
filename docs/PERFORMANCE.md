# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**.

- **Fecha**: 2026-11-11, 10:05–10:08 (Europe/Madrid). Ronda 30: **VP8 lossy
  completo (píxeles)**, **container queries**, **Web Workers cooperativos**,
  **PSL/trailers**, **historial de navegación + chrome fast path** y filas
  `shell/chrome-png-*`.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `f7dad8d`** (cierre r30
  `67365af`; WIP ajeno en docs/tests, sin tocar el bench).
- **Ventana**: **load 1,27–1,35** (5 pasadas + canónica); **platform profile
  `performance`** (`powerprofilesctl`; la máquina estaba en `quiet` y capaba
  1,3–2,2 GHz; calibraciones descartadas); freq max muestreada **4,1–4,6 GHz**.
- **Binarios**: bench `a0f6b7a7…` (5 322 592 B), render `d75e0a16…`, shell
  `9406cfb6…`.
- **Artefactos**: `target/bench-r30-run1..5.json`,
  `target/bench-aggregate-r30.json` (`sha256 ead9bd70…`),
  `target/bench-r30-feature-costs.json`, `target/bench-r30/measure/` (VP8).
  Canónica: **TOTAL 563.421**.
- **Caveat**: r29/r30 comparten `performance`, pero r30 se midió con load algo
  mayor (1,27 vs 1,04); comparaciones informativas. La fila de bench
  `image/decode-webp-lossy` no existe y el scope prohíbe tocar `crates/bench`:
  se mide con arnés ad-hoc en `target/bench-r30/measure` (§5.3).

---

## 1. Metodología

- Cargas generadas en memoria (`crates/bench/src/generators.rs`), E2E
  gradients/grid/vars-calc/clipping/sticky, efectos de capa, save-png aislado,
  decode PNG/JPEG/GIF/WebP y nuevas filas chrome-png.
- Reloj `Instant`, `--warmup 1`, `--iterations 5` (heavy 3); mediana de 5 pasadas
  válidas; canónica `./tools/bench.sh`.
- Render/shell: 7 ejecuciones intercaladas con `--json`/md5; fixtures propias en
  `target/bench-r30/feature/`.
- Máquina: i7-12700H, 15 GiB, CachyOS 7.3.1, rustc 1.99.1 release; ACPI
  `performance`; load 1,0–1,35; render/shell load ~0,7–1,0.

## 2. Catálogo

| Grupo | Benchmarks |
|---|---|
| html/css/layout | parseo, cascada, selector matching, layout block/sticky |
| paint | display-list, raster, render-to-png, clipping, layer-effects, object-fit, filter/backdrop raster+png |
| pipeline | html-to-png, gradients, grid, filter/backdrop 300×260 |
| shell | save-png encode/write/total, **chrome-png 640×400 y 1024×768** |
| image | decode PNG/JPEG/GIF, WebP VP8L lossless/alpha/palette; **VP8 lossy medido ad-hoc (§5.3)** |

## 3. Baseline vigente (ronda 30, HEAD `f7dad8d`)

5 pasadas (load 1,27–1,35). **Filas legacy (19)**: `TOTAL 322,706 ms`; **todas
(49)**: `TOTAL 562,146 ms`; canónica: `TOTAL 563.421 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `css/parse-10k-rules` | **13.479** | 13.297–13.956 |
| `css/resolve-styles-10k-nodes` | **27.855** | 27.480–28.278 |
| `css/resolve-styles-2k-nodes` | **5.950** | 5.895–6.061 |
| `css/selector-match-descendant` | **1.311** | 1.260–1.326 |
| `css/selector-match-simple` | **0.654** | 0.645–0.730 |
| `css/vars-calc-10k` | **20.202** | 19.845–20.310 |
| `html/massive-end-p` | **18.240** | 17.923–18.628 |
| `html/massive-formatting` | **10.735** | 9.981–10.962 |
| `html/nested-div-10k` | **2.804** | 2.757–2.844 |
| `html/nested-div-32k` | **8.748** | 7.939–8.903 |
| `html/nested-div-64k` | **17.677** | 17.495–18.292 |
| `html/nested-div-64k-default-limits` | **10.407** | 10.147–10.494 |
| `html/pretty-5k-blocks` | **8.993** | 8.864–9.321 |
| `html/realistic-1mb` | **17.508** | 17.135–17.809 |
| `html/tag-40k-attrs` | **9.513** | 8.893–9.934 |
| `image/decode-gif` | **3.013** | 2.806–3.059 |
| `image/decode-jpeg-800x600` | **11.231** | 10.762–11.349 |
| `image/decode-png-800x600` | **6.634** | 6.553–6.822 |
| `image/decode-webp-alpha` | **0.025** | 0.023–0.025 |
| `image/decode-webp-lossless` | **0.049** | 0.048–0.051 |
| `image/decode-webp-palette` | **0.006** | 0.006–0.007 |
| `layout/realistic-1mb` | **37.283** | 37.036–38.460 |
| `layout/sticky-text-1mb` | **33.038** | 32.615–34.004 |
| `paint/backdrop-fixture` | **0.314** | 0.310–0.419 |
| `paint/backdrop-fixture-no-effect` | **0.024** | 0.024–0.024 |
| `paint/backdrop-fixture-png` | **0.792** | 0.773–0.810 |
| `paint/backdrop-fixture-png-no-effect` | **0.567** | 0.552–0.584 |
| `paint/backdrop-fixture-raster` | **1.255** | 1.178–1.299 |
| `paint/backdrop-fixture-raster-no-effect` | **0.990** | 0.966–0.995 |
| `paint/build-display-list` | **7.825** | 7.409–8.606 |
| `paint/clipping-800x600` | **51.989** | 51.670–52.368 |
| `paint/filter-fixture` | **0.297** | 0.273–0.302 |
| `paint/filter-fixture-no-effect` | **0.292** | 0.287–0.331 |
| `paint/layer-effects-800x600` | **27.393** | 26.808–27.670 |
| `paint/object-fit-800x600` | **3.403** | 3.341–3.446 |
| `paint/rasterize-800x600` | **0.971** | 0.876–1.093 |
| `paint/render-to-png-800x600` | **8.863** | 8.506–9.145 |
| `pipeline/backdrop-fixture-300x260` | **2.066** | 2.002–2.106 |
| `pipeline/backdrop-fixture-300x260-no-effect` | **1.601** | 1.589–1.613 |
| `pipeline/filter-fixture-300x260` | **1.013** | 0.999–1.048 |
| `pipeline/filter-fixture-300x260-no-effect` | **1.008** | 0.927–1.031 |
| `pipeline/gradients-800x600` | **52.218** | 51.466–53.043 |
| `pipeline/grid-800x600` | **5.988** | 5.807–6.247 |
| `pipeline/html-to-png-800x600` | **113.890** | 112.205–115.367 |
| `shell/chrome-png-1024x768` | **4.876** | 4.536–5.098 |
| `shell/chrome-png-640x400` | **1.546** | 1.447–1.597 |
| `shell/save-png-encode-800x600` | **3.807** | 3.622–3.885 |
| `shell/save-png-total-800x600` | **3.800** | 3.609–4.035 |
| `shell/save-png-write-800x600` | **0.003** | 0.003–0.004 |

> El TOTAL legacy suma etapas solapadas; no es presupuesto. Todos los
> presupuestos cumplen con margen salvo **decode-png**, en el foco (§11).

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k 2,804 · 32k 8,748 · 64k 17,4 ms | lineal |
| `css/parse-10k-rules` | 5k 6,9 · 10k 13,479 · 20k 30,4 ms | lineal |
| `css/resolve-styles-10k-nodes` | **27,855 ms** | +2,3 % vs r29 (ventana); container queries solo con `@container` |
| `layout/realistic-1mb` | **37,283 ms** | −1,3 % vs r29 |
| `paint/build-display-list` | **7,825 ms** | −6,9 % vs r29 |
| `paint/render-to-png-800x600` | **8,863 ms** | −0,5 % |
| `paint/backdrop-fixture-raster` | 1,29 vs 0,99 sin efecto | +0,30 ms blur |
| `paint/backdrop-fixture-png` | 0,83 vs 0,60 sin efecto | +0,22 ms blur+PNG |
| `pipeline` | **113,890 ms** (112,2–115,4) | +0,6 % vs r29; margen 26,1 ms |
| `image/decode-png-800x600` | **6,634 ms** (6,55–6,82) | −0,4 % vs r29; foco |
| `image/decode-webp-lossy` (arnés) | **13,56 ms** 1024×768 | +1,9 % vs PNG 1024×768 (13,32) |

## 5. Ronda 30: features y verificación

### 5.1 Qué entró

- `7ed6be3` **VP8 keyframe completo**: bool decoder, segmentación, loop filters
  normal/simple, particiones 1/2/4/8, intra DC/V/H/TM y B_PRED, dequant/IDCT,
  YUV420 fancy upsampling — WebP lossy con píxeles reales.
- `a2ee4a0` **Container Queries**: `container-type`, `container-name`,
  `@container` con `and`, ancestro más cercano; predicción de tamaño pre-layout.
- `89c56ba` **Web Workers cooperativos deterministas**.
- `87b493d` **PSL vendorizada y request trailers**.
- `67365af` **historial de navegación** (about:history, menú Ctrl+H,
  back/forward) y **chrome fast path** (banda directa al Pixmap, alpha=255) +
  filas `shell/chrome-png-*`.

### 5.2 Presupuestos (sin subir)

| Medida | r30 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png` | **113,89 ms** | ≤140 | 81,4 % | 26,11 ms | ✅ |
| `layout/realistic-1mb` | **37,28 ms** | ≤45 | 82,9 % | 7,72 ms | ✅ |
| `css/resolve-styles-10k` | **27,86 ms** | ≤35 | 79,6 % | 7,14 ms | ✅ |
| showcase render | **28,95 ms** | ≤35 | 82,7 % | 6,05 ms | ✅ |
| shell headless showcase | 28,89 ms | ≤35 | 82,5 % | 6,11 ms | ✅ |
| `paint/render-to-png` | **8,86 ms** | ref ~8,5 | — | +0,36 ms | ✅ foco |
| `paint/build-display-list` | **7,83 ms** | ≤25 | 31,3 % | 17,18 ms | ✅ |
| `html/realistic-1mb` | 17,51 ms | ≤100 | 17,5 % | 82,5 ms | ✅ |
| `image/decode-png` | **6,63 ms** | foco ≤6,6 | 100,5 % | −0,034 ms | ⚠️ foco |

Todo cumple salvo decode-png en el foco (mediana +0,5 %; canónica 6,541 dentro).
Sin regresión estructural: pipeline +0,6 %, layout −1,3 %, display −6,9 %,
render-to-png −0,5 %, decode-png −0,4 % (ventanas distintas, sin A/B prístino).

### 5.3 A/B VP8 lossy 1024×768

Arnés ad-hoc `target/bench-r30/measure` (release, 20 iter, warm-up):

| Decode | Tiempo | Notas |
|---|---:|---|
| WebP **lossy** VP8 1024×768 q75 | **13,56 ms** | 13,53–13,84; 40 552 B |
| PNG 1024×768 (misma imagen) | **13,32 ms** | full decode RGBA |
| WebP lossy 64×48 | 0,115 ms | vs VP8L 64×48 0,048 ms (~2,4×) |

**La referencia 53,6 ms no se reproduce** en esta ventana (4,1–4,6 GHz): el mismo
decode da ~13,6 ms, probablemente la referencia se tomó con `quiet`/1,3–2,2 GHz o
con otra fixture. Se anota para r31; el lossy queda **+1,9 % vs PNG full decode**
y no amenaza el presupuesto de pipeline. No se añade fila de bench por scope.

## 6. Filas chrome-png y chrome fast path (r30)

| Fila / medida | r30 | r29 | Lectura |
|---|---:|---:|---|
| `shell/chrome-png-640x400` | **1,546 ms** | — | nuevo componente (encode banda) |
| `shell/chrome-png-1024x768` | **4,876 ms** | — | nuevo componente |
| shell `--chrome` wall | **42,85 ms** | 46,20 | **−3,35 ms (−7,3 %)** |
| shell plain wall | **28,89 ms** | 29,29 | base |
| `--chrome` − plain | **+13,96 ms** | +16,91 | fast path recorta ~2,95 ms |
| `--chrome-menu` wall | **45,82 ms** | 46,23 | +2,97 ms vs chrome; PNG 88 003 B |

## 7. Coste de features r30

7 ejecuciones intercaladas, load ~1,0, performance:

| Feature | Medición | Lectura |
|---|---|---|
| Container queries | **4,83 ms** vs sin reglas **4,36 ms** | **+0,47 ms (+10,8 %)** en 400×300; fast path vacío sin `@container` |
| WebP lossy img | **3,54 ms** vs PNG **6,04 ms** | **−2,50 ms (−41,4 %)** con píxeles reales (1 460 B vs 26 427 B); foto 64×48 |
| Worker sum | **7,99 ms** vs síncrono **6,05 ms** | **+1,94 ms (+32,1 %)**; PNG md5 idéntico |
| about:history | **10,07 ms** vs página simple **8,67 ms** | **+1,40 ms (+16,1 %)**; 18 204 vs 13 733 B |
| Chrome fast path componente | 640 **1,546** / 1024 **4,876 ms** | ver §6 |
| Chrome fast path wall | **42,85 ms** vs plain **28,89** | **+13,96 ms**; r29 +16,91 ⇒ −2,95 ms |
| `--find` | **18,22 ms** | overlay |
| `--save-png` pareja | **30,85 ms** | reutiliza PNG |

## 8. Codificador PNG y goldens

| Métrica | r29 | r30 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8,906 | **8,863** | −0,5 % |
| `shell/chrome-png-640x400` | — | **1,546** | fila nueva |
| `shell/chrome-png-1024x768` | — | **4,876** | fila nueva |
| Golden showcase | 80 220 B `c95c7eff…` | **80 220 B `c95c7eff…`** | intacto |
| Golden basic | 21 090 B `e943565d…` | **21 090 B `e943565d…`** | intacto |
| Golden phase4 | 42 853 B `9e957177…` | **42 853 B `9e957177…`** | intacto |
| Golden basic.rgba | 1 920 000 B `60f8984e…` | **1 920 000 B `60f8984e…`** | intacto |
| Chrome (no golden) | `fe196851…` 85 753 B | **`931e45ce…` 86 629 B** | nueva UI |
| Chrome menu | `c7ccc0e4…` 87 156 B | **`c06b0268…` 88 003 B** | menú |

## 9. Pipeline render, shell y páginas (r30)

| Fixture / modo | pared r30 | pared r29 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 | **28,95 ms** | 29,06 | 80 220 B (`c95c7eff…`) |
| basic 800×600 | **9,31 ms** | 10,17 | 21 090 B (`e943565d…`) |
| showcase shell headless | **28,89 ms** | 29,29 | 80 220 B |
| showcase `--chrome` | **42,85 ms** | 46,20 | 86 629 B (`931e45ce…`) |
| `--chrome --chrome-menu` | **45,82 ms** | 46,23 | 88 003 B (`c06b0268…`) |
| about:history | **10,07 ms** | — | 18 204 B (`209d2950…`) |
| página simple | **8,67 ms** | — | 13 733 B (`84193939…`) |
| `find.html --find megi` | **18,22 ms** | 16,48 | 29 950 B (`a818f32a…`) |
| showcase --save-png | **30,85 ms** | 29,25 | 80 220 B |

## 10. Filas E2E

| Fila | r30 (ms) | r29 (ms) | Δ |
|---|---:|---:|---:|
| `pipeline/gradients-800x600` | **51,9** | 52,812 | −1,7 % |
| `pipeline/grid-800x600` | **5,9** | 6,154 | −4,1 % |
| `css/vars-calc-10k` | **20,202** | 20,514 | −1,5 % |
| `paint/clipping-800x600` | **49,8** | 50,421 | −1,2 % |
| `layout/sticky-text-1mb` | **33,2** | 33,258 | −0,2 % |

## 11. Presupuestos: veredicto

| Medida | r30 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png` | **113,89 ms** | **≤140** | 81,4 % | 26,11 ms | ✅ |
| `layout/realistic-1mb` | **37,28 ms** | **≤45** | 82,9 % | 7,72 ms | ✅ |
| `css/resolve-styles-10k` | **27,86 ms** | **≤35** | 79,6 % | 7,14 ms | ✅ |
| showcase render | **28,95 ms** | **≤35** | 82,7 % | 6,05 ms | ✅ |
| shell headless showcase | 28,89 ms | ≤35 | 82,5 % | 6,11 ms | ✅ |
| `paint/render-to-png` | **8,86 ms** | ref ~8,5 | — | +0,36 ms | ✅ foco |
| `paint/build-display-list` | **7,83 ms** | **≤25** | 31,3 % | 17,18 ms | ✅ |
| `html/realistic-1mb` | 17,51 ms | ≤100 | 17,5 % | 82,5 ms | ✅ |
| `image/decode-png` | **6,63 ms** | foco ≤6,6 | 100,5 % | −0,034 ms | ⚠️ foco |

**No se suben los presupuestos.** Todos con margen; decode-png sigue en foco
(mediana 6,634; canónica 6,541 dentro). Siguiente foco: **render PNG skip en
`--chrome`/`--find`**, **backdrop/PNG regions**, **perf render/JS**, **fuzz largo**.

---

## 12. Comparación histórica

Veintiocho baselines en la misma máquina, mismo arnés (la ronda 14 no publicó baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 | R28 | R29 | **R30 (vigente)** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | 00:34 | 01:18 | 02:07 | 03:04 | 04:06 | 04:39 | 06:48 | 07:25 | **10:05** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | `00d8388` | `d1f3121` | `0b3adee` | `4718408` | `f42b9b9` | `9d2298b` | `a0cdfcb` | `8f9cfa9` | **`f7dad8d`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | 1,20–1,23 | 1,76–1,86 | 1,32–1,38 | 0,79–0,84 | 1,30–1,35 | 1,36–1,42 | 0,67–0,96 | 1,04–1,05 | **1,27–1,35** |

Filas clave (mediana, ms; r22–r30 incluyen filas E2E/decode/paint que no entran en TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 | R28 | R29 | **R30** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | 16.847 | 16.566 | 16.171 | 16.701 | 16.609 | **16.536** | **17.613** | 17.175 | 17.508 |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | 35.805 | 25.477 | 27.085 | 27.322 | 26.577 | **25.957** | **27.508** | 27.216 | 27.855 |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | 37.069 | 35.043 | 34.900 | 35.355 | 35.396 | **35.930** | **37.988** | 37.786 | 37.283 |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | 8.973 | 7.940 | 7.982 | 7.702 | 7.974 | **7.729** | **8.606** | 8.409 | 7.825 |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | 8.354 | 8.626 | 8.775 | 8.905 | 8.695 | **8.877** | **9.313** | 8.906 | 8.863 |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | 141.194 | 106.440 | 107.974 | 107.635 | 107.950 | **106.522** | **115.674** | 113.238 | 113.890 |
| `image/decode-png-800x600` | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 6.495 | 6.506 | 6.524 | 6.551 | 7.975 | **6.399** | **6.699** | 6.661 | 6.634 |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** | **305.461** | **307.687** | **309.478** | **309.460** | **306.611** | **330.650** | 323.023 | **322.706** |


### Ronda 29 → ronda 30, fila a fila

Mismo perfil `performance` (4,1–4,7 GHz) con load distinto (r29 1,04; r30
1,27–1,35). Deltas informativos:

| Benchmark | R29 (ms) | R30 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 17,175 | **17,508** | +1,9 % |
| `css/resolve-styles-10k-nodes` | 27,216 | **27,855** | +2,3 % |
| `layout/realistic-1mb` | 37,786 | **37,283** | −1,3 % |
| `paint/build-display-list` | 8,409 | **7,825** | −6,9 % |
| `paint/render-to-png-800x600` | 8,906 | **8,863** | −0,5 % |
| `pipeline/html-to-png-800x600` | 113,238 | **113,890** | +0,6 % |
| `image/decode-png-800x600` | 6,661 | **6,634** | −0,4 % |
| `shell/chrome-png-640x400` | — | **1,546** | nuevo |
| `shell/chrome-png-1024x768` | — | **4,876** | nuevo |
| **TOTAL legacy** | **323.023** | **322.706** | **−0,1 %** |



## 13. Cómo comparar y detectar regresiones

1. Medir con load <2 y **perfil `performance`**; sin él la máquina capa a
   1,3–2,2 GHz y todo se infla 2–3×. Documentar load/freq.
2. Atribuir deltas con A/B de árboles prístinos intercalados; los absolutos
   cambian ±5–9 % entre ventanas por energía/load.
3. Guardar JSON y comparar medianas; regresión si empeora >15 % reproducible.
4. Filas: `--filter image`, `--filter paint`, `--filter shell/chrome-png`,
   `--filter pipeline`, `--filter css/vars-calc`, `--filter layout/sticky`.
5. Goldens: showcase `c95c7eff…` 80 220 B; basic `e943565d…` 21 090 B; phase4
   `9e957177…` 42 853 B; `basic.rgba` `60f8984e…`; chrome `931e45ce…` 86 629 B;
   chrome-menu `c06b0268…` 88 003 B.

## 14. Limitaciones

- **VP8 lossy**: decoder completo verificado por `image`; fila de bench ausente
  (scope docs/target) y referencia 53,6 ms no reproducida (13,6 ms a 4,1–4,6 GHz);
  A/B vs PNG 1024×768 +1,9 %; re-medir en r31.
- **Container queries**: predicción pre-layout por cadena de bloques, aprox.
  flex/grid/intrínseco; `size` solo con altura definida; sin style queries/`cqi`.
- **Workers**: cooperativos/deterministas (no hilos); coste mínimo +1,94 ms con
  carga trivial.
- **about:history**: historial de sesión sin reloj de pared; medido con página
  generada.
- **Chrome fast path**: chrome md5 no son goldens de página (incluyen
  back/forward/favicon); wall −3,35 ms.
- **Perfil**: `performance` vía `powerprofilesctl` (reversible); r29/r30 mismo
  perfil, loads distintos.
- **website/pages.yml**: del dueño, no se tocan.
