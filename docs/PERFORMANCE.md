# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **primera baseline de rendimiento medida**, para que cualquier
regresión de complejidad sea detectable comparando números en lugar de
discutiendo impresiones.

- **Fecha de medición**: 2026-11-10, 09:45–09:47 (Europe/Madrid).
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, HEAD `9cca2e1`.
- **Estado del árbol en la medición**: **dirty** (42 rutas modificadas/sin
  trackear por los agentes del sprint que trabajaban en paralelo; ver
  «Limitaciones»). No es un commit limpio: la baseline corresponde al binario
  identificado abajo, no a una etiqueta de Git.
- **Binario medido**: `target/release/bench`,
  `sha256 = 0fd520aaf4fc87f2450724757d04b59a899a1255a94562cfdb9a0001805f00bf`,
  compilado con `cargo build --release -p bench` el 2026-11-10T09:45:26+01:00.
- **Artefactos guardados** (gitignored, no versionados):
  - `target/bench-latest.txt` — tabla de la pasada de referencia.
  - `target/bench-latest.json` — misma build, ejecución posterior en JSON.
  - `target/bench-latest-meta.txt` — commit/rustc/args/fecha de la pasada.
  - `target/bench-fonts-latest.txt` — baseline opcional del motor de fuentes.

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
- Se reportan **mediana** y **mínimo** de las muestras; la mediana absorbe el
  ruido del sistema y el mínimo aproxima el coste «en frío de contención».
- No se usa `criterion` ni `hyperfine`: el crate no tiene dependencias externas
  y el runner es un binario normal (`std` + crates internos del workspace).
- `--scale <factor>` multiplica todos los tamaños sintéticos (p. ej. `0.25`
  para un smoke test de ~10 s). Los resultados a escala distinta de `1.0` no
  son comparables directamente con la baseline.

### 1.3 Máquina

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` (opt-level 3) |
| Carga de fondo | load average 0.7–1.2 durante la pasada (otros agentes del sprint compilando/ejecutando) |

> Nota: el handoff del sprint describe el sandbox como «2 vCPU»; el entorno
> actual expone 20 hilos lógicos. El bench es **monohilo**, así que lo relevante
> es el núcleo efectivo y su estado térmico/frecuencia, no el total de núcleos.
> Aun así, la contención de otros procesos introduce varianza: se usa la
> mediana y se documenta el estado del árbol.

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Baseline completa (compila release + ejecuta + guarda target/bench-latest.txt)
./tools/bench.sh

# Equivalente manual, salida JSON estable (schema megi-bench-v1)
cargo run --release -p bench -- --json > target/bench-latest.json

# Smoke rápido para CI / portátil
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

`crates/bench` se construye siempre con dependencias internas del workspace
(`dom`, `html`, `css`, `layout`, `paint`; `fonts` solo bajo `--features fonts`)
y pasa `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y
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
| paint | `paint/rasterize-800x600` | Rasterizado software de la display list a un `Pixmap` 800×600 (sin PNG). |
| paint | `paint/render-to-png-800x600` | Rasterizado + `Pixmap::to_png()` (PNG determinista propio, 1 920 813 B). |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo: parse → styles → layout → display list → raster → PNG. |
| fonts | `fonts/measure-1k-words` | `Font::measure_text` de 1 000 palabras (DejaVu Sans 16 px). Feature `fonts`. |
| fonts | `fonts/shape-1k-words` | `Font::shape` de 1 000 palabras (shaping/kern/ligaduras). Feature `fonts`. |

---

## 3. Baseline medida (2026-11-10, HEAD `9cca2e1` dirty)

Pasada de referencia: `./tools/bench.sh` (5 iteraciones, 1 warm-up; cargas
*heavy* limitadas a 3). `setup` de fixtures: 1 769 ms (no cronometrado).
TOTAL = suma de medianas de todas las filas (ver aviso al final).

| Benchmark | Carga | iter | mediana (ms) | mínimo (ms) | throughput |
|---|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | 5 | **51.179** | 50.740 | 19.6 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | 5 | **3.355** | 3.307 | 31.3 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | 3 | **10.638** | 10.476 | 31.6 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | 3 | **21.262** | 21.156 | 31.6 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | 5 | **13.364** | 13.056 | — (59 906 start tags descartados, 4 100 nodos) |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | 5 | **20.129** | 19.988 | 4.97 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | 5 | **14.138** | 14.060 | 707 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | 3 | **11.076** | 11.046 | 3.61 M atributos/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | 5 | **23.861** | 23.618 | 210 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | 5 | **12.759** | 12.355 | 46.7 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | 5 | **12.701** | 12.452 | 158 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | 5 | **61.908** | 61.384 | 162 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | 5 | **0.638** | 0.619 | 62.7 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | 5 | **1.182** | 1.164 | 33.9 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos con estilo, 800×600 | 5 | **49.506** | 49.067 | 570 k elementos/s |
| `paint/build-display-list` | 19 542 items | 5 | **3.356** | 3.273 | — |
| `paint/rasterize-800x600` | 19 542 items → pixmap 800×600 | 3 | **1 484.776** | 1 483.850 | 0.32 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (1.83 MiB) | 3 | **1 513.860** | 1 504.633 | 1.21 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | 3 | **1 766.298** | 1 761.096 | — |
| **TOTAL (suma)** | | | **5 075.985** | | |

> **Aviso sobre el TOTAL**: la suma incluye a la vez las etapas sueltas y el
> pipeline end-to-end, que las solapa. No es un presupuesto de página; sirve
> solo como métrica agregada de la suite.

La ejecución `--json` inmediatamente posterior (misma build) dio
`total_median_ms = 5 194.376` (+2,3 %): es la varianza esperada en esta máquina
compartida. Para comparaciones usar la **mediana** y repetir con el sistema
ocioso.

### 3.1 Baseline opcional del motor de fuentes

Compilada con `cargo build --release -p bench --features fonts` (la feature
está desactivada por defecto para no acoplar `crates/bench` a la migración en
curso del crate `fonts`); carga `crates/fonts/assets/DejaVuSans.ttf` y, si el
fixture no existe, reporta `SKIP` en vez de fallar.

| Benchmark | Carga | iter | mediana (ms) | mínimo (ms) | throughput |
|---|---|---:|---:|---:|---:|
| `fonts/measure-1k-words` | 1 000 palabras, DejaVu Sans 16 px | 3 | **0.442** | 0.438 | 2.26 M palabras/s |
| `fonts/shape-1k-words` | 1 000 palabras, DejaVu Sans 16 px | 3 | **7.496** | 7.462 | 133 k palabras/s |

---

## 4. Complejidad observada

Mediciones de escalado con el mismo binario (1–2 iteraciones, sin warm-up):

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.36 ms · 32k: 10.64 ms · 64k: 21.26 ms | **Lineal**: 3,2× input → 3,17× tiempo; 2× input → 2,00× tiempo. Throughput constante ~31 MiB/s. La complejidad cuadrática del tree builder detectada en la auditoría (F2) **ya no está** en el estado medido. |
| `html/nested-div-64k-default-limits` | 13.36 ms (59 906 start tags descartados) | Los límites anti-DoS cortan a profundidad 4 096; el coste es el del guardián, no el del árbol completo. |
| `html/tag-40k-attrs` | 10k: 2.92 ms · 20k: 5.58 ms · 40k: 13.16 ms | **Lineal** (~3 µs/atributo). Importante: una build intermedia del propio sprint midió ~2 400 ms para 40k atributos (comportamiento cuadrático en la detección de atributos duplicados); la versión final medida lo resuelve en ~11 ms. El bench existe precisamente para cazar ese tipo de regresión. |
| `css/parse-10k-rules` | 5k: 6.22 ms · 10k: 12.76 ms · 20k: 27.27 ms | **Lineal** (~2× por cada 2×; ~46 MiB/s). |
| `css/resolve-styles-2k/10k` | 2 005 el: 12.70 ms · 10 005 el: 61.91 ms | **Lineal**: 5× elementos → 4,87× tiempo (~160 k elementos/s). |
| `css/selector-match-*` | simple: 0.638 ms · descendiente: 1.182 ms (10 005 el × 4 selectores) | 62,7 M / 33,9 M matches/s: no es un cuello de botella actual. |
| `layout/realistic-1mb` | 28 195 elementos → 49.51 ms | ~570 k elementos/s; proporcional al tamaño del documento realista. |
| `paint/rasterize-800x600` | 3 838 items: 299.6 ms · 11 699 items: 913.9 ms · 19 542 items: 1 484.8 ms | **Lineal en items** (~76 µs/item). Es el **punto caliente** del pipeline. |
| `paint/render-to-png-800x600` | 1 513.9 ms − 1 484.8 ms raster ≈ **29 ms** de PNG | El codificador PNG propio (DEFLATE *stored*) supone solo ~2 % del coste de paint; el rasterizado domina. |
| `pipeline/html-to-png-800x600` | 1 766.3 ms | Descomposición aproximada: parse 51 + styles ≈150 (por diferencia) + layout 49,5 + display list 3,4 + raster 1 485 + PNG 29 ≈ 1 768 ms. **El rasterizado es ~84 % del end-to-end.** |

---

## 5. Presupuestos orientativos

Presupuestos de ingeniería para esta máquina y build `release` (no son
comparaciones con navegadores comerciales; el rasterizador es software puro,
monohilo y sin GPU). Un benchmark que supere el presupuesto debe justificarse o
abrir un follow-up.

| Benchmark | Baseline | Presupuesto | Estado |
|---|---:|---:|---|
| `html/realistic-1mb` | 51 ms | ≤ 100 ms | ✅ |
| `html/nested-div-64k` | 21 ms | ≤ 100 ms | ✅ (lineal) |
| `html/massive-end-p` | 20 ms | ≤ 50 ms | ✅ |
| `html/massive-formatting` | 14 ms | ≤ 50 ms | ✅ |
| `html/tag-40k-attrs` | 11 ms | ≤ 50 ms | ✅ |
| `html/pretty-5k-blocks` | 24 ms | ≤ 75 ms | ✅ |
| `css/parse-10k-rules` | 13 ms | ≤ 30 ms | ✅ |
| `css/resolve-styles-10k-nodes` | 62 ms | ≤ 150 ms | ✅ |
| `layout/realistic-1mb` | 50 ms | ≤ 150 ms | ✅ |
| `paint/build-display-list` | 3.4 ms | ≤ 20 ms | ✅ |
| `paint/rasterize-800x600` | 1 485 ms | ≤ 250 ms | ❌ **6× sobre presupuesto (hotspot)** |
| `paint/render-to-png-800x600` | 1 514 ms | ≤ 300 ms | ❌ 5× |
| `pipeline/html-to-png-800x600` | 1 766 ms | ≤ 400 ms | ❌ 4,4× |

**Siguiente objetivo de optimización**: el rasterizador
(`crates/paint/src/rasterizer.rs`). Coste ~76 µs por item de display list a
800×600; con 19,5 k items supone 1,48 s. Líneas de trabajo: culling por clip
antes de tocar píxeles, agrupar `SolidColor` por rectángulo, coste de
`draw_text` por glifo, y evitar recorrer el área completa del viewport por item.
Cualquier mejora debe re-medirse con `paint/rasterize-800x600` y
`pipeline/html-to-png-800x600`.

---

## 6. Cómo comparar y detectar regresiones

1. Medir **antes** y **después** en la misma máquina y con el sistema ocioso
   (`uptime` < ~1.5 de load average). Misma `--scale` y mismas iteraciones.
2. Guardar la salida JSON (`target/bench-latest.json`) y comparar medianas:

   ```bash
   # Ejemplo de diff de medianas entre dos ejecuciones
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
3. Considerar regresión si la mediana empeora >15 % de forma reproducible; la
   varianza observada entre pasadas de la suite completa es ~2–5 %.
4. Para cambios de complejidad, usar el escalado (`--scale 0.5` vs `1.0`) y
   comprobar que el tiempo crece como el tamaño (ratio ≈1: ver §4).
5. La suite completa tarda ~45 s en esta máquina (más ~2–4 s de fixtures); si
   se busca un chequeo rápido, `--scale 0.1 --iterations 2` tarda ~1 s.

JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
`worktree_dirty`, `rustc`, `cpu`, `cores`, `scale`, `iterations`, `warmup`,
`setup_ms`, `total_median_ms`, `results[]` (con `name`, `group`, `detail`,
`iterations`, `median_ms`, `min_ms`, `samples_ms`, `throughput`) y `skipped[]`.
Los campos existentes no cambian; solo se añaden campos nuevos.

---

## 7. CI

La suite **se ejecuta a mano** (`./tools/bench.sh`); no se ha añadido un job en
este round para no tocar `.github/workflows/` (fuera del scope asignado a R12).
Es barata y no tiene dependencias externas, así que un job opcional
`workflow_dispatch` es trivial de añadir:

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

## 8. Limitaciones y notas

- **Árbol sucio**: la baseline se midió en `sprint/robustez-fase4` con 42 rutas
  modificadas por otros agentes del sprint. Es válida para el binario
  identificado arriba; al cerrar el sprint (commit limpio) conviene re-medir y
  sustituir la tabla.
- **Máquina portátil compartida**: i7-12700H híbrido, con *frequency scaling* y
  otros procesos; de ahí el uso de mediana/mínimo y la varianza de ~2–5 %.
  Los números absolutos no son comparables con CI ni con otras máquinas, pero
  las **ratios de escalado** sí.
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
  fuente, floats/posicionados, ni el motor de fuentes dentro de layout (eso
  llegará cuando `crates/fonts` se integre en layout/paint).
