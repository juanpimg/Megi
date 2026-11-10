# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 14:09–14:11 (Europe/Madrid).
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `532bb6a`**
  (`docs: add compressed Phase 4 render screenshot`; es `14500e6` + un commit
  de documentación). `14500e6` es ancestro de `532bb6a`.
- **Estado del árbol en la medición**: **limpio** (`git status --porcelain`
  vacío). A diferencia de la baseline anterior (medida sobre un árbol *dirty*
  con agentes del sprint en paralelo), esta refleja exactamente el código
  commiteado.
- **Binario medido**: `target/release/bench`,
  `sha256 = 9936527c437ce31b4dcec8c0ef05772931daeb6415a5edf3cf92eb9f7afa37da`,
  4 062 176 bytes, compilado con `cargo build --release -p bench` el
  2026-11-10T14:10:58+01:00.
- **Artefactos guardados** (gitignored, no versionados):
  - `target/bench-run1.json`, `target/bench-run2.json`, `target/bench-run3.json`
    — las 3 pasadas completas usadas para la tabla (mediana por fila).
  - `target/bench-aggregate.json` — medianas agregadas de las 3 pasadas.
  - `target/bench-latest.txt` / `target/bench-latest.json` — pasada de
    reproducción del script y su JSON (misma build).
  - `target/bench-latest-meta.txt` — commit/rustc/args/fecha de esa pasada.
  - `target/bench-fonts-latest.txt` — baseline opcional del motor de fuentes.

> La baseline anterior (2026-11-10 09:45, HEAD `9cca2e1`, árbol dirty, binario
> `sha256 = 0fd520aa…`) queda **reemplazada**. Sus cifras solo se conservan en
> la tabla de comparación de §6.

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
- La tabla de §5 es la **mediana, por fila, de 3 pasadas completas** de la
  suite (cada pasada ya reporta su propia mediana). La columna «rango» muestra
  el mínimo–máximo de las 3 medianas: la dispersión observada fue **0,0 %–7,6 %**
  (la mayoría ≤3 %).
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
| Carga de fondo | load average 0,6–1,1 sobre 20 hilos; sin agentes ni `cargo` de otros procesos durante la ventana |

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
| layout | `layout/realistic-1mb` | `layout_document` sobre el documento realista ya parseado y con estilos resueltos (~28 195 elementos), viewport 800×600. |
| paint | `paint/build-display-list` | Construcción de la display list (19 542 items) desde el árbol de layout. |
| paint | `paint/rasterize-800x600` | Rasterizado software **con culling/cachés** de la display list a un `Pixmap` 800×600 (sin PNG). |
| paint | `paint/render-to-png-800x600` | Rasterizado + `Pixmap::to_png()` (PNG determinista propio, 1 920 813 B). |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo: parse → styles → layout → display list → raster → PNG. |
| fonts | `fonts/measure-1k-words` | `Font::measure_text` de 1 000 palabras (DejaVu Sans 16 px). Feature `fonts`. |
| fonts | `fonts/shape-1k-words` | `Font::shape` de 1 000 palabras (shaping/kern/ligaduras). Feature `fonts`. |

---

## 3. Baseline vigente (2026-11-10, HEAD `532bb6a` limpio)

Tabla = mediana por fila de **3 pasadas completas** de la suite (5 iteraciones,
1 warm-up; cargas *heavy* limitadas a 3). `setup` de fixtures: ~223 ms (no
cronometrado). El binario `sha256 = 9936527c…` es el medido.

| Benchmark | Carga | iter | mediana (ms) | rango 3 pasadas (ms) | throughput |
|---|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | 5 | **52.530** | 52.462–53.717 | 19.0 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | 5 | **3.613** | 3.556–3.644 | 27.7 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | 3 | **11.497** | 11.460–12.100 | 29.6 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | 3 | **23.534** | 23.306–23.565 | 28.5 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | 5 | **13.898** | 13.893–13.899 | — (59 906 start tags descartados, 4 100 nodos) |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | 5 | **22.069** | 21.436–22.544 | 4.53 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | 5 | **14.730** | 14.730–14.775 | 679 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | 3 | **12.106** | 12.007–12.202 | 3.30 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | 5 | **25.116** | 25.005–25.292 | 199 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | 5 | **12.814** | 12.783–12.966 | 46.8 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | 5 | **5.687** | 5.347–5.781 | 353 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | 5 | **27.037** | 26.810–27.325 | 370 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | 5 | **0.652** | 0.650–0.682 | 61.4 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | 5 | **1.245** | 1.222–1.256 | 32.1 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos con estilo, 800×600 | 5 | **63.541** | 62.476–63.760 | 444 k elementos/s |
| `paint/build-display-list` | 19 542 items | 5 | **3.802** | 3.798–3.872 | — |
| `paint/rasterize-800x600` | 19 542 items → pixmap 800×600 | 3 | **0.861** | 0.838–0.882 | 557 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (1.83 MiB) | 3 | **5.194** | 5.158–5.291 | 352 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | 3 | **152.519** | 148.794–153.385 | — |
| **TOTAL (suma)** | | | **452.445** | | |

> **Aviso sobre el TOTAL**: la suma incluye a la vez las etapas sueltas y el
> pipeline end-to-end, que las solapa. No es un presupuesto de página; sirve
> solo como métrica agregada de la suite.

### 3.1 Baseline opcional del motor de fuentes

Compilada con `cargo build --release -p bench --features fonts` (feature
desactivada por defecto; carga `crates/fonts/assets/DejaVuSans.ttf` y reporta
`SKIP` si el fixture falta). Medida el mismo día sobre el mismo HEAD.

| Benchmark | Carga | iter | mediana (ms) | mínimo (ms) | throughput |
|---|---|---:|---:|---:|---:|
| `fonts/measure-1k-words` | 1 000 palabras, DejaVu Sans 16 px | 5 | **0.429** | 0.427 | 2.33 M palabras/s |
| `fonts/shape-1k-words` | 1 000 palabras, DejaVu Sans 16 px | 5 | **7.459** | 7.201 | 134 k palabras/s |

---

## 4. Complejidad observada

Mediciones de escalado con el mismo binario (1–5 iteraciones, sin warm-up),
complementarias a la tabla de §3:

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.61 ms · 32k: 11.50 ms · 64k: 23.53 ms | **Lineal**: 3,2× input → 3,18× tiempo; 2× input → 2,05× tiempo. Throughput ~28–30 MiB/s constante. La complejidad cuadrática del tree builder (hallazgo F2 de la auditoría) sigue eliminada. |
| `html/nested-div-64k-default-limits` | 13.90 ms (59 906 start tags descartados) | Los límites anti-DoS cortan a profundidad 4 096; el coste es el del guardián, no el del árbol completo. |
| `html/tag-40k-attrs` | 10k: 2.49 ms · 20k: 5.39 ms · 40k: 11.94 ms | **Lineal** (~0,3 µs/atributo). |
| `css/parse-10k-rules` | 5k: 6.28 ms · 10k: 13.67 ms · 20k: 28.40 ms | **Lineal** (~2,1× por cada 2×; ~44–47 MiB/s). |
| `css/resolve-styles-2k/10k` | 2 005 el: 5.69 ms · 10 005 el: 27.04 ms | **Lineal**: 5× elementos → 4,75× tiempo (~370 k elementos/s), y **2,2–2,3× más rápido** que la baseline anterior (indexación de reglas CSS). |
| `css/selector-match-*` | 5 005 el: simple 0.298 ms / descendiente 0.565 ms · 10 005 el: 0.610 / 1.200 ms | **Lineal** en elementos (2,05× / 2,12× al duplicar). 61–67 M / 33–35 M matches/s. |
| `layout/realistic-1mb` | 28 195 elementos → 63.54 ms | ~444 k elementos/s. **1,28× más lento** que la baseline anterior; coincide con la entrada del IFC y métricas nuevas de la ronda. No es el hotspot, pero conviene vigilarlo. |
| `paint/rasterize-800x600` | 3 838 items: 0.784 ms · 11 699 items: 0.784 ms · 19 542 items: 0.793 ms | **Plano respecto al número de items**: el culling/cachés de la ronda hacen que el coste dependa del viewport (~557–612 MPix/s), no de la display list. Antes era lineal (~76 µs/item): 1 484,8 ms para 19 542 items. |
| `paint/render-to-png-800x600` | 5.19 ms total − 0.86 ms raster ≈ **4.33 ms de PNG** | El rasterizado ya no domina la cola de paint: el **codificador PNG** (DEFLATE *stored*) cuesta ~5× más que rasterizar. Próximo objetivo de paint si se quiere bajar de ~5 ms. |
| `pipeline/html-to-png-800x600` | 152.52 ms | Composición: parse 52.5 ms (~34 %), layout 63.5 ms (~42 %) y resto (estilos + display list + raster + PNG) ≈36.5 ms (~24 %). El rasterizado, antes ~84 % del end-to-end, ahora es ~0,6 %. |

---

## 5. Presupuestos orientativos

Presupuestos de ingeniería para esta máquina y build `release` (no son
comparaciones con navegadores comerciales; el rasterizador es software puro,
monohilo y sin GPU). Un benchmark que supere el presupuesto debe justificarse o
abrir un follow-up.

| Benchmark | Baseline | Presupuesto | Estado |
|---|---:|---:|---|
| `html/realistic-1mb` | 53 ms | ≤ 100 ms | ✅ |
| `html/nested-div-64k` | 24 ms | ≤ 100 ms | ✅ (lineal) |
| `html/massive-end-p` | 22 ms | ≤ 50 ms | ✅ |
| `html/massive-formatting` | 15 ms | ≤ 50 ms | ✅ |
| `html/tag-40k-attrs` | 12 ms | ≤ 50 ms | ✅ |
| `html/pretty-5k-blocks` | 25 ms | ≤ 75 ms | ✅ |
| `css/parse-10k-rules` | 13 ms | ≤ 30 ms | ✅ |
| `css/resolve-styles-10k-nodes` | 27 ms | ≤ 75 ms | ✅ |
| `layout/realistic-1mb` | 64 ms | ≤ 150 ms | ✅ |
| `paint/build-display-list` | 3.8 ms | ≤ 20 ms | ✅ |
| `paint/rasterize-800x600` | 0.86 ms | ≤ 25 ms | ✅ |
| `paint/render-to-png-800x600` | 5.2 ms | ≤ 50 ms | ✅ |
| `pipeline/html-to-png-800x600` | 153 ms | ≤ 250 ms | ✅ |

Todos los presupuestos se cumplen tras la ronda 3. El margen más ajustado está
en `pipeline/html-to-png-800x600` (153 ms de 250 ms) y en `layout/realistic-1mb`
(64 ms de 150 ms); ambos dominados por CPU de parse/layout, no por pintado.

---

## 6. Comparación con la baseline anterior

- **Antes**: 2026-11-10 09:45, HEAD `9cca2e1`, árbol **dirty** (42 rutas),
  binario `sha256 = 0fd520aaf4fc87f2450724757d04b59a899a1255a94562cfdb9a0001805f00bf`.
- **Ahora**: 2026-11-10 14:09–14:11, HEAD `532bb6a` (`14500e6` + docs), árbol
  **limpio**, binario `sha256 = 9936527c…`.

La comparación mezcla el cambio de código de la ronda con el paso de árbol
*dirty* a *clean*; aun así, los factores grandes (raster/PNG/pipeline) son
mejoras reales de órdenes de magnitud, no ruido.

| Benchmark | Antes (ms) | Ahora (ms) | Factor |
|---|---:|---:|---|
| `html/realistic-1mb` | 51.179 | **52.530** | 1,03× más lento |
| `html/nested-div-10k` | 3.355 | **3.613** | 1,08× más lento |
| `html/nested-div-32k` | 10.638 | **11.497** | 1,08× más lento |
| `html/nested-div-64k` | 21.262 | **23.534** | 1,11× más lento |
| `html/nested-div-64k-default-limits` | 13.364 | **13.898** | 1,04× más lento |
| `html/massive-end-p` | 20.129 | **22.069** | 1,10× más lento |
| `html/massive-formatting` | 14.138 | **14.730** | 1,04× más lento |
| `html/tag-40k-attrs` | 11.076 | **12.106** | 1,09× más lento |
| `html/pretty-5k-blocks` | 23.861 | **25.116** | 1,05× más lento |
| `css/parse-10k-rules` | 12.759 | **12.814** | 1,00× (sin cambio) |
| `css/resolve-styles-2k-nodes` | 12.701 | **5.687** | **2,23× más rápido** |
| `css/resolve-styles-10k-nodes` | 61.908 | **27.037** | **2,29× más rápido** |
| `css/selector-match-simple` | 0.638 | **0.652** | 1,02× más lento |
| `css/selector-match-descendant` | 1.182 | **1.245** | 1,05× más lento |
| `layout/realistic-1mb` | 49.506 | **63.541** | 1,28× más lento |
| `paint/build-display-list` | 3.356 | **3.802** | 1,13× más lento |
| `paint/rasterize-800x600` | 1 484.776 | **0.861** | **1 724× más rápido** |
| `paint/render-to-png-800x600` | 1 513.860 | **5.194** | **291× más rápido** |
| `pipeline/html-to-png-800x600` | 1 766.298 | **152.519** | **11,6× más rápido** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **11,2× más rápido** |

Lectura: la ronda 3 ha eliminado el antiguo hotspot de pintado (culling/cachés
del rasterizador) y, de paso, ha acelerado la resolución de estilos por
indexación de reglas. El resto de etapas de CPU quedan en el mismo orden
(pequeñas regresiones del 3–13 % atribuibles al nuevo código y al cambio de
árbol dirty→clean) salvo `layout`, un 28 % más lento, que pasa a ser la etapa
más cara junto con el parse.

---

## 7. Cómo comparar y detectar regresiones

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
   esta máquina, la dispersión entre pasadas completas de la suite es
   ~0–8 % (la mayoría <3 %); con el sistema cargado puede superar el 10 %.
4. Para cambios de complejidad, usar el escalado (`--scale 0.5` vs `1.0`) y
   comprobar que el tiempo crece como el tamaño (ratio ≈1: ver §4).
5. La suite completa tarda ~2,6 s de reloj (más ~0,2 s de fixtures); si se
   busca un chequeo rápido, `--scale 0.1 --iterations 2` tarda ~1 s.

JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
`worktree_dirty`, `rustc`, `cpu`, `cores`, `scale`, `iterations`, `warmup`,
`setup_ms`, `total_median_ms`, `results[]` (con `name`, `group`, `detail`,
`iterations`, `median_ms`, `min_ms`, `samples_ms`, `throughput`) y `skipped[]`.
Los campos existentes no cambian; solo se añaden campos nuevos.

---

## 8. CI

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

## 9. Limitaciones y notas

- **Baseline sobre árbol limpio**: esta tabla corresponde al commit `532bb6a`
  con `git status` vacío y el binario identificado; es reproducible desde un
  clon limpio con `./tools/bench.sh`.
- **Máquina portátil compartida**: i7-12700H híbrido, con *frequency scaling*;
  de ahí el uso de mediana/rango y la dispersión de ~0–8 %. Los números
  absolutos no son comparables con CI ni con otras máquinas, pero las **ratios
  de escalado** sí.
- **Comparación entre baselines**: la anterior se midió sobre un árbol *dirty*
  (agentes del sprint en paralelo) y con otro código; los factores grandes
  (raster, PNG, pipeline) son mejoras reales, las diferencias de ±3–13 % en
  etapas de CPU deben tomarse como indicativas.
- **Rasterizado software**: `paint/rasterize-800x600` mide CPU pura, monohilo,
  sin GPU; 800×600 es el viewport de los tests de Fase 4, no un tamaño de
  producción.
- **Parse incluye liberación del DOM**: los benchmarks de parse destruyen el
  `Document` dentro del cronómetro; es el coste real de parsear una página.
- **`--scale` cambia el mix**: las reglas CSS embebidas del documento realista
  son fijas (135) mientras las secciones escalan, y los benchmarks con nombres
  `*-10k`/`*-40k` conservan el nombre aunque la escala cambie el tamaño real;
  por eso `scale != 1.0` es solo para smoke tests y comparaciones relativas.
- **Cobertura**: el bench cubre parse HTML/CSS, cascada, selectores, layout BFC
  básico, display list, rasterizado y PNG. No cubre IFC con métricas reales de
  fuente dentro de layout, floats/posicionados ni el crate `net` (fase 5).
