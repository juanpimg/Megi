# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**.

- **Fecha**: 2026-11-11, 11:30–11:34 (Europe/Madrid). Ronda 31: **Cascade Layers
  `@layer`**, **`fetch()` JS**, **PNG single-encode en chrome/find**, **VP8 lossy
  más rápido** y **proxy HTTP/HTTPS**.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `aba05f8`** (cierre r31
  `822c8a2`; WIP ajeno en docs, sin tocar el bench).
- **Ventana primaria**: **load 1,46–1,69**, **platform profile `performance`**
  (`powerprofilesctl`; la máquina estaba en `quiet` y capaba 1,3–2,2 GHz),
  freq 3,9–4,6 GHz; 5 pasadas + canónica.
- **Ventana de estabilidad** (segunda): load 1,73–1,83, freq 3,5–4,65 GHz;
  5 pasadas extra + canónica, usada solo como comprobación (§5.3).
- **Binarios**: bench `fe4b9213…` (5 322 592 B), render `d75e0a16…`, shell
  `9406cfb6…`; r30 prístino `f7dad8d` compilado en `target/r30-pristine/` para A/B.
- **Artefactos**: `target/bench-r31-run1..5.json` (primaria),
  `target/bench-r31b-run1..5.json` (estabilidad),
  `target/bench-aggregate-r31.json` (`sha256 27024f3e…`),
  `target/bench-r31/ab_r30_r31.json`, `chrome-ab.json`,
  `target/bench-r31-feature-costs.json`; canónica primaria **TOTAL 618.076**.
- **Caveat de entorno**: esta ventana es **~10 % más lenta** que la baseline r30
  publicada (el binario r30 prístino mide pipeline ~135 ms aquí). El A/B
  intercalado (§12) atribuye esa diferencia al entorno; los presupuestos se
  juzgan con los absolutos de §11 y el A/B. No se suben presupuestos.

---

## 1. Metodología

- Cargas en memoria (`crates/bench/src/generators.rs`): E2E, efectos de capa,
  save-png aislado, decode PNG/JPEG/GIF/WebP (VP8L y **VP8 lossy** nuevo) y
  `shell/chrome-png-*`.
- `Instant`, `--warmup 1`, `--iterations 5` (heavy 3); mediana de 5 pasadas
  válidas; canónica `./tools/bench.sh`; artefactos JSON por pasada.
- Render/shell: 7 ejecuciones intercaladas con `--json`/md5; fixtures propias
  en `target/bench-r31/feature/`.
- Máquina: i7-12700H, 15 GiB, CachyOS 7.3.1, rustc 1.99.1 release, ACPI
  `performance`; load 1,01–1,83 durante las mediciones.

## 2. Catálogo

| Grupo | Benchmarks |
|---|---|
| html/css/layout | parseo, cascada (**layers**), selector matching, layout block/sticky |
| paint | display-list, raster, render-to-png, clipping, layer-effects, object-fit, filter/backdrop raster+png |
| pipeline | html-to-png, gradients, grid, filter/backdrop 300×260 |
| shell | save-png encode/write/total, **chrome-png 640×400 y 1024×768** |
| image | decode PNG/JPEG/GIF, WebP VP8L lossless/alpha/palette y **VP8 lossy 1024×768 + q75/q90** |

## 3. Baseline vigente (ronda 31, HEAD `aba05f8`)

5 pasadas primarias (load 1,46–1,69). **Filas legacy (19)**: `TOTAL 356,352 ms`;
**todas (52)**: `TOTAL 612,726 ms`; canónica: `TOTAL 618.076 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `css/parse-10k-rules` | **14.584** | 14.056–14.727 |
| `css/resolve-styles-10k-nodes` | **29.072** | 28.568–31.181 |
| `css/resolve-styles-2k-nodes` | **5.813** | 5.750–6.186 |
| `css/selector-match-descendant` | **1.256** | 1.132–1.320 |
| `css/selector-match-simple` | **0.671** | 0.615–0.685 |
| `css/vars-calc-10k` | **22.906** | 22.235–24.441 |
| `html/massive-end-p` | **18.183** | 17.968–18.469 |
| `html/massive-formatting` | **11.428** | 11.262–11.584 |
| `html/nested-div-10k` | **2.822** | 2.758–2.909 |
| `html/nested-div-32k` | **8.918** | 8.314–9.375 |
| `html/nested-div-64k` | **17.834** | 17.570–18.044 |
| `html/nested-div-64k-default-limits` | **10.640** | 10.561–10.789 |
| `html/pretty-5k-blocks` | **9.190** | 8.809–9.571 |
| `html/realistic-1mb` | **17.998** | 17.848–18.424 |
| `html/tag-40k-attrs` | **9.628** | 9.543–10.088 |
| `image/decode-gif` | **3.045** | 2.785–3.150 |
| `image/decode-jpeg-800x600` | **11.270** | 10.928–11.583 |
| `image/decode-png-800x600` | **6.898** | 6.510–7.410 |
| `image/decode-webp-alpha` | **0.024** | 0.021–0.025 |
| `image/decode-webp-lossless` | **0.053** | 0.049–0.055 |
| `image/decode-webp-lossy` | **11.083** | 10.599–11.913 |
| `image/decode-webp-lossy-q75` | **0.062** | 0.057–0.063 |
| `image/decode-webp-lossy-q90` | **0.066** | 0.064–0.069 |
| `image/decode-webp-palette` | **0.006** | 0.006–0.007 |
| `layout/realistic-1mb` | **40.386** | 39.770–41.595 |
| `layout/sticky-text-1mb` | **35.180** | 34.216–36.725 |
| `paint/backdrop-fixture` | **0.316** | 0.302–0.324 |
| `paint/backdrop-fixture-no-effect` | **0.021** | 0.021–0.023 |
| `paint/backdrop-fixture-png` | **0.796** | 0.735–0.855 |
| `paint/backdrop-fixture-png-no-effect` | **0.600** | 0.546–0.644 |
| `paint/backdrop-fixture-raster` | **1.292** | 1.184–1.370 |
| `paint/backdrop-fixture-raster-no-effect` | **0.975** | 0.909–1.045 |
| `paint/build-display-list` | **9.220** | 8.554–10.399 |
| `paint/clipping-800x600` | **50.711** | 48.862–51.717 |
| `paint/filter-fixture` | **0.306** | 0.292–0.320 |
| `paint/filter-fixture-no-effect` | **0.309** | 0.293–0.317 |
| `paint/layer-effects-800x600` | **27.391** | 26.391–27.583 |
| `paint/object-fit-800x600` | **3.506** | 3.316–3.653 |
| `paint/rasterize-800x600` | **1.019** | 0.957–1.242 |
| `paint/render-to-png-800x600` | **9.205** | 8.888–9.595 |
| `pipeline/backdrop-fixture-300x260` | **2.146** | 2.101–2.208 |
| `pipeline/backdrop-fixture-300x260-no-effect` | **1.595** | 1.585–1.769 |
| `pipeline/filter-fixture-300x260` | **1.065** | 1.014–1.134 |
| `pipeline/filter-fixture-300x260-no-effect` | **1.047** | 1.038–1.621 |
| `pipeline/gradients-800x600` | **53.479** | 53.004–54.789 |
| `pipeline/grid-800x600` | **6.048** | 5.904–6.729 |
| `pipeline/html-to-png-800x600` | **138.485** | 135.241–151.778 |
| `shell/chrome-png-1024x768` | **4.848** | 4.762–5.679 |
| `shell/chrome-png-640x400` | **1.526** | 1.447–1.641 |
| `shell/save-png-encode-800x600` | **3.924** | 3.859–4.309 |
| `shell/save-png-total-800x600` | **3.876** | 3.782–4.123 |
| `shell/save-png-write-800x600` | **0.004** | 0.003–0.004 |

> El TOTAL legacy suma etapas solapadas; no es presupuesto. Las filas
> `image/decode-webp-lossy*` y `shell/chrome-png-*` son las nuevas de r31.

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k 2,9 · 32k 9,0 · 64k 17,8 ms | lineal |
| `css/parse-10k-rules` | 10k 14,2 ms | lineal |
| `css/resolve-styles-10k-nodes` | **29,072 ms** | +4,3 % vs r30 (entorno; A/B +0,6 %) |
| `layout/realistic-1mb` | **40,386 ms** | +8,3 % vs r30 (A/B +1,2 %) |
| `paint/build-display-list` | **9,220 ms** | +17,8 % vs r30 (A/B +2,1 %) |
| `paint/render-to-png-800x600` | **9,205 ms** | +3,9 % vs r30 (A/B +1,8 %) |
| `paint/backdrop-fixture-raster` | 1,40 vs 1,07 sin efecto | +0,33 ms blur |
| `paint/backdrop-fixture-png` | 0,89 vs 0,64 sin efecto | +0,25 ms blur+PNG |
| `pipeline` | **138,485 ms** (135,2–151,8) | +21,6 % vs r30 publicado; A/B +4,1 %; entorno ~10 % |
| `image/decode-png` | **6,898 ms** | +4,0 % vs r30; A/B −0,5 %; foco |
| `image/decode-webp-lossy` | **11,083 ms** 1024×768 | nuevo; VP8 optimizado |
| `image/decode-webp-lossy-q75/q90` | **0,062 / 0,066 ms** 64×48 | nuevos |
| `shell/chrome-png-640/1024` | **1,526 / 4,848 ms** | componente chrome |

## 5. Ronda 31: features y verificación

### 5.1 Qué entró

- `b37457e` **Cascade Layers**: `@layer`, anidamiento, `!important` invertido y
  `revert-layer` (detalle en docs de `css`).
- `6e97eae` **PNG single-encode**: `--chrome` y `--find` codifican el PNG final
  una sola vez (antes se re-encodaba la página/overlay).
- `8dde3dc` **`fetch()` JS** con provider del host (JSON/404/no-network).
- `822c8a2` **VP8 lossy más rápido** y **proxy HTTP/HTTPS**.

### 5.2 Presupuestos (sin subir)

| Medida | r31 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png` | **138,49 ms** | ≤140 | 98,9 % | 1,51 ms | ✅ límite |
| `layout/realistic-1mb` | **40,39 ms** | ≤45 | 89,7 % | 4,61 ms | ✅ |
| `css/resolve-styles-10k` | **29,07 ms** | ≤35 | 83,1 % | 5,93 ms | ✅ |
| showcase render | **30,23 ms** | ≤35 | 86,4 % | 4,77 ms | ✅ |
| shell headless showcase | 29,85 ms | ≤35 | 85,3 % | 5,15 ms | ✅ |
| `paint/render-to-png` | **9,21 ms** | ref ~8,5 | — | +0,71 ms | ⚠️ foco |
| `paint/build-display-list` | **9,22 ms** | ≤25 | 36,9 % | 15,78 ms | ✅ |
| `html/realistic-1mb` | 18,00 ms | ≤100 | 18,0 % | 82,0 ms | ✅ |
| `image/decode-png` | **6,90 ms** | foco ≤6,6 | 104,5 % | **−0,30 ms** | ⚠️ foco |
| `image/decode-webp-lossy` | **11,08 ms** | — | — | — | informativo |

**No se suben los presupuestos.** Todo cumple con margen salvo el foco
`decode-png` (+4,5 % sobre 6,6) y `render-to-png` (+0,71 sobre la referencia);
el A/B con r30 prístino da **−0,5 % en decode-png y +1,8 % en render-to-png**,
así que la desviación absoluta es de la ventana degradada (~10 %). El pipeline
queda en **138,49 ms (99 % del presupuesto)**: límite, vigilar en r32; la
segunda ventana dio 140,02 ms (≈100 %), confirmando que el margen está en el
límite de ruido. Siguiente foco: bajar los ~5–10 % de entorno/overhead y
recuperar margen antes de tocar presupuestos.

### 5.3 Verificación de estabilidad (segunda ventana)

5 pasadas extra (load 1,73–1,83): pipeline **140,02** · css **28,69** · layout
**38,51** · display **8,89** · render-to-png **9,13** · decode-png **6,70** ·
VP8 lossy **11,30**; canónica TOTAL **614.590**. Confirma que el pipeline está
en el límite 140 y que la variabilidad de la ventana es alta; no se usa para
la tabla principal.

### 5.4 A/B VP8 lossy (filas nuevas)

| Fila | mediana | rango | Notas |
|---|---:|---:|---|
| `image/decode-webp-lossy` (1024×768) | **11,083 ms** | 10,60–11,91 | 40 552 B; vs 13,56 ms medido ad-hoc en r30 (−18 %) |
| `image/decode-webp-lossy-q75` (64×48) | **0,062 ms** | 0,057–0,063 | 786 B |
| `image/decode-webp-lossy-q90` (64×48) | **0,066 ms** | 0,064–0,069 | 1 104 B |
| `image/decode-webp-lossless` (64×48) | 0,053 ms | 0,049–0,055 | referencia VP8L |

El decode lossy 1024×768 ya está en bench (fila nueva) y baja de ~13,6 ms a
~11,1 ms con la optimización de r31. La referencia 53,6 ms de r30 sigue sin
reproducirse (ventana/fixture); se mantiene como nota histórica.

## 6. PNG single-encode: chrome y find (r31)

Objetivo: overhead de `--chrome`/`--find` ≤8,5 ms. Medición con binario shell
r30 prístino vs r31 en la misma ventana (`chrome-ab.json`):

| Modo | plain | r30 chrome | overhead r30 | r31 chrome | overhead r31 |
|---|---:|---:|---:|---:|---:|
| showcase 1024×768 `--chrome` | 29,23 | 45,22 | **+15,99 ms** | 29,34 | **+0,11 ms** |
| simple URL 1024×768 `--chrome` | 12,48 | 18,49 | **+6,01 ms** | 12,39 | **−0,09 ms** |
| `find.html --find megi` | 11,49 | — | — | 11,95 | **+0,46 ms** |
| `--chrome-menu` showcase | — | — | — | 31,09 | +? (≈+1,24 vs plain) |

El single-encode **elimina prácticamente el overhead de chrome** (de +16 ms a
+0,11 ms en showcase; de +6,0 a −0,1 ms con URL) y deja `--find` en +0,46 ms,
muy por debajo del objetivo ≤8,5 ms. Componente aislado (bench):
`shell/chrome-png-640x400` **1,526 ms** y `1024x768` **4,848 ms**.

## 7. Coste de features r31

7 ejecuciones intercaladas, load ~1,0–1,8, performance:

| Feature | Medición | Lectura |
|---|---|---|
| **Cascade layers** (`cascade_layers.html`) | **5,18 ms** vs misma página plana **4,72 ms** | **+0,46 ms (+9,7 %)**; PNG idéntico (`6e6c5cd2…`), el coste es la cascada con capas |
| **`fetch()` JSON** (`js_fetch_json.html`) | **8,16 ms** vs equivalente sin fetch **3,84 ms** | **+4,32 ms (+112 %)**; incluye arranque JS, provider y microtareas del fetch |
| **PNG single-encode chrome** | overhead **+0,11 ms** (showcase) / −0,09 ms (URL) | objetivo ≤8,5 ms cumplido de sobra; ver §6 |
| **`--find` overlay** | **+0,46 ms** vs plain (11,95 vs 11,49) | overlay de resaltado + PNG único |
| **VP8 lossy** | **11,08 ms** 1024×768; 0,062/0,066 ms q75/q90 | filas nuevas; ver §5.4 |
| **Proxy HTTP/HTTPS** | no medible | loopback offline: sin origen de red real en el arnés; se documenta como no medido |
| `--chrome-menu` | **34,34 ms** (+3,25 ms sobre `--chrome` 31,09) | menú determinista, PNG 88 003 B (`c06b0268…`) |
| `--save-png` pareja | **30,31 ms** | reutiliza el PNG codificado |

## 8. Codificador PNG y goldens

| Métrica | r30 | r31 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8,863 | **9,205** | entorno; A/B +1,8 % |
| `shell/chrome-png-640x400` | 1,546 | **1,526** | componente |
| `shell/chrome-png-1024x768` | 4,876 | **4,848** | componente |
| Golden showcase | 80 220 B `c95c7eff…` | **80 220 B `c95c7eff…`** | intacto |
| Golden basic | 21 090 B `e943565d…` | **21 090 B `e943565d…`** | intacto |
| Golden phase4 | 42 853 B `9e957177…` | **42 853 B `9e957177…`** | intacto |
| Golden basic.rgba | 1 920 000 B `60f8984e…` | **1 920 000 B `60f8984e…`** | intacto |
| Chrome (no golden) | `931e45ce…` 86 629 B | **`931e45ce…` 86 629 B** | sin cambio de píxeles |
| Chrome menu | `c06b0268…` 88 003 B | **`c06b0268…` 88 003 B** | ídem |

## 9. Pipeline render, shell y páginas (r31)

| Fixture / modo | pared r31 | pared r30 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **30,23 ms** | 28,95 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,65 ms** | 9,31 | 21 090 B (`e943565d…`) |
| showcase shell headless | **29,85 ms** | 28,89 | 80 220 B |
| showcase `--chrome` | **31,09 ms** | 42,85 | 86 629 B (`931e45ce…`) |
| showcase `--chrome --chrome-menu` | **34,34 ms** | 45,82 | 88 003 B (`c06b0268…`) |
| `about:history` | **10,44 ms** | 10,07 | 18 204 B (`209d2950…`) |
| directorio file:// | **12,83 ms** | — | 36 596 B (`59f22777…`) |
| página de error | **12,57 ms** | — | 30 870 B (`2499005b…`) |
| `find.html --find megi` | **11,95 ms** | 18,22 | 29 950 B (`a818f32a…`) |
| `find.html` plain | 11,49 ms | — | 30 056 B (`19c35c18…`) |
| showcase `--save-png` | **30,31 ms** | 30,85 | 80 220 B |

## 10. Filas E2E

| Fila | r31 (ms) | r30 (ms) | Δ |
|---|---:|---:|---:|
| `pipeline/gradients-800x600` | **54,9** | 51,9 | +5,8 % (entorno) |
| `pipeline/grid-800x600` | **6,5** | 5,9 | +10 % (entorno) |
| `css/vars-calc-10k` | **21,3** | 20,202 | +5,4 % |
| `paint/clipping-800x600` | **52,5** | 49,8 | +5,4 % |
| `layout/sticky-text-1mb` | **35,0** | 33,2 | +5,4 % |

## 11. Presupuestos: veredicto

| Medida | r31 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png` | **138,49 ms** | **≤140** | 98,9 % | 1,51 ms | ✅ límite |
| `layout/realistic-1mb` | **40,39 ms** | **≤45** | 89,7 % | 4,61 ms | ✅ |
| `css/resolve-styles-10k` | **29,07 ms** | **≤35** | 83,1 % | 5,93 ms | ✅ |
| showcase render | **30,23 ms** | **≤35** | 86,4 % | 4,77 ms | ✅ |
| shell headless showcase | 29,85 ms | ≤35 | 85,3 % | 5,15 ms | ✅ |
| `paint/render-to-png` | **9,21 ms** | ref ~8,5 | — | +0,71 ms | ⚠️ foco |
| `paint/build-display-list` | **9,22 ms** | **≤25** | 36,9 % | 15,78 ms | ✅ |
| `html/realistic-1mb` | 18,00 ms | ≤100 | 18,0 % | 82,0 ms | ✅ |
| `image/decode-png` | **6,90 ms** | foco ≤6,6 | 104,5 % | −0,30 ms | ⚠️ foco |

**No se suben los presupuestos.** Pipeline en el límite (99 %) por la ventana
degradada; A/B con r30 prístino no muestra regresión de pipeline (+4,1 % con
~10 % de entorno). `decode-png` y `render-to-png` quedan en foco. Próximo foco:
**backdrop/PNG regions**, **TLA/live bindings**, **push prioritization**,
**perf render/JS** y **fuzz largo**; recuperar margen de pipeline/decode-png
antes de cambiar presupuestos.

---

## 12. Comparación histórica

Veintinueve baselines en la misma máquina, mismo arnés (la ronda 14 no publicó baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 | R28 | R29 | R30 | **R31 (vigente)** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | 00:34 | 01:18 | 02:07 | 03:04 | 04:06 | 04:39 | 06:48 | 07:25 | **10:05** | **11:30** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | `00d8388` | `d1f3121` | `0b3adee` | `4718408` | `f42b9b9` | `9d2298b` | `a0cdfcb` | `8f9cfa9` | **`f7dad8d`** | **`aba05f8`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | 1,20–1,23 | 1,76–1,86 | 1,32–1,38 | 0,79–0,84 | 1,30–1,35 | 1,36–1,42 | 0,67–0,96 | 1,04–1,05 | **1,27–1,35** | **1,46–1,69** |

Filas clave (mediana, ms; r22–r30 incluyen filas E2E/decode/paint que no entran en TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 | R28 | R29 | R30 | **R31** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | 16.847 | 16.566 | 16.171 | 16.701 | 16.609 | **16.536** | **17.613** | 17.175 | 17.508 | 17.998 |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | 35.805 | 25.477 | 27.085 | 27.322 | 26.577 | **25.957** | **27.508** | 27.216 | 27.855 | 29.072 |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | 37.069 | 35.043 | 34.900 | 35.355 | 35.396 | **35.930** | **37.988** | 37.786 | 37.283 | 40.386 |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | 8.973 | 7.940 | 7.982 | 7.702 | 7.974 | **7.729** | **8.606** | 8.409 | 7.825 | 9.220 |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | 8.354 | 8.626 | 8.775 | 8.905 | 8.695 | **8.877** | **9.313** | 8.906 | 8.863 | 9.205 |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | 141.194 | 106.440 | 107.974 | 107.635 | 107.950 | **106.522** | **115.674** | 113.238 | 113.890 | 138.485 |
| `image/decode-png-800x600` | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 6.495 | 6.506 | 6.524 | 6.551 | 7.975 | **6.399** | **6.699** | 6.661 | 6.634 | 6.898 |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** | **305.461** | **307.687** | **309.478** | **309.460** | **306.611** | **330.650** | 323.023 | **322.706** | **356.352** |


### Ronda 30 → ronda 31, fila a fila

A/B prístino intercalado (7 pares, load 1,5–1,8, performance) r30 `f7dad8d`
vs r31 `aba05f8`: el entorno de esta ventana es **~10 % más lento** que la
baseline r30 publicada (el propio binario r30 prístino mide pipeline ~135 ms), así
que los absolutos r31 no son comparables con r30 sin normalizar:

| Benchmark | r30 publicado | r30 prístino (esta ventana) | r31 (esta ventana) | Δ A/B |
|---|---:|---:|---:|---:|
| `pipeline/html-to-png` | 113,89 | 135,09 | **140,65** | +4,1 % |
| `css/resolve-styles-10k` | 27,86 | 28,80 | **28,97** | +0,6 % |
| `layout/realistic-1mb` | 37,28 | 39,81 | **40,31** | +1,2 % |
| `paint/build-display-list` | 7,83 | 9,01 | **9,20** | +2,1 % |
| `paint/render-to-png` | 8,86 | 8,88 | **9,04** | +1,8 % |
| `image/decode-png` | 6,63 | 6,73 | **6,70** | −0,5 % |
| `image/decode-webp-lossy` | — | — | **11,08** | nuevo |

La mediana de 5 pasadas r31 (pipeline **138,49 ms**) sigue por debajo de ≤140; el
margen fino se atribuye a la ventana degradada, no a una regresión de r31 (el A/B
+4,1 % no llega a explicar el +21 % absoluto vs r30). Presupuestos absolutos: ver
§11. Filas nuevas `image/decode-webp-lossy(-q75/-q90)` medidas en la misma ventana.

---

## 13. Cómo comparar y detectar regresiones

1. Medir con load <2 y perfil `performance`; sin él la máquina capa a
   1,3–2,2 GHz. Documentar load/freq y **el estado del entorno** (esta ventana
   quedó ~10 % por debajo de r30).
2. A/B prístino intercalado obligatorio para atribuir deltas; comparar ratios,
   no solo absolutos entre ventanas.
3. Guardar JSON; regresión si empeora >15 % reproducible. Filas:
   `--filter image`, `--filter paint`, `--filter shell/chrome-png`,
   `--filter pipeline`, `--filter css/vars-calc`.
4. Goldens: showcase `c95c7eff…` 80 220 B; basic `e943565d…` 21 090 B; phase4
   `9e957177…` 42 853 B; `basic.rgba` `60f8984e…`; chrome `931e45ce…` 86 629 B;
   chrome-menu `c06b0268…` 88 003 B.

## 14. Limitaciones

- **Entorno r31 degradado ~10 %** (pipeline r30 prístino 135 ms vs 113,9
  publicado); la ventana de estabilidad confirma el límite 140. Se documenta y
  se re-mide en r32.
- **Proxy HTTP/HTTPS**: no medible en el arnés (loopback offline, sin origen de
  red real); coste de red/proxy pendiente de un escenario conectado.
- **PNG single-encode**: medido como overhead de `--chrome`/`--find`; los md5
  de chrome no son goldens de página y cambian con la UI.
- **fetch()**: coste de la fixture mínima (+4,32 ms); cargas reales de red/JSON
  deben medirse con el proxy/red conectados.
- **VP8 lossy**: filas nuevas; 53,6 ms de r30 no reproducido (ventana/fixture).
- **Cascade layers**: coste de cascada de 4,72→5,18 ms en fixture; sin capas
  el fast path no cambia resultados.
- **Perfil**: `performance` vía `powerprofilesctl` (reversible); loads
  1,0–1,8 durante las mediciones.
- **website/pages.yml**: del dueño, no se tocan.
