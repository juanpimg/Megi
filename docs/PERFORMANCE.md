# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 17:12–17:20 (Europe/Madrid). Ronda 9:
  ejecución de `<script>` en el pipeline `render` (con re-render del DOM
  mutado), navegación/enlaces/títulos dinámicos en el shell, API **opt-in** de
  hit-testing en `paint` y sincronización documental.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `c86682c`**
  (`feat(render): execute page scripts and re-render the mutated DOM`).
- **Estado del árbol**: **limpio durante toda la ventana** (`worktree_dirty =
  false` en las 3 pasadas). Otro agente actualizó el resto de documentación en
  paralelo, pero no había cambios de código sin commitear: los binarios medidos
  corresponden a `c86682c`.
- **Binario de bench medido**: `target/release/bench`,
  `sha256 = 43f7748d…` (4 278 144 B) en las 3 pasadas JSON y la pasada del
  script (`TOTAL 480.105 ms`). Como el `target/` es compartido entre agentes, el
  binario se recompiló después (`871e136e…`, mismo código): de ahí que los
  shas de artefactos no coincidan con el `sha256sum` actual.
- **Artefactos guardados** (gitignored):
  - `target/bench-r9-run1..3.json` (shas `199c7ce0…`, `13a36d0f…`,
    `95cd05f1…`) y `target/bench-aggregate-r9.json`
    (`sha256 = fb25b8a3…`).
  - `target/bench-latest.txt` / `-meta.txt` (pasada del script;
    `sha256 = c81272ed…`).
  - `target/bench-js-r9/` — matriz de los 7 fixtures JS con/sin scripting:
    JSON de `render --json`, `.summary.json` y muestras de pared por fixture.
  - `target/perf-r9-render-timings2.json`, `target/perf-r9-cargo-test.json`,
    `target/perf-r9-test-cold.json`, `target/perf-r9-cold-build.json`.

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §11 (rondas 2–8).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Todas las cargas se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`). La misma `--scale` produce siempre el mismo
flujo de bytes. El *fixture* se construye **fuera** del cronómetro; los
benchmarks de parse incluyen la construcción y liberación del `Document`.

**El crate `bench` no cambió en esta ronda**, así que las diferencias r8 → r9
son de condiciones del host o del código del motor. `bench` solo depende de
`dom/html/css/layout/paint` (y `fonts` opcional): **no enlaza `net`, `js`,
`image` ni `shell`**, no ejecuta JavaScript y no usa la API de hit-testing
(opt-in, §5.2). Sus generadores no contienen `url(...)`/`background-image`.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- La tabla de §3 es la **mediana, por fila, de 3 pasadas completas**. Rango =
  mínimo–máximo de las 3 medianas. Una cuarta pasada canónica con
  `./tools/bench.sh` dio `TOTAL 480.105 ms` (vs 485.132 de la mediana de las 3
  pasadas JSON).
- `setup_ms` de las pasadas: 254,5–258,1 ms (no cronometrado).
- Ventana: **load average 2,91–3,35** en las 3 pasadas del bench; builds y
  medidas de pared hasta load 9,5 (otros agentes compilando en paralelo). Los
  benchmarks son monohilo y la máquina tiene 20 hilos lógicos; aun así las
  medidas de pared con load alto se reportan con rango.
- **Matriz JS**: `render --json` una vez por fixture/modo para extraer
  `timings.scripting`, `scripts_executed`, `timings_us.total` y `png_bytes`; el
  tiempo de pared es la mediana de **7 ejecuciones intercaladas** por modo con
  el propio binario `target/release/render` (se excluye el overhead de
  `cargo run`). Load 3–5 en esa sub-ventana.
- Las medidas de render/shell usan cronómetro de pared de Python (11
  ejecuciones intercaladas, mediana; el entorno no tiene `/usr/bin/time`).
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` por defecto |
| Carga de fondo | bench: 2,91–3,35; builds/render: 3–9,5 |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Pasada canónica (compila release + ejecuta + guarda target/bench-latest.txt)
./tools/bench.sh

# Las 3 pasadas usadas para la tabla de esta baseline
for i in 1 2 3; do
  target/release/bench --json > "target/bench-r9-run$i.json"
done

# Smoke rápido (~1 s)
./tools/bench.sh --scale 0.1 --iterations 2

# Coste de JS en el pipeline (cada fixture, con y sin scripting)
for f in js_text js_create js_external js_document_write js_throw js_loop js_console; do
  target/release/render "tests/fixtures/render/$f.html" "/tmp/$f.png" --json \
      > "target/bench-js-r9/$f.scripting.json"
  target/release/render "tests/fixtures/render/$f.html" "/tmp/$f-nojs.png" --json \
      --no-scripting > "target/bench-js-r9/$f.noscripting.json"
done

# Render del showcase 1024x768 y shell headless
target/release/render tests/fixtures/render/showcase/index.html /tmp/showcase.png \
    --width 1024 --height 768 --json
target/release/shell  # (el binario se llama megi)
target/release/megi tests/fixtures/render/showcase/index.html \
    --screenshot /tmp/showcase.png --width 1024 --height 768
```

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb` | Parse de documento «realista» de ~1 MiB (incluye 1 044 tablas 3×3). |
| html | `html/nested-div-10k/32k/64k` | Parse de `<div>` anidados. |
| html | `html/nested-div-64k-default-limits` | Mismo input con límites anti-DoS (profundidad 4096). |
| html | `html/massive-end-p` | 100 000 `</p>` consecutivos. |
| html | `html/massive-formatting` | 10 000 bloques `<b><i><u>` mal anidados. |
| html | `html/tag-40k-attrs` | Un elemento con 40 000 atributos. |
| html | `html/pretty-5k-blocks` | Documento indentado con 5 000 bloques. |
| css | `css/parse-10k-rules` | Parse de hoja CSS con 10 000 reglas. |
| css | `css/resolve-styles-2k/10k-nodes` | Cascada + `ComputedStyle` sobre ~2 005 / ~10 005 elementos. |
| css | `css/selector-match-simple` | 4 selectores simples sobre 10 005 elementos. |
| css | `css/selector-match-descendant` | 4 selectores con combinadores sobre 10 005 elementos. |
| layout | `layout/realistic-1mb` | `layout_document` sobre el documento realista (28 195 elementos, 88 155 cajas), 800×600. Fast path de tablas. |
| paint | `paint/build-display-list` | Display list: 34 158 items (sin hit regions: opt-in, §5.2). |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` DEFLATE (57 422 B). |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo: parse → styles → layout → display list → raster → PNG. |
| fonts | (`feature fonts`) | No re-medido en r9 (sin cambios en `crates/fonts`; referencia r6: measure 0,456 ms, shape 8,119 ms). |

**Nuevos en esta ronda (fuera del bench)**: matriz de ejecución JS sobre los 7
fixtures `js_*.html` (§8.1); `paint` ganó una API de hit-testing **aditiva y
opt-in** que el bench no invoca.

---

## 3. Baseline vigente (2026-11-10, HEAD `c86682c`)

Mediana por fila de **3 pasadas completas** (5 iteraciones, 1 warm-up; *heavy*
3). `setup`: 254,5–258,1 ms. Binario `sha256 = 43f7748d…`. La suma de medianas
da `TOTAL 485.132 ms`; la pasada del script dio `TOTAL 480.105 ms`.

| Benchmark | Carga | mediana (ms) | rango 3 pasadas (ms) | throughput |
|---|---|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | **55.316** | 54.292–56.627 | 18,1 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | **3.662** | 3.604–3.685 | 28,7 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | **11.611** | 11.587–11.769 | 28,9 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | **24.022** | 23.575–24.085 | 28,0 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | **13.963** | 13.740–13.995 | — (59 906 start tags descartados) |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | **23.729** | 23.382–23.735 | 4,21 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | **15.113** | 15.045–15.204 | 662 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | **11.645** | 11.385–11.737 | 3,43 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | **25.863** | 25.482–26.182 | 193 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | **13.698** | 13.456–13.786 | 43,5 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | **5.940** | 5.896–5.971 | 338 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | **28.241** | 28.053–28.243 | 354 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | **0.666** | 0.653–0.668 | 60,1 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | **1.228** | 1.227–1.257 | 32,6 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | **55.140** | 54.885–55.843 | 511 k elementos/s |
| `paint/build-display-list` | 34 158 items | **14.050** | 13.875–14.424 | — |
| `paint/rasterize-800x600` | 34 158 items → pixmap 800×600 | **0.881** | 0.878–0.903 | 545 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (57 422 B) | **12.283** | 12.229–12.295 | 4,46 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | **168.081** | 167.688–168.383 | — |
| **TOTAL (suma)** | | **485.132** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página.

---

## 4. Complejidad observada

Escalados con el mismo binario, complementarios a §3:

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.66 ms · 32k: 11.61 ms · 64k: 24.02 ms | **Lineal**: 3,2× input → 3,17×; 2× → 2,07×. |
| `html/tag-40k-attrs` | 10k: 2.66 ms · 20k: 6.50 ms · 40k: 11.65 ms | **Lineal** (~0,29 µs/atributo). |
| `css/parse-10k-rules` | 5k: 6.69 ms · 10k: 13.70 ms · 20k: 30.50 ms | **Lineal** (≈2× por cada 2×). |
| `css/resolve-styles-2k/10k` | 2 005 el: 5.94 ms · 10 005 el: 28.24 ms | **Lineal**: 5× elementos → 4,75× tiempo (~354 k el/s). |
| `paint/build-display-list` | 24 762 items (r7): 12.46 · 34 158: 14.05 ms | **+21 % acumulado por los 9 396 items de texto de tabla** (~0,28 µs/item); estable desde r8. |
| `paint/rasterize-800x600` | 34 158 items: 0.881 ms | **Plano respecto al nº de items**: lo fija el viewport. |
| `layout/realistic-1mb` | 28 195 elementos → 55.14 ms (88 155 cajas) | **−7,7 % vs r8**, dentro de la variación entre ventanas. |

---

## 5. Ronda 9: JS en el pipeline, hit-testing opt-in y estabilidad del bench

### 5.1 Qué entró en la ronda

- `c86682c feat(render): execute page scripts and re-render the mutated DOM`:
  el pipeline `render` crea un `JsRuntime`, ejecuta `<script>` inline/externo y,
  si el DOM cambió, **re-renderiza** (nueva cascada + layout + paint) antes de
  pintar. `--no-scripting` desactiva todo (DOM intacto). La etapa `scripting`
  del JSON mide creación del runtime + ejecución (sin el re-render).
- `4ac56ba feat(shell): clickable links, navigation history and dynamic
  titles`: shell usa la nueva API de hit-testing; añade navegación e historial.
- `429a30f feat(paint): aditive hit-testing API with paint-order regions`:
  API **opt-in** (`build_display_list_with_hit_regions`,
  `build_display_list_and_hit_regions`, `hit_test`/`find_link_at`). Las
  funciones históricas (`build_display_list`, `…with_images_and_backgrounds`)
  no cambian.
- Sincronización documental (`40817d9` y el resto de docs de la ronda).

### 5.2 El bench no paga hit-regions ni JS

- El bench llama `paint::build_display_list` (sin hit regions): verificado en
  `crates/bench/src/benches.rs`; el render llama
  `build_display_list_with_images_and_backgrounds`, tampoco las construye. Por
  tanto la fila `paint/build-display-list` no incluye el coste de hit-regions,
  que es **opt-in** y solo lo activa el shell.
- `bench` **no** enlaza `js` ni ejecuta scripts: el bench no tiene coste JS de
  runtime. La fila `pipeline/html-to-png-800x600` es el pipeline *sin*
  scripting (`crates/bench` no depende de render).

### 5.3 Estabilidad del bench r8 → r9

| Métrica | r8 | r9 | Δ |
|---|---:|---:|---:|
| items de display list | 34 158 | 34 158 | 0 |
| PNG del bench | 57 422 B | 57 422 B | 0 |
| `paint/build-display-list` | 15.090 ms | 14.050 ms | −6,9 % |
| `paint/rasterize-800x600` | 0.993 ms | 0.881 ms | −11,3 % |
| `pipeline/html-to-png-800x600` | 179.699 ms | 168.081 ms | −6,5 % |
| **TOTAL (suma)** | **512.440** | **485.132** | **−5,3 %** |

La mejora es **sistemática en todas las filas** (incluidas HTML/CSS, que no se
tocaron en r9) y el crate `bench`/generadores no cambió: se atribuye a las
condiciones del host (frecuencia/térmica; r8 se midió tras una tormenta de
compilaciones y con más presión térmica, mientras que r9 se tomó con el árbol
limpio y carga similar pero menos sostenida). **No hay regresión**: el código
nuevo de la ronda (hit regions, scripting) no participa en las filas del bench,
como se detalla en §5.2. Para la comparación histórica (§11) se usan las cifras
de cada ventana; la varianza entre ventanas es ~±5 %.

---

## 6. Compresión PNG (bench)

| Métrica | r7 | r8 | r9 | Notas |
|---|---:|---:|---:|---|
| PNG del bench (documento realista 800×600) | 54 526 B | 57 422 B | **57 422 B** | sin cambio en r9 |
| `paint/render-to-png-800x600` | 13.045 ms | 13.296 ms | **12.283 ms** | encoder + raster |
| — rasterizar | 0.843 ms | 0.993 ms | **0.881 ms** | mismo display list |
| — codificar PNG (por diferencia) | ~12,2 ms | ~12,3 ms | **~11,4 ms** | sigue siendo el mayor coste de pintado |

---

## 7. Pipeline `render` y shell headless (ronda 9)

### 7.1 Showcase 1024×768 (golden sin cambios en r9)

`target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png
--width 1024 --height 768 --json`; **11 ejecuciones** intercaladas con `basic`,
mediana de pared (load 4,46). Golden verificado byte a byte:

- PNG **80 220 B**, **md5 `c95c7eff83e7bd08f815631aa42afdb0`**
  (`sha256 0fc70eae…`), 1024×768, opaco. Sin cambios respecto a r8: el showcase
  no tiene `<script>` (`scripts_executed = 0`).
- Metadatos: `@import` 1, hojas externas 1, hojas saltadas por media 1,
  `<img>` 2, fondos 2 (185 B), items 80, `document_bytes` 2 383,
  `css_bytes` 4 163.

| Etapa (single run, µs) | fetch | decode/parse | styles | images | backgrounds | scripting | layout | paint | raster | **total** |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| r9 showcase | 14 | 200 | 595 | 100 | 33 | **9** | 1 472 | 20 | **9 282** | **28 941** |
| r8 (referencia) | 15 | 178 | 570 | 98 | 35 | — | 1 466 | 22 | 9 475 | 29 679 |

| Variante | pared (ms) | rango (ms) | PNG |
|---|---:|---:|---:|
| **showcase 1024×768 (render, 11 runs)** | **31,23** | 30,05–32,74 | 80 220 B (golden) |
| `basic.html` 800×600 | **15,00** | 13,79–16,36 | 21 090 B (`e943565d…`) |
| `images.html` 800×600 | — | — | 15 625 B (2 tags `<img>`: 1 OK, 1 fallback) |
| `backgrounds.html` 800×600 | — | — | 15 778 B (2 fondos) |

El total de etapas baja ligeramente (28,94 vs 29,68 ms en r8) y el raster
también (−193 µs); la pared sube ~0,8 ms por ruido de arranque/escritura y
scheduling. El **PNG+driver sigue siendo ~59 %** del total (17,2 de 28,9 ms).

### 7.2 Shell headless (`megi --screenshot`, mismo pipeline, sin ventana)

| Fixture | pared (ms) | rango | PNG |
|---|---:|---:|---:|
| showcase 1024×768 | **32,02** | 29,97–36,07 | 80 220 B (golden, byte-idéntico a `render`) |
| basic.html 800×600 | **15,80** | 13,57–20,48 | 21 090 B |

Rangos altos por load 3–5 de agentes en paralelo (carga del binario de 22 MB y
arranque del proceso incluidos). El shell **no** crea ventana ni `EventLoop` en
`--screenshot`.

---

## 8. Coste de la Fase 6 (build, binario y **ejecución real de JS**)

### 8.1 Ejecución de JS en el pipeline (nuevo en r9)

7 fixtures `tests/fixtures/render/js_*.html`; por modo: 1 run `--json` + **7
ejecuciones intercaladas** de pared con el binario release (media de la mediana).
`timings.scripting` = crear runtime + ejecutar scripts (sin re-render).

| Fixture | pared JS (ms) | pared sin JS (ms) | Δ pared | `timings.scripting` | scripts OK | Δ PNG | Qué hace |
|---|---:|---:|---:|---:|---:|---:|---|
| `js_text` | 15,42 | 14,75 | **+0,67** | 698 µs | 1 | **+377 B** | cambia texto/estilos del DOM |
| `js_create` | 14,68 | 13,43 | **+1,25** | 698 µs | 1 | +7 B | `createElement` + `appendChild` |
| `js_external` | 16,44 | 14,62 | **+1,82** | 704 µs | 1 | **+1 112 B** | `<script src>` externo |
| `js_document_write` | 14,73 | 13,83 | **+0,91** | 561 µs | 1 | **+2 948 B** | `document.write` |
| `js_throw` | 15,92 | 14,08 | **+1,84** | 553 µs | 0 | 0 B | excepción: DOM intacto |
| `js_loop` | 16,41 | 14,43 | **+1,98** | 1 965 µs | 0 | 0 B | bucle infinito cortado por **budget** (100k iters) |
| `js_console` | 14,91 | 13,63 | **+1,27** | 655 µs | 1 | 0 B | `console.*` (DOM intacto) |

**Lectura**:

- Ejecutar JS en una página cuesta **≈0,55–0,7 ms de scripting** (fixtures
  normales) y **≈2,0 ms** cuando el script agota el budget anti-DoS (100 000
  iteraciones). El Δpared mediano es **+1,27 ms** (0,7–2,0 ms) porque el
  pipeline re-renderiza si el DOM cambió (Tabla §5.1) y por el arranque del
  realm; en los fixtures que no mutan el DOM (`js_throw`, `js_console`,
  `js_loop`) el ΔPNG es 0.
- Es un **2–7 % del render de una página pequeña** (~14–16 ms pared) y
  despreciable frente al pipeline del showcase (30 ms, sin scripts). El coste
  dominante del pipeline sigue siendo **PNG+driver (~17 ms)**.
- `js_loop` demuestra que el budget funciona: el script no bloquea el render y
  el coste está acotado (~2 ms en este caso).
- Artefactos: `target/bench-js-r9/{fixture}.{scripting,noscripting}.json`,
  `.summary.json` y `.times` por muestra.

### 8.2 Build y CI

| Comando | Tiempo | Notas |
|---|---:|---|
| `cargo build --release --workspace` (frío, `CARGO_TARGET_DIR` vacío) | **37,3 s** | incluye Boa, winit, rustls/net, shell y todos los crates (r8: 43,3 s; varianza de host) |
| `cargo build --release -p render` (frío, feature `js`) | **33,0 s** | enlaza Boa; 18,03 MiB de binario |
| `cargo build --release -p render --no-default-features` (frío) | **10,4 s** | sin `js`; 6,16 MiB |
| `cargo test --workspace` (frío, target vacío) | **31,8 s** | **70 suites, 793 tests**, todo OK; 4,2 GB de target |
| `cargo test --workspace` (incremental, target compartido) | **8,1 s** | 70 suites, 793 tests, todo OK |

**Lectura**: la feature `js` añade **≈22,7 s de compilación en frío** al crate
`render` (33,0 s vs 10,4 s) porque arrastra Boa; es el mayor coste individual de
build de la Fase 6. Para CI: **≈37 s de build release frío** y **≈32 s de
`cargo test --workspace` frío** en esta máquina de 20 hilos (±varianza de host);
en runners de 2–4 núcleos será varias veces mayor por el paralelismo de Boa.

### 8.3 Binarios (r9 ya enlazan Boa)

| Binario | r9 | r8 | Δ |
|---|---:|---:|---:|
| `target/release/render` (con JS) | **18,03 MiB** (18 907 360 B) | 6,15 MiB (6 454 664 B) | **+11,87 MiB** |
| `target/release/megi` (shell, vía render) | **21,89 MiB** (22 955 984 B) | 9,99 MiB (10 476 672 B) | **+11,90 MiB** |
| `target/release/render --no-default-features` | 6,16 MiB (6 460 840 B) | — | — |
| `target/release/bench` | 4,08 MiB (4 279 032 B) | 4,08 MiB | ≈0 |

**El coste de enlazar Boa es ≈+11,87 MiB** de binario (render con `js` vs sin
`js`, misma build/código): pasa a ser el mayor sobrecoste de distribución de la
Fase 6 (el binario casi se triplica). `megi` lo hereda al depender de `render`.
`bench` no enlaza JS y no cambia de tamaño. `sha256` de la ventana:
`render ca23f2aa…`, `megi 8d48ce49…`.

### 8.4 Runtime del motor JS (arranque/eval)

La crate `js` **no cambió entre r8 y r9** (sin commits en el rango), así que se
mantienen las cifras de la sonda r8: `JsRuntime::new()` **0,096 ms** (mediana;
mín. 0,066), `eval("1+1")` en realm caliente **0,002 ms**, arranque+primer eval
**0,084 ms**; script de 10 000 iteraciones 5,19 ms (budgets). El coste de Fase 6
está en **build, CI y tamaño de binario**, no en el arranque del runtime; el
coste real de ejecución en una página está medido en §8.1.

---

## 9. Coste de floats, posicionamiento, stacking y opacidad (referencia r4)

Medido en r4 con el driver `html_to_png` sobre 2 000 cajas (15 ejecuciones
intercaladas; overhead del driver ~11,2 ms):

| Variante | Δ vs control | Factor |
|---|---:|---:|
| `position: relative` | +0,93–1,13 ms | 1,02× |
| `position: relative; z-index` | +0,20–0,59 ms | 1,00–1,01× |
| `opacity: 0.85` | +2,96–3,55 ms | 1,05–1,06× |
| `float: left` | +12,54–12,72 ms | **1,22×** |

El documento realista del bench no usa floats ni opacidad; no interviene en §3.

---

## 10. Presupuestos orientativos

Rasterizador software, monohilo, sin GPU. Objetivo global: **pipeline ≤ 200 ms**
y **render showcase ≤ 35 ms**.

| Benchmark / medida | Baseline r9 | Presupuesto | Estado |
|---|---:|---:|---|
| `html/realistic-1mb` | 55 ms | ≤ 100 ms | ✅ (55 %) |
| `html/nested-div-64k` | 24 ms | ≤ 100 ms | ✅ |
| `html/massive-end-p` | 24 ms | ≤ 50 ms | ✅ |
| `html/tag-40k-attrs` | 12 ms | ≤ 50 ms | ✅ |
| `html/pretty-5k-blocks` | 26 ms | ≤ 75 ms | ✅ |
| `css/parse-10k-rules` | 14 ms | ≤ 30 ms | ✅ |
| `css/resolve-styles-10k-nodes` | 28 ms | ≤ 75 ms | ✅ |
| `layout/realistic-1mb` | 55 ms | ≤ 150 ms | ✅ |
| `paint/build-display-list` | 14,1 ms | ≤ 20 ms | ✅ (**70 %**) |
| `paint/rasterize-800x600` | 0,88 ms | ≤ 25 ms | ✅ |
| `paint/render-to-png-800x600` | 12,3 ms | ≤ 50 ms | ✅ |
| `pipeline/html-to-png-800x600` | **168,1 ms** | **≤ 200 ms** | ✅ (**84,0 %**, margen 31,9 ms) |
| `render` showcase 1024×768 | **31,2 ms** | **≤ 35 ms** | ✅ (**89,2 %**, margen 3,8 ms) |
| shell headless showcase | **32,0 ms** | **≤ 35 ms** | ✅ (**91,5 %**, margen 3,0 ms) |
| JS scripting por página (7 fixtures) | 0,7–2,0 ms de pared | — | ✅ (2–7 % del render) |

Todos los presupuestos se cumplen. El pipeline gana margen (84 % vs 90 % en
r8); en cambio, el **render/shell del showcase queda al 89–92 %**, el más
ajustado: si el pipeline de render crece (p. ej. fuentes reales o más JS), el
presupuesto de 35 ms es el primero que habría que revisar.

---

## 11. Comparación histórica

Ocho baselines en la misma máquina, mismo arnés:

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 (vigente) |
|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | **17:12** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | **`c86682c`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | limpio | docs dirty | **limpio** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | **`43f7748d…`** |
| Load (bench) | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | **2,91–3,35** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | **55.316** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | 25.658 | 25.084 | 24.772 | **24.022** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | **28.241** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | **55.140** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | **14.050** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | 0.843 | 0.993 | **0.881** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | **12.283** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | **168.081** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** |

### Ronda 8 → ronda 9, fila a fila

| Benchmark | R8 (ms) | R9 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 56.140 | **55.316** | −1,5 % |
| `html/nested-div-10k` | 3.811 | **3.662** | −3,9 % |
| `html/nested-div-32k` | 12.128 | **11.611** | −4,3 % |
| `html/nested-div-64k` | 24.772 | **24.022** | −3,0 % |
| `html/nested-div-64k-default-limits` | 14.949 | **13.963** | −6,6 % |
| `html/massive-end-p` | 23.879 | **23.729** | −0,6 % |
| `html/massive-formatting` | 16.292 | **15.113** | −7,2 % |
| `html/tag-40k-attrs` | 13.187 | **11.645** | −11,7 % |
| `html/pretty-5k-blocks` | 26.800 | **25.863** | −3,5 % |
| `css/parse-10k-rules` | 14.941 | **13.698** | −8,3 % |
| `css/resolve-styles-2k-nodes` | 6.097 | **5.940** | −2,6 % |
| `css/resolve-styles-10k-nodes` | 28.662 | **28.241** | −1,5 % |
| `css/selector-match-simple` | 0.665 | **0.666** | +0,2 % |
| `css/selector-match-descendant` | 1.272 | **1.228** | −3,5 % |
| `layout/realistic-1mb` | 59.767 | **55.140** | −7,7 % |
| `paint/build-display-list` | 15.090 | **14.050** | −6,9 % |
| `paint/rasterize-800x600` | 0.993 | **0.881** | −11,3 % |
| `paint/render-to-png-800x600` | 13.296 | **12.283** | −7,6 % |
| `pipeline/html-to-png-800x600` | 179.699 | **168.081** | −6,5 % |
| **TOTAL (suma)** | **512.440** | **485.132** | **−5,3 %** |

La mejora es homogénea y afecta a filas que no cambiaron de código (HTML/CSS),
por lo que se atribuye a condiciones del host (térmica/frecuencia), no a una
optimización de r9; los cambios r9 medidos fuera del bench están en §5 y §8
(scripting añade 0,7–2,0 ms por página; hit-regions no participa).

---

## 12. Cómo comparar y detectar regresiones

1. Medir antes/después en la misma máquina y con el sistema ocioso
   (`uptime` < ~1.5); misma `--scale` e iteraciones.
2. Guardar el JSON y comparar medianas:

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
3. Regresión si la mediana empeora >15 % de forma reproducible; dispersión
   típica aquí ~1–6 % intra-ventana y hasta ±5–10 % entre ventanas.
4. Para complejidad, usar `--scale 0.5` vs `1.0`: ratios ≈1.
5. Suite completa ~2,8 s + ~0,26 s de fixtures; smoke
   `--scale 0.1 --iterations 2` ~1 s.
6. Para el pipeline, además del bench: `render --json` (etapas en µs), la
   matriz JS de §8.1 y el golden showcase (`md5 c95c7eff…`); un cambio de golden
   exige justificar el diff de píxeles en el commit.

---

## 13. CI

Se ejecuta a mano (`./tools/bench.sh`); no hay job para no tocar
`.github/workflows/`. Es barata y sin dependencias externas:

```yaml
name: Benchmarks (manual)
on:
  workflow_dispatch:
jobs:
  bench:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: ./tools/bench.sh --scale 0.25 --iterations 2
```

Coste de referencia de CI en esta máquina (20 hilos, §8.2):
`cargo build --release --workspace` frío **37,3 s**; `cargo test --workspace`
frío **31,8 s** (70 suites, 793 tests). Los números de CI no son comparables
(otra CPU, runners compartidos); el job solo comprueba que compila y ejecuta, y
deja el JSON como artefacto.

---

## 14. Limitaciones y notas

- **Baseline sobre commit limpio**: `c86682c`, `worktree_dirty = false` en las 3
  pasadas. El `target/` es compartido: `target/release/bench` se recompiló
  después con la misma fuente (`43f7748d…` → `871e136e…`), por lo que el
  `sha256sum` actual del binario puede no coincidir con el de la tabla.
- **Host compartido**: load 2,91–3,35 en el bench y hasta 9,5 en builds/render
  (otros agentes). Variabilidad intra-ventana ±1–6 %; entre ventanas, hasta
  ±5–10 % (la mejora r8 → r9 es sistemática y se atribuye al host, §5.3).
  Las cifras absolutas no son comparables con CI.
- **`bench` no cubre todo el motor**: no enlaza `net`, `js`, `image`, `shell` ni
  hit regions (opt-in) y sus generadores no tienen `url(...)`; el coste JS se
  mide con la matriz de fixtures de §8.1 y los fondos/render en §7.
- **Matriz JS**: los fixtures resuelven `js_external` por `file://` (sin red);
  el tiempo de pared excluye `cargo run` (se usa el binario release) pero
  incluye arranque de proceso y escritura del PNG (~0,7–1 ms). Los fixtures que
  mutan el DOM fuerzan un re-render completo, que no está descontado del Δpared
  (el JSON sí separa `timings.scripting`).
- **Render/shell showcase**: rangos amplios por load de agentes; el golden y
  los metadatos de etapas se validaron byte a byte/idénticos en cada corrida.
  Las build dirs temporales (`target/perf-r9-*`, ~5,5 GB) se borraron tras
  extraer cifras; se conservan los JSON de medidas en `target/`.
- **Hit regions**: API aditiva y opt-in; solo el shell la usa. El bench y el
  render por defecto no la invocan (verificado en código), así que no hay coste
  en las cifras de §3.
- **Fase 6**: medido el scripting del pipeline con 7 fixtures pequeños; no hay
  todavía una página JS «real» de referencia ni un bench end-to-end con JS en
  `crates/bench`. Es el siguiente foco natural junto con el encoder PNG (~58 %
  del pipeline de render).
- **Fast path de tablas (r7)**: la comparación 67,2 → 61,4 ms usa el valor
  intermedio documentado por el commit de layout; no hay un commit compilable
  con el camino avanzado sin fast path.
