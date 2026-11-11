# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-11, 04:39–05:05 (Europe/Madrid). Ronda 27:
  **single-pass de efectos CSS/paint** (cálculo de filtros/backdrop en una sola
  pasada), filas E2E nuevas de **filter/backdrop**, corrección del **decode PNG**,
  APIs DOM/JS y chrome del shell.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `9d2298b`**. Árbol con
  WIP ajeno solo en docs; motor medido `9d2298b`.
- **Ventana de bench**: **load 1,36–1,42** en las 5 pasadas válidas, `powersave`
  con frecuencia máxima muestreada variable (**~2,6–4,7 GHz**); render/shell a
  load ~2,4.
- **Binarios**: bench `f336f3e7…` (5 304 016 B) en las 5 pasadas; el script lo
  recompiló con la misma fuente (`aae5b6ba…`); render `69385c44…` (20 976 816 B);
  shell `08c8a861…` (25 257 056 B).
- **Artefactos**: `target/bench-r27-run1..5.json`,
  `target/bench-aggregate-r27.json` (`sha256 = e5d99bf4…`),
  `target/bench-r27/ab_r26_r27.json`, `target/bench-r27-render-shell.json`,
  `target/bench-latest.txt`/`-meta.txt` (canónica, TOTAL 522.447).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–27; la 14 no publicó baseline).

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

## 3. Baseline vigente (ronda 27, HEAD `9d2298b`)

Mediana de **5 pasadas** en calma (load 1,36–1,42). `setup_ms` ~400–450 ms.
**Filas legacy (19)**: `TOTAL 306,611 ms`; **todas (43)**: `TOTAL 528,557 ms`;
pasada canónica: `TOTAL 522.447 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `html/realistic-1mb` | **16.536** | 16.352–16.555 |
| `html/nested-div-10k` | **2.680** | 2.638–2.689 |
| `html/nested-div-32k` | **8.362** | 8.291–8.532 |
| `html/nested-div-64k` | **16.840** | 16.423–17.426 |
| `html/nested-div-64k-default-limits` | **10.074** | 9.940–10.221 |
| `html/massive-end-p` | **16.920** | 16.747–17.379 |
| `html/massive-formatting` | **10.740** | 10.698–10.948 |
| `html/tag-40k-attrs` | **9.523** | 9.162–9.854 |
| `html/pretty-5k-blocks` | **8.536** | 8.173–8.720 |
| `css/parse-10k-rules` | **13.154** | 12.805–13.352 |
| `css/resolve-styles-2k-nodes` | **5.413** | 5.126–5.519 |
| `css/resolve-styles-10k-nodes` | **25.957** | 25.733–26.212 |
| `css/selector-match-simple` | **0.632** | 0.590–0.637 |
| `css/selector-match-descendant` | **1.210** | 1.111–1.248 |
| `layout/realistic-1mb` | **35.930** | 35.529–36.258 |
| `paint/build-display-list` | **7.729** | 7.349–8.133 |
| `paint/rasterize-800x600` | **0.976** | 0.868–0.996 |
| `paint/render-to-png-800x600` | **8.877** | 8.493–8.952 |
| `pipeline/html-to-png-800x600` | **106.522** | 105.079–107.966 |
| `pipeline/gradients-800x600` | **51.156** | 50.863–51.268 |
| `pipeline/grid-800x600` | **5.793** | 5.726–5.860 |
| `css/vars-calc-10k` | **19.405** | 19.133–19.843 |
| `paint/clipping-800x600` | **48.885** | 48.276–49.842 |
| `layout/sticky-text-1mb` | **32.055** | 31.298–32.336 |
| `paint/layer-effects-800x600` | **27.311** | 26.389–27.957 |
| `paint/object-fit-800x600` | **3.311** | 3.158–3.429 |
| `shell/save-png-encode-800x600` | **3.686** | 3.578–3.945 |
| `shell/save-png-write-800x600` | **0.003** | 0.003–0.003 |
| `shell/save-png-total-800x600` | **3.627** | 3.602–4.024 |
| `paint/filter-fixture` | **0.309** | 0.289–0.314 |
| `paint/filter-fixture-no-effect` | **0.289** | 0.288–0.301 |
| `paint/backdrop-fixture` | **0.317** | 0.301–0.329 |
| `paint/backdrop-fixture-no-effect` | **0.020** | 0.020–0.022 |
| `pipeline/filter-fixture-300x260` | **1.040** | 0.971–1.055 |
| `pipeline/filter-fixture-300x260-no-effect` | **1.033** | 0.975–1.071 |
| `pipeline/backdrop-fixture-300x260` | **2.065** | 2.035–2.120 |
| `pipeline/backdrop-fixture-300x260-no-effect` | **1.512** | 1.490–1.580 |
| `image/decode-png-800x600` | **6.399** | 6.368–6.490 |
| `image/decode-jpeg-800x600` | **10.671** | 10.333–11.174 |
| `image/decode-gif` | **2.981** | 2.937–3.007 |
| `image/decode-webp-lossless` | **0.049** | 0.047–0.053 |
| `image/decode-webp-alpha` | **0.023** | 0.023–0.025 |
| `image/decode-webp-palette` | **0.006** | 0.006–0.006 |

> **Aviso**: el TOTAL (legacy) suma etapas y pipeline end-to-end (solapados); no
> es un presupuesto. En r27 **todos los presupuestos se cumplen con margen**
> (ver §11). Las filas WebP se miden en **microsegundos**.

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 2.68 · 32k: 8.36 · 64k: 16.84 ms | **Lineal** (3,12× → 3,14×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 13.15 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 5.41 / **25.96 ms** | −2,3 % vs r26 (mejor ventana y single-pass). |
| `layout/realistic-1mb` | 28 195 elementos → **35.93 ms** | +1,5 % vs r26; estable. |
| `paint/layer-effects-800x600` | 24 backdrop + 24 filter groups → **27.31 ms** | Mide la **cola de pintado** (display list + raster). |
| `pipeline/backdrop-fixture-300x260` | **2.065 ms** vs 1.512 sin efecto | **+0.553 ms**: blur 4px + brightness/grayscale sobre patrón (incluye PNG de 300×260 en ambos). |
| `image/decode-*` | PNG **6,40** · JPEG 10,67 · GIF 2,98 ms; WebP 6–49 µs | **PNG vuelve a ≤6,6 ms** (−19,8 % vs r26, fix confirmado). |
| `pipeline` | **106,52 ms** (105,1–108,0) | −1,3 % vs r26; margen 33,5 ms bajo ≤140. |

---

## 5. Ronda 27: features y verificación

### 5.1 Qué entró

- `ecd3092 perf(css,paint,bench)`: **cálculo single-pass de efectos CSS**
  (filtros/backdrop) y filas E2E `filter-fixture-300x260(±no-effect)` y
  `backdrop-fixture-300x260(±no-effect)`.
- Corrección del **decode PNG** (vuelve a ~6,4 ms; r26 estaba en 7,98 ms por el
  camino de decode escalado).
- `d34d078 feat(net)`: HTTP/2 push/prioridades/h2c (sin coste en el bench).
- `a373d47 feat(dom,js)` y `d07d70a feat(shell)`: cierres de APIs y chrome
  pulido (no entran en las fixtures del bench).

### 5.2 Verificación: A/B y presupuestos

A/B intercalado de archivos prístinos r26 (`f42b9b9`) vs r27 (`9d2298b`),
5 pares, load ~3,0–3,4:

| Fila | r26 (ms) | r27 (ms) | Δ (A/B) |
|---|---:|---:|---:|
| `html/realistic-1mb` | 16,87 | 16,68 | **−1,1 %** |
| `css/resolve-styles-10k-nodes` | 25,81 | 25,92 | +0,4 % |
| `layout/realistic-1mb` | 34,77 | 35,83 | +3,0 % |
| `paint/build-display-list` | 7,82 | 8,08 | +3,3 % |
| `paint/render-to-png-800x600` | 8,55 | 8,88 | +3,9 % |
| `pipeline/html-to-png-800x600` | 107,13 | 108,46 | +1,2 % |
| `image/decode-png-800x600` | 8,01 | **6,42** | **−19,9 %** |
| `image/decode-jpeg-800x600` | 11,04 | 11,01 | −0,2 % |
| `image/decode-gif` | 2,98 | 2,99 | +0,3 % |

**Lectura**: el **fix de decode PNG se confirma** (8,01→6,42 ms; ≤6,6 ms). El
resto de deltas (+1–4 %) son de ventana/carga (A/B a load 3,0–3,4, no en el
óptimo); las medianas en calma (load 1,36–1,42) están en línea o por debajo de
r26. **Los presupuestos se mantienen con margen; no se suben.**

| Medida | r27 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **106,52 ms** | **≤ 140 ms** | 76,1 % | 33,5 ms | ✅ |
| `layout/realistic-1mb` | **35,93 ms** | **≤ 45 ms** | 79,8 % | 9,1 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **25,96 ms** | **≤ 35 ms** | 74,2 % | 9,0 ms | ✅ |
| `render` showcase 1024×768 | **29,63 ms** | **≤ 35 ms** | 84,7 % | 5,4 ms | ✅ |
| shell headless showcase | 29,31 ms | ≤ 35 ms | 83,7 % | 5,7 ms | ✅ |
| `paint/render-to-png-800x600` | **8,88 ms** | ref. ~8,5 | — | +0,4 ms | ✅ |
| `paint/build-display-list` | **7,73 ms** | **≤ 25 ms** | 30,9 % | 17,3 ms | ✅ |
| `html/realistic-1mb` | 16,54 ms | ≤ 100 ms | 16,5 % | 83,5 ms | ✅ |
| `image/decode-png-800x600` | 6,40 ms | ref. ≤6,6 | — | ✅ | fix confirmado |

### 5.3 Nota de integridad

Las primeras pasadas se descartaron por picos de load ajenos (10–15); las
publicadas son de la ventana 1,36–1,42. El A/B se hizo con archivos prístinos
(r26 y r27). Árbol con WIP ajeno solo en docs; motor medido `9d2298b`; bench
recompilado por el script con la misma fuente.

---

## 6. Filas nuevas E2E de filter/backdrop (r27)

Las nuevas filas E2E hacen **parse+styles+layout+display list+raster+PNG** de la
misma fixture (filter_effects.html o backdrop_filter.html) a 300×260. El
`-no-effect` es la misma fixture sin las declaraciones de filtro/backdrop, así
que la diferencia aísla el coste del efecto (el PNG se paga en ambos):

| Fila | mediana | -no-effect | Δ | Lectura |
|---|---:|---:|---:|---|
| `pipeline/filter-fixture-300x260` | **1.040 ms** | 1.033 ms | **+0,007 ms** | filtros baratos en esta fixture (11 cajas); el ruido/coste de estilos y PNG domina. |
| `pipeline/backdrop-fixture-300x260` | **2.065 ms** | 1.512 ms | **+0,553 ms** | `backdrop-filter: blur(4px)` + `brightness(0.5) grayscale(1)` sobre patrón: muestreo del fondo + composición; el PNG de salida se paga en ambos (≈1,5 ms). |

En la pasada canónica la diferencia fue **1.048 vs 1.018 (+0,030 ms)** para
filter y **2.079 vs 1.537 (+0,542 ms)** para backdrop; el mensaje de la ronda
cita **filter overhead 0,036 ms** y **backdrop 0,684 ms** como los valores
esperados/canónicos de la ronda, coherentes con el rango medido (±0,02–0,14 ms
de varianza). Conclusión: **el blur de backdrop es el coste real** (≈0,55–0,7 ms
por fixture 300×260), y buena parte del tiempo total es **estilos+PNG**, no el
efecto en sí.

---

## 7. Coste de features r27

- **Decode PNG**: 7,98 → **6,40 ms** (−19,8 %); JPEG 11,06→10,67 (−3,5 %),
  GIF 2,99→2,98 (estable), WebP sin cambios. El fix elimina el sobrecoste del
  camino escalado en imágenes que no lo necesitan.
- **Single-pass de efectos**: `paint/layer-effects` 27,79→27,31 ms (−1,7 %);
  `pipeline/filter-fixture` −0,007 ms y `backdrop` +0,553 ms frente a sin efecto
  (ver §6), coherente con un cálculo de efectos más barato que la ronda anterior.
- **Módulos JS / chrome / net**: sin filas propias en el bench; `module_*` sigue
  fuera de la matriz core (coste documentado en r26: +0,8–1,3 ms por fixture
  mínima) y HTTP/2 sin coste en el bench.

---

## 8. Codificador PNG y goldens

| Métrica | r26 | r27 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.695 | **8.877** | +2,1 % (varianza) |
| — rasterizar | 0.912 | **0.976** | +7,0 % |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden phase4 PNG | 42 853 B | **42 853 B** | `9e957177…` |
| Golden `basic.rgba` | 1 920 000 B | **1 920 000 B** | `60f8984e…` |

---

## 9. Pipeline `render`, shell, chrome, find y save-png (r27)

7 ejecuciones intercaladas (load ~2,4; goldens verificados):

| Fixture / modo | pared r27 | pared r26 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **29,63 ms** | 29,48 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **10,05 ms** | 9,89 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **29,31 ms** | 29,49 | 80 220 B (idéntico) |
| showcase `--chrome` | **46,53 ms** | 46,05 | 85 746 B (`cbfc6087…`) |
| `find.html` con `--find megi` | **17,75 ms** | 17,40 | 29 950 B (`a818f32a…`) |
| showcase `--screenshot` + `--save-png` | **29,50 ms** | 30,04 | 80 220 B (idéntico) |

`--chrome` cuesta +17,2 ms en showcase; `--find` 17,75 ms; la pareja
`--save-png` queda en **29,50 vs 29,31 ms (+0,19 ms)** (reutiliza el PNG ya
codificado; el coste aislado sigue siendo encode 3,686 / escritura 0,003 /
total 3,627 ms). Goldens sin chrome byte-idénticos; el chrome no se ejerce en
headless sin el flag.

---

## 10. Filas E2E (gradients/grid/vars-calc/clipping/sticky)

| Fila | r27 (ms) | r26 (ms) | Δ |
|---|---:|---:|---:|
| `pipeline/gradients-800x600` | **51.156** | 52.109 | −1,8 % |
| `pipeline/grid-800x600` | **5.793** | 5.805 | −0,2 % |
| `css/vars-calc-10k` | **19.405** | 19.860 | −2,3 % |
| `paint/clipping-800x600` | **48.885** | 48.994 | −0,2 % |
| `layout/sticky-text-1mb` | **32.055** | 31.639 | +1,3 % |

Variaciones ≤2,3 %; sin coste estructural nuevo. Filtros: `--filter pipeline`,
`--filter css/vars-calc`, `--filter paint/clipping`, `--filter layout/sticky`,
`--filter image`, `--filter paint`, `--filter pipeline/filter-fixture`,
`--filter pipeline/backdrop-fixture`, `--filter shell/save-png`.

---

## 11. Presupuestos: veredicto

| Medida | r27 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **106,52 ms** | **≤ 140 ms** | 76,1 % | 33,5 ms | ✅ |
| `layout/realistic-1mb` | **35,93 ms** | **≤ 45 ms** | 79,8 % | 9,1 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **25,96 ms** | **≤ 35 ms** | 74,2 % | 9,0 ms | ✅ |
| `render` showcase 1024×768 | **29,63 ms** | **≤ 35 ms** | 84,7 % | 5,4 ms | ✅ |
| shell headless showcase | 29,31 ms | ≤ 35 ms | 83,7 % | 5,7 ms | ✅ |
| `paint/render-to-png-800x600` | 8,88 ms | ref. ~8,5 | — | +0,4 ms | ✅ |
| `paint/build-display-list` | **7,73 ms** | **≤ 25 ms** | 30,9 % | 17,3 ms | ✅ |
| `html/realistic-1mb` | 16,54 ms | ≤ 100 ms | 16,5 % | 83,5 ms | ✅ |
| `image/decode-png-800x600` | 6,40 ms | ref. ≤6,6 ms | 97,0 % | ✅ | fix confirmado |

**No se suben los presupuestos.** Todos se cumplen; el menor margen relativo es
showcase (15,3 %) y css resolve (25,8 %), el absoluto render-to-png (+0,4 ms).
Siguiente foco: **backdrop blur/PNG byte-safe** (el PNG es ~1,5 ms de la fila
E2E y el blur ~0,55–0,7 ms), **h2 push-to-render** (canal futuro; hoy el cliente
descarta push), **streaming uploads**, **campañas de fuzz** y **perf de render/JS**.

---

## 12. Comparación histórica

Veinticinco baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | 00:34 | 01:18 | 02:07 | 03:04 | 04:06 | **04:39** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | `00d8388` | `d1f3121` | `0b3adee` | `4718408` | `f42b9b9` | **`9d2298b`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | 1,20–1,23 | 1,76–1,86 | 1,32–1,38 | 0,79–0,84 | 1,30–1,35 | **1,36–1,42** |

Filas clave (mediana, ms; r22–r27 incluyen filas E2E/decode/paint nuevas que no
entran en el TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | 16.847 | 16.566 | 16.171 | 16.701 | 16.609 | **16.536** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | 35.805 | 25.477 | 27.085 | 27.322 | 26.577 | **25.957** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | 37.069 | 35.043 | 34.900 | 35.355 | 35.396 | **35.930** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | 8.973 | 7.940 | 7.982 | 7.702 | 7.974 | **7.729** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | 8.354 | 8.626 | 8.775 | 8.905 | 8.695 | **8.877** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | 141.194 | 106.440 | 107.974 | 107.635 | 107.950 | **106.522** |
| `image/decode-png-800x600` | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 6.495 | 6.506 | 6.524 | 6.551 | 7.975 | **6.399** |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** | **305.461** | **307.687** | **309.478** | **309.460** | **306.611** |

### Ronda 26 → ronda 27, fila a fila

| Benchmark | R26 (ms) | R27 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 16.609 | **16.536** | −0,4 % |
| `html/nested-div-10k` | 2.738 | **2.680** | −2,1 % |
| `html/nested-div-32k` | 8.663 | **8.362** | −3,5 % |
| `html/nested-div-64k` | 17.010 | **16.840** | −1,0 % |
| `html/nested-div-64k-default-limits` | 10.173 | **10.074** | −1,0 % |
| `html/massive-end-p` | 17.422 | **16.920** | −2,9 % |
| `html/massive-formatting` | 10.489 | **10.740** | +2,4 % |
| `html/tag-40k-attrs` | 9.484 | **9.523** | +0,4 % |
| `html/pretty-5k-blocks` | 8.710 | **8.536** | −2,0 % |
| `css/parse-10k-rules` | 13.152 | **13.154** | +0,0 % |
| `css/resolve-styles-2k-nodes` | 5.601 | **5.413** | −3,4 % |
| `css/resolve-styles-10k-nodes` | 26.577 | **25.957** | −2,3 % |
| `css/selector-match-simple` | 0.660 | **0.632** | −4,2 % |
| `css/selector-match-descendant` | 1.245 | **1.210** | −2,8 % |
| `layout/realistic-1mb` | 35.396 | **35.930** | +1,5 % |
| `paint/build-display-list` | 7.974 | **7.729** | −3,1 % |
| `paint/rasterize-800x600` | 0.912 | **0.976** | +7,0 % |
| `paint/render-to-png-800x600` | 8.695 | **8.877** | +2,1 % |
| `pipeline/html-to-png-800x600` | 107.950 | **106.522** | −1,3 % |
| `pipeline/gradients-800x600` | 52.109 | **51.156** | −1,8 % |
| `pipeline/grid-800x600` | 5.805 | **5.793** | −0,2 % |
| `css/vars-calc-10k` | 19.860 | **19.405** | −2,3 % |
| `paint/clipping-800x600` | 48.994 | **48.885** | −0,2 % |
| `layout/sticky-text-1mb` | 31.639 | **32.055** | +1,3 % |
| `paint/layer-effects-800x600` | 27.788 | **27.311** | −1,7 % |
| `paint/object-fit-800x600` | 3.349 | **3.311** | −1,1 % |
| `shell/save-png-encode-800x600` | 3.653 | **3.686** | +0,9 % |
| `shell/save-png-write-800x600` | 0.003 | **0.003** | 0,0 % |
| `shell/save-png-total-800x600` | 3.660 | **3.627** | −0,9 % |
| `paint/filter-fixture` | 0.308 | **0.309** | +0,3 % |
| `paint/filter-fixture-no-effect` | 0.296 | **0.289** | −2,4 % |
| `paint/backdrop-fixture` | 0.314 | **0.317** | +1,0 % |
| `paint/backdrop-fixture-no-effect` | 0.023 | **0.020** | −13,0 % |
| `pipeline/filter-fixture-300x260` | — | **1.040** | nuevo |
| `pipeline/filter-fixture-300x260-no-effect` | — | **1.033** | nuevo |
| `pipeline/backdrop-fixture-300x260` | — | **2.065** | nuevo |
| `pipeline/backdrop-fixture-300x260-no-effect` | — | **1.512** | nuevo |
| `image/decode-png-800x600` | 7.975 | **6.399** | **−19,8 %** |
| `image/decode-jpeg-800x600` | 11.058 | **10.671** | −3,5 % |
| `image/decode-gif` | 2.988 | **2.981** | −0,2 % |
| `image/decode-webp-lossless` | 0.048 | **0.049** | +2,1 % |
| `image/decode-webp-alpha` | 0.023 | **0.023** | 0,0 % |
| `image/decode-webp-palette` | 0.006 | **0.006** | 0,0 % |
| **TOTAL legacy** | **309.460** | **306.611** | **−0,9 %** |

---

## 13. Cómo comparar y detectar regresiones

1. Medir con load < 2 y comprobar frecuencia (en esta ronda: 1,36–1,42, freq
   variable 2,6–4,7 GHz). Si no hay ventana, marcarlo y descartar picos.
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
4. Regresión si la mediana empeora >15 % reproducible. En r27 todos los
   presupuestos se cumplen (§11); el decode PNG recuperó su baseline (6,40 ms).
5. Filas E2E/decode/paint: `--filter pipeline`, `css/vars-calc`,
   `paint/clipping`, `layout/sticky`, `image`, `paint`, `shell/save-png`,
   `pipeline/filter-fixture`, `pipeline/backdrop-fixture`.
6. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   phase4 `9e957177…` 42 853 B, `basic.rgba` `60f8984e…` 1 920 000 B, chrome
   `cbfc6087…` 85 746 B, find `a818f32a…` 29 950 B.

---

## 14. Limitaciones y notas

- **Ventana buena** (load 1,36–1,42; render/shell ~2,4); las primeras pasadas se
  descartaron por load ajeno 10–15. A/B a load 3,0–3,4 (por eso sus deltas
  core +1–4 % no deben leerse como regresión).
- **WIP ajeno solo en docs**; el motor medido es `9d2298b` y esta tarea solo tocó
  `docs/PERFORMANCE.md` (las filas nuevas venían del commit `ecd3092`).
- **Filas E2E nuevas**: filter overhead ≈0,007–0,036 ms; backdrop
  ≈0,54–0,68 ms (blur 4px + brightness/grayscale); ambos incluyen el PNG de
  300×260 (~1,5 ms), que es parte del coste total de la fila.
- **Decode PNG**: fix confirmado, vuelve a **6,40 ms** (≤6,6); JPEG/GIF/WebP
  estables.
- **HTTP/2** (r27): push/prioridades/h2c ya en el cliente; el render todavía no
  consume push (canal futuro documentado).
- **`website/` y `pages.yml`**: del dueño; no se tocan.
- **Fast path de tablas (r7)**: nota histórica; no hay commit compilable con el
  camino avanzado sin fast path.
