# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 15:15–15:22 (Europe/Madrid). Ronda 5:
  layout con tablas/inline-block/reemplazados, `paint` con DEFLATE PNG y painter
  iterativo, DOM generacional y crates `render`/`image`.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`. Código medido: **`49a2792`**
  (`feat(render,image): browser pipeline and dependency-free PNG decoding`).
  Sobre él solo hay un commit posterior de documentación (`c3ee824`), sin
  cambios de código, así que la baseline sigue siendo válida para el HEAD actual.
- **Estado del árbol**: **limpio al inicio** de la ventana (`git status` vacío;
  el binario se compiló de ese commit). Durante la pasada del script otro agente
  añadió cambios en documentación (`README`, `docs/*` y una imagen); no afectan
  al código medido ni al binario.
- **Binario medido**: `target/release/bench`,
  `sha256 = e9fcecae67d22d6ba6c46f713ffc58be6c1815b0588f5ebf495524d99a6afd6a`,
  4 205 120 bytes, compilado con `cargo build --release -p bench` el
  2026-11-10T15:15:46+01:00.
- **Artefactos guardados** (gitignored, no versionados):
  - `target/bench-run1.json` … `target/bench-run5.json` — las 5 pasadas
    completas usadas para la tabla (mediana por fila).
  - `target/bench-aggregate.json` — medianas agregadas.
  - `target/bench-latest.txt` / `.json` / `-meta.txt` — pasada de reproducción
    del script (`TOTAL 560.090 ms`) y su metadata.
  - `target/bench-fonts-latest.txt` — baseline opcional del motor de fuentes.
  - `target/bench-layout-investigation/results.md` — investigación del coste de
    layout con tablas (§5).
  - `target/bench-render/results.md` — compresión PNG y pipeline `render` (§6).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §9:
> ronda 2 (09:45, `9cca2e1`, árbol **dirty**, `sha256 = 0fd520aa…`),
> ronda 3 (14:09, `532bb6a` limpio, `sha256 = 9936527c…`) y
> ronda 4 (14:41, `3cc8449` limpio, `sha256 = d752a856…`).

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
la construcción **y liberación** del `Document` resultante.

**El crate `bench` no cambió en esta ronda**, así que todas las diferencias
r4 → r5 son de código del motor, no del arnés.

### 1.2 Medición

- Reloj: `std::time::Instant` alrededor de cada ejecución.
- Por benchmark: `--warmup` ejecuciones sin cronometrar (por defecto **1**) y
  `--iterations` ejecuciones cronometradas (por defecto **5**; *heavy*, 3).
- La tabla de §3 es la **mediana, por fila, de 5 pasadas completas** de la
  suite. La columna «rango» muestra el mínimo–máximo de las 5 medianas.
- Ventana canónica: load average **1,22–1,29** sobre 20 hilos. Se descartó una
  primera tanda de 3 pasadas (load 1,76→2,50) con outliers de hasta ×2 en
  `nested-div`/`rasterize`; esas cifras no aparecen en la tabla.
- Dispersión típica entre pasadas: **1–10 %** (algunos benchmarks de pintado,
  del orden de 1 ms, llegaron al 20 %). El host se comparte con otros agentes.
- No se usa `criterion` ni `hyperfine`: el runner es un binario `std` sin
  dependencias externas. Las medidas del CLI `render` y de las herramientas de
  investigación usan cronómetro de pared (`time.perf_counter` de Python; el
  entorno no tiene `/usr/bin/time`).
- `--scale` multiplica todos los tamaños sintéticos (p. ej. `0.25` para un
  smoke test de ~1 s). No es comparable con la baseline a `1.0`.

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Carga de fondo | load average 1,22–1,29 durante la ventana canónica (host compartido) |

> Nota: el handoff histórico describe el sandbox como «2 vCPU»; el entorno
> actual expone 20 hilos lógicos. El bench es **monohilo**.

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Pasada completa (compila release + ejecuta + guarda target/bench-latest.txt)
./tools/bench.sh

# Las 5 pasadas usadas para la tabla de esta baseline
for i in 1 2 3 4 5; do
  cargo run --release -p bench -- --json > "target/bench-run$i.json"
done

# Smoke rápido para CI / portátil (~1 s)
./tools/bench.sh --scale 0.1 --iterations 2

# Un grupo o un benchmark concreto
./tools/bench.sh --filter layout
cargo run --release -p bench -- --filter layout/realistic-1mb

# Benchmarks opcionales del motor de fuentes (feature no incluida por defecto)
BENCH_FEATURES=fonts ./tools/bench.sh --filter fonts

# CLI render (no es benchmark del crate bench): pipeline completo + PNG
cargo run --release -p render -- tests/fixtures/phase4/basic.html /tmp/basic.png --json

# Listar nombres registrados / ayuda
cargo run --release -p bench -- --list
cargo run --release -p bench -- --help
```

`crates/bench` se construye con dependencias internas del workspace (`dom`,
`html`, `css`, `layout`, `paint`; `fonts` solo bajo `--features fonts`) y pasa
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y
`cargo test -p bench` (8 tests unitarios).

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb` | Parse de documento «realista» de ~1 MiB (headings, párrafos, inline, listas, tablas, 135 reglas CSS embebidas). |
| html | `html/nested-div-10k/32k/64k` | Parse de `<div>` anidados 10k/32k/64k (detector clásico de árbol cuadrático). |
| html | `html/nested-div-64k-default-limits` | El mismo input con los límites anti-DoS de producción (profundidad 4096). |
| html | `html/massive-end-p` | 100 000 `</p>` consecutivos. |
| html | `html/massive-formatting` | 10 000 bloques `<b><i><u>` mal anidados (adoption agency). |
| html | `html/tag-40k-attrs` | Un elemento con 40 000 atributos. |
| html | `html/pretty-5k-blocks` | Documento indentado con 5 000 bloques. |
| css | `css/parse-10k-rules` | Parse de hoja CSS con 10 000 reglas. |
| css | `css/resolve-styles-2k/10k-nodes` | Cascada + `ComputedStyle` sobre DOM de ~2 005 / ~10 005 elementos. |
| css | `css/selector-match-simple` | Matching de 4 selectores simples sobre 10 005 elementos. |
| css | `css/selector-match-descendant` | Matching de 4 selectores con combinadores sobre 10 005 elementos. |
| layout | `layout/realistic-1mb` | `layout_document` sobre el documento realista ya parseado y con estilos resueltos (~28 195 elementos, **88 155 cajas**), 800×600. Incluye el nuevo layout de tablas. |
| paint | `paint/build-display-list` | Construcción de la display list (24 762 items) con la nueva ordenación de apilado y grupos de opacidad. |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA a un `Pixmap` 800×600. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` con **compresión DEFLATE** propia (54 526 B para el documento realista). |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo: parse → styles → layout → display list → raster → PNG. |
| fonts | `fonts/measure-1k-words` | `Font::measure_text` de 1 000 palabras (DejaVu Sans 16 px). Feature `fonts`. |
| fonts | `fonts/shape-1k-words` | `Font::shape` de 1 000 palabras. Feature `fonts`. |

---

## 3. Baseline vigente (2026-11-10, HEAD `49a2792` limpio)

Tabla = mediana por fila de **5 pasadas completas** (5 iteraciones, 1 warm-up;
*heavy* 3). `setup` de fixtures: ~301 ms (no cronometrado). Binario
`sha256 = e9fceca…`. La pasada del script dio `TOTAL 560.090 ms` (dentro de la
varianza).

| Benchmark | Carga | iter | mediana (ms) | rango 5 pasadas (ms) | throughput |
|---|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | 5 | **57.234** | 55.512–60.279 | 17.5 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | 5 | **3.910** | 3.841–4.120 | 25.6 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | 3 | **12.284** | 12.065–13.010 | 27.7 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | 3 | **24.564** | 24.399–26.841 | 27.3 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | 5 | **14.771** | 14.475–15.503 | — (59 906 start tags descartados, 4 100 nodos) |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | 5 | **24.736** | 23.813–25.504 | 4.04 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | 5 | **16.493** | 16.108–16.939 | 606 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | 3 | **12.623** | 12.481–12.906 | 3.17 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | 5 | **27.513** | 27.026–28.325 | 182 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | 5 | **14.763** | 13.323–16.031 | 40.6 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | 5 | **5.871** | 5.689–6.051 | 342 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | 5 | **28.199** | 27.585–28.532 | 355 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | 5 | **0.662** | 0.651–0.671 | 60.5 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | 5 | **1.224** | 1.216–1.236 | 32.7 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, **88 155 cajas**, 800×600 | 5 | **84.958** | 84.705–97.013 | 332 k elementos/s |
| `paint/build-display-list` | 24 762 items | 5 | **12.237** | 12.202–12.572 | — |
| `paint/rasterize-800x600` | 24 762 items → pixmap 800×600 | 3 | **0.899** | 0.768–0.949 | 534 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (**54 526 B**) | 3 | **12.964** | 12.713–13.086 | 4.01 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | 3 | **214.159** | 212.241–219.478 | — |
| **TOTAL (suma)** | | | **570.064** | | |

> **Aviso sobre el TOTAL**: suma de medianas de las etapas y del pipeline
> end-to-end (solapados). No es un presupuesto de página.

### 3.1 Baseline opcional del motor de fuentes

| Benchmark | Carga | iter | mediana (ms) | mínimo (ms) | throughput |
|---|---|---:|---:|---:|---:|
| `fonts/measure-1k-words` | 1 000 palabras, DejaVu Sans 16 px | 5 | **0.453** | 0.439 | 2.21 M palabras/s |
| `fonts/shape-1k-words` | 1 000 palabras, DejaVu Sans 16 px | 5 | **7.846** | 7.754 | 127 k palabras/s |

---

## 4. Complejidad observada

Mediciones de escalado con el mismo binario (1–5 iteraciones, sin warm-up),
complementarias a la tabla de §3 (ventana con load 1,2–2,7: se usan para
ratios, no para valores absolutos):

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.91 ms · 32k: 12.28 ms · 64k: 24.56 ms | **Lineal**: 3,2× input → 3,14× tiempo; 2× input → 2,00×; throughput ~25–28 MiB/s. La cuadrática del tree builder sigue eliminada. |
| `html/tag-40k-attrs` | 10k: 2.52 ms · 20k: 5.80 ms · 40k: 12.62 ms | **Lineal** (~0,3 µs/atributo). |
| `css/parse-10k-rules` | 5k: 6.90 ms · 10k: 14.20 ms · 20k: 30.48 ms | **Lineal** (2,06× y 2,15× por cada 2×). |
| `css/resolve-styles-2k/10k` | 2 005 el: 5.87 ms · 10 005 el: 28.20 ms | **Lineal**: 5× elementos → 4,80× tiempo (~355 k elementos/s). |
| `css/selector-match-*` | 60,5 M matches/s simples · 32,7 M con combinadores | Lineal por elemento; no es cuello de botella. |
| `paint/rasterize-800x600` | 4 863 items: 0.771 ms · 14 824: 1.006 ms · 24 762: 0.747 ms | **Plano respecto al nº de items** (~534–643 MPix/s): el coste lo fija el viewport, no la display list. El culling/cachés siguen funcionando con el painter nuevo. |
| `layout/realistic-1mb` | 28 195 elementos → 84,96 ms y **88 155 cajas** | El árbol de cajas creció +78,0 % respecto a r4 (49 527 → 88 155) y el layout +22,7 %; el desglose y la causa están en §5. |

---

## 5. Investigación: ¿por qué subió `layout/realistic-1mb`? (tablas)

### 5.1 Magnitud real (verificada)

El agente de `paint` reportó `layout/realistic-1mb` ~49,5 → ~98,5 ms y un árbol
de layout de **88 155 cajas** en la ronda 5. Lo reproducimos:

- **Cajas: 88 155 exactas** para el documento realista del bench (contadas con
  una herramienta auxiliar que reutiliza el generador exacto del crate `bench`,
  fuera del repo; ver `target/bench-layout-investigation/results.md`).
- **Tiempo**: la subida es real pero **~1,2–1,3×, no ~2×**:
  - bench `layout/realistic-1mb`: r4 **69.248 ms** → r5 **84.958 ms** (**×1,23**;
    rango de las 5 pasadas 84,7–97,0 ms).
  - misma herramienta y mismo documento en ambos commits: r4 70,295 ms →
    r5 83,424 ms (**×1,19**).
  - El valor ~98 ms es la **cola alta** bajo carga (la pasada del script bajo
    load ~1,3 dio 95,6 ms; con load >2 se vieron picos de 97 ms), no la mediana.
- Comparación directa en `git worktree` temporal de `8f273f0` (mismo hardware):
  bench r4 medido de nuevo 70,157 ms; r5 84,958 ms. Consistente.

### 5.2 Causa: tablas (wrapper/anónimas + IFC por celda), no un post-pase

El documento sintético del bench **siempre ha contenido 1 044 tablas** de 3×3
(9 396 celdas, una por sección); la ronda 5 es la primera que implementa layout
de tablas (`crates/layout/src/table.rs`, `Display::Table*` en `tree_builder.rs`).

Árbol de cajas, mismo documento (conteo independiente):

| Tipo de caja | r4 (`8f273f0`) | r5 (`49a2792`) | Δ |
|---|---:|---:|---:|
| BlockNode | 11 487 | 10 443 | −1 044 |
| InlineNode | 4 176 | 3 132 | −1 044 |
| LineBox | 14 322 | 22 674 | **+8 352** |
| TextNode | 19 542 | 32 070 | **+12 528** |
| Table / RowGroup / Row / Cell / AnonRow / AnonCell | 0 | 1 044 / 1 044 / 3 132 / 9 396 / 4 176 / 1 044 | **+19 836** |
| **TOTAL** | **49 527** | **88 155** | **+38 628 (+78,0 %)** |

- La tabla aporta **19 836 cajas nuevas** (wrappers, filas/celdas y dos tipos
  anónimos de *fix-up*); el resto del crecimiento (+20 880) son cajas de línea
  (+8 352) y fragmentos de texto (+12 528) porque cada celda ahora dispone de
  su propio IFC. No aparecen cajas `InlineBlock`/`Replaced` en este documento
  (0), así que esas features no contribuyen aquí.
- Desglose temporal (misma herramienta, mediana de 6):
  - r4: `build_layout_tree` 11,966 ms + `layout_block_with_measurer` 57,974 ms
    = 69,940 ms.
  - r5: `build_layout_tree` 14,599 ms (+22 %, +2,63 ms) +
    `layout_block_with_measurer` 77,525 ms (+34 %, +19,55 ms) = 92,124 ms.
  - **~88 % del coste añadido está en el pase de layout**, no en la
    construcción del árbol.
- En el pase de layout, `table::compute_column_widths` mide el ancho intrínseco
  (`cell_intrinsic_widths`, min/max) de **cada una de las 9 396 celdas** antes
  de colocar las filas, y después `layout_row` vuelve a recorrerlas: es un
  doble recorrido por tabla, sobre un árbol 78 % mayor.
- **No hay post-pase global nuevo**: el diff de `crates/layout` solo añade
  `table.rs` y APIs aditivas (`layout_document_with_replaced_sizes`, etc.);
  `layout_document` conserva su firma y su flujo.

### 5.3 Efecto colateral en `paint/build-display-list`

`paint/build-display-list` pasa de 8,108 a 12,237 ms (**×1,51**) **con los
mismos 24 762 items**: el painter iterativo nuevo recorre el árbol de cajas
(88 155 vs 49 527, +78 %) en sus distintos recorridos; no hay más items que
pintar, sino más cajas que visitar (y un painter reescrito con pilas
explícitas). El rasterizado no cambia (`0,93 → 0,90 ms`).

### 5.4 Conclusión y WP propuesto

- El empeoramiento de layout es **×1,23 (< 1,5×)**: no dispara la alarma, pero
  suma +22,7 % en layout y contribuye al +27 % del pipeline.
- No es un problema de conformidad: el trabajo por caja corresponde a features
  nuevas reales (tablas CSS 2.1, inline-block y reemplazados no usados aquí).
- Si se quiere recuperar el terreno, el trabajo debería ir a un
  **WP de optimización de tablas** (candidato **WP-4.13**, o el ID que asigne
  el ROADMAP): (a) memoizar min/max intrínseco por celda y por tabla para no
  medir dos veces; (b) evitar materializar cajas de línea de celdas hasta la
  fase de pintado/medida final; (c) reducir cajas anónimas de *fix-up*
  reutilizando el row-group implícito; objetivo: volver a ≤ 70 ms en
  `layout/realistic-1mb` y ≤ 50 ms en el pipeline de estilo/layout.
- El WP de `paint` (si se aborda) es independiente: reducir el coste de
  recorrido para árboles grandes sin tocar el rasterizado.

---

## 6. Compresión PNG y pipeline `render`

La ronda 5 sustituye el deflate *stored* del encoder PNG por **DEFLATE
comprimido propio** (`crates/paint/src/deflate.rs`). Medido en el bench
(documento realista 800×600) y con el CLI `render`:

| Métrica | r4 (stored) | r5 (comprimido) | Factor |
|---|---:|---:|---:|
| PNG del bench (`render-to-png`, documento realista) | 1 920 813 B | **54 526 B** | **×35,2 menor** (−97,2 %) |
| Tiempo `paint/render-to-png-800x600` | 5,852 ms | **12,964 ms** | ×2,22 |
| — desglose: rasterizar | 0,932 ms | 0,899 ms | ×0,96 |
| — desglose: codificar PNG (por diferencia) | ~4,92 ms | **~12,07 ms** | ×2,45 |
| PNG de `tests/fixtures/phase4/basic.html` (CLI `render`) | 1 920 813 B | **42 853 B** | ×44,8 (−97,8 %) |

El coste de comprimir (~+7,2 ms por PNG 800×600) se paga a cambio de reducir el
PNG 35–45×; en un pipeline con I/O, caché HTTP o memoria de imágenes, el
intercambio es claramente favorable. En imágenes muy pequeñas el coste fijo de
la compresión domina (ver abajo).

**Pipeline `render` (CLI, release, mediana de 7 ejecuciones de pared):**

| Fixture | Pared (ms) | PNG | `timings` del `--json` (ms) |
|---|---:|---:|---|
| `tests/fixtures/phase4/basic.html` | **18,01** (16,57–19,07) | 42 853 B | total 16,40 · fetch 0,027 · decode+parse 0,200 · styles 0,380 · **layout 1,377** · paint 0,014 · raster 2,539 · resto (PNG+setup) **≈11,86** |
| `tests/fixtures/render/basic.html` | **15,14** (14,39–16,05) | 21 090 B | total 13,92 · layout 0,653 · raster 1,380 · resto (PNG+setup) **≈11,48** |

Lectura: el pipeline `render` de una página 800×600 tarda ~15–18 ms, de los
cuales **~70 % es la codificación PNG** (~11,5–11,9 ms), ~8–14 % el rasterizado
y ~1–4 % layout/paint. Si se prioriza latencia de render por encima de tamaño de
salida, el siguiente objetivo es el encoder (p. ej. elegir DEFLATE *stored* para
imágenes con salida pequeña o paralelizar/cachear la compresión); si se prioriza
I/O (el caso de `<img>`/caché), el intercambio actual es el correcto.

---

## 7. Coste de floats, posicionamiento, stacking y opacidad (referencia r4)

Estas features se midieron en la ronda 4 con el driver `html_to_png` sobre
2 000 cajas idénticas (15 ejecuciones intercaladas; overhead del driver
~11,2 ms); los valores siguen siendo la referencia de coste relativo:

| Variante | Δ vs control | Factor |
|---|---:|---:|
| `control` (bloques) | — | 1,00× |
| `position: relative` | +0,93–1,13 ms | 1,02× |
| `position: relative; z-index` | +0,20–0,59 ms | 1,00–1,01× |
| `opacity: 0.85` | +2,96–3,55 ms | 1,05–1,06× |
| `float: left` | +12,54–12,72 ms | **1,22×** |

El documento realista del bench no usa floats ni opacidad; estas cifras no
intervienen en la baseline de §3. Artefactos del experimento original en
`target/bench-feature/` (si se regeneran en el futuro).

---

## 8. Presupuestos orientativos

Presupuestos para esta máquina y build `release` (rasterizador software,
monohilo, sin GPU). Un benchmark que supere el presupuesto debe justificarse.

| Benchmark | Baseline | Presupuesto | Estado |
|---|---:|---:|---|
| `html/realistic-1mb` | 57 ms | ≤ 100 ms | ✅ |
| `html/nested-div-64k` | 25 ms | ≤ 100 ms | ✅ (lineal) |
| `html/massive-end-p` | 25 ms | ≤ 50 ms | ✅ |
| `html/massive-formatting` | 16 ms | ≤ 50 ms | ✅ |
| `html/tag-40k-attrs` | 13 ms | ≤ 50 ms | ✅ |
| `html/pretty-5k-blocks` | 28 ms | ≤ 75 ms | ✅ |
| `css/parse-10k-rules` | 15 ms | ≤ 30 ms | ✅ |
| `css/resolve-styles-10k-nodes` | 28 ms | ≤ 75 ms | ✅ |
| `layout/realistic-1mb` | 85 ms | ≤ 150 ms | ✅ (×1,23 vs r4) |
| `paint/build-display-list` | 12,2 ms | ≤ 20 ms | ✅ (×1,51 vs r4) |
| `paint/rasterize-800x600` | 0,90 ms | ≤ 25 ms | ✅ |
| `paint/render-to-png-800x600` | 13,0 ms | ≤ 50 ms | ✅ (compresión incluida) |
| `pipeline/html-to-png-800x600` | 214 ms | ≤ 250 ms | ✅ (**85,7 % del presupuesto**, margen 36 ms) |

**¿Pipeline < 250 ms?** Sí: 214,2 ms (85,7 % del presupuesto). El margen se ha
reducido (r4: 168,6 ms = 67 %) por el layout de tablas y la compresión PNG;
si no se optimiza tablas en la próxima ronda, el presupuesto de 250 ms puede
quedar justo. **¿Layout sigue dominando?** Sí: 85,0 ms = **39,7 % del pipeline**;
le siguen parse (57,2 ms = 26,7 %), estilos+resto (~46,8 ms = 21,8 %), display
list (12,2 ms = 5,7 %) y PNG (12,1 ms = 5,6 %). El rasterizado es ~0,4 %.

---

## 9. Comparación histórica

Cuatro baselines en la misma máquina, mismo arnés:

| | Ronda 2 | Ronda 3 | Ronda 4 | Ronda 5 (vigente) |
|---|---|---|---|---|
| Fecha | 2026-11-10 09:45 | 2026-11-10 14:09 | 2026-11-10 14:41 | 2026-11-10 15:15 |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | **`49a2792`** |
| Árbol | dirty (42) | limpio | limpio | **limpio al inicio** |
| Binario (sha256) | `0fd520aa…` | `9936527c…` | `d752a856…` | **`e9fceca…`** |

Filas clave (mediana, ms):

| Benchmark | Ronda 2 | Ronda 3 | Ronda 4 | Ronda 5 |
|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | **57.234** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | **24.564** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | **28.199** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | **84.958** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | **12.237** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | **0.899** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | **12.964** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | **214.159** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** |

### Ronda 4 → ronda 5, fila a fila

| Benchmark | Ronda 4 (ms) | Ronda 5 (ms) | Factor |
|---|---:|---:|---|
| `html/realistic-1mb` | 55.569 | **57.234** | 1,03× más lento |
| `html/nested-div-10k` | 3.688 | **3.910** | 1,06× más lento |
| `html/nested-div-32k` | 11.786 | **12.284** | 1,04× más lento |
| `html/nested-div-64k` | 24.050 | **24.564** | 1,02× más lento |
| `html/nested-div-64k-default-limits` | 14.524 | **14.771** | 1,02× más lento |
| `html/massive-end-p` | 22.851 | **24.736** | 1,08× más lento |
| `html/massive-formatting` | 15.719 | **16.493** | 1,05× más lento |
| `html/tag-40k-attrs` | 12.489 | **12.623** | 1,01× más lento |
| `html/pretty-5k-blocks` | 26.193 | **27.513** | 1,05× más lento |
| `css/parse-10k-rules` | 14.191 | **14.763** | 1,04× más lento |
| `css/resolve-styles-2k-nodes` | 5.873 | **5.871** | 1,00× (sin cambio) |
| `css/resolve-styles-10k-nodes` | 27.604 | **28.199** | 1,02× más lento |
| `css/selector-match-simple` | 0.653 | **0.662** | 1,01× más lento |
| `css/selector-match-descendant` | 1.248 | **1.224** | 1,02× más rápido |
| `layout/realistic-1mb` | 69.248 | **84.958** | **1,23× más lento** |
| `paint/build-display-list` | 8.108 | **12.237** | **1,51× más lento** |
| `paint/rasterize-800x600` | 0.932 | **0.899** | 1,04× más rápido |
| `paint/render-to-png-800x600` | 5.852 | **12.964** | **2,22× más lento** (compresión) |
| `pipeline/html-to-png-800x600` | 168.612 | **214.159** | **1,27× más lento** |
| **TOTAL (suma)** | **489.190** | **570.064** | **1,17× más lento** |

---

## 10. Cómo comparar y detectar regresiones

1. Medir antes/después en la misma máquina y con el sistema ocioso
   (`uptime` < ~1.5 de load average); misma `--scale` e iteraciones.
2. Guardar el JSON (`target/bench-latest.json`) y comparar medianas:

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
3. Regresión si la mediana empeora >15 % de forma reproducible. En esta
   máquina la dispersión entre pasadas completas es ~1–10 % (algunos benchmarks
   de pintado de ~1 ms llegan al 20 % con el host cargado).
4. Para cambios de complejidad, usar el escalado (`--scale 0.5` vs `1.0`) y
   comprobar que el tiempo crece como el tamaño (ratio ≈1: §4).
5. La suite completa tarda ~3–4 s de reloj (más ~0,3 s de fixtures); `--scale
   0.1 --iterations 2` tarda ~1 s.

JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
`worktree_dirty`, `rustc`, `cpu`, `cores`, `scale`, `iterations`, `warmup`,
`setup_ms`, `total_median_ms`, `results[]` (con `name`, `group`, `detail`,
`iterations`, `median_ms`, `min_ms`, `samples_ms`, `throughput`) y `skipped[]`.
Los campos existentes no cambian; solo se añaden campos nuevos.

---

## 11. CI

La suite **se ejecuta a mano** (`./tools/bench.sh`); no se ha añadido un job
para no tocar `.github/workflows/`. Es barata (~3–4 s) y sin dependencias
externas, así que un job opcional `workflow_dispatch` es trivial:

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

Los números de CI no son comparables (otra CPU, runners compartidos); el job
solo detecta que compila y ejecuta, y el JSON queda como artefacto.

---

## 12. Limitaciones y notas

- **Baseline sobre commit limpio**: corresponde a `49a2792`; el árbol estaba
  limpio al empezar y el binario se compiló de ese commit. La documentación
  cambió por otro agente durante la ventana (no afecta al código medido).
- **Host compartido**: load 1,22–1,29 en la ventana canónica; se descartó una
  tanda previa con load >2 que produjo outliers de ×2 en parse de divs y
  rasterizado. Se usa mediana de 5 pasadas.
- **Tooling de investigación fuera del repo**: los conteos de cajas y el
  desglose build/layout se hicieron con un crate auxiliar en `/tmp` que
  reutiliza el generador del bench; el repo principal no se modificó y el
  `git worktree` temporal de `8f273f0` se creó y eliminó durante la
  investigación (verificado: `git worktree list` solo muestra el repo).
- **`render` CLI**: las medidas de pared incluyen arranque del proceso (~1,2 ms)
  y carga del font DB; los `timings` del `--json` no incluyen la codificación
  PNG (se calcula por diferencia).
- **Compresión PNG**: el sobrecoste medido (~+7,2 ms por PNG 800×600) es del
  encoder DEFLATE propio; cualquier comparación futura debe usar el mismo
  tamaño de viewport/imagen.
- **Rasterizado software**: CPU pura, monohilo, sin GPU; 800×600 es el viewport
  de los tests de Fase 4/5.
- **Cobertura**: no se benchmarkean `crates/render`, `crates/net` ni
  `crates/image` (el bench no los incluye); el pipeline `render` se mide aparte
  con su CLI. El documento realista no contiene floats, inline-block ni
  imágenes reemplazadas: su coste se referencia en §7 y no entra en la baseline.
