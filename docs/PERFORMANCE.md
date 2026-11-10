# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 16:18–16:30 (Europe/Madrid). Ronda 7:
  fast path de tablas + rowspan/fixed/inline-table/collapsed borders, fix del
  rasterizador (underflow de cobertura parcial), `@import` recursivo y media
  queries reales, showcase 1024×768, shell con scroll/worker/HiDPI.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `c002ec0`**
  (`feat(render): recursive @import, real media evaluation and a showcase golden`).
- **Estado del árbol**: **limpio al inicio** (`git status` vacío; el binario se
  compiló de ese commit). Durante la ventana otro agente actualizó documentación
  (`README`, `docs/*`, imágenes); no afecta al código medido.
- **Binario medido**: `target/release/bench`,
  `sha256 = 687627e09221f28e186a5fb23e31f1b8a9e6d7e6837fc22cda11a6fd47e489e9`,
  4 272 544 bytes, compilado con `cargo build --release -p bench`.
- **Artefactos guardados** (gitignored, no versionados):
  - `target/bench-run1.json` … `target/bench-run5.json` — las 5 pasadas
    completas usadas para la tabla (mediana por fila).
  - `target/bench-aggregate.json` — medianas agregadas.
  - `target/bench-latest.txt` / `.json` / `-meta.txt` — pasada del script
    (`TOTAL 507.045 ms`) y su metadata.
  - `target/bench-render-r7/` — showcase 1024×768 (`showcase-1024x768.json`,
    PNGs), `@import`/imágenes variantes, `render-results.json` y
    `shell-results.json`.

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §10 (rondas 2–6).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Todas las cargas se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`). La misma `--scale` produce siempre el mismo
flujo de bytes. El *fixture* se construye **fuera** del cronómetro; los
benchmarks de parse incluyen la construcción y liberación del `Document`.

**El crate `bench` no cambió en esta ronda**, así que todas las diferencias
r6 → r7 son de código del motor.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- La tabla de §3 es la **mediana, por fila, de 5 pasadas completas**. Rango =
  mínimo–máximo de las 5 medianas.
- Ventana canónica: load average **0,99–1,52** sobre 20 hilos.
- El binario se reconstruyó una vez durante la ventana (cargo recompila
  `bench` al cambiar dependencias): la tabla corresponde a la build
  `sha256 = 687627e0…`; una build intermedia (`bf9700af…`, mismo código) dio
  cifras dentro de ±1,5 %.
- Las medidas del CLI `render`/`megi` usan cronómetro de pared de Python
  (7–15 ejecuciones, mediana; el entorno no tiene `/usr/bin/time`), load
  1,1–1,5.
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Carga de fondo | load average 0,99–1,52 durante la ventana canónica |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Pasada completa (compila release + ejecuta + guarda target/bench-latest.txt)
./tools/bench.sh

# Las 5 pasadas usadas para la tabla de esta baseline
for i in 1 2 3 4 5; do
  cargo run --release -p bench -- --json > "target/bench-run$i.json"
done

# Smoke rápido (~1 s)
./tools/bench.sh --scale 0.1 --iterations 2

# Un grupo o un benchmark concreto
./tools/bench.sh --filter layout
cargo run --release -p bench -- --filter layout/realistic-1mb

# Render del showcase 1024x768 (fixture real con @import, media e imágenes)
cargo run --release -p render -- \
    tests/fixtures/render/showcase/index.html /tmp/showcase.png \
    --width 1024 --height 768 --json

# Shell headless (mismo pipeline, sin ventana)
cargo run --release -p shell -- \
    tests/fixtures/render/showcase/index.html \
    --screenshot /tmp/showcase.png --width 1024 --height 768
```

`crates/bench` pasa `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings` y `cargo test -p bench` (8 tests).

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb` | Parse de documento «realista» de ~1 MiB (incluye 1 044 tablas). |
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
| layout | `layout/realistic-1mb` | `layout_document` sobre el documento realista (28 195 elementos, 88 155 cajas), 800×600. Usa el **fast path** de tablas. |
| paint | `paint/build-display-list` | Display list (24 762 items). |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` DEFLATE (54 526 B). |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo: parse → styles → layout → display list → raster → PNG. |
| fonts | (`feature fonts`) | No re-medido en r7 (sin cambios en `crates/fonts`; referencia r6: measure 0.456 ms, shape 8.119 ms). |

---

## 3. Baseline vigente (2026-11-10, HEAD `c002ec0` limpio)

Mediana por fila de **5 pasadas completas** (5 iteraciones, 1 warm-up; *heavy*
3). `setup` de fixtures: ~270 ms (no cronometrado). Binario
`sha256 = 687627e0…`. La pasada del script dio `TOTAL 507.045 ms`.

| Benchmark | Carga | iter | mediana (ms) | rango 5 pasadas (ms) | throughput |
|---|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | 5 | **55.926** | 55.186–58.537 | 17.9 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | 5 | **3.897** | 3.847–4.049 | 25.7 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | 3 | **12.492** | 12.236–13.059 | 27.2 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | 3 | **25.084** | 24.578–29.355 | 26.7 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | 5 | **14.625** | 14.509–14.761 | — (59 906 start tags descartados) |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | 5 | **24.656** | 24.050–24.961 | 4.06 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | 5 | **16.382** | 16.097–16.605 | 610 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | 3 | **12.899** | 12.640–13.109 | 3.10 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | 5 | **26.994** | 26.578–27.433 | 185 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | 5 | **14.470** | 13.662–14.919 | 41.5 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | 5 | **6.253** | 6.038–6.355 | 321 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | 5 | **29.853** | 29.370–30.512 | 335 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | 5 | **0.718** | 0.715–0.822 | 55.7 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | 5 | **1.311** | 1.296–1.334 | 30.5 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | 5 | **61.350** | 59.461–63.754 | 460 k elementos/s |
| `paint/build-display-list` | 24 762 items | 5 | **12.458** | 12.022–14.295 | — |
| `paint/rasterize-800x600` | 24 762 items → pixmap 800×600 | 3 | **0.843** | 0.769–0.900 | 569 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (54 526 B) | 3 | **13.045** | 12.747–13.195 | 3.99 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | 3 | **178.791** | 175.596–182.791 | — |
| **TOTAL (suma)** | | | **512.047** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página.

---

## 4. Complejidad observada

Escalados con el mismo binario (1–5 iteraciones), complementarios a §3:

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.90 ms · 32k: 12.49 ms · 64k: 25.08 ms | **Lineal**: 3,2× input → 3,21× tiempo; 2× → 2,01×. |
| `html/tag-40k-attrs` | 10k: 2.66 ms · 20k: 6.50 ms · 40k: 12.46 ms | **Lineal** (~0,3 µs/atributo). |
| `css/parse-10k-rules` | 5k: 6.69 ms · 10k: 14.26 ms · 20k: 30.50 ms | **Lineal** (2,13× y 2,14× por cada 2×). |
| `css/resolve-styles-2k/10k` | 2 005 el: 6.25 ms · 10 005 el: 29.85 ms | **Lineal**: 5× elementos → 4,78× tiempo (~335 k el/s). |
| `paint/rasterize-800x600` | 4 863 items: 0.792 ms · 14 824: 0.863 ms · 24 762: 0.763 ms | **Plano respecto al nº de items** (~556–629 MPix/s): coste ligado al viewport. |
| `layout/realistic-1mb` | 28 195 elementos → 61.35 ms (88 155 cajas) | **+6,2 % respecto a r6** (57,74 ms) por el nuevo pipeline de tablas; el fast path evita el peor caso (ver §5). |

---

## 5. Layout r7: fast path de tablas y fix del rasterizador

### 5.1 Qué entró en la ronda

`5978984 feat(layout): rowspan, fixed table layout, inline-table and collapsed
borders` añadió: `rowspan` con matriz de ocupación, `table-layout: fixed`,
`inline-table` (caja atómica inline con baseline), `border-collapse` con
resolución de conflictos y un **fast path** para tablas «planas»
(`border-collapse: separate`, `table-layout: auto` y sin `rowspan`).

Según el mensaje del propio commit, el camino avanzado completo llevó
`layout/realistic-1mb` a **67,2 ms** y el fast path lo devolvió a **58,98 ms**.
El documento del bench usa exactamente la forma del fast path (1 044 tablas
3×3, sin collapse ni rowspan).

### 5.2 Medición oficial

| Métrica | r6 (HEAD `13e9945`) | r7 (HEAD `c002ec0`) | Δ |
|---|---:|---:|---:|
| `layout/realistic-1mb` (bench, mediana de 5) | 57.742 ms | **61.350 ms** | **+6,2 %** |
| `layout/realistic-1mb` (re-medido en ventana limpia)* | — | 61,7 ms (60,2–64,5) | — |
| `pipeline/html-to-png-800x600` | 173.305 ms | **178.791 ms** | +3,2 % |

\* 5 pasadas filtradas adicionales de `--filter layout`, load 1,25–1,4.

- El fast path **funciona**: sin él, el commit documenta 67,2 ms; con él, la
  medición oficial da ~61,4 ms (−5,8 ms, **−8,7 %** frente al camino avanzado).
- Aun así **no recupera del todo** el valor de r6 (57,7 ms): quedan **+3,6 ms
  (+6,2 %)** atribuibles al nuevo código de tablas (detección de collapse/
  rowspan, parsing de atributos, cambios en `box_tree`/`tree_builder`) y a la
  ventana de carga (r6 se midió a 0,96–1,39; r7 a 0,99–1,52). La diferencia
  entre los 58,98 ms del commit y nuestros 61,35 ms es del mismo orden (~4 %) y
  compatible con su harness/condiciones de medida.
- El árbol de layout no cambia con la optimización: mismo documento, mismas
  88 155 cajas finales (61 599 cajas antes del pase de layout).
- `pipeline/html-to-png` **sigue dentro del presupuesto** (178,8 ms ≤ 200 ms,
  89,4 %; margen 21,2 ms).

### 5.3 Fix del rasterizador

`4d64917 fix(paint): correct partial-coverage underflow in fill_rect` corrige
un underflow en la cobertura parcial (AA) de `fill_rect`. Efecto medido:

- `paint/rasterize-800x600`: **0,890 → 0,843 ms (−5,3 %)** (~0,83 ms en varias
  pasadas), dentro de la expectativa.
- `paint/render-to-png-800x600` sin cambio (13,016 → 13,045 ms): la diferencia
  es el encoder PNG (~12,2 ms), no el rasterizado.
- El golden de `paint` sigue siendo byte-idéntico (según el commit) y el
  showcase produce el golden esperado (§7).

---

## 6. Compresión PNG (bench)

| Métrica | r5–r6 | r7 | Notas |
|---|---:|---:|---|
| PNG del bench (documento realista 800×600) | 54 526 B | **54 526 B** | sin cambio |
| `paint/render-to-png-800x600` | 13,02 ms | **13,05 ms** | estable |
| — rasterizar | 0,890 ms | **0,843 ms** | fix de AA |
| — codificar PNG (por diferencia) | ~12,2 ms | **~12,2 ms** | sigue siendo el mayor coste de pintado |

---

## 7. Pipeline `render` y shell headless (round 7)

### 7.1 Showcase 1024×768 (fixture real: `@import` recursivo, media queries, 2 PNG)

`cargo run --release -p render -- tests/fixtures/render/showcase/index.html
/tmp/s.png --width 1024 --height 768 --json`; mediana de 15 ejecuciones
intercaladas (wall de pared). PNG final: **65 151 B**, `md5
570aa554c88c2fdf75481e8cd076bf61` = golden del repo.

| Variante | pared (ms) | fetch | parse | styles | images | layout | paint | raster | total | PNG | css bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **full** | **25,18** | 23 µs | 256 µs | 626 µs | 114 µs | 1 530 µs | 19 µs | 3 018 µs | 23 641 µs | 65 151 B | 3 574 |
| sin `@import` | 25,31 | 24 | 274 | 721 | 160 | 1 627 | 22 | 2 841 | 23 706 | 65 175 | **3 083** |
| sin imágenes | 24,53 | 23 | 280 | 642 | **4** | 1 568 | 21 | 2 750 | 22 854 | **63 162** | 3 574 |

- **Coste de `@import`** (`theme.css`, 491 B de CSS extra): **no medible por
  encima del ruido** — Δpared −0,12 ms, Δcss_bytes +491 B, Δstyles dentro de la
  variación entre pasadas (la etapa `styles` es ~0,6–0,7 ms). El `@import`
  recursivo y las media queries no son un cuello de botella.
- **Coste de las 2 imágenes** (logo 1 219 B + tile 125 B; `image_bytes=1 344`):
  etapa `images` **114 µs** frente a 4 µs sin imágenes (**+110 µs**); el PNG
  crece **+1 989 B**; Δpared +0,65 ms (dentro del ruido de la ventana).
- **Reparto del pipeline showcase**: rasterizado 3,0 ms (~13 %), layout 1,5 ms
  (~6 %), y **PNG+setup ≈18,1 ms (~76 %)**; fetch/parse/styles/imágenes suman
  ~1,0 ms. El encoder PNG es el coste dominante del pipeline de render.
- `basic.html` (800×600): pared **15,68 ms**, PNG 21 090 B; `images.html`:
  **16,63 ms**, PNG 15 625 B (2 imágenes: 1 OK, 1 fallback).

### 7.2 Shell headless (`megi --screenshot`, mismo pipeline, sin ventana)

| Fixture | pared (ms) | rango | PNG |
|---|---:|---:|---:|
| showcase 1024×768 | **25,63** | 24,11–26,70 | 65 151 B (golden) |
| basic.html 800×600 | **16,29** | 15,72–16,75 | 21 090 B |
| images.html 800×600 | **15,52** | 14,11–16,43 | 15 625 B |

El shell headless no crea ventana ni `EventLoop` y produce **PNG idéntico** al
CLI `render` (y al golden); su sobrecoste frente a `render` es <0,7 ms (carga
del binario + parseo de args + escritura). El worker de la ventana no participa
en `--screenshot` (no hay event loop), así que aquí solo se mide la pared del
proceso.

---

## 8. Coste de floats, posicionamiento, stacking y opacidad (referencia r4)

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

## 9. Presupuestos orientativos

Rasterizador software, monohilo, sin GPU. Objetivo global de la ronda:
**pipeline ≤ 200 ms**.

| Benchmark / medida | Baseline | Presupuesto | Estado |
|---|---:|---:|---|
| `html/realistic-1mb` | 56 ms | ≤ 100 ms | ✅ |
| `html/nested-div-64k` | 25 ms | ≤ 100 ms | ✅ |
| `html/massive-end-p` | 25 ms | ≤ 50 ms | ✅ |
| `html/tag-40k-attrs` | 13 ms | ≤ 50 ms | ✅ |
| `html/pretty-5k-blocks` | 27 ms | ≤ 75 ms | ✅ |
| `css/parse-10k-rules` | 14 ms | ≤ 30 ms | ✅ |
| `css/resolve-styles-10k-nodes` | 30 ms | ≤ 75 ms | ✅ |
| `layout/realistic-1mb` | 61 ms | ≤ 150 ms | ✅ (+6,2 % vs r6) |
| `paint/build-display-list` | 12,5 ms | ≤ 20 ms | ✅ |
| `paint/rasterize-800x600` | 0,84 ms | ≤ 25 ms | ✅ (fix AA) |
| `paint/render-to-png-800x600` | 13,0 ms | ≤ 50 ms | ✅ |
| `pipeline/html-to-png-800x600` | **178,8 ms** | **≤ 200 ms** | ✅ (**89,4 %**, margen 21,2 ms) |
| `render` showcase 1024×768 | 25,2 ms | ≤ 35 ms | ✅ |
| shell headless showcase | 25,6 ms | ≤ 35 ms | ✅ |

Todos los presupuestos se cumplen. El margen del pipeline (21 ms) es el más
ajustado y depende de layout (61,4 ms) y parse (55,9 ms), que juntos son el
**65,6 %** del pipeline.

---

## 10. Comparación histórica

Seis baselines en la misma máquina, mismo arnés:

| | R2 | R3 | R4 | R5 | R6 | R7 (vigente) |
|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | **16:18** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | **`c002ec0`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | **limpio al inicio** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | **`687627e0…`** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 |
|---|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | **55.926** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | 25.658 | **25.084** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | **29.853** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | **61.350** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | **12.458** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | **0.843** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | **13.045** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | **178.791** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** |

### Ronda 6 → ronda 7, fila a fila

| Benchmark | R6 (ms) | R7 (ms) | Factor |
|---|---:|---:|---|
| `html/realistic-1mb` | 58.738 | **55.926** | 1,05× más rápido |
| `html/nested-div-10k` | 3.966 | **3.897** | 1,02× más rápido |
| `html/nested-div-32k` | 12.895 | **12.492** | 1,03× más rápido |
| `html/nested-div-64k` | 25.658 | **25.084** | 1,02× más rápido |
| `html/nested-div-64k-default-limits` | 14.721 | **14.625** | 1,01× más rápido |
| `html/massive-end-p` | 24.343 | **24.656** | 1,01× más lento |
| `html/massive-formatting` | 16.499 | **16.382** | 1,01× más rápido |
| `html/tag-40k-attrs` | 13.558 | **12.899** | 1,05× más rápido |
| `html/pretty-5k-blocks` | 27.912 | **26.994** | 1,03× más rápido |
| `css/parse-10k-rules` | 14.641 | **14.470** | 1,01× más rápido |
| `css/resolve-styles-2k-nodes` | 6.209 | **6.253** | 1,01× más lento |
| `css/resolve-styles-10k-nodes` | 29.595 | **29.853** | 1,01× más lento |
| `css/selector-match-simple` | 0.684 | **0.718** | 1,05× más lento |
| `css/selector-match-descendant` | 1.291 | **1.311** | 1,02× más lento |
| `layout/realistic-1mb` | 57.742 | **61.350** | **1,06× más lento** (tablas) |
| `paint/build-display-list` | 12.745 | **12.458** | 1,02× más rápido |
| `paint/rasterize-800x600` | 0.890 | **0.843** | 1,06× más rápido (fix AA) |
| `paint/render-to-png-800x600` | 13.016 | **13.045** | 1,00× (sin cambio) |
| `pipeline/html-to-png-800x600` | 173.305 | **178.791** | 1,03× más lento |
| **TOTAL (suma)** | **508.408** | **512.047** | **1,01× más lento** |

El resto es estable: las diferencias de ±1–5 % en HTML/CSS están dentro de la
varianza de una máquina compartida; no hay regresiones de complejidad.

---

## 11. Cómo comparar y detectar regresiones

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
   típica aquí ~1–6 %.
4. Para complejidad, usar `--scale 0.5` vs `1.0`: ratios ≈1.
5. Suite completa ~3 s + ~0,27 s de fixtures; smoke
   `--scale 0.1 --iterations 2` ~1 s.

JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
`worktree_dirty`, `rustc`, `cpu`, `cores`, `scale`, `iterations`, `warmup`,
`setup_ms`, `total_median_ms`, `results[]` y `skipped[]`.

---

## 12. CI

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

Los números de CI no son comparables (otra CPU, runners compartidos); el job
solo comprueba que compila y ejecuta, y deja el JSON como artefacto.

---

## 13. Limitaciones y notas

- **Baseline sobre commit limpio**: `c002ec0`; el árbol estaba limpio al
  empezar. La documentación cambió por otro agente durante la ventana (no
  afecta al código medido). El binario se reconstruyó una vez (build
  `bf9700af` → `687627e0`, mismo código; cifras dentro de ±1,5 %).
- **Host compartido**: load 0,99–1,52 en la ventana canónica; variabilidad
  ±1–6 % entre pasadas. Las cifras absolutas no son comparables con CI.
- **Showcase**: las variantes «sin `@import`» y «sin imágenes» se generan
  copiando el fixture a `/tmp` y editando una línea; las diferencias de pared
  están dentro del ruido, por eso el coste se reporta también por etapa
  (`styles`, `images`) y por bytes (`css_bytes`, `image_bytes`, tamaño PNG).
- **Shell headless**: `--screenshot` no crea ventana/`EventLoop` ni usa el
  worker; mide el proceso completo (carga del binario + render + escritura del
  PNG). El modo ventana (scroll/worker/HiDPI) no se benchmarkea aquí.
- **Cobertura**: no se benchmarkean `crates/render`, `net`, `image` ni `shell`
  dentro de `crates/bench`; se miden aparte con sus CLIs.
- **Fast path de tablas**: la comparación directa 67,2 → 61,4 ms usa el valor
  intermedio documentado por el commit; no hay un commit compilable con el
  camino avanzado sin fast path, así que esa cifra es referencia del carril de
  layout, no una medición propia.
