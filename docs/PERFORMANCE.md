# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 15:43–15:52 (Europe/Madrid). Ronda 6:
  optimización de layout (−43 % en el pase de IFC según su carril), imágenes
  E2E en paint/render, `@import`/media en CSS, APIs DOM (`innerHTML`,
  `document.write`) y shell headless de Fase 7.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `13e9945`**
  (`feat(shell): first browser window with winit, softbuffer and headless screenshots`).
- **Estado del árbol**: **limpio al inicio** (`git status` vacío; el binario se
  compiló de ese commit). Durante la ventana otro agente añadió cambios de
  documentación (`README`, `docs/ROADMAP`, una imagen); no afectan al código
  medido.
- **Binario medido**: `target/release/bench`,
  `sha256 = f2ea6d77c83b948a5d2b995d252b83dc90256e28707ad10a517cf8ccf1ede0a0`,
  4 216 648 bytes, compilado con `cargo build --release -p bench`.
- **Artefactos guardados** (gitignored, no versionados):
  - `target/bench-run1.json` … `target/bench-run5.json` — las 5 pasadas
    completas usadas para la tabla (mediana por fila).
  - `target/bench-aggregate.json` — medianas agregadas.
  - `target/bench-latest.txt` / `.json` / `-meta.txt` — pasada de reproducción
    del script (`TOTAL 518.926 ms`) y su metadata.
  - `target/bench-fonts-latest.txt` — baseline opcional del motor de fuentes.
  - `target/bench-layout-investigation/results.md` y `r6-verification.md` —
    investigación del coste de layout con tablas (§5) y verificación de la
    mejora r6.
  - `target/bench-render-r6/` — pipeline `render`/shell: `results.json`,
    `results.md` y PNGs de los fixtures.

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §9 (rondas 2–5).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Todas las cargas se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`): sin `rand`, sin reloj, sin red. La misma
`--scale` produce siempre el mismo flujo de bytes.

El *fixture* se construye **fuera** del cronómetro; solo se mide la operación
bajo test. Los benchmarks de parse incluyen la construcción **y liberación**
del `Document` resultante.

**El crate `bench` no cambió en esta ronda**, así que todas las diferencias
r5 → r6 son de código del motor.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- La tabla de §3 es la **mediana, por fila, de 5 pasadas completas**. La
  columna «rango» es el mínimo–máximo de las 5 medianas.
- Ventana canónica: load average **0,96–1,39** sobre 20 hilos. Se hicieron dos
  tandas (el binario se recompiló al alternar la feature opcional `fonts`); la
  tanda previa (`sha256 9acc5799…`) dio `layout 58.418 / pipeline 175.704 ms`,
  es decir ±1,5 % respecto a la tabla. Se usa la segunda tanda (binario
  `f2ea6d77…`).
- Dispersión típica entre pasadas: **1–6 %**.
- Las medidas del CLI `render`/`megi` usan cronómetro de pared de Python
  (7 ejecuciones, mediana; el entorno no tiene `/usr/bin/time`), load 1,1–1,4.
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Carga de fondo | load average 0,96–1,39 durante la ventana canónica |

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

# Benchmarks opcionales del motor de fuentes
BENCH_FEATURES=fonts ./tools/bench.sh --filter fonts

# Pipeline render (con imágenes) y shell headless
cargo run --release -p render -- tests/fixtures/render/images.html /tmp/i.png --json
cargo run --release -p shell  -- tests/fixtures/render/images.html --screenshot /tmp/s.png
```

`crates/bench` pasa `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings` y `cargo test -p bench` (8 tests).

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb` | Parse de documento «realista» de ~1 MiB (headings, párrafos, inline, listas, tablas, 135 reglas CSS embebidas). |
| html | `html/nested-div-10k/32k/64k` | Parse de `<div>` anidados 10k/32k/64k. |
| html | `html/nested-div-64k-default-limits` | El mismo input con límites anti-DoS (profundidad 4096). |
| html | `html/massive-end-p` | 100 000 `</p>` consecutivos. |
| html | `html/massive-formatting` | 10 000 bloques `<b><i><u>` mal anidados. |
| html | `html/tag-40k-attrs` | Un elemento con 40 000 atributos. |
| html | `html/pretty-5k-blocks` | Documento indentado con 5 000 bloques. |
| css | `css/parse-10k-rules` | Parse de hoja CSS con 10 000 reglas. |
| css | `css/resolve-styles-2k/10k-nodes` | Cascada + `ComputedStyle` sobre ~2 005 / ~10 005 elementos. |
| css | `css/selector-match-simple` | 4 selectores simples sobre 10 005 elementos. |
| css | `css/selector-match-descendant` | 4 selectores con combinadores sobre 10 005 elementos. |
| layout | `layout/realistic-1mb` | `layout_document` sobre el documento realista (28 195 elementos, 88 155 cajas), 800×600; incluye tablas, IFC optimizado en r6. |
| paint | `paint/build-display-list` | Display list (24 762 items) con stacking/grupos de opacidad. |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA a 800×600. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` con DEFLATE propio (54 526 B). |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo: parse → styles → layout → display list → raster → PNG. |
| fonts | `fonts/measure-1k-words`, `fonts/shape-1k-words` | Motor de fuentes (feature opcional). |

---

## 3. Baseline vigente (2026-11-10, HEAD `13e9945` limpio)

Mediana por fila de **5 pasadas completas** (5 iteraciones, 1 warm-up; *heavy*
3). `setup` de fixtures: ~264 ms (no cronometrado). Binario `sha256 =
f2ea6d77…`. La pasada del script dio `TOTAL 518.926 ms`.

| Benchmark | Carga | iter | mediana (ms) | rango 5 pasadas (ms) | throughput |
|---|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | 5 | **58.738** | 57.821–59.659 | 17.1 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | 5 | **3.966** | 3.762–4.000 | 25.2 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | 3 | **12.895** | 12.405–13.205 | 26.4 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | 3 | **25.658** | 25.370–25.912 | 26.1 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | 5 | **14.721** | 14.214–15.147 | — (59 906 start tags descartados) |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | 5 | **24.343** | 23.736–25.192 | 4.11 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | 5 | **16.499** | 16.332–16.823 | 606 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | 3 | **13.558** | 13.083–13.657 | 2.95 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | 5 | **27.912** | 27.549–28.395 | 179 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | 5 | **14.641** | 13.996–15.158 | 41.0 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | 5 | **6.209** | 5.724–6.642 | 323 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | 5 | **29.595** | 29.133–29.837 | 338 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | 5 | **0.684** | 0.664–0.727 | 58.5 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | 5 | **1.291** | 1.241–1.329 | 31.0 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | 5 | **57.742** | 55.900–58.973 | 488 k elementos/s |
| `paint/build-display-list` | 24 762 items | 5 | **12.745** | 12.618–13.185 | — |
| `paint/rasterize-800x600` | 24 762 items → pixmap 800×600 | 3 | **0.890** | 0.832–0.935 | 539 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (**54 526 B**) | 3 | **13.016** | 12.908–13.835 | 3.99 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | 3 | **173.305** | 172.242–177.426 | — |
| **TOTAL (suma)** | | | **508.408** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página.

### 3.1 Baseline opcional del motor de fuentes

| Benchmark | iter | mediana (ms) | mínimo (ms) | throughput |
|---|---:|---:|---:|---:|
| `fonts/measure-1k-words` | 5 | **0.456** | 0.441 | 2.19 M palabras/s |
| `fonts/shape-1k-words` | 5 | **8.119** | 7.950 | 123 k palabras/s |

---

## 4. Complejidad observada

Mediciones de escalado con el mismo binario, complementarias a §3 (load
1,3–1,5; se usan para ratios):

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.97 ms · 32k: 12.90 ms · 64k: 25.66 ms | **Lineal**: 3,2× input → 3,25× tiempo; 2× → 1,99×. Throughput ~25–26 MiB/s. |
| `html/tag-40k-attrs` | 10k: 2.80 ms · 20k: 6.46 ms · 40k: 13.57 ms | **Lineal** (~0,3 µs/atributo). |
| `css/parse-10k-rules` | 5k: 7.31 ms · 10k: 15.16 ms · 20k: 31.40 ms | **Lineal** (2,07× y 2,07× por cada 2×). |
| `css/resolve-styles-2k/10k` | 2 005 el: 6.21 ms · 10 005 el: 29.60 ms | **Lineal**: 5× elementos → 4,77× tiempo (~338 k elementos/s). |
| `css/selector-match-*` | 58,5 M matches/s simples · 31,0 M con combinadores | Lineal por elemento; no es cuello de botella. |
| `paint/rasterize-800x600` | 4 863 items: 0.768 ms · 14 824: 0.833 ms · 24 762: 0.897 ms | **Plano respecto al nº de items** (~535–625 MPix/s). |
| `layout/realistic-1mb` | 28 195 elementos → 57.74 ms (88 155 cajas) | **−32,0 % respecto a r5** (84,96 ms): el árbol no cambió (61 599 cajas pre-layout); la mejora está en el pase de layout (§5). |

---

## 5. Layout: coste con tablas (r5) y verificación de la mejora (r6)

### 5.1 Por qué subió en r5 (contexto)

El documento sintético del bench contiene **1 044 tablas 3×3** (9 396 celdas,
una por sección) desde siempre; r5 implementó layout de tablas. Eso añadió
**19 836 cajas** de tabla (Table/RowGroup/Row/Cell + anónimas de *fix-up*) y
20 880 cajas de línea/texto de las IFC de celda: **49 527 → 88 155 cajas**
(+78 %) y layout **69,25 → 84,96 ms** en r5. `compute_column_widths` medía el
min/max intrínseco de cada celda y luego se recorrían otra vez. Detalle
completo en `target/bench-layout-investigation/results.md`.

### 5.2 Verificación de la mejora reportada en r6

El carril de layout reportó **100,6 → 57,2 ms** en `layout/realistic-1mb`
(−43 %) y **214,2 → 178,8 ms** en el pipeline. Medición oficial con el bench
(misma máquina, árbol limpio, mediana de 5 pasadas):

| Métrica | Baseline limpia r5 | Medición oficial r6 | Factor real |
|---|---:|---:|---:|
| `layout/realistic-1mb` | 84.958 ms | **57.742 ms** | **×1,47 más rápido (−32,0 %)** |
| `pipeline/html-to-png` | 214.159 ms | **173.305 ms** | **×1,24 más rápido (−19,1 %)** |
| `paint/build-display-list` | 12.237 ms | 12.745 ms | 1,04× más lento (ruido) |
| `paint/render-to-png` | 12.964 ms | 13.016 ms | 1,00× (sin cambio) |

**Diferencias metodológicas.** El valor «después» del carril coincide con el
oficial (57,2 vs 57,7 ms; 178,8 vs 173,3 ms, −3 %). El valor «antes» (100,6 ms)
está **por encima de la baseline limpia r5** (84,958 ms; rango de 5 pasadas
84,7–97,0 ms; la pasada del script con load ~1,3 dio 95,6 ms), lo que es
consistente con una medición de su harness en condiciones menos controladas
(menos pasadas, host cargado) o incluyendo trabajo adicional. Por eso su
−43 % usa una línea base inflada; el factor sobre la baseline limpia es
**−32 % (×1,47)**. Ambos confirman la misma mejora absoluta de ~27 ms.

**Localización de la mejora** (herramienta auxiliar, mismo documento,
`DefaultTextMeasurer`):

| Etapa | r5 (`49a2792`) | r6 (`13e9945`) | Δ |
|---|---:|---:|---:|
| `build_layout_tree` | 14,599 ms | 14,653 ms | +0,4 % |
| `layout_block_with_measurer` (pase de layout) | 77,525 ms | **35,294 ms** | **−54,5 % (−42,2 ms)** |
| suma | 92,124 ms | 49,947 ms | −45,8 % |

El árbol pre-layout es **idéntico** (61 599 cajas en ambos): la optimización
acelera el pase de layout (IFC/line breaking y colocación), no cambia la
estructura de cajas. El árbol final sigue teniendo 88 155 cajas. Esto es
coherente con el commit `perf(layout): cut inline layout cost 43 %`; el −54,5 %
local es aún mayor porque concentra toda la mejora IFC.

**Conclusión**: la mejora es real y grande. `layout` deja de ser la etapa
dominante en solitario: en el pipeline r6 es 33,3 % frente al parse 33,9 %
(§8). No hace falta un WP de emergencia; si se quiere seguir bajando, el
objetivo ya no es layout sino el conjunto parse+layout (67 % del pipeline) y
el resto (estilos 17,9 %, display list 7,4 %, PNG 7,0 %).

---

## 6. Compresión PNG, pipeline `render` y shell headless

### 6.1 Compresión PNG (bench, documento realista 800×600)

| Métrica | r4 (stored) | r5–r6 (DEFLATE) | Factor |
|---|---:|---:|---:|
| PNG (`render-to-png`) | 1 920 813 B | **54 526 B** | ×35,2 menor (−97,2 %) |
| Tiempo `paint/render-to-png` | 5,852 ms (r4) | **13,016 ms (r6)** | ×2,22 vs r4 |
| — rasterizar | 0,932 ms | 0,890 ms | — |
| — codificar PNG (por diferencia) | ~4,92 ms | **~12,13 ms** | ×2,47 |

La compresión cuesta ~+7,2 ms por PNG 800×600 a cambio de 35–45× menos bytes.
Sigue siendo el intercambio correcto para I/O/caché; el encoder es ahora el
segundo mayor coste del pipeline de pintado.

### 6.2 Pipeline `render` con imágenes (CLI, HEAD `13e9945`)

Medido con `target/release/render` (`sha256 3a01601d…`): 7 ejecuciones de pared
por fixture (mediana), load 1,1–1,4. `--json` separa etapas (µs).

| Fixture | Pared (ms) | rango | PNG | display items | etapas `--json` (µs) |
|---|---:|---:|---:|---:|---|
| `render/basic.html` | **15,64** | 15,14–15,95 | 21 090 B | 3 | fetch 27 · parse 68 · styles 289 · images 2 · **layout 776** · paint 5 · **raster 1 215** · total 13 440 |
| `render/images.html` | **15,03** | 14,20–15,67 | 15 625 B | 4 | fetch 15 · parse 49 · styles 188 · **images 25** · **layout 549** · paint 4 · **raster 1 010** · total 12 131 |
| `phase4/basic.html` | **17,57** | 16,93–19,36 | 42 853 B | 26 | fetch 18 · parse 150 · styles 293 · images 3 · **layout 929** · paint 11 · **raster 1 755** · total 15 701 |

- `images.html` (`--json`): `images_loaded=1`, `images_failed=1`,
  `image_bytes=82`; la imagen fallida produce placeholder gris con warning.
  Decodificar la imagen cuesta 25 µs (el 0,2 % del total); el peso está en
  rasterizar (1,0 ms) y, sobre todo, en codificar el PNG (~10,3 ms por
  diferencia).
- Reparto aproximado del pipeline `render`: **PNG ~66–72 %**, rasterizado
  ~8–11 %, layout ~4–6 %, estilos ~1–2 %, resto (fetch/parse/setup) el resto.
- Evolución vs r5 (mismo fixture `phase4/basic.html`): pared 18,01 → **17,57 ms**;
  layout 1 377 → **929 µs (−32,5 %)**, raster 2 539 → **1 755 µs (−30,9 %)**,
  total 16,40 → 15,70 ms; la codificación PNG se mantiene ~11,9–12,5 ms.

### 6.3 Shell headless (`megi --screenshot`)

`target/release/megi` (`sha256 eedcd50d…`), sin display ni `EventLoop`:

| Fixture | Pared (ms) | rango | PNG | Overhead vs `render` CLI |
|---|---:|---:|---:|---:|
| `basic.html` | **16,00** | 15,34–16,41 | 21 090 B | +0,36 ms |
| `images.html` | **15,55** | 14,73–16,70 | 15 625 B | +0,52 ms |

El shell headless usa el mismo pipeline `render` y produce **PNG idéntico**
byte a byte; su sobrecoste es <0,6 ms (parseo de argumentos + escritura). El
coste de Fase 7 medido aquí es, por tanto, despreciable frente al render.

---

## 7. Coste de floats, posicionamiento, stacking y opacidad (referencia r4)

Medido en r4 con el driver `html_to_png` sobre 2 000 cajas (15 ejecuciones
intercaladas; overhead del driver ~11,2 ms):

| Variante | Δ vs control | Factor |
|---|---:|---:|
| `position: relative` | +0,93–1,13 ms | 1,02× |
| `position: relative; z-index` | +0,20–0,59 ms | 1,00–1,01× |
| `opacity: 0.85` | +2,96–3,55 ms | 1,05–1,06× |
| `float: left` | +12,54–12,72 ms | **1,22×** |

El documento realista del bench no usa floats ni opacidad; estas cifras no
intervienen en la baseline de §3.

---

## 8. Presupuestos orientativos

Objetivo de la ronda: **pipeline ≤ 200 ms**. Rasterizador software, monohilo.

| Benchmark | Baseline | Presupuesto | Estado |
|---|---:|---:|---|
| `html/realistic-1mb` | 59 ms | ≤ 100 ms | ✅ |
| `html/nested-div-64k` | 26 ms | ≤ 100 ms | ✅ (lineal) |
| `html/massive-end-p` | 24 ms | ≤ 50 ms | ✅ |
| `html/massive-formatting` | 16 ms | ≤ 50 ms | ✅ |
| `html/tag-40k-attrs` | 14 ms | ≤ 50 ms | ✅ |
| `html/pretty-5k-blocks` | 28 ms | ≤ 75 ms | ✅ |
| `css/parse-10k-rules` | 15 ms | ≤ 30 ms | ✅ |
| `css/resolve-styles-10k-nodes` | 30 ms | ≤ 75 ms | ✅ |
| `layout/realistic-1mb` | 58 ms | ≤ 150 ms | ✅ (−32 % vs r5) |
| `paint/build-display-list` | 12,7 ms | ≤ 20 ms | ✅ |
| `paint/rasterize-800x600` | 0,89 ms | ≤ 25 ms | ✅ |
| `paint/render-to-png-800x600` | 13,0 ms | ≤ 50 ms | ✅ |
| `pipeline/html-to-png-800x600` | **173,3 ms** | **≤ 200 ms** | ✅ (**86,7 %**, margen 26,7 ms) |

**¿Pipeline dentro de presupuesto?** Sí: 173,3 ms (86,7 % de 200 ms; margen
26,7 ms), tras bajar de 214,2 ms en r5. **¿Layout ya no domina?** No domina en
solitario: layout 33,3 % y parse 33,9 % (antes layout 39,7 % y parse 26,7 %).
El pipeline se reparte en parse+layout (~67 %), estilos+resto (~18 %), display
list (7,4 %) y PNG (7,0 %); el rasterizado es 0,5 %.

---

## 9. Comparación histórica

Cinco baselines en la misma máquina, mismo arnés:

| | Ronda 2 | Ronda 3 | Ronda 4 | Ronda 5 | Ronda 6 (vigente) |
|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | **15:43** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | **`13e9945`** |
| Árbol | dirty (42) | limpio | limpio | limpio | **limpio al inicio** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | **`f2ea6d77…`** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 |
|---|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | **58.738** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | **25.658** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | **29.595** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | **57.742** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | **12.745** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | **0.890** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | **13.016** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | **173.305** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** |

> Nota: las cifras de R5 son las de la baseline limpia publicada en el commit
> `fa98f64`; el carril de layout usó una línea base propia de 100,6 ms para
> layout (ver §5.2).

### Ronda 5 → ronda 6, fila a fila

| Benchmark | R5 (ms) | R6 (ms) | Factor |
|---|---:|---:|---|
| `html/realistic-1mb` | 57.234 | **58.738** | 1,03× más lento |
| `html/nested-div-10k` | 3.910 | **3.966** | 1,01× más lento |
| `html/nested-div-32k` | 12.284 | **12.895** | 1,05× más lento |
| `html/nested-div-64k` | 24.564 | **25.658** | 1,04× más lento |
| `html/nested-div-64k-default-limits` | 14.771 | **14.721** | 1,00× (sin cambio) |
| `html/massive-end-p` | 24.736 | **24.343** | 1,02× más rápido |
| `html/massive-formatting` | 16.493 | **16.499** | 1,00× (sin cambio) |
| `html/tag-40k-attrs` | 12.623 | **13.558** | 1,07× más lento |
| `html/pretty-5k-blocks` | 27.513 | **27.912** | 1,01× más lento |
| `css/parse-10k-rules` | 14.763 | **14.641** | 1,01× más rápido |
| `css/resolve-styles-2k-nodes` | 5.871 | **6.209** | 1,06× más lento |
| `css/resolve-styles-10k-nodes` | 28.199 | **29.595** | 1,05× más lento |
| `css/selector-match-simple` | 0.662 | **0.684** | 1,03× más lento |
| `css/selector-match-descendant` | 1.224 | **1.291** | 1,05× más lento |
| `layout/realistic-1mb` | 84.958 | **57.742** | **1,47× más rápido** |
| `paint/build-display-list` | 12.237 | **12.745** | 1,04× más lento |
| `paint/rasterize-800x600` | 0.899 | **0.890** | 1,01× más rápido |
| `paint/render-to-png-800x600` | 12.964 | **13.016** | 1,00× (sin cambio) |
| `pipeline/html-to-png-800x600` | 214.159 | **173.305** | **1,24× más rápido** |
| **TOTAL (suma)** | **570.064** | **508.408** | **1,12× más rápido** |

El resto de filas empeora 1–7 %, dentro de la varianza observada y del ruido
de una máquina compartida; no hay regresiones estructurales.

---

## 10. Cómo comparar y detectar regresiones

1. Medir antes/después en la misma máquina, sistema ocioso (`uptime` < ~1.5),
   misma `--scale` e iteraciones.
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
4. Para complejidad, usar el escalado (`--scale 0.5` vs `1.0`): ratios ≈1.
5. La suite completa tarda ~3–4 s (más ~0,26 s de fixtures); el smoke
   `--scale 0.1 --iterations 2` ~1 s.

JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
`worktree_dirty`, `rustc`, `cpu`, `cores`, `scale`, `iterations`, `warmup`,
`setup_ms`, `total_median_ms`, `results[]` y `skipped[]`.

---

## 11. CI

Se ejecuta a mano (`./tools/bench.sh`); no se ha añadido job para no tocar
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

## 12. Limitaciones y notas

- **Baseline sobre commit limpio**: `13e9945`; el árbol estaba limpio al
  empezar y el binario se compiló de ese commit. La documentación cambió por
  otro agente durante la ventana (no afecta al código medido).
- **Dos tandas de medición**: el binario se recompiló al alternar la feature
  opcional `fonts`; la primera tanda (`sha256 9acc5799…`) dio `layout 58.418 /
  pipeline 175.704 ms`, ±1,5 % respecto a la tabla. La tabla usa la segunda
  (binario `f2ea6d77…`).
- **Host compartido**: load 0,96–1,39 en la ventana canónica; las filas de
  HTML/CSS empeoran 1–7 % respecto a r5 por ruido y cambios de código
  menores, no por regresiones de complejidad.
- **Herramientas de investigación fuera del repo**: el desglose
  build/layout y los conteos de cajas usan un crate auxiliar en `/tmp` que
  reutiliza el generador del bench; el repo no se modificó (el `git worktree`
  temporal de `8f273f0` de la ronda anterior se creó y eliminó, verificado).
- **`render`/`megi`**: las medidas de pared incluyen arranque del proceso
  (~1,2 ms) y carga del font DB; los `timings` del `--json` no incluyen la
  codificación PNG (se calcula por diferencia). El shell headless no abre
  ventana ni `EventLoop`.
- **Imágenes**: el fixture `images.html` usa un PNG de 82 B; el coste de
  decodificar imágenes reales no está cubierto por esta baseline (solo el
  pipeline y su integración E2E).
- **Cobertura**: no se benchmarkean los crates `render`, `net`, `image` ni
  `shell` dentro de `crates/bench`; se miden aparte con sus CLIs.
