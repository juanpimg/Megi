# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 14:41–14:44 (Europe/Madrid). Ronda 4 del
  sprint (`layout`: positioning/floats/overflow; `paint`: stacking/opacity/
  clip/decoraciones/markers; `net`: TLS/caché).
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `3cc8449`**
  (`feat(paint): CSS 2.2 Appendix E stacking order, opacity groups, …`).
- **Estado del árbol en la medición**: **limpio** al inicio de la ventana
  (`git status --porcelain` vacío), y el binario se compiló de ese commit
  limpio. Después otro agente empezó a actualizar documentación (README,
  ARCHITECTURE, HANDOFF, ROADMAP); no afecta a las medidas, que usan el
  binario identificado arriba.
- **Binario medido**: `target/release/bench`,
  `sha256 = d752a856e959d8650c2cb97707c0a96dbb358a896a8dca1589d4f67380fa4992`,
  4 116 296 bytes, compilado con `cargo build --release -p bench` el
  2026-11-10T14:41:31+01:00.
- **Artefactos guardados** (gitignored, no versionados):
  - `target/bench-run1.json`, `target/bench-run2.json`, `target/bench-run3.json`
    — las 3 pasadas completas usadas para la tabla (mediana por fila).
  - `target/bench-aggregate.json` — medianas agregadas de las 3 pasadas.
  - `target/bench-latest.txt` / `target/bench-latest.json` — pasada de
    reproducción del script y su JSON (misma build).
  - `target/bench-latest-meta.txt` — commit/rustc/args/fecha de esa pasada.
  - `target/bench-fonts-latest.txt` — baseline opcional del motor de fuentes.
  - `target/bench-feature/` — experimento de coste de floats/posicionamiento/
    stacking/opacidad: fixtures HTML, `run.sh` reproducible y
    `feature-cost.json` con las medianas.

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §7:
> ronda 2 (2026-11-10 09:45, HEAD `9cca2e1`, árbol **dirty**, binario
> `sha256 = 0fd520aa…`) y ronda 3 (2026-11-10 14:09, HEAD `532bb6a`, árbol
> limpio, binario `sha256 = 9936527c…`).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Todas las cargas se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`): sin `rand`, sin reloj, sin red. La misma
`--scale` produce siempre el mismo flujo de bytes, de modo que dos máquinas o
dos commits parsean exactamente el mismo documento.

El *fixture* (texto HTML, hoja CSS, DOM, árbol de layout, display list) se
construye **fuera** del cronómetro. Solo la operación bajo test se mide. La
única excepción deliberada son los benchmarks de parse, cuya medición incluye
la construcción **y liberación** del `Document` resultante (coste real de
parsear y descartar una página).

### 1.2 Medición

- Reloj: `std::time::Instant` alrededor de cada ejecución.
- Por benchmark: `--warmup` ejecuciones sin cronometrar (por defecto **1**) y
  `--iterations` ejecuciones cronometradas (por defecto **5**). Las cargas
  marcadas como *heavy* se limitan a **3** iteraciones para que la suite
  completa quede por debajo de ~2 minutos.
- La tabla de §3 es la **mediana, por fila, de 3 pasadas completas** de la
  suite (cada pasada ya reporta su propia mediana). La columna «rango» muestra
  el mínimo–máximo de las 3 medianas: la dispersión observada fue **0,1 %–5,7 %**
  para casi todas las filas, con una excepción puntual del 16 % en
  `html/tag-40k-attrs` (una pasada coincidió con un pico de carga del host).
- El host se comparte con otros agentes del sprint: durante la ventana
  canónica el load average fue **1,1–1,9** sobre 20 hilos. Las medidas de paint
  son del orden de milisegundos y son las más sensibles al ruido; una tanda de
  pasadas filtradas coincidió con un pico de carga y se descartó (daba
  `render-to-png` ~7,9 ms frente a ~5,85 ms estable). Se usa la mediana de
  pasadas completas, consistente con la pasada del script.
- No se usa `criterion` ni `hyperfine`: el crate no tiene dependencias externas
  y el runner es un binario normal (`std` + crates internos del workspace).
- `--scale <factor>` multiplica todos los tamaños sintéticos (p. ej. `0.25`
  para un smoke test de ~1 s). Los resultados a escala distinta de `1.0` no son
  comparables directamente con esta baseline.

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Carga de fondo | load average 1,1–1,9 sobre 20 hilos (host compartido con otros agentes) |

> Nota: el handoff histórico describe el sandbox como «2 vCPU»; el entorno
> actual expone 20 hilos lógicos. El bench es **monohilo**, así que lo relevante
> es el núcleo efectivo y su estado térmico/frecuencia, no el total de núcleos.

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Pasada completa (compila release + ejecuta + guarda target/bench-latest.txt)
./tools/bench.sh

# Las 3 pasadas usadas para la tabla de esta baseline
for i in 1 2 3; do
  cargo run --release -p bench -- --json > "target/bench-run$i.json"
done

# Smoke rápido para CI / portátil (~1 s)
./tools/bench.sh --scale 0.1 --iterations 2

# Un grupo o un benchmark concreto
./tools/bench.sh --filter html
cargo run --release -p bench -- --filter html/nested-div-64k

# Benchmarks opcionales del motor de fuentes (feature no incluida por defecto)
BENCH_FEATURES=fonts ./tools/bench.sh --filter fonts

# Experimento de coste de floats/stacking/opacidad (§5, fuera del crate bench)
./target/bench-feature/run.sh

# Listar nombres registrados / ayuda
cargo run --release -p bench -- --list
cargo run --release -p bench -- --help
```

`crates/bench` se construye con dependencias internas del workspace (`dom`,
`html`, `css`, `layout`, `paint`; `fonts` solo bajo `--features fonts`) y pasa
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y
`cargo test -p bench` (8 tests unitarios de CLI, generadores y mediana).

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb` | Parse de documento «realista» de ~1 MiB (headings, párrafos, inline, listas, tablas, 135 reglas CSS embebidas). |
| html | `html/nested-div-10k/32k/64k` | Parse de `<div>` anidados 10k/32k/64k (detector clásico de árbol cuadrático). |
| html | `html/nested-div-64k-default-limits` | El mismo input con los límites anti-DoS de producción (profundidad 4096): coste del guardián. |
| html | `html/massive-end-p` | 100 000 `</p>` consecutivos (ignorar/cerrar etiquetas sin apertura). |
| html | `html/massive-formatting` | 10 000 bloques `<b><i><u>` mal anidados (adoption agency en cada cierre). |
| html | `html/tag-40k-attrs` | Un solo elemento con 40 000 atributos (detección de duplicados y copia de atributos). |
| html | `html/pretty-5k-blocks` | Documento indentado con 5 000 bloques (mucho whitespace significativo entre bloques). |
| css | `css/parse-10k-rules` | Parse de hoja CSS con 10 000 reglas + reglas base. |
| css | `css/resolve-styles-2k/10k-nodes` | Cascada + `ComputedStyle` sobre DOM de ~2 005 / ~10 005 elementos con 130 reglas de autor. |
| css | `css/selector-match-simple` | Matching de 4 selectores simples (tag/clase/id) sobre 10 005 elementos. |
| css | `css/selector-match-descendant` | Matching de 4 selectores con combinadores descendientes/hijos sobre 10 005 elementos. |
| layout | `layout/realistic-1mb` | `layout_document` sobre el documento realista ya parseado y con estilos resueltos (~28 195 elementos), viewport 800×600. Incluye la maquinaria nueva de positioning/floats/overflow. |
| paint | `paint/build-display-list` | Construcción de la display list (24 762 items) desde el árbol de layout, con orden de apilado CSS 2.2 App. E, grupos de opacidad, clip, decoraciones y markers. |
| paint | `paint/rasterize-800x600` | Rasterizado software **con culling/cachés y AA** de la display list a un `Pixmap` 800×600 (sin PNG). |
| paint | `paint/render-to-png-800x600` | Rasterizado + `Pixmap::to_png()` (PNG determinista propio, 1 920 813 B). |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo: parse → styles → layout → display list → raster → PNG. |
| fonts | `fonts/measure-1k-words` | `Font::measure_text` de 1 000 palabras (DejaVu Sans 16 px). Feature `fonts`. |
| fonts | `fonts/shape-1k-words` | `Font::shape` de 1 000 palabras (shaping/kern/ligaduras). Feature `fonts`. |

---

## 3. Baseline vigente (2026-11-10, HEAD `3cc8449` limpio)

Tabla = mediana por fila de **3 pasadas completas** de la suite (5 iteraciones,
1 warm-up; cargas *heavy* limitadas a 3). `setup` de fixtures: ~256 ms (no
cronometrado). El binario `sha256 = d752a856…` es el medido.

| Benchmark | Carga | iter | mediana (ms) | rango 3 pasadas (ms) | throughput |
|---|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | 5 | **55.569** | 55.209–56.560 | 18.0 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | 5 | **3.688** | 3.642–3.729 | 27.1 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | 3 | **11.786** | 11.654–11.921 | 28.8 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | 3 | **24.050** | 23.261–24.349 | 27.9 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | 5 | **14.524** | 14.500–14.701 | — (59 906 start tags descartados, 4 100 nodos) |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | 5 | **22.851** | 22.847–23.032 | 4.38 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | 5 | **15.719** | 15.510–16.056 | 636 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | 3 | **12.489** | 12.056–14.076 | 3.20 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | 5 | **26.193** | 26.158–26.284 | 191 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | 5 | **14.191** | 13.464–14.276 | 42.3 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | 5 | **5.873** | 5.809–6.108 | 341 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | 5 | **27.604** | 27.569–27.632 | 362 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | 5 | **0.653** | 0.646–0.664 | 61.3 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | 5 | **1.248** | 1.248–1.260 | 32.1 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos con estilo, 800×600 | 5 | **69.248** | 69.185–69.432 | 407 k elementos/s |
| `paint/build-display-list` | 24 762 items | 5 | **8.108** | 8.051–8.227 | — |
| `paint/rasterize-800x600` | 24 762 items → pixmap 800×600 | 3 | **0.932** | 0.925–0.951 | 515 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (1.83 MiB) | 3 | **5.852** | 5.849–5.853 | 313 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | 3 | **168.612** | 167.088–168.710 | — |
| **TOTAL (suma)** | | | **489.190** | | |

> **Aviso sobre el TOTAL**: la suma incluye a la vez las etapas sueltas y el
> pipeline end-to-end, que las solapa. No es un presupuesto de página; sirve
> solo como métrica agregada de la suite. La pasada del script (`./tools/bench.sh`)
> dio 490,8 ms, dentro de la varianza esperada.

### 3.1 Baseline opcional del motor de fuentes

Compilada con `cargo build --release -p bench --features fonts` (feature
desactivada por defecto; carga `crates/fonts/assets/DejaVuSans.ttf` y reporta
`SKIP` si el fixture falta). Medida el mismo día sobre el mismo HEAD.

| Benchmark | Carga | iter | mediana (ms) | mínimo (ms) | throughput |
|---|---|---:|---:|---:|---:|
| `fonts/measure-1k-words` | 1 000 palabras, DejaVu Sans 16 px | 5 | **0.455** | 0.451 | 2.20 M palabras/s |
| `fonts/shape-1k-words` | 1 000 palabras, DejaVu Sans 16 px | 5 | **8.021** | 7.945 | 125 k palabras/s |

---

## 4. Complejidad observada

Mediciones de escalado con el mismo binario (1–5 iteraciones, sin warm-up),
complementarias a la tabla de §3:

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.69 ms · 32k: 11.79 ms · 64k: 24.05 ms | **Lineal**: 3,2× input → 3,19× tiempo; 2× input → 2,04× tiempo. Throughput ~27–29 MiB/s constante. La complejidad cuadrática del tree builder (hallazgo F2) sigue eliminada. |
| `html/nested-div-64k-default-limits` | 14.52 ms (59 906 start tags descartados) | Los límites anti-DoS cortan a profundidad 4 096; el coste es el del guardián, no el del árbol completo. |
| `html/tag-40k-attrs` | 10k: 2.65 ms · 20k: 5.87 ms · 40k: 12.07 ms | **Lineal** (~0,3 µs/atributo). |
| `css/parse-10k-rules` | 5k: 7.85 ms · 10k: 14.73 ms · 20k: 29.90 ms | **Lineal** (1,88× y 2,03× por cada 2×; ~40 MiB/s). |
| `css/resolve-styles-2k/10k` | 2 005 el: 5.87 ms · 10 005 el: 27.60 ms | **Lineal**: 5× elementos → 4,70× tiempo (~362 k elementos/s). La indexación de reglas de la ronda 3 se mantiene. |
| `css/selector-match-*` | 5 005 el: simple 0.317 ms / descendiente 0.598 ms · 10 005 el: 0.629 / 1.245 ms | **Lineal** en elementos (1,98× / 2,08× al duplicar). 61–64 M / 32–33 M matches/s. |
| `layout/realistic-1mb` | 28 195 elementos → 69.25 ms | ~407 k elementos/s. **1,09× más lento que la ronda 3** por la maquinaria nueva de positioning/floats/overflow (aunque el documento realista no usa floats). |
| `paint/build-display-list` | 24 762 items → 8.11 ms | **2,13× más lento que la ronda 3** (3.80 ms): la display list crece de 19 542 a 24 762 items (**+26,7 %**, por markers de listas, decoraciones de texto, grupos de opacidad y orden de apilado) y cada item cuesta más (~0,33 µs/item frente a ~0,19 µs). Sigue siendo <5 % del pipeline. |
| `paint/rasterize-800x600` | 4 863 items: 0.851 ms · 14 824 items: 0.905 ms · 24 762 items: 0.920 ms | **Plano respecto al número de items**: +8 % al multiplicar ×5 los items; el coste depende del viewport (~515–564 MPix/s), no de la display list. El culling/cachés siguen absorbiendo el crecimiento de items de la ronda 4. |
| `paint/render-to-png-800x600` | 5.85 ms total − 0.93 ms raster ≈ **4.92 ms de PNG** | El codificador PNG (DEFLATE *stored*) sigue siendo ~5× más caro que rasterizar; es el siguiente objetivo si se quiere bajar la cola de paint. |
| `pipeline/html-to-png-800x600` | 168.61 ms | Composición: parse 55.6 ms (~33 %), layout 69.2 ms (~41 %) y resto (estilos + display list + raster + PNG) ≈43.8 ms (~26 %). **`layout` sigue siendo la etapa dominante**, seguida del parse; el rasterizado es ~0,6 %. |

---

## 5. Efecto de floats, posicionamiento, stacking y opacidad

El documento realista del bench **no usa floats ni opacidad**, así que la tabla
de §3 no aísla el coste de las features nuevas de la ronda 4. Para cuantificarlo
se usó el driver `paint` `examples/html_to_png` (pipeline HTML→PNG completo,
fuera del crate `bench`) sobre documentos sintéticos de **2 000 cajas
idénticas** que solo cambian la regla CSS del feature. Artefactos y script
reproducible: `target/bench-feature/` (`run.sh`, `feature-cost.json`).

Metodología: 15 ejecuciones por variante intercaladas (+1 de warm-up), mediana
del tiempo de pared de cada proceso. El tiempo incluye arranque del proceso,
carga del font DB, parse, estilos, layout, display list, rasterizado y
escritura del PNG; el **overhead fijo del driver medido con un documento
mínimo es ~11,16 ms**, por lo que los deltas contra el control son
atribuibles a la feature.

| Variante (2 000 cajas) | Regla CSS añadida | Mediana (ms) | Δ vs control | Factor |
|---|---|---:|---:|---:|
| `control` | — (bloques normales) | **56,79** | — | 1,00× |
| `positioned` | `position: relative; top:1px; left:1px` | **57,72** | +0,93 ms | 1,02× |
| `stacking` | `position: relative; z-index: 2` | **56,98** | +0,20 ms | 1,00× |
| `opacity` | `opacity: 0.85` | **59,75** | +2,96 ms | 1,05× |
| `floats` | `float: left; width: 120px` | **69,51** | **+12,72 ms** | **1,22×** |

Una segunda tanda independiente dio cifras consistentes: posicionado
+0,93–1,13 ms, stacking +0,20–0,59 ms, opacidad +2,96–3,55 ms y floats
+12,54–12,72 ms.

Lectura:

- **Floats**: es la feature más cara de las tres para esta forma de documento
  (+22 % end-to-end). Con 2 000 cajas flotantes el layout pasa a hacer
  colocación y detección de colisiones por caja; el coste es de layout, no de
  pintado.
- **Opacidad**: `opacity < 1` crea un grupo offscreen por caja (+5 % con
  `opacity: 0.85` en 2 000 cajas): el compositor debe renderizar el grupo y
  mezclarlo. En un documento pequeño el efecto es despreciable
  (`tests/fixtures/phase4/basic.html` con y sin `opacity: 0.7`: 12,30 vs
  12,29 ms, Δ +0,01 ms).
- **Stacking** (`position: relative` + `z-index`) y **posicionamiento** sin
  z-index: +0–2 %, dentro del ruido para documentos de este tamaño; el orden de
  apilado se resuelve en la construcción de la display list (que sí crece:
  ver §4), no en el rasterizado.
- El bench de la suite confirma el efecto agregado en documentos sin estas
  features: `paint/build-display-list` +2,13× (más items y más caros por
  markers/decoraciones/stacking) y `layout/realistic-1mb` +9 % (maquinaria de
  floats/positioning por caja), mientras el rasterizado sube solo un 8 % porque
  el culling absorbe el 26,7 % más de items.

**Sin alarma**: el pipeline completo pasa de 152,5 a 168,6 ms (+10,5 %) y sigue
holgado dentro del presupuesto de 250 ms; el coste incremental corresponde a
features reales (floats, stacking, opacidad, clip, decoraciones, markers) que
antes no existían.

---

## 6. Presupuestos orientativos

Presupuestos de ingeniería para esta máquina y build `release` (no son
comparaciones con navegadores comerciales; el rasterizador es software puro,
monohilo y sin GPU).

| Benchmark | Baseline | Presupuesto | Estado |
|---|---:|---:|---|
| `html/realistic-1mb` | 56 ms | ≤ 100 ms | ✅ |
| `html/nested-div-64k` | 24 ms | ≤ 100 ms | ✅ (lineal) |
| `html/massive-end-p` | 23 ms | ≤ 50 ms | ✅ |
| `html/massive-formatting` | 16 ms | ≤ 50 ms | ✅ |
| `html/tag-40k-attrs` | 12 ms | ≤ 50 ms | ✅ |
| `html/pretty-5k-blocks` | 26 ms | ≤ 75 ms | ✅ |
| `css/parse-10k-rules` | 14 ms | ≤ 30 ms | ✅ |
| `css/resolve-styles-10k-nodes` | 28 ms | ≤ 75 ms | ✅ |
| `layout/realistic-1mb` | 69 ms | ≤ 150 ms | ✅ |
| `paint/build-display-list` | 8.1 ms | ≤ 20 ms | ✅ |
| `paint/rasterize-800x600` | 0.93 ms | ≤ 25 ms | ✅ |
| `paint/render-to-png-800x600` | 5.9 ms | ≤ 50 ms | ✅ |
| `pipeline/html-to-png-800x600` | 169 ms | ≤ 250 ms | ✅ (68 % del presupuesto) |

**¿Pipeline < 250 ms?** Sí: 168,6 ms (67,4 % del presupuesto, 81 ms de margen).
**¿Layout sigue dominando?** Sí: 69,2 ms = 41 % del pipeline, por delante del
parse (55,6 ms = 33 %). El rasterizado ya no es el hotspot (0,9 ms); el coste
está en CPU de parse/layout/estilos.

---

## 7. Comparación histórica

Tres baselines medidas en la misma máquina, todas con el mismo arnés:

| | Ronda 2 | Ronda 3 | Ronda 4 (vigente) |
|---|---|---|---|
| Fecha | 2026-11-10 09:45 | 2026-11-10 14:09 | 2026-11-10 14:41 |
| HEAD | `9cca2e1` | `532bb6a` | **`3cc8449`** |
| Árbol | dirty (42 rutas) | limpio | **limpio** |
| Binario (sha256) | `0fd520aa…` | `9936527c…` | **`d752a856…`** |

Filas clave (mediana, ms):

| Benchmark | Ronda 2 | Ronda 3 | Ronda 4 |
|---|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | **55.569** |
| `html/nested-div-64k` | 21.262 | 23.534 | **24.050** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | **27.604** |
| `layout/realistic-1mb` | 49.506 | 63.541 | **69.248** |
| `paint/build-display-list` | 3.356 | 3.802 | **8.108** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | **0.932** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | **5.852** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | **168.612** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** |

### Ronda 3 → ronda 4, fila a fila

La ronda 4 añade layout con positioning/floats/overflow y paint con
stacking/opacity/clip/decoraciones/markers. El efecto en la baseline es un
incremento moderado y uniforme, concentrado en las etapas que tocan las cajas:

| Benchmark | Ronda 3 (ms) | Ronda 4 (ms) | Factor |
|---|---:|---:|---|
| `html/realistic-1mb` | 52.530 | **55.569** | 1,06× más lento |
| `html/nested-div-10k` | 3.613 | **3.688** | 1,02× más lento |
| `html/nested-div-32k` | 11.497 | **11.786** | 1,03× más lento |
| `html/nested-div-64k` | 23.534 | **24.050** | 1,02× más lento |
| `html/nested-div-64k-default-limits` | 13.898 | **14.524** | 1,05× más lento |
| `html/massive-end-p` | 22.069 | **22.851** | 1,04× más lento |
| `html/massive-formatting` | 14.730 | **15.719** | 1,07× más lento |
| `html/tag-40k-attrs` | 12.106 | **12.489** | 1,03× más lento |
| `html/pretty-5k-blocks` | 25.116 | **26.193** | 1,04× más lento |
| `css/parse-10k-rules` | 12.814 | **14.191** | 1,11× más lento |
| `css/resolve-styles-2k-nodes` | 5.687 | **5.873** | 1,03× más lento |
| `css/resolve-styles-10k-nodes` | 27.037 | **27.604** | 1,02× más lento |
| `css/selector-match-simple` | 0.652 | **0.653** | 1,00× (sin cambio) |
| `css/selector-match-descendant` | 1.245 | **1.248** | 1,00× (sin cambio) |
| `layout/realistic-1mb` | 63.541 | **69.248** | 1,09× más lento |
| `paint/build-display-list` | 3.802 | **8.108** | 2,13× más lento |
| `paint/rasterize-800x600` | 0.861 | **0.932** | 1,08× más lento |
| `paint/render-to-png-800x600` | 5.194 | **5.852** | 1,13× más lento |
| `pipeline/html-to-png-800x600` | 152.519 | **168.612** | 1,11× más lento |
| **TOTAL (suma)** | **452.445** | **489.190** | **1,08× más lento** |

---

## 8. Cómo comparar y detectar regresiones

1. Medir **antes** y **después** en la misma máquina y con el sistema ocioso
   (`uptime` < ~1.5 de load average). Misma `--scale` y mismas iteraciones.
2. Guardar la salida JSON (`target/bench-latest.json`) y comparar medianas:

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
3. Considerar regresión si la mediana empeora >15 % de forma reproducible. En
   esta máquina, la dispersión entre pasadas completas de la suite es ~0–6 %
   (la mayoría <3 %); con el host cargado puede superar el 10 %, sobre todo en
   las medidas de paint (milisegundos).
4. Para cambios de complejidad, usar el escalado (`--scale 0.5` vs `1.0`) y
   comprobar que el tiempo crece como el tamaño (ratio ≈1: ver §4).
5. La suite completa tarda ~3 s de reloj (más ~0,26 s de fixtures); si se
   busca un chequeo rápido, `--scale 0.1 --iterations 2` tarda ~1 s.

JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
`worktree_dirty`, `rustc`, `cpu`, `cores`, `scale`, `iterations`, `warmup`,
`setup_ms`, `total_median_ms`, `results[]` (con `name`, `group`, `detail`,
`iterations`, `median_ms`, `min_ms`, `samples_ms`, `throughput`) y `skipped[]`.
Los campos existentes no cambian; solo se añaden campos nuevos.

---

## 9. CI

La suite **se ejecuta a mano** (`./tools/bench.sh`); no se ha añadido un job
para no tocar `.github/workflows/` en este round. Es barata (~3 s la suite
completa en esta máquina) y no tiene dependencias externas, así que un job
opcional `workflow_dispatch` es trivial de añadir:

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
      - name: Smoke benchmark (no thresholds; baseline informativa)
        run: ./tools/bench.sh --scale 0.25 --iterations 2
```

Los números de CI no son comparables con esta baseline (otra CPU, runners
compartidos); el job solo detecta que el runner compila y ejecuta, y el JSON
queda como artefacto para comparar tendencias.

---

## 10. Limitaciones y notas

- **Baseline sobre árbol limpio**: esta tabla corresponde al commit `3cc8449`
  con `git status` vacío y el binario identificado; es reproducible desde un
  clon limpio con `./tools/bench.sh`.
- **Host compartido**: el load average durante la ventana fue 1,1–1,9 sobre 20
  hilos (otros agentes del sprint). Se usan medianas de 3 pasadas completas; la
  dispersión es baja salvo una pasada puntual de `tag-40k-attrs`. Las medidas
  de paint (ms) son las más sensibles.
- **Experimento de features**: floats/stacking/opacidad se miden con el driver
  `html_to_png` sobre documentos sintéticos amplificados (2 000 cajas), no con
  el crate `bench` (que no incluye esas cargas). El delta es end-to-end; para
  separar layout y paint habría que añadir esos benchmarks al crate, fuera del
  scope de este refresco. Los fixtures y el `run.sh` quedan en
  `target/bench-feature/` (gitignored).
- **Comparación histórica**: la ronda 2 se midió sobre árbol *dirty* con otro
  código; los factores grandes de la ronda 3 (raster/PNG/pipeline) siguen
  siendo mejoras reales. Las diferencias de ±2–13 % de la ronda 4 deben leerse
  como coste de features nuevas, no como ruido puro.
- **Rasterizado software**: `paint/rasterize-800x600` mide CPU pura, monohilo,
  sin GPU; 800×600 es el viewport de los tests de Fase 4, no un tamaño de
  producción.
- **Parse incluye liberación del DOM**: los benchmarks de parse destruyen el
  `Document` dentro del cronómetro; es el coste real de parsear una página.
- **`--scale` cambia el mix**: las reglas CSS embebidas del documento realista
  son fijas (135) mientras las secciones escalan, y los benchmarks con nombres
  `*-10k`/`*-40k` conservan el nombre aunque la escala cambie el tamaño real;
  por eso `scale != 1.0` es solo para smoke tests y comparaciones relativas.
- **Cobertura**: el bench cubre parse HTML/CSS, cascada, selectores, layout
  BFC/IFC básico, display list, rasterizado y PNG. No cubre network/TLS
  (`crates/net`), IFC con métricas reales dentro del benchmark ni cargas
  dedicadas de floats/stacking/opacidad (ver experimento de §5).
