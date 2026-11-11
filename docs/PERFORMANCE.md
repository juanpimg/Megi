# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-11, 07:25–07:55 (Europe/Madrid). Ronda 29:
  **streaming de respuestas + reutilización de conexión**, **import maps/dynamic
  import**, **SVG textPath/SMIL**, **WebP lossy MVP (cabecera)**, **shell
  directorios/páginas de error/favicons** y fuzz matrix.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `8f9cfa9`**. Árbol con
  WIP ajeno solo en docs; motor medido `8f9cfa9`.
- **Ventana de bench**: **load 1,04–1,05** en las 5 pasadas válidas y la canónica;
  **platform profile `performance`** (`powerprofilesctl set performance`; la
  máquina estaba en `quiet`/`power-saver` y capaba 1,3–2,2 GHz — pasadas de
  calibración descartadas); frecuencia máxima muestreada **4,1–4,6 GHz**.
- **Binarios**: bench `bf273de5…` (5 322 592 B; el script lo recompiló con la misma
  fuente), render `d75e0a16…`, shell `9406cfb6…`.
- **Artefactos**: `target/bench-r29-run1..5.json`,
  `target/bench-aggregate-r29.json` (`sha256 = faed93eb…`),
  `target/bench-r29/ab_r28_r29.json`, `target/bench-r29-feature-costs.json`,
  `target/bench-latest.txt`/`-meta.txt` (canónica, TOTAL 555.744).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–29; la 14 no publicó baseline).
> **Caveat de entorno**: el perfil de energía cambió respecto a la ventana en que
> se midió r27; r28 y r29 sí comparten `performance`/freq ~4–4,6 GHz, así que las
> comparaciones de esta ronda son homogéneas.

---

## 1. Metodología

### 1.1 Cargas

Las cargas del bench se generan en memoria (`crates/bench/src/generators.rs`); el
*fixture* se construye **fuera** del cronómetro. Matriz actual: E2E
(gradients/grid/vars-calc/clipping/sticky), efectos de capa (layer-effects,
object-fit, backdrop raster/png, filter), save-png aislado (encode/write/total) y
decode de imagen (PNG/JPEG/GIF 800×600 y WebP VP8L real). WebP lossy es
**experimental de cabecera** (sin píxeles todavía, §14).

### 1.2 Medición

- Reloj `std::time::Instant`; `--warmup 1`, `--iterations 5` (*heavy* 3: decode y
  pipeline). La tabla de §3 es la **mediana por fila de 5 pasadas válidas**.
- Pasada canónica `./tools/bench.sh` → `TOTAL 555.744 ms` (47 filas); sirve de
  fingerprint.
- **A/B prístino intercalado**: `git archive cb7af43` (r28) + bench de esa ronda
  (`target/r28-pristine`) vs r29, 5 pares alternando orden, load 0,93–0,96
  (§5.2). Es la atribución fiable; los absolutos entre ventanas pueden variar
  ±5–9 % por energía/frecuencia.
- Render/shell: 7 ejecuciones intercaladas bajo el mismo perfil; costes con
  fixtures propias en `target/bench-r29/` (`--json`).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E), 20 hilos |
| RAM / SO | 15 GiB · CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Perfil / governor | **ACPI `performance`** (`powerprofilesctl`), `intel_pstate` powersave; 4,1–4,6 GHz |
| Load | **1,04–1,05** bench; ~0,5–1,0 render/shell |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi
powerprofilesctl set performance            # entorno: sin esto el techo era 1,3–2,2 GHz
./tools/bench.sh
for i in 1 2 3 4 5; do target/release/bench --json > target/bench-r29-run$i.json; done
git archive cb7af43 | tar -x -C target/r28-pristine
cd target/r28-pristine && CARGO_TARGET_DIR="$PWD/target" cargo build --release -p bench
target/release/bench --filter image        # PNG/JPEG/GIF/WebP
target/release/bench --filter paint        # backdrop raster/png
target/release/render <fixture> /tmp/x.png --width 1024 --height 768 [--json]
target/release/megi <fixture|dir|url> --screenshot /tmp/s.png --chrome [--chrome-menu]
```

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb`, `nested-div-*`, `massive-*`, `tag-40k-attrs`, `pretty-5k-blocks` | Parse/tree construction. |
| css | `parse-10k-rules`, `resolve-styles-2k/10k`, `selector-match-*`, `vars-calc-10k` | Parse, cascada, matching, custom properties/calc. |
| layout | `layout/realistic-1mb`, `layout/sticky-text-1mb` | Layout; sticky + text-transform. |
| paint | `build-display-list`, `rasterize`, `render-to-png`, `clipping`, `layer-effects`, `object-fit`, `filter-fixture[/-no-effect]`, `backdrop-fixture[/-no-effect]`, **`backdrop-fixture-raster[/-no-effect]`**, **`backdrop-fixture-png[/-no-effect]`** | Display list, raster/PNG, cola de pintado con efectos y sub-etapas de backdrop (raster sin PNG vs encode PNG). |
| pipeline | `html-to-png`, `gradients`, `grid`, `filter-fixture-300x260[/-no-effect]`, `backdrop-fixture-300x260[/-no-effect]` | Pipeline completo. |
| shell | `save-png-encode/write/total-800x600` | Encode PNG, escritura y coste combinado. |
| image | `decode-png/jpeg/gif`, `decode-webp-lossless/alpha/palette` | Decode a RGBA8 (WebP lossy experimental sin píxeles). |
| fonts | (`feature fonts`) | No re-medido. |

---

## 3. Baseline vigente (ronda 29, HEAD `8f9cfa9`)

Mediana de **5 pasadas** en calma (load 1,04–1,05). **Filas legacy (19)**:
`TOTAL 323,023 ms`; **todas (47)**: `TOTAL 556,364 ms`; canónica:
`TOTAL 555.744 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `css/parse-10k-rules` | **13.995** | 13.798–14.254 |
| `css/resolve-styles-10k-nodes` | **27.216** | 26.839–27.460 |
| `css/resolve-styles-2k-nodes` | **5.733** | 5.520–5.848 |
| `css/selector-match-descendant` | **1.286** | 1.266–1.350 |
| `css/selector-match-simple` | **0.681** | 0.651–0.690 |
| `css/vars-calc-10k` | **20.514** | 20.238–20.823 |
| `html/massive-end-p` | **18.055** | 18.035–18.131 |
| `html/massive-formatting` | **10.852** | 10.725–10.879 |
| `html/nested-div-10k` | **2.831** | 2.779–2.891 |
| `html/nested-div-32k` | **8.959** | 8.471–8.986 |
| `html/nested-div-64k` | **17.678** | 17.549–18.024 |
| `html/nested-div-64k-default-limits` | **10.416** | 10.329–10.574 |
| `html/pretty-5k-blocks` | **8.795** | 8.777–8.802 |
| `html/realistic-1mb` | **17.175** | 16.423–17.331 |
| `html/tag-40k-attrs` | **10.010** | 9.955–10.325 |
| `image/decode-gif` | **3.030** | 3.003–3.040 |
| `image/decode-jpeg-800x600` | **11.243** | 11.188–11.256 |
| `image/decode-png-800x600` | **6.661** | 6.470–6.680 |
| `image/decode-webp-alpha` | **0.025** | 0.024–0.025 |
| `image/decode-webp-lossless` | **0.049** | 0.049–0.052 |
| `image/decode-webp-palette` | **0.006** | 0.006–0.006 |
| `layout/realistic-1mb` | **37.786** | 37.168–38.961 |
| `layout/sticky-text-1mb` | **33.258** | 32.932–33.445 |
| `paint/backdrop-fixture` | **0.321** | 0.316–0.327 |
| `paint/backdrop-fixture-no-effect` | **0.022** | 0.021–0.023 |
| `paint/backdrop-fixture-png` | **0.816** | 0.801–0.833 |
| `paint/backdrop-fixture-png-no-effect` | **0.601** | 0.587–0.613 |
| `paint/backdrop-fixture-raster` | **1.275** | 1.256–1.276 |
| `paint/backdrop-fixture-raster-no-effect` | **0.979** | 0.966–0.988 |
| `paint/build-display-list` | **8.409** | 8.259–10.253 |
| `paint/clipping-800x600` | **50.421** | 50.297–50.505 |
| `paint/filter-fixture` | **0.305** | 0.296–0.308 |
| `paint/filter-fixture-no-effect` | **0.305** | 0.295–0.309 |
| `paint/layer-effects-800x600` | **27.570** | 27.535–27.777 |
| `paint/object-fit-800x600` | **3.439** | 3.437–3.464 |
| `paint/rasterize-800x600` | **1.002** | 0.911–1.323 |
| `paint/render-to-png-800x600` | **8.906** | 8.888–9.181 |
| `pipeline/backdrop-fixture-300x260` | **2.115** | 2.100–2.142 |
| `pipeline/backdrop-fixture-300x260-no-effect` | **1.593** | 1.590–1.641 |
| `pipeline/filter-fixture-300x260` | **1.038** | 1.031–1.092 |
| `pipeline/filter-fixture-300x260-no-effect` | **1.022** | 1.013–1.041 |
| `pipeline/gradients-800x600` | **52.812** | 52.245–53.614 |
| `pipeline/grid-800x600` | **6.154** | 5.842–6.181 |
| `pipeline/html-to-png-800x600` | **113.238** | 111.272–114.434 |
| `shell/save-png-encode-800x600` | **3.908** | 3.839–3.952 |
| `shell/save-png-total-800x600` | **3.856** | 3.836–3.873 |
| `shell/save-png-write-800x600` | **0.003** | 0.003–0.004 |

> **Aviso**: el TOTAL legacy suma etapas solapadas; no es presupuesto. En r29
> todos los presupuestos se cumplen salvo **decode-png**, que queda **~0,9 % por
> encima del foco ≤6,6 ms** (ver §11). Las filas WebP se miden en µs.

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 2,831 · 32k: 8,959 · 64k: 17,678 ms | **Lineal** (3,16× → 3,12×). |
| `css/parse-10k-rules` | 5k: 7,1 · 10k: 13,995 · 20k: 31,9 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 5,733 / **27,216 ms** | −2,6 % vs r28 (A/B −1,7 %). |
| `layout/realistic-1mb` | 28 195 elementos → **37,786 ms** | −0,6 % vs r28. |
| `paint/backdrop-fixture-raster` | **1,275 ms** vs 0,979 sin efecto | **+0,296 ms** de blur en raster. |
| `paint/backdrop-fixture-png` | **0,816 ms** vs 0,601 sin efecto | **+0,215 ms** de blur+PNG (encode incluido). |
| `pipeline` | **113,238 ms** (111,3–114,4) | +0,8 % vs r28 (A/B −0,5 %); margen 26,8 ms bajo ≤140. |
| `image/decode-png-800x600` | **6,661 ms** (6,47–6,68) | +0,3 % vs r28; **0,9 % por encima del foco ≤6,6**. |

---

## 5. Ronda 29: features y verificación

### 5.1 Qué entró

- `20b662b feat(net)`: **streaming de respuestas + keep-alive h1 pool + sesión h2**
  (API opt-in; el pipeline de render sigue buffered por diseño).
- `6b5ee45 feat(js,render)`: **import maps**, **dynamic import** real y gap tipado
  de **top-level await**.
- `5bddd19 feat(svg)`: **textPath**, `keyTimes`/`keySplines` y **SMIL** de
  opacidad/color.
- `4673582 feat(image,render)`: **WebP lossy MVP** (contenedor/cabecera validada;
  píxeles experimentales pendientes).
- `af20b06 feat(shell)`: **listados de directorio file://**, **páginas de error**
  y **favicons** de pestaña; menú contextual determinista (`--chrome-menu`).
- `376fbf3 test(fuzz)`: matriz de 14 targets (h2 push, body stream, webp vp8,
  js module).

### 5.2 Verificación: A/B prístino r28 vs r29

5 pares intercalados, load 0,93–0,96, performance:

| Fila | r28 (ms) | r29 (ms) | Δ (A/B) |
|---|---:|---:|---:|
| `html/realistic-1mb` | 17,50 | 17,62 | +0,7 % |
| `css/resolve-styles-10k-nodes` | 27,94 | 27,46 | **−1,7 %** |
| `layout/realistic-1mb` | 38,02 | 37,41 | **−1,6 %** |
| `paint/build-display-list` | 9,03 | 8,85 | **−2,0 %** |
| `paint/render-to-png-800x600` | 8,90 | 8,76 | **−1,6 %** |
| `pipeline/html-to-png-800x600` | 112,31 | 111,78 | **−0,5 %** |
| `image/decode-png-800x600` | 6,682 | **6,661** | **−0,3 %** |
| `image/decode-jpeg-800x600` | 11,01 | 11,24 | +2,1 % |
| `image/decode-gif` | 3,02 | 3,03 | +0,3 % |
| `paint/backdrop-fixture-raster` | 1,262 | 1,275 | +1,0 % |
| `paint/backdrop-fixture-png` | 0,802 | 0,816 | +1,7 % |

**Lectura**: paridad o ligeras mejoras; no hay coste estructural nuevo en el
camino caliente. Los absolutos de r29 son algo más bajos que la baseline r28
publicada (ventana/frecuencia) pero el A/B confirma que el motor no ha empeorado.
**No se suben los presupuestos.**

| Medida | r29 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **113,24 ms** | ≤140 ms | 80,9 % | 26,76 ms | ✅ |
| `layout/realistic-1mb` | **37,79 ms** | ≤45 ms | 84,0 % | 7,21 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **27,22 ms** | ≤35 ms | 77,8 % | 7,78 ms | ✅ |
| `render` showcase 1024×768 | **29,06 ms** | ≤35 ms | 83,0 % | 5,94 ms | ✅ |
| shell headless showcase | 29,29 ms | ≤35 ms | 83,7 % | 5,71 ms | ✅ |
| `paint/render-to-png-800x600` | **8,91 ms** | ref ~8,5 | — | +0,4 ms | ✅ (foco) |
| `paint/build-display-list` | **8,41 ms** | ≤25 ms | 33,6 % | 16,6 ms | ✅ |
| `html/realistic-1mb` | 17,18 ms | ≤100 ms | 17,2 % | 82,8 ms | ✅ |
| `image/decode-png-800x600` | **6,66 ms** | foco ≤6,6 | 100,9 % | **−0,06 ms** | ⚠️ **foco** |

---

## 6. Filas backdrop raster vs PNG (r29)

Separar la **cola de pintado** del **encode PNG** aísla dónde está el coste de
`backdrop-filter`:

| Fila | con efecto | sin efecto | Δ |
|---|---:|---:|---:|
| `paint/backdrop-fixture-raster` (display list → píxmap, sin PNG) | **1,275 ms** | 0,979 ms | **+0,296 ms** |
| `paint/backdrop-fixture-png` (píxmap → PNG) | **0,816 ms** | 0,601 ms | **+0,215 ms** |
| `pipeline/backdrop-fixture-300x260` (completo) | **2,115 ms** | 1,593 ms | **+0,522 ms** |

El **blur** encarece raster (~0,30 ms) y PNG (~0,22 ms) en esta fixture 300×260;
las E2E completas (~2,1 ms) incluyen parse/styles/layout/display-list y son
coherentes con la suma. La reutilización segura de buffers (r28) mantiene el A/B
en +1,0/+1,7 % (dentro de ruido).

---

## 7. Coste de features r29

7 ejecuciones intercaladas, load ~0,5–1,0, performance, fixtures propias en
`target/bench-r29/`; `--json` confirma páginas equivalentes donde aplica:

| Feature | Medición | Lectura |
|---|---|---|
| **Import map** (`importmap/index.html`) | **6,20 ms** vs equivalente sin import map **6,01 ms** | **+0,19 ms (+3,2 %)** por resolver el mapa + cargar el módulo; PNG 839 vs 843 B. |
| **textPath + SMIL** | **5,50 ms** vs mismo SVG sin `<animate>` **5,57 ms** | **−0,07 ms** en captura estática (en t=0 la animación no cambia píxeles; md5 idéntico). |
| **WebP lossy placeholder** | **3,45 ms** vs misma foto como PNG **4,82 ms** | **−1,37 ms (−28,4 %)** porque el lossy experimental no se decodifica (placeholder+warning); no es comparable a decodificar píxeles reales. |
| **Streaming de respuestas** | API-only: `RenderOptions.stream_responses` | El pipeline renderiza **buffered** y produce PNG **byte-idéntico** con true/false (documentado); no hay flag CLI, así que no hay coste medible sin tocar código. |
| **Shell `--chrome`** | **46,20 ms** vs página plain **29,29 ms** | **+16,91 ms** (tab strip + address bar + favicon determinista). |
| **`--chrome-menu`** | **46,23 ms** | **+0,03 ms** vs `--chrome` (dibujo del menú; PNG 87 156 vs 85 753 B). |
| **Directorio file://** | **12,08 ms** | Página generada 30 555 B. |
| **Página de error** | **12,19 ms** | Página generada 30 410 B. |
| **Favicon de pestaña** | sitio con `<link rel=icon>` **15,25 ms** vs mismo sin icono **15,16 ms** | **+0,09 ms** (favicon 16×16 determinista; sin red). |
| **`--find`** | **16,48 ms** | índice + overlay de resaltado. |
| **`--save-png` pareja** | **29,25 ms** vs shell plain **29,29 ms** | reutiliza el PNG ya codificado (encode aislado 3,908 / escritura 0,003 / total 3,856 ms). |

---

## 8. Codificador PNG y goldens

| Métrica | r28 | r29 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 9.313 | **8.906** | −4,4 % |
| — rasterizar | 1.084 | **1.002** | −7,6 % |
| `shell/save-png-encode-800x600` | 3.912 | **3.908** | encode aislado |
| `shell/save-png-total-800x600` | 3.907 | **3.856** | encode+write |
| Golden showcase | 80 220 B `c95c7eff…` | **80 220 B `c95c7eff…`** | byte-idéntico |
| Golden basic | 21 090 B `e943565d…` | **21 090 B `e943565d…`** | byte-idéntico |
| Golden phase4 PNG | 42 853 B `9e957177…` | **42 853 B `9e957177…`** | byte-idéntico |
| Golden `basic.rgba` | 1 920 000 B `60f8984e…` | **1 920 000 B `60f8984e…`** | byte-idéntico |
| Chrome (no golden) | `fe196851…` 85 753 B | **`fe196851…` 85 753 B** | incluye favicon/dot determinista |
| Chrome menu | `c7ccc0e4…` 87 156 B | **`c7ccc0e4…` 87 156 B** | menú determinista |

---

## 9. Pipeline `render`, shell y páginas (r29)

7 ejecuciones intercaladas, performance; goldens verificados:

| Fixture / modo | pared r29 | pared r28 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **29,06 ms** | 26,74 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **10,17 ms** | 11,79 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **29,29 ms** | 29,25 | 80 220 B (idéntico) |
| showcase `--chrome` | **46,20 ms** | 46,25 | 85 753 B (`fe196851…`) |
| showcase `--chrome --chrome-menu` | **46,23 ms** | 45,98 | 87 156 B (`c7ccc0e4…`) |
| directorio `file://…/site/` | **12,08 ms** | — | 30 555 B (`1836525c…`) |
| página de error `file://missing` | **12,19 ms** | — | 30 410 B (`fc9dae97…`) |
| sitio con favicon (`--chrome`) | **15,25 ms** | — | 17 935 B (`7c8d34b1…`) |
| sitio sin icono (`--chrome`) | **15,16 ms** | — | 17 995 B (`bef43adb…`) |
| `find.html` `--find megi` | **16,48 ms** | 21,37 | 29 950 B (`a818f32a…`) |
| showcase `--screenshot`+`--save-png` | **29,25 ms** | 29,23 | 80 220 B (idéntico) |

`--chrome` cuesta **+16,91 ms**; `--chrome-menu` es gratis en tiempo (+0,03 ms)
pero añade 1 403 B de PNG; favicon +0,09 ms; directorio/error generan páginas
~12 ms. Los goldens sin chrome siguen byte-idénticos.

---

## 10. Filas E2E (gradients/grid/vars-calc/clipping/sticky)

| Fila | r29 (ms) | r28 (ms) | Δ |
|---|---:|---:|---:|
| `pipeline/gradients-800x600` | **52.812** | 54.368 | −2,9 % |
| `pipeline/grid-800x600` | **6.154** | 6.030 | +2,1 % |
| `css/vars-calc-10k` | **20.514** | 20.711 | −1,0 % |
| `paint/clipping-800x600` | **50.421** | 51.911 | −2,9 % |
| `layout/sticky-text-1mb` | **33.258** | 34.009 | −2,2 % |

Variaciones ≤3 % (ventana), sin coste estructural nuevo. Filtros: `--filter
pipeline`, `css/vars-calc`, `paint/clipping`, `layout/sticky`, `image`,
`paint`, `shell/save-png`, `paint/backdrop-fixture-raster`,
`paint/backdrop-fixture-png`.

---

## 11. Presupuestos: veredicto

| Medida | r29 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **113,24 ms** | **≤ 140 ms** | 80,9 % | 26,76 ms | ✅ |
| `layout/realistic-1mb` | **37,79 ms** | **≤ 45 ms** | 84,0 % | 7,21 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **27,22 ms** | **≤ 35 ms** | 77,8 % | 7,78 ms | ✅ |
| `render` showcase 1024×768 | **29,06 ms** | **≤ 35 ms** | 83,0 % | 5,94 ms | ✅ |
| shell headless showcase | 29,29 ms | ≤ 35 ms | 83,7 % | 5,71 ms | ✅ |
| `paint/render-to-png-800x600` | **8,91 ms** | ref ~8,5 | — | +0,4 ms | ✅ (foco) |
| `paint/build-display-list` | **8,41 ms** | **≤ 25 ms** | 33,6 % | 16,6 ms | ✅ |
| `html/realistic-1mb` | 17,18 ms | ≤ 100 ms | 17,2 % | 82,8 ms | ✅ |
| `image/decode-png-800x600` | **6,66 ms** | foco ≤6,6 | 100,9 % | −0,06 ms | ⚠️ **foco** |

**No se suben los presupuestos.** Todos se cumplen con margen; el único punto a
vigilar es **decode-png**, que queda **~0,9 % por encima del foco ≤6,6 ms**
(6,661 mediana; canónica 6,645; A/B −0,3 % vs r28: no es regresión de esta
ronda, sino que el foco es ajustado). Siguiente foco: **VP8 pixel decoder**,
**TLA/workers**, **PSL/trailers**, **perf render/JS** y **fuzz de larga
duración**; mantener A/B intercalado antes de tocar presupuestos.

---

## 12. Comparación histórica

Veintisiete baselines en la misma máquina, mismo arnés (la ronda 14 no publicó baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 | R28 | **R29 (vigente)** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | 00:34 | 01:18 | 02:07 | 03:04 | 04:06 | 04:39 | 06:48 | 07:25 |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | `00d8388` | `d1f3121` | `0b3adee` | `4718408` | `f42b9b9` | `9d2298b` | `a0cdfcb` | `8f9cfa9` |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | 1,20–1,23 | 1,76–1,86 | 1,32–1,38 | 0,79–0,84 | 1,30–1,35 | 1,36–1,42 | 0,67–0,96 | 1,04–1,05 |

Filas clave (mediana, ms; r22–r29 incluyen filas E2E/decode/paint que no entran en TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 | R25 | R26 | R27 | R28 | **R29** |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | 16.847 | 16.566 | 16.171 | 16.701 | 16.609 | **16.536** | **17.613** | 17.175 |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | 35.805 | 25.477 | 27.085 | 27.322 | 26.577 | **25.957** | **27.508** | 27.216 |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | 37.069 | 35.043 | 34.900 | 35.355 | 35.396 | **35.930** | **37.988** | 37.786 |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | 8.973 | 7.940 | 7.982 | 7.702 | 7.974 | **7.729** | **8.606** | 8.409 |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | 8.354 | 8.626 | 8.775 | 8.905 | 8.695 | **8.877** | **9.313** | 8.906 |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | 141.194 | 106.440 | 107.974 | 107.635 | 107.950 | **106.522** | **115.674** | 113.238 |
| `image/decode-png-800x600` | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 6.495 | 6.506 | 6.524 | 6.551 | 7.975 | **6.399** | **6.699** | 6.661 |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** | **305.461** | **307.687** | **309.478** | **309.460** | **306.611** | **330.650** | 323.023 |


### Ronda 28 → ronda 29, fila a fila

A/B prístino intercalado r28 (`cb7af43`) vs r29 (`8f9cfa9`), 5 pares, load 0,93–0,96, performance: paridad o mejoras (pipeline −0,5 %, css −1,7 %, layout −1,6 %, display −2,0 %, render-to-png −1,6 %, html +0,7 %, decode-png −0,3 %).

| Benchmark | R28 (ms) | R29 (ms) | Δ |
|---|---:|---:|---:|
| `css/parse-10k-rules` | 14.241 | **13.995** | −1,7 % |
| `css/resolve-styles-10k-nodes` | 27.508 | **27.216** | −1,1 % |
| `html/realistic-1mb` | 17.613 | **17.175** | −2,5 % |
| `image/decode-png-800x600` | 6.699 | **6.661** | −0,6 % |
| `layout/realistic-1mb` | 37.988 | **37.786** | −0,5 % |
| `paint/build-display-list` | 8.606 | **8.409** | −2,3 % |
| `paint/render-to-png-800x600` | 9.313 | **8.906** | −4,4 % |
| `pipeline/html-to-png-800x600` | 115.674 | **113.238** | −2,1 % |
| `paint/backdrop-fixture-raster` | 1.262 | **1.275** | +1,0 % |
| `paint/backdrop-fixture-png` | 0.802 | **0.816** | +1,7 % |
| **TOTAL legacy** | **330.650** | **323.023** | **−2,3 %** |

## 13. Cómo comparar y detectar regresiones

1. Medir con load < 2 y **perfil `performance`** (`powerprofilesctl`); sin él la
   máquina capa a 1,3–2,2 GHz y todo se infla ~2–3×. Documentar load/freq.
2. Atribuir deltas con A/B de archivos prístinos intercalados (mismo load), como
   §5.2; los absolutos entre ventanas cambian ±5–9 % por energía.
3. Guardar JSON y comparar medianas (script en r25 docs/§13). Regresión si la
   mediana empeora >15 % reproducible.
4. En r29 todos los presupuestos se cumplen salvo el foco decode-png; el A/B
   descarta regresión estructural.
5. Filas: `--filter image`, `--filter paint`, `--filter shell/save-png`,
   `--filter pipeline`, `--filter css/vars-calc`, `--filter paint/clipping`,
   `--filter layout/sticky`.
6. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   phase4 `9e957177…` 42 853 B, `basic.rgba` `60f8984e…` 1 920 000 B; chrome
   `fe196851…` 85 753 B y chrome-menu `c7ccc0e4…` 87 156 B.

---

## 14. Limitaciones y notas

- **Perfil de energía**: se cambió de `quiet`/`power-saver` a **`performance`**
  con `powerprofilesctl set performance` (runtime, reversible) porque el techo de
  frecuencia era 1,3–2,2 GHz; las primeras pasadas de calibración se descartaron.
  r28 y r29 comparten perfil, así que su comparación es homogénea.
- **Streaming de respuestas**: `RenderOptions.stream_responses` es API-only (no
  hay flag CLI); el pipeline sigue buffered y el PNG es byte-idéntico, por lo que
  no hay coste medible sin tocar código. Documentado como canal futuro.
- **WebP lossy**: solo contenedor/cabecera experimental; render lo degrada con
  warning no fatal (no píxeles falsos). Coste medido de placeholder no equivale
  a decode real.
- **Import maps/SMIL/favicons**: fixtures mínimas; el coste escala con el grafo
  de módulos/SVG real, no con la fixture.
- **`--chrome`/chrome-menu**: los md5 de chrome no son goldens de página (el
  chrome incluye favicon/dot determinista); los goldens sin chrome no cambian.
- **`website/` y `pages.yml`**: del dueño; no se tocan.
- **Fast path de tablas (r7)**: nota histórica; no hay commit compilable con el
  camino avanzado sin fast path.
