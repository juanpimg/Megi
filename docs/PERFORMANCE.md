# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 18:21–18:36 (Europe/Madrid). Ronda 11:
  hito de conformidad **3 904/3 904** con las suites `scripted_*` ejecutando
  scripts reales (Boa ya dentro del gate), WP-B de la superficie DOM DOM↔JS,
  **event loop presupuestado con timers virtuales** y lifecycle events, y fix de
  rendimiento del parser (recuperación de `tag-40k-attrs`).
- **Repo**: `Megi`, rama `sprint/robustez-fase4`. **HEAD medido `13f3f21`**
  (un commit de test sobre `786fbcc`, el HEAD que citaba la tarea); mismo código
  de producción para ambos.
- **Estado del árbol**: código limpio; durante la ventana otro agente actualizó
  `README.md`/docs (los 6 JSON de bench marcan `worktree_dirty = true` por eso,
  no por código).
- **Binario de bench**: `target/release/bench`,
  `sha256 = c34ff65c…` (4 275 608 B). Tabla = mediana de **6 pasadas completas**
  (2 tandas de 3); pasada canónica del script `TOTAL 408.404 ms`.
- **Artefactos guardados** (gitignored):
  - `target/bench-r11-run1..6.json`, `target/bench-aggregate-r11.json`
    (`sha256 = d8549f7c…`), `target/bench-latest.txt` (`7ed228ec…`).
  - `target/perf-r11-harness.json`, `target/perf-r11-harness-tests.json`,
    `target/perf-r11-builds.json`, `target/perf-r11-render.json`,
    `target/perf-r11-workspace-tests.json`.
  - `target/bench-js-r11/` — fixtures temporales de event loop y WP-B
    (`event_loop_summary.json`, `wp_b_summary.json`).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §11 (rondas 2–10).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Todas las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`). El *fixture* se construye **fuera** del
cronómetro; los benchmarks de parse incluyen la construcción y liberación del
`Document`.

**El crate `bench` y sus generadores no cambiaron en esta ronda.** `bench` solo
depende de `dom/html/css/layout/paint` (y `fonts` opcional): **no enlaza `js`,
`net`, `image` ni `test_harness`**. Las filas de §3 son el motor sin Boa; el
coste del gate con JS se mide aparte (§8.1).

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- **La tabla de §3 es la mediana, por fila, de 6 pasadas completas** (2 tandas
  de 3) a load 2,16–4,23 (otros agentes compilando). Rango = mínimo–máximo de
  las 6 medianas. La pasada canónica de `./tools/bench.sh` dio
  `TOTAL 408.404 ms` (mediana de las 6: 405.845).
- `setup_ms`: 213,8–229,4 ms (no cronometrado).
- **Gate de conformidad** (§8.1): `target/release/test_harness --summary`
  7 ejecuciones (110,3 ms mediana) y `cargo run --release -p test_harness --
  --summary`; tiempos por suite con el filtro posicional del runner
  (sustrayendo la línea base de suites no-tree de 1,47 ms).
- **Event loop** (§8.2): 6 fixtures `js_event_*`, `--json` (3 ejecuciones por
  modo para las etapas) + pared de 7 ejecuciones intercaladas default vs
  `--no-scripting`.
- Render/shell (§7): 11 ejecuciones intercaladas, mediana de pared; etapas del
  `--json` con 7 ejecuciones.
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` por defecto |
| Carga de fondo | bench: 2,16–4,23; render/harness: 1,76–4,2 |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Bench (6 pasadas de esta baseline)
./tools/bench.sh
for i in 1 2 3 4 5 6; do target/release/bench --json > "target/bench-r11-run$i.json"; done

# Gate de conformidad con Boa (release)
cargo run --release -p test_harness -- --summary
target/release/test_harness --summary
target/release/test_harness scripted_        # tiempos de las 4 suites scripted_*
cargo test -p test_harness                    # debug
cargo test -p test_harness --release

# Event loop JS (6 fixtures, default vs --no-scripting)
for f in tests/fixtures/render/js_event_*.html; do
  target/release/render "$f" /tmp/e.png --json
  target/release/render "$f" /tmp/e.png --json --no-scripting
done

# WP-B (fixture temporal en target/, no versionado)
target/release/render target/bench-js-r11/wp_b.html /tmp/wp.png --json
```

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb` | Parse de documento «realista» de ~1 MiB (1 044 tablas 3×3). |
| html | `html/nested-div-10k/32k/64k` | Parse de `<div>` anidados. |
| html | `html/nested-div-64k-default-limits` | Mismo input con límites anti-DoS (profundidad 4096). |
| html | `html/massive-end-p` | 100 000 `</p>` consecutivos. |
| html | `html/massive-formatting` | 10 000 bloques `<b><i><u>` mal anidados. |
| html | `html/tag-40k-attrs` | Un elemento con 40 000 atributos. |
| html | `html/pretty-5k-blocks` | Documento indentado con 5 000 bloques. |
| css | `css/parse-10k-rules` | Parse de hoja CSS con 10 000 reglas. |
| css | `css/resolve-styles-2k/10k-nodes` | Cascada + `ComputedStyle` sobre ~2 005 / ~10 005 elementos. |
| css | `css/selector-match-simple/descendant` | 4 selectores sobre 10 005 elementos. |
| layout | `layout/realistic-1mb` | `layout_document` sobre el documento realista (28 195 elementos), 800×600. |
| paint | `paint/build-display-list` | Display list: 34 158 items (sin hit regions: opt-in). |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` DEFLATE (57 422 B). |
| pipeline | `pipeline/html-to-png-800x600` | Parse → styles → layout → display list → raster → PNG. |
| fonts | (`feature fonts`) | No re-medido en r11 (sin cambios en `crates/fonts`). |

**Nuevo en r11, fuera del bench**: el **gate de conformidad ahora ejecuta Boa**
en las 4 suites `scripted_*` (3 904/3 904) — coste medido en §8.1; event loop
JS virtual (§8.2); WP-B DOM surface (§8.3).

---

## 3. Baseline vigente (2026-11-10, HEAD `13f3f21`)

Mediana por fila de **6 pasadas completas** (5 iteraciones, 1 warm-up; *heavy*
3). `setup`: 213,8–229,4 ms. La suma de medianas da `TOTAL 405.845 ms`; la
pasada del script dio `TOTAL 408.404 ms`.

| Benchmark | Carga | mediana (ms) | rango 6 pasadas (ms) | throughput |
|---|---|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | **20.361** | 19.908–21.034 | 49,1 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | **3.343** | 3.150–3.382 | 31,4 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | **10.800** | 10.553–11.057 | 31,1 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | **21.469** | 20.812–22.455 | 31,3 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | **13.369** | 12.948–13.737 | — |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | **21.416** | 21.253–21.904 | 4,67 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | **11.979** | 11.851–12.657 | 835 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | **11.633** | 11.466–15.236 | 3,44 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | **11.061** | 10.826–13.421 | 452 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | **13.893** | 13.104–16.152 | 42,9 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | **5.969** | 5.609–6.550 | 336 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | **28.797** | 28.291–31.906 | 347 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | **0.680** | 0.648–0.708 | 58,8 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | **1.287** | 1.259–1.418 | 31,1 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | **58.818** | 55.745–64.105 | 479 k elementos/s |
| `paint/build-display-list` | 34 158 items | **15.056** | 13.994–16.939 | — |
| `paint/rasterize-800x600` | 34 158 items → pixmap 800×600 | **0.925** | 0.903–0.980 | 518 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (57 422 B) | **13.146** | 12.837–13.661 | 4,17 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | **141.841** | 135.426–146.144 | — |
| **TOTAL (suma)** | | **405.845** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página. Las filas de layout/paint siguen siendo las más
> sensibles a la carga del host (±10–15 %).

---

## 4. Complejidad observada

Escalados con el mismo binario, complementarios a §3:

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.34 ms · 32k: 10.80 ms · 64k: 21.47 ms | **Lineal**: 3,2× input → 3,23×; 2× → 1,99×. |
| `html/tag-40k-attrs` | 10k: 2.66 ms · 20k: 6.50 ms · 40k: **11.63 ms** | **Lineal recuperado** (~0,29 µs/atributo): el fix de r11 deshace la regresión de r10 (14,9 ms). |
| `html/massive-end-p` | 100k `</p>` | **21,4 ms** (+17,5 % vs r10, que marcó 18,2): parcialmente recuperado de r9 (23,7), pero no al nivel de r10 → flagged (§5.3). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 13.89 · 20k: 30.50 ms | **Lineal** (~2,1× por cada 2×). |
| `css/resolve-styles-2k/10k` | 2 005 el: 5.97 ms · 10 005 el: 28.80 ms | **Lineal**: 5× elementos → 4,82× tiempo (~347 k el/s). |
| `paint/rasterize-800x600` | 34 158 items → 800×600 | **Plano respecto al nº de items** (coste de viewport). |
| `pipeline` | 141,8 ms (135,4–146,1) | Dominado por parse HTML (20) + layout (59) + paint/PNG (15+13) + CSS (29). |

---

## 5. Ronda 11: hito de conformidad, event loop y fix del parser

### 5.1 Qué entró en la ronda

- `3e20b94 feat(html): parser-inserted scripts with document.write, plus a
  parser perf fix`: el parser ejecuta `<script>` insertados por el propio parser
  (con `document.write`) y **recupera el rendimiento perdido en r10** para
  entradas con muchísimos atributos (`tag-40k-attrs`).
- `32b421d test(conformance): execute parser-inserted scripts in the harness -
  tree 3904/3904`: el gate de conformidad ejecuta Boa en las 4 suites
  `scripted_*`; **el árbol pasa de 3 898/3 904 a 3 904/3 904**.
- `3a8c0ac feat(js): Rust-facing script/event APIs and the full WP-B DOM
  surface`: `getComputedStyle`, `style`, `NodeList`, `matches`, etc.
- `786fbcc`/`13f3f21`: **event loop presupuestado** en render con timers
  virtuales (hasta 1000 ms virtuales, batches de 1 ms), lifecycle events
  (`DOMContentLoaded`, `load`) y dispatch a través del `Document` event target.

### 5.2 Verificación con el bench oficial

Claims de la ronda vs nuestra medición (mediana de 6 pasadas):

| Métrica | Claim de la ronda | Nuestra medición | Veredicto |
|---|---:|---:|---|
| `html/tag-40k-attrs` | ~12,4 ms (fix) | **11.633 ms** (r10: 14.921) | ✅ recuperado (−22,0 %) |
| `html/realistic-1mb` | ~19–23 ms | **20.361 ms** | ✅ dentro del rango |
| `pipeline/html-to-png-800x600` | ~140–155 ms | **141.841 ms** (rango 135,4–146,1) | ✅ dentro del rango |
| `html/massive-end-p` | (no citado) | **21.416 ms** (+17,5 % vs r10) | ⚠️ regresión parcial (§5.3) |

El resto de filas HTML queda entre +0,3 % y +7,8 % respecto a r10, dentro de la
varianza del host (load 3,7–4,2 en la primera tanda vs 2,2 en la segunda); CSS
mejora −0,6…−2,8 %, layout −5,4 %, paint −3…−14 %. El **TOTAL queda en
405,8 ms (−1,1 % vs r10)**, y el pipeline en 141,8 ms: el objetivo de la ronda
se cumple.

### 5.3 Hallazgo: `html/massive-end-p` +17,5 % vs r10

100 000 `</p>` consecutivos: r10 midió 18,2 ms y r11 mide 21,4 ms (rango
21,25–21,90, todas las pasadas altas). Sigue siendo mejor que r9 (23,7 ms), pero
**pierde ~3 ms del win de r10**. Es el precio aparente del fix del parser para
atributos/modos de inserción; se deja **flagged** para el carril de parser junto
con una posible revisión del camino de end-tags. No afecta a la página
realista (20,4 ms, estable).

### 5.4 Diferencias metodológicas

- El bench se midió con load 2,16–4,23 (agentes en paralelo) y 6 pasadas; los
  claims de la ronda probablemente se tomaron en ventana más limpia. Las filas
  HTML/pipeline coinciden dentro del 5–10 %.
- `worktree_dirty=true` en los JSON por `README.md` (otro agente), no por
  código: el binario es de `13f3f21`.

---

## 6. Compresión PNG (bench)

| Métrica | r8 | r9 | r10 | r11 | Notas |
|---|---:|---:|---:|---:|---|
| PNG del bench (800×600) | 57 422 B | 57 422 B | 57 422 B | **57 422 B** | sin cambio |
| `paint/render-to-png-800x600` | 13.296 | 12.283 | 13.601 | **13.146** | varianza del host |
| — rasterizar | 0.993 | 0.881 | 1.082 | **0.925** | ídem |
| — codificar PNG | ~12,3 | ~11,4 | ~12,5 | **~12,2 ms** | sigue siendo el mayor coste de pintado |

---

## 7. Pipeline `render` y shell headless (ronda 11)

### 7.1 Showcase 1024×768 (golden sin cambios)

`target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png
--width 1024 --height 768 --json`; 11 ejecuciones intercaladas con `basic`
(load 1,76). Golden verificado byte a byte:

- PNG **80 220 B**, **md5 `c95c7eff83e7bd08f815631aa42afdb0`**, 1024×768,
  opaco. Sin cambios desde r8 (las hit regions y el event loop no alteran la
  salida).
- Metadatos: `@import` 1, hojas externas 1, hojas saltadas por media 1,
  `<img>` 2, fondos 2, items 80, **hit regions 121**, `document_bytes` 2 383,
  `css_bytes` 4 163.

| Etapa (mediana de 7 runs, µs) | fetch | decode/parse | styles | images | backgrounds | scripting | event_loop | layout | paint | raster | **total** |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| r11 showcase | 23 | 230 | 799 | 161 | 57 | 14 | 0 | 1 934 | 29 | 9 840 | **31 413** |
| r10 (referencia) | 25 | 230 | 810 | 159 | 54 | 14 | — | 1 797 | 28 | 10 170 | 32 795 |

| Variante | pared (ms) | rango (ms) | PNG |
|---|---:|---:|---:|
| **showcase 1024×768 (render, 11 runs)** | **33,69** | 31,68–43,63 | 80 220 B (golden) |
| `basic.html` 800×600 | **15,40** | 14,83–16,82 | 21 090 B (`e943565d…`) |

El total de etapas baja ~1,4 ms vs r10 (raster −330 µs, styles −11 µs) pero la
pared queda en 33,7 ms (rango alto por algún outlier de scheduling). El
PNG+driver sigue siendo **~59 %** del total; el presupuesto de 40 ms propuesto
en r10 tiene margen (~84 %).

### 7.2 Shell headless (`megi --screenshot`, mismo pipeline, sin ventana)

| Fixture | pared (ms) | rango | PNG |
|---|---:|---:|---:|
| showcase 1024×768 | **32,57** | 31,52–37,11 | 80 220 B (golden, byte-idéntico) |
| basic.html 800×600 | **15,24** | 14,38–17,22 | 21 090 B |

El shell no crea ventana ni `EventLoop` en `--screenshot`; usa las hit regions
del render para enlazar sin pasada extra.

---

## 8. Coste del gate con JS, event loop y WP-B (ronda 11)

### 8.1 Gate de conformidad con Boa

`target/release/test_harness --summary` (gate completo, release):

- **Tiempo total: 110,33 ms de mediana** (108,24–112,50, 7 ejecuciones).
  `cargo run --release -p test_harness -- --summary` tarda **184,1 ms**
  (≈74 ms de overhead de cargo).
- Resultado: exit 0, **Tokenizer 7 050/7 050**, **Tree 3 904/3 904** (incluye
  las 4 suites `scripted_*` con Boa), Serializer 230/230, CSS 30/30;
  allowlist sin fails no documentados.
- Coste de Boa en el gate: línea base sin suites tree = **1,47 ms**; las 4
  suites scripted cuestan (sobre esa base, mediana de 7):

| Suite | total (ms) | coste sobre base (ms) |
|---|---:|---:|
| `scripted_adoption01.dat` | 2,53 | **+1,06** |
| `scripted_ark.dat` | 2,64 | **+1,17** |
| `scripted_foster01.dat` | 3,02 | **+1,55** |
| `scripted_webkit01.dat` | 4,32 | **+2,85** |
| **las 4 juntas** | 6,60 | **+5,13** |

  Es decir, **~5 ms (~4,6 %) del gate de 110 ms** son Boa; el resto son los
  ~7 050 casos de tokenizer, 3 904 de tree (sin scripts) y serializer. Para
  comparar: suites no scripted `adoption01` +1,40 ms y `webkit01` +4,30 ms.
- **Binario de test_harness release**: **14 116 400 B (13,46 MiB)** frente a
  `bench` 4 275 608 B. El harness enlaza Boa (más serde/serde_json).
- **Coste de build**: build release frío en target vacío
  `cargo build --release -p test_harness` = **31,42 s** vs
  `cargo build --release -p bench` = **6,16 s** → **Boa añade ≈25,3 s de
  compilación al gate/CI**. `cargo test -p test_harness` incremental: debug
  1,2 s (11 tests: 3 unit + 8 integración), release 4,14 s. `cargo test
  --workspace` incremental: 8,0 s, **75 suites / 862 tests**.

**Lectura**: ejecutar Boa en el gate es **barato en runtime** (~5 ms de
110 ms) pero **caro en build/CI** (+25 s frío) y en tamaño del binario
(13,5 MiB). El gate ya no puede compilarse sin Boa; si CI crece, es el primer
sitio donde cachear el target.

### 8.2 Coste del event loop en render (6 fixtures `js_event_*`)

`--json`, default vs `--no-scripting`; 3 ejecuciones por modo para las etapas y
7 de pared intercaladas. `timings.event_loop` incluye los batches de timers
virtuales y los pasos de 1 ms de reloj virtual; `timings.scripting` es crear el
runtime + ejecutar scripts (sin loop).

| Fixture | eventos | scripting (µs) | event_loop (µs) | tasks | iter | virtual (ms) | eventos | pend. | Δpared (ms) | ΔPNG |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `js_event_domcontentloaded` | 2 | 544 | 64 | 0 | 0 | 0 | 2 | 0 | −0,98 | +705 B |
| `js_event_interval` | 2 | 611 | 77 | 3 | 15 | 15 | 2 | 0 | +0,32 | +168 B |
| `js_event_load` | 2 | 572 | 57 | 1 | 5 | 5 | 2 | 0 | −0,31 | +592 B |
| `js_event_loop` (budget) | 2 | 565 | **855** | **100** | **1 000** | **1 000** | 2 | 1 | +0,06 | +237 B |
| `js_event_timeout` | 2 | 518 | 51 | 1 | 10 | 10 | 2 | 0 | +0,36 | +499 B |
| `js_event_timeout_zero` | 2 | 506 | 47 | 1 | 1 | 0 | 2 | 0 | +0,55 | +262 B |

Conclusiones:

- **Los timers ahora sí se ejecutan**: con el reloj virtual, `setTimeout(…,10)`
  dispara en el mismo render (`timeout` muta el DOM, ΔPNG +499 B), arreglando la
  limitación de r10 (que solo ejecutaba los de 0 ms). `setInterval` se bombea
  hasta `clearInterval` (3 tareas, 15 ms virtuales).
- **Coste del loop**: 47–77 µs para un batch normal; el fixture de **bucle
  recursivo se corta por budget** en 100 tareas / 1 000 iteraciones / 1 000 ms
  virtuales con **855 µs** de coste y 1 timer pendiente → el presupuesto
  funciona.
- `timings.scripting` (~0,5–0,6 ms) manda sobre el Δpared (dominado por la
  creación del runtime y el re-render); el Δpared de 7 runs es ruidoso
  (−1,0…+0,6 ms) porque el render base es 14–15 ms.
- `DOMContentLoaded` y `load` se despachan con 2 eventos cada uno sin tareas
  pendientes; sin warnings.

### 8.3 WP-B: superficie DOM (`getComputedStyle`, `style`, `NodeList`, `matches`)

Fixture temporal `target/bench-js-r11/wp_b.html`: lee `getComputedStyle(...).color`,
muta `style.backgroundColor/width/fontSize`, recorre un `NodeList`
(`querySelectorAll(".item")`), usa `matches("p.item#a")` y reescribe textos.

| Modo | scripting (µs) | event_loop (µs) | scripts | pared (ms) | PNG |
|---|---:|---:|---:|---:|---:|
| con JS | **803** | 11 | 1 | 15,57 | 27 623 B |
| sin JS | 0 | 0 | 0 | 14,49 | 18 845 B |
| **Δ** | — | — | — | **+1,08 ms** | **+8 778 B** |

Sin warnings: las APIs WP-B funcionan y el coste de scripting (803 µs, algo
mayor que los fixtures de eventos por el trabajo DOM+estilo) está en línea con
r9/r10. ΔPNG +8,78 KB confirma que los cambios de estilo/texto llegan al
pintado.

### 8.4 Binarios y build (resumen)

| Binario | r11 | r10 | Δ |
|---|---:|---:|---:|
| `render` (con JS + hit regions) | **19 026 424 B** (18,15 MiB) | 18 953 968 B | +72 456 B |
| `megi` (shell) | **23 058 072 B** (21,99 MiB) | 22 985 824 B | +72 248 B |
| `test_harness` (con Boa) | **14 116 400 B** (13,46 MiB) | — | nuevo dato |
| `bench` | 4 275 608 B | 4 264 984 B | +10 624 B |

Build frío: harness 31,42 s vs bench 6,16 s (Boa +25,3 s); render/shell con JS
(medido en r9/r10) ~33 s frío vs 10,4 s sin js.

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

Presupuestos vigentes (r10): pipeline **≤160 ms**, render showcase **≤40 ms**,
`build-display-list` **≤25 ms**. Con los datos de r11:

| Benchmark / medida | Baseline r11 | Presupuesto | % | Veredicto |
|---|---:|---:|---:|---|
| `html/realistic-1mb` | 20,4 ms | ≤ 100 ms | 20 % | ✅ |
| `html/nested-div-64k` | 21,5 ms | ≤ 100 ms | 21 % | ✅ |
| `html/massive-end-p` | 21,4 ms | ≤ 50 ms | 43 % | ✅ (flagged vs r10) |
| `html/tag-40k-attrs` | 11,6 ms | ≤ 50 ms | 23 % | ✅ recuperado |
| `css/parse-10k-rules` | 13,9 ms | ≤ 30 ms | 46 % | ✅ |
| `css/resolve-styles-10k-nodes` | 28,8 ms | ≤ 75 ms | 38 % | ✅ |
| `layout/realistic-1mb` | 58,8 ms | ≤ 150 ms | 39 % | ✅ |
| `paint/build-display-list` | 15,1 ms | ≤ 25 ms | 60 % | ✅ |
| `paint/rasterize-800x600` | 0,93 ms | ≤ 25 ms | 4 % | ✅ |
| `paint/render-to-png-800x600` | 13,1 ms | ≤ 50 ms | 26 % | ✅ |
| `pipeline/html-to-png-800x600` | **141,8 ms** | **≤ 160 ms** | **88,6 %** | ✅ mantener 160 |
| `render` showcase 1024×768 | **33,7 ms** | **≤ 40 ms** | **84,2 %** | ✅ mantener 40 |
| shell headless showcase | 32,6 ms | ≤ 40 ms | 81,4 % | ✅ |
| gate de conformidad (runtime) | 110,3 ms | ≤ 200 ms | 55 % | ✅ nuevo |
| build release frío del gate (CI) | 31,4 s (con Boa) | — | — | ⚠️ +25,3 s por Boa |

**Conclusión de presupuestos**: mantener **pipeline ≤160 ms** y **render
showcase ≤40 ms**; ambos dejan margen razonable (11 % y 16 %) con la varianza
actual. El pipeline ya está dominado por layout (59 ms) y parse HTML (20 ms) +
paint/PNG (28 ms) + CSS (29 ms). El **coste nuevo del gate** es de build/CI
(+25 s frío por Boa), no de runtime (5 ms de 110).

---

## 11. Comparación histórica

Diez baselines en la misma máquina, mismo arnés:

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | **18:21** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | **`13f3f21`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | limpio | docs dirty | limpio | limpio | **docs dirty (README)** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | `43f7748d…` | `a188a1da…` | **`c34ff65c…`** |
| Load (bench) | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | **2,16–4,23** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | **20.361** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | 25.658 | 25.084 | 24.772 | 24.022 | 20.396 | **21.469** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | **28.797** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | **58.818** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | **15.056** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | 0.843 | 0.993 | 0.881 | 1.082 | **0.925** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | **13.146** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | **141.841** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** |

### Ronda 10 → ronda 11, fila a fila

| Benchmark | R10 (ms) | R11 (ms) | Factor | Δ |
|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 20.305 | **20.361** | 1,00× | +0,3 % |
| `html/nested-div-10k` | 3.186 | **3.343** | 0,95× | +4,9 % |
| `html/nested-div-32k` | 10.018 | **10.800** | 0,93× | +7,8 % |
| `html/nested-div-64k` | 20.396 | **21.469** | 0,95× | +5,3 % |
| `html/nested-div-64k-default-limits` | 12.941 | **13.369** | 0,97× | +3,3 % |
| `html/massive-end-p` | 18.225 | **21.416** | 0,85× | **+17,5 %** ⚠️ |
| `html/massive-formatting` | 11.713 | **11.979** | 0,98× | +2,3 % |
| `html/tag-40k-attrs` | 14.921 | **11.633** | **1,28×** | **−22,0 %** ✅ |
| `html/pretty-5k-blocks` | 10.990 | **11.061** | 0,99× | +0,7 % |
| `css/parse-10k-rules` | 14.127 | **13.893** | 1,02× | −1,7 % |
| `css/resolve-styles-2k-nodes` | 6.101 | **5.969** | 1,02× | −2,2 % |
| `css/resolve-styles-10k-nodes` | 29.422 | **28.797** | 1,02× | −2,1 % |
| `css/selector-match-simple` | 0.671 | **0.680** | 0,99× | +1,4 % |
| `css/selector-match-descendant` | 1.260 | **1.287** | 0,98× | +2,1 % |
| `layout/realistic-1mb` | 62.171 | **58.818** | 1,06× | −5,4 % |
| `paint/build-display-list` | 16.128 | **15.056** | 1,07× | −6,6 % |
| `paint/rasterize-800x600` | 1.082 | **0.925** | 1,17× | −14,5 % |
| `paint/render-to-png-800x600` | 13.601 | **13.146** | 1,03× | −3,3 % |
| `pipeline/html-to-png-800x600` | 143.229 | **141.841** | 1,01× | −1,0 % |
| **TOTAL (suma)** | **410.485** | **405.845** | **1,01×** | **−1,1 %** |

**Lectura**: ronda de consolidación. El fix del parser recupera `tag-40k-attrs`
(−22 %) y mantiene la página realista (~20,4 ms) y el pipeline (~141,8 ms,
dentro del presupuesto de 160). El único retroceso es `massive-end-p`
(+17,5 % vs r10, aún mejor que r9); el resto de diferencias son varianza del
host (load 2,2–4,2). El coste nuevo de la ronda es el **gate con Boa**
(+25 s de build, 5 ms de runtime) y el **event loop** (47–855 µs según timers).

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
3. Usar la mediana de ≥5 pasadas si el host está cargado (ronda 11: 6). Las
   filas layout/paint/raster son las más sensibles (±10–15 %).
4. Regresión si la mediana empeora >15 % de forma reproducible.
5. Para complejidad, usar `--scale 0.5` vs `1.0`: ratios ≈1.
6. **Gate**: `target/release/test_harness --summary` debe dar exit 0 con
   7 050/7 050 tokenizer, 3 904/3 904 tree (con Boa), 230/230 serializer y
   30/30 CSS. El **modo filtro** (`test_harness <suite>`) devuelve exit 1 a
   propósito (no gatea los totales): úsalo solo para medir suites.
7. **Event loop**: `render --json` expone `timings.event_loop/scripting`,
   `script_tasks/loop_iterations/virtual_time_ms/events/timers_pending`;
   default vs `--no-scripting` para el Δ.
8. JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
   `worktree_dirty`, `rustc`, `cpu`, `cores`, `scale`, `iterations`, `warmup`,
   `setup_ms`, `total_median_ms`, `results[]` y `skipped[]`.

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

Con el gate ahora enlazando Boa, el coste de CI sube: build release frío de
`test_harness` **31,4 s** (vs 6,2 s de `bench`, +25,3 s por Boa) y binario de
13,46 MiB. Runtime del gate: **110 ms** (7 050 tokenizer + 3 904 tree + 230
serializer + 30 CSS, exit 0). `cargo test --workspace` incremental: 8,0 s,
**75 suites / 862 tests**. En runners de 2–4 núcleos el build de Boa dominará el
CI; cachear `target/` (o separar el job del gate del job de bench) es la
mitigación obvia. Los números de CI no son comparables (otra CPU).

---

## 14. Limitaciones y notas

- **Host cargado/variable**: load 2,16–4,23 en el bench (agentes compilando) y
  picos mayores en harness/render; las filas layout/paint oscilan ±10–15 %.
  La baseline usa 6 pasadas y publica rangos.
- **`worktree_dirty=true`** en los JSON por `README.md`/docs editados por otro
  agente; el código es `13f3f21` (un commit de test sobre el `786fbcc` que cita
  la tarea).
- **`massive-end-p` +17,5 % vs r10**: flagged para el carril de parser; el
  resto de HTML mantiene o mejora. `tag-40k-attrs` recuperado (−22 %).
- **Gate con Boa**: runtime barato (~5 ms de 110 ms en las suites scripted) pero
  build caro (+25,3 s frío) y binario 13,46 MiB. El modo filtro del runner
  devuelve exit 1 (no gatea totales); no usar su exit code en CI.
- **Event loop**: presupuestado (100 tareas / 1 000 iteraciones / 1 000 ms
  virtuales); los timers usan reloj virtual y se bombean en batches de 1 ms, así
  que `setTimeout(…,10)` dispara dentro del render, pero no hay event loop de
  pared ni I/O de red en JS. `js_event_loop` demuestra el corte por budget.
- **WP-B**: medido con un fixture temporal bajo `target/` (no versionado);
  getComputedStyle/style/NodeList/matches funcionan sin warnings.
- **Presupuestos**: se mantienen pipeline ≤160 ms (actual 141,8; 89 %) y render
  showcase ≤40 ms (actual 33,7; 84 %); nuevo presupuesto de referencia del gate
  ≤200 ms de runtime (actual 110) con el aviso de +25 s de build en CI.
- **Fixtures de medida** (eventos, WP-B) viven en `target/bench-js-r11/`; el
  servidor loopback de r10 y sus resultados siguen en `target/bench-net-r10/`.
- **Fast path de tablas (r7)**: la comparación 67,2 → 61,4 ms usa el valor
  intermedio documentado por el commit de layout; no hay un commit compilable
  con el camino avanzado sin fast path.
