# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-11, 04:06–04:35 (Europe/Madrid). Ronda 26:
  **module scripts** cableados en render (resolver/loader real), **filtros SVG**
  (`feColorMatrix`/`feBlend`/`feMerge`/composite aritmético + SMIL básico),
  **decodificación escalada** de imágenes grandes, y filas nuevas de bench para
  efectos de capa y `--save-png` aislado.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `f42b9b9`**. Árbol con
  WIP ajeno solo en docs (`CHANGELOG`, `CONTRIBUTING`, `README`, `SECURITY`,
  `ARCHITECTURE`, `HANDOFF`, `JAVASCRIPT-STRATEGY`, `PHASE4-SUPPORT`, `ROADMAP`,
  `TOOLING`, `status/progress.toml`); motor medido `f42b9b9`.
- **Ventana de bench**: **load 1,30–1,35** en las 5 pasadas válidas, `powersave`
  con frecuencia máxima muestreada **~4,1–4,7 GHz**; render/shell a load ~1,1–1,3.
- **Binarios**: bench `de65c0dc…` (5 279 776 B) en las 5 pasadas; el script lo
  recompiló con la misma fuente (`c7c2ec09…`); render `98fee537…` (20 917 032 B);
  shell `f43d898a…` (25 181 264 B).
- **Artefactos**: `target/bench-r26-run1..5.json`,
  `target/bench-aggregate-r26.json` (`sha256 = 74d22dbd…`),
  `target/bench-r26/ab_r25_r26.json`, `target/bench-r26-feature-costs.json`,
  `target/bench-latest.txt`/`-meta.txt` (canónica, TOTAL 530.600).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–26; la 14 no publicó baseline).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. La matriz incluye E2E (gradients/grid/vars-calc/clipping/sticky),
**efectos de capa** (layer-effects, filtros/backdrop aislados), **save-png
aislado** (encode/write/total) y **decode de imagen** (PNG/JPEG/GIF 800×600 y
WebP VP8L real). En r26 no cambia el camino de datos del motor: el commit de
bench añade filas y reutilización segura de buffers.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5; *heavy* 3 — decode y pipeline son heavy).
- **La tabla de §3 es la mediana, por fila, de 5 pasadas completas válidas**
  (load 1,30–1,35; `setup_ms` ~400–440 ms). La pasada canónica de
  `./tools/bench.sh` (`TOTAL 530.600 ms`, 39 filas) sirve de fingerprint.
- **A/B intercalado prístino**: `git archive 4718408` (r25) + bench de esa ronda
  (`target/r25-pristine`) vs binario r26, 5 pares alternando orden, load
  1,73–2,11 (§5.2).
- Render/shell: 7 ejecuciones intercaladas; fixtures r26 (`image_scaled`,
  `module_external`, `module_chain`) frente a equivalentes sin la feature
  (temporales en `target/bench-r26/`), con `--json` para estadios/metadata.
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E), 20 hilos |
| RAM / SO | 15 GiB · CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Governor / térmica | `powersave`; freq máx. ~4,1–4,7 GHz; paquete ~50–55 °C |
| Load | **1,30–1,35** bench; ~1,1–1,3 render/shell |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

./tools/bench.sh                                  # pasada canónica
for i in 1 2 3 4 5; do target/release/bench --json > target/bench-r26-run$i.json; done

# A/B prístino r25 vs r26
git archive 4718408 | tar -x -C target/r25-pristine
cd target/r25-pristine && CARGO_TARGET_DIR="$PWD/target" cargo build --release -p bench

# Filas nuevas de paint/save-png y decode
target/release/bench --filter paint
target/release/bench --filter shell/save-png
target/release/bench --filter image

# Coste de features r26
target/release/render tests/fixtures/render/image_scaled.html /tmp/a.png --width 800 --height 600 --json
target/release/render tests/fixtures/render/image_scaled.html /tmp/b.png --width 800 --height 600 \
    --max-image-dim 128 --json
target/release/render tests/fixtures/render/module_external.html /tmp/m.png --width 800 --height 600 --json

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
| paint | `build-display-list`, `rasterize`, `render-to-png`, `clipping`, **`layer-effects-800x600`**, **`object-fit-800x600`**, **`filter-fixture[/-no-effect]`**, **`backdrop-fixture[/-no-effect]`** | Display list, raster, PNG, clipping, **cola de pintado con efectos** (filtros/backdrop) y fixtures aisladas con/sin efecto. |
| pipeline | `html-to-png-800x600`, `gradients`, `grid` | Pipeline completo. |
| shell | **`save-png-encode-800x600`**, **`save-png-write-800x600`**, **`save-png-total-800x600`** | `Pixmap::to_png()`, escritura a disco y coste combinado del `--save-png` aislado. |
| image | `decode-png/jpeg/gif`, `decode-webp-lossless/alpha/palette` | Decode completo a RGBA8. |
| fonts | (`feature fonts`) | No re-medido. |

---

## 3. Baseline vigente (ronda 26, HEAD `f42b9b9`)

Mediana de **5 pasadas** en calma (load 1,30–1,35). `setup_ms` ~400–440 ms.
**Filas legacy (19)**: `TOTAL 309,460 ms`; **todas (39)**: `TOTAL 529,359 ms`;
pasada canónica: `TOTAL 530.600 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `html/realistic-1mb` | **16.609** | 16.365–16.908 |
| `html/nested-div-10k` | **2.738** | 2.643–2.748 |
| `html/nested-div-32k` | **8.663** | 8.391–9.088 |
| `html/nested-div-64k` | **17.010** | 16.528–17.394 |
| `html/nested-div-64k-default-limits` | **10.173** | 10.068–10.440 |
| `html/massive-end-p` | **17.422** | 17.236–17.596 |
| `html/massive-formatting` | **10.489** | 10.218–10.586 |
| `html/tag-40k-attrs` | **9.484** | 9.388–9.654 |
| `html/pretty-5k-blocks` | **8.710** | 8.394–8.758 |
| `css/parse-10k-rules` | **13.152** | 12.991–13.275 |
| `css/resolve-styles-2k-nodes` | **5.601** | 5.349–5.706 |
| `css/resolve-styles-10k-nodes` | **26.577** | 26.052–26.982 |
| `css/selector-match-simple` | **0.660** | 0.630–0.674 |
| `css/selector-match-descendant` | **1.245** | 1.168–1.264 |
| `layout/realistic-1mb` | **35.396** | 34.675–35.937 |
| `paint/build-display-list` | **7.974** | 7.842–8.241 |
| `paint/rasterize-800x600` | **0.912** | 0.905–1.028 |
| `paint/render-to-png-800x600` | **8.695** | 8.504–8.860 |
| `pipeline/html-to-png-800x600` | **107.950** | 106.621–109.567 |
| `pipeline/gradients-800x600` | **52.109** | 49.940–52.476 |
| `pipeline/grid-800x600` | **5.805** | 5.751–5.921 |
| `css/vars-calc-10k` | **19.860** | 19.120–21.430 |
| `paint/clipping-800x600` | **48.994** | 48.087–49.186 |
| `layout/sticky-text-1mb` | **31.639** | 31.262–32.072 |
| `paint/layer-effects-800x600` | **27.788** | 26.265–27.890 |
| `paint/object-fit-800x600` | **3.349** | 3.173–3.385 |
| `shell/save-png-encode-800x600` | **3.653** | 3.613–3.723 |
| `shell/save-png-write-800x600` | **0.003** | 0.003–0.003 |
| `shell/save-png-total-800x600` | **3.660** | 3.576–3.705 |
| `paint/filter-fixture` | **0.308** | 0.304–0.315 |
| `paint/filter-fixture-no-effect` | **0.296** | 0.290–0.309 |
| `paint/backdrop-fixture` | **0.314** | 0.302–0.325 |
| `paint/backdrop-fixture-no-effect` | **0.023** | 0.022–0.024 |
| `image/decode-png-800x600` | **7.975** | 7.949–8.084 |
| `image/decode-jpeg-800x600` | **11.058** | 10.748–11.159 |
| `image/decode-gif` | **2.988** | 2.826–2.998 |
| `image/decode-webp-lossless` | **0.048** | 0.043–0.049 |
| `image/decode-webp-alpha` | **0.023** | 0.021–0.024 |
| `image/decode-webp-palette` | **0.006** | 0.006–0.006 |

> **Aviso**: el TOTAL (legacy) suma etapas y pipeline end-to-end (solapados); no
> es un presupuesto. En r26 **todos los presupuestos se cumplen con margen**
> (ver §11). Las filas WebP se miden en **microsegundos** (fixtures pequeñas).

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 2.74 · 32k: 8.66 · 64k: 17.01 ms | **Lineal** (3,16× → 3,14×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 13.15 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 5.60 / **26.58 ms** | −2,7 % vs r25 (ventana/varianza). |
| `layout/realistic-1mb` | 28 195 elementos → **35.40 ms** | +0,1 % vs r25; estable. |
| `paint/layer-effects-800x600` | 24 backdrop + 24 filter groups → **27.79 ms** | Mide la **cola de pintado** (display list + raster), no la cascada. |
| `image/decode-*` | PNG 7,98 · JPEG 11,06 · GIF 2,99 ms; WebP 6–48 µs | **PNG +21,7 % vs r25** por el nuevo camino de decode escalado (ver §5.2). |
| `pipeline` | **107,95 ms** (106,6–109,6) | +0,3 % vs r25; margen 32,1 ms bajo ≤140. |

---

## 5. Ronda 26: features y verificación

### 5.1 Qué entró

- `0167bf8 feat(render)`: **module scripts** (`<script type="module">`) con
  resolver/loader reales y grafo de imports.
- `2fb05a1 feat(svg)`: **feColorMatrix, feBlend, feMerge, composite aritmético**
  y animación **SMIL** básica.
- `cf258fa feat(image,render)`: **decodificación escalada** de imágenes grandes
  (`--max-image-dim`).
- `7ab354c perf(paint,bench)`: filas aisladas de efectos de capa, filas de
  `save-png` y reutilización segura de buffers de pintado.
- `a39c4ab docs(fixtures)`: documentación de las fixtures de la ronda.

### 5.2 Verificación: A/B y presupuestos

A/B intercalado de archivos prístinos r25 (`4718408`) vs r26 (`f42b9b9`),
5 pares, load 1,73–2,11:

| Fila | r25 (ms) | r26 (ms) | Δ (A/B) |
|---|---:|---:|---:|
| `html/realistic-1mb` | 17,21 | 17,04 | **−1,0 %** |
| `css/resolve-styles-10k-nodes` | 27,08 | 27,05 | −0,1 % |
| `layout/realistic-1mb` | 35,76 | 36,92 | +3,3 % |
| `paint/build-display-list` | 8,07 | 8,03 | −0,5 % |
| `paint/render-to-png-800x600` | 8,84 | 9,21 | +4,2 % |
| `pipeline/html-to-png-800x600` | 110,22 | 111,69 | **+1,3 %** |
| `image/decode-png-800x600` | 6,55 | 7,95 | **+21,3 %** |
| `image/decode-jpeg-800x600` | 11,15 | 11,15 | 0,0 % |

**Lectura**: el pipeline apenas cambia (+1,3 %), pero hay **dos hallazgos**:
(a) el decode de **PNG 800×600 sube ~22 %** (JPEG/GIF no) por el nuevo camino de
decodificación escalada — es una regresión de una fila de referencia, no de un
presupuesto, y se cuantifica para el siguiente foco; (b) layout +3,3 % y
render-to-png +4,2 % están dentro de la varianza de ventana. **Los presupuestos
se mantienen con margen; no se suben.**

| Medida | r26 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **107,95 ms** | **≤ 140 ms** | 77,1 % | 32,1 ms | ✅ |
| `layout/realistic-1mb` | **35,40 ms** | **≤ 45 ms** | 78,7 % | 9,6 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **26,58 ms** | **≤ 35 ms** | 75,9 % | 8,4 ms | ✅ |
| `render` showcase 1024×768 | **29,48 ms** | **≤ 35 ms** | 84,2 % | 5,5 ms | ✅ |
| shell headless showcase | 29,49 ms | ≤ 35 ms | 84,3 % | 5,5 ms | ✅ |
| `paint/render-to-png-800x600` | **8,70 ms** | ref. ~8,5 | — | +0,2 ms | ✅ |
| `paint/build-display-list` | **7,97 ms** | **≤ 25 ms** | 31,9 % | 17,0 ms | ✅ |
| `html/realistic-1mb` | 16,61 ms | ≤ 100 ms | 16,6 % | 83,4 ms | ✅ |
| `paint/layer-effects-800x600` | 27,79 ms | — | — | — | referencia §6 |

### 5.3 Nota de integridad

Las primeras pasadas se descartaron por picos de load ajenos (8–11); las
publicadas son de la ventana 1,30–1,35. El A/B se hizo con archivos prístinos
(r25 y r26) para no mezclar WIP. Árbol con WIP ajeno solo en docs; motor medido
`f42b9b9`; bench recompilado por el script con la misma fuente.

---

## 6. Filas nuevas del carril paint/bench (r26)

### 6.1 Efectos de capa y fixtures aisladas

| Fila | Mediana | Qué mide |
|---|---:|---|
| `paint/layer-effects-800x600` | **27,788 ms** | Cola de pintado de 24 grupos `backdrop-filter` + 24 grupos `filter` sobre 800×600: display list + rasterizado a píxmap. |
| `paint/object-fit-800x600` | **3,349 ms** | Display list + raster de imágenes con `object-fit`/`object-position` (7 imágenes). |
| `paint/filter-fixture` | **0,308 ms** | `filter_effects.html` (11 cajas filtradas) → display list + píxmap. |
| `paint/filter-fixture-no-effect` | **0,296 ms** | La misma fixture sin declaraciones `filter`: **Δ +0,012 ms (+4 %)**. |
| `paint/backdrop-fixture` | **0,314 ms** | `backdrop_filter.html` (blur + dim) → display list + píxmap. |
| `paint/backdrop-fixture-no-effect` | **0,023 ms** | La misma sin `backdrop-filter`: **Δ +0,291 ms (×13,5)**. |

**Cómo leerlas**: `layer-effects` mide **solo la cola de pintado** (los estilos y
la cascada ya están hechos), así que sus 27,8 ms no son el sobrecoste de la
feature en una página real; la fixture aislada muestra que el **filtro en sí es
barato** (+0,012 ms) y el **backdrop-filter** es el que pesa (+0,291 ms en la
fixture). El sobrecoste real de las páginas con efectos se reparte entre
**estilos/cascada** (que no entran en estas filas) y **PNG/escritura**; por eso
el siguiente foco es perfilar estilos/PNG para layer effects, no optimizar el
raster a ciegas.

### 6.2 `--save-png` aislado

| Fila | Mediana | Qué mide |
|---|---:|---|
| `shell/save-png-encode-800x600` | **3,653 ms** | `Pixmap::to_png()` de 800×600 (15 071 B en la pasada canónica). |
| `shell/save-png-write-800x600` | **0,003 ms** | `fs::write` de esos bytes. |
| `shell/save-png-total-800x600` | **3,660 ms** | Encode + escritura (coste aislado del `--save-png`). |

El **encode domina** (3,653 ms) y la escritura es despreciable (0,003 ms). En la
pareja real `--screenshot` vs `--screenshot`+`--save-png` el incremento medido es
**30,04 vs 29,49 ms (+0,55 ms)**: el PNG ya se codifica para la captura y
`--save-png` reutiliza esos bytes, así que el coste marginal es mucho menor que
el total aislado. Coincide con la fila aislada: **encode 3,76 / escritura 0,003
/ pareja ~30 vs 29** en la ronda.

---

## 7. Coste de las features r26

Wall time de `render` (7 ejecuciones, load ~1,1–1,3) frente a la misma fixture
sin la feature. `--json` confirma `images_loaded=1`, `display_items=3` y
`script_virtual_time_ms=0` en todos los casos:

| Fixture | con feature | sin feature | Δ | notas |
|---|---:|---:|---:|---|
| `image_scaled.html` (foto 1024×768 mostrada a 128×96) | **27,45 ms** | **21,14 ms** con `--max-image-dim 128` | **−6,31 ms (−23,0 %)** | decodifica a 128 px de lado; PNG de salida 14 619→14 620 B (imagen idéntica en pantalla). |
| `module_external.html` | **9,51 ms** | 8,71 ms (sin `<script type="module">`) | **+0,80 ms (+9,2 %)** | resolución + carga del módulo externo. |
| `module_chain.html` | **9,54 ms** | 8,25 ms (sin módulo) | **+1,29 ms (+15,6 %)** | cadena de imports (2 módulos). |
| `--save-png` (showcase, pareja con `--screenshot`) | **30,04 ms** | 29,49 ms | **+0,55 ms** | reutiliza el PNG ya codificado; ver §6.2. |

La decodificación escalada **ahorra un 23 %** en la fixture con foto 1024×768
(la imagen visible es la misma), a costa de ese +21,7 % en el decode PNG 800×600
del bench (fila de referencia, no de presupuesto). Los módulos JS añaden <1,3 ms
en fixtures mínimas; el coste real dependerá del grafo del sitio.

---

## 8. Codificador PNG y goldens

| Métrica | r25 | r26 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.905 | **8.695** | −2,4 % (varianza) |
| — rasterizar | 0.965 | **0.912** | −5,5 % |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden phase4 PNG | 42 853 B | **42 853 B** | `9e957177…` |
| Golden `basic.rgba` | 1 920 000 B | **1 920 000 B** | `60f8984e…` |

---

## 9. Pipeline `render`, shell, chrome, find y save-png (r26)

7 ejecuciones intercaladas (load ~1,1–1,3; goldens verificados):

| Fixture / modo | pared r26 | pared r25 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **29,48 ms** | 28,34 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,89 ms** | 9,38 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **29,49 ms** | 28,50 | 80 220 B (idéntico) |
| showcase `--chrome` | **46,05 ms** | 45,80 | 85 746 B (`cbfc6087…`) |
| `find.html` con `--find megi` | **17,40 ms** | 17,23 | 29 950 B (`a818f32a…`) |
| showcase `--screenshot` + `--save-png` | **30,04 ms** | 67,33 (incorrecto: dos escrituras) | 80 220 B (idéntico) |

`--chrome` cuesta +16,6 ms en showcase; `--find` 17,40 ms; `--save-png` con
`--screenshot` añade **+0,55 ms** (la pareja correcta es la de esta tabla: el
bench aislado y la pareja confirman encode 3,65 ms y escritura 0,003 ms). Los
goldens sin chrome siguen byte-idénticos.

---

## 10. Filas E2E (gradients/grid/vars-calc/clipping/sticky)

| Fila | r26 (ms) | r25 (ms) | Δ |
|---|---:|---:|---:|
| `pipeline/gradients-800x600` | **52.109** | 52.591 | −0,9 % |
| `pipeline/grid-800x600` | **5.805** | 5.818 | −0,2 % |
| `css/vars-calc-10k` | **19.860** | 19.932 | −0,4 % |
| `paint/clipping-800x600` | **48.994** | 49.295 | −0,6 % |
| `layout/sticky-text-1mb` | **31.639** | 31.006 | +2,0 % |

Sin cambios estructurales; variaciones ≤2 %. Filtros: `--filter pipeline`,
`--filter css/vars-calc`, `--filter paint/clipping`, `--filter layout/sticky`,
`--filter image`, `--filter paint`, `--filter shell/save-png`.

---

## 11. Presupuestos: veredicto

| Medida | r26 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **107,95 ms** | **≤ 140 ms** | 77,1 % | 32,1 ms | ✅ |
| `layout/realistic-1mb` | **35,40 ms** | **≤ 45 ms** | 78,7 % | 9,6 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **26,58 ms** | **≤ 35 ms** | 75,9 % | 8,4 ms | ✅ |
| `render` showcase 1024×768 | **29,48 ms** | **≤ 35 ms** | 84,2 % | 5,5 ms | ✅ |
| shell headless showcase | 29,49 ms | ≤ 35 ms | 84,3 % | 5,5 ms | ✅ |
| `paint/render-to-png-800x600` | 8,70 ms | ref. ~8,5 | — | +0,2 ms | ✅ |
| `paint/build-display-list` | **7,97 ms** | **≤ 25 ms** | 31,9 % | 17,0 ms | ✅ |
| `html/realistic-1mb` | 16,61 ms | ≤ 100 ms | 16,6 % | 83,4 ms | ✅ |
| `image/decode-*` | 0,006–11,06 ms | — | — | — | referencia §3 |

**No se suben los presupuestos.** Todos se cumplen; el menor margen relativo es
showcase (15,8 %) y css resolve (24,1 %), el absoluto render-to-png (+0,2 ms).
Hallazgo a vigilar: **PNG decode +21,7 % vs r25** (fila de referencia) por el
camino escalado; no afecta a los presupuestos pero debe perfilarse. Siguiente
foco: **perf de estilos/PNG para layer effects**, **investigar/optimizar decode
PNG**, **h2 push/priorities contra servidor real**, **streaming uploads** y
**campañas de fuzz**.

---

## 12. Comparación histórica

Veinticuatro baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | 00:34 | 01:18 | 02:07 | 03:04 | **04:06** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | `00d8388` | `d1f3121` | `0b3adee` | `4718408` | **`f42b9b9`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | 1,20–1,23 | 1,76–1,86 | 1,32–1,38 | 0,79–0,84 | **1,30–1,35** |

Filas clave (mediana, ms; r22–r26 incluyen filas E2E/decode/paint nuevas que no
entran en el TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | 16.847 | 16.566 | 16.171 | 16.701 | **16.609** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | 35.805 | 25.477 | 27.085 | 27.322 | **26.577** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | 37.069 | 35.043 | 34.900 | 35.355 | **35.396** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | 8.973 | 7.940 | 7.982 | 7.702 | **7.974** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | 8.354 | 8.626 | 8.775 | 8.905 | **8.695** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | 141.194 | 106.440 | 107.974 | 107.635 | **107.950** |
| `image/decode-png-800x600` | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 6.495 | 6.506 | 6.524 | 6.551 | **7.975** |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** | **305.461** | **307.687** | **309.478** | **309.460** |

### Ronda 25 → ronda 26, fila a fila

| Benchmark | R25 (ms) | R26 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 16.701 | **16.609** | −0,6 % |
| `html/nested-div-10k` | 2.698 | **2.738** | +1,5 % |
| `html/nested-div-32k` | 8.470 | **8.663** | +2,3 % |
| `html/nested-div-64k` | 16.810 | **17.010** | +1,2 % |
| `html/nested-div-64k-default-limits` | 9.978 | **10.173** | +2,0 % |
| `html/massive-end-p` | 17.529 | **17.422** | −0,6 % |
| `html/massive-formatting` | 10.507 | **10.489** | −0,2 % |
| `html/tag-40k-attrs` | 9.380 | **9.484** | +1,1 % |
| `html/pretty-5k-blocks` | 8.755 | **8.710** | −0,5 % |
| `css/parse-10k-rules` | 13.162 | **13.152** | −0,1 % |
| `css/resolve-styles-2k-nodes` | 5.703 | **5.601** | −1,8 % |
| `css/resolve-styles-10k-nodes` | 27.322 | **26.577** | −2,7 % |
| `css/selector-match-simple` | 0.659 | **0.660** | +0,2 % |
| `css/selector-match-descendant` | 1.242 | **1.245** | +0,2 % |
| `layout/realistic-1mb` | 35.355 | **35.396** | +0,1 % |
| `paint/build-display-list` | 7.702 | **7.974** | +3,5 % |
| `paint/rasterize-800x600` | 0.965 | **0.912** | −5,5 % |
| `paint/render-to-png-800x600` | 8.905 | **8.695** | −2,4 % |
| `pipeline/html-to-png-800x600` | 107.635 | **107.950** | +0,3 % |
| `pipeline/gradients-800x600` | 52.591 | **52.109** | −0,9 % |
| `pipeline/grid-800x600` | 5.818 | **5.805** | −0,2 % |
| `css/vars-calc-10k` | 19.932 | **19.860** | −0,4 % |
| `paint/clipping-800x600` | 49.295 | **48.994** | −0,6 % |
| `layout/sticky-text-1mb` | 31.006 | **31.639** | +2,0 % |
| `image/decode-png-800x600` | 6.551 | **7.975** | **+21,7 %** |
| `image/decode-jpeg-800x600` | 10.978 | **11.058** | +0,7 % |
| `image/decode-gif` | 2.912 | **2.988** | +2,6 % |
| `image/decode-webp-lossless` | 0.046 | **0.048** | +4,3 % |
| `image/decode-webp-alpha` | 0.023 | **0.023** | 0,0 % |
| `image/decode-webp-palette` | 0.006 | **0.006** | 0,0 % |
| `paint/layer-effects-800x600` | — | **27.788** | nuevo |
| `paint/object-fit-800x600` | — | **3.349** | nuevo |
| `shell/save-png-encode-800x600` | — | **3.653** | nuevo |
| `shell/save-png-write-800x600` | — | **0.003** | nuevo |
| `shell/save-png-total-800x600` | — | **3.660** | nuevo |
| `paint/filter-fixture` | — | **0.308** | nuevo |
| `paint/filter-fixture-no-effect` | — | **0.296** | nuevo |
| `paint/backdrop-fixture` | — | **0.314** | nuevo |
| `paint/backdrop-fixture-no-effect` | — | **0.023** | nuevo |
| **TOTAL legacy** | **309.478** | **309.460** | **−0,0 %** |

---

## 13. Cómo comparar y detectar regresiones

1. Medir con load < 2 y comprobar frecuencia (en esta ronda: 1,30–1,35, freq
   máx. ~4,1–4,7 GHz). Si no hay ventana, marcarlo y descartar pases con picos.
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
4. Regresión si la mediana empeora >15 % reproducible. En r26 todos los
   presupuestos se cumplen (§11); vigilar **PNG decode +21,7 %**.
5. Filas E2E/decode/paint: `--filter pipeline`, `css/vars-calc`,
   `paint/clipping`, `layout/sticky`, `image`, `paint`, `shell/save-png`.
6. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   phase4 `9e957177…` 42 853 B, `basic.rgba` `60f8984e…` 1 920 000 B, chrome
   `cbfc6087…` 85 746 B, find `a818f32a…` 29 950 B.

---

## 14. Limitaciones y notas

- **Ventana buena** (load 1,30–1,35, freq ~4,1–4,7 GHz; render/shell ~1,1–1,3);
  las primeras pasadas se descartaron por load ajeno 8–11. A/B a load 1,73–2,11.
- **WIP ajeno solo en docs**; el motor medido es `f42b9b9` y esta tarea solo
  tocó `docs/PERFORMANCE.md` (filas de bench ya venían del commit `7ab354c`).
- **Filas nuevas**: `layer-effects` mide la cola de pintado, no estilos/PNG;
  `filter` aislado +0,012 ms, `backdrop` +0,291 ms; `save-png` encode 3,653 ms /
  escritura 0,003 ms / pareja real +0,55 ms (30 vs 29).
- **Hallazgo**: `image/decode-png-800x600` +21,7 % vs r25 por el camino de
  decodificación escalada; JPEG/GIF/WebP estables. Es una fila de referencia, no
  un presupuesto; siguiente foco perfilar/optimizar ese camino.
- **HTTP/2**: MVP + body/trailers/100-continue (r26 net); sin push/priorities
  real contra servidor.
- **`website/` y `pages.yml`**: del dueño; no se tocan.
- **Fast path de tablas (r7)**: nota histórica; no hay commit compilable con el
  camino avanzado sin fast path.
