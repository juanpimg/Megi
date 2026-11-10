# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 17:42–17:52 (Europe/Madrid). Ronda 10:
  optimización del lexer/parser HTML y de layout (pipeline 800×600), hit regions
  en el pipeline `render` y navegación del shell, networking avanzado
  (cookies/CORS/gzip) y eventos/timers en el motor JS (WP-A).
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `f10635c`**
  (`feat(render,shell): hit regions in the pipeline and advanced networking`).
- **Estado del árbol**: **limpio en las 12 pasadas** (`worktree_dirty = false`);
  otros agentes actualizaron documentación en paralelo, sin cambios de código
  sin commitear durante la ventana de medida.
- **Binario de bench medido**: `target/release/bench`, compilado de `f10635c`.
  El `target/` es compartido entre agentes y el binario se recompiló una vez
  (`sha256 a188a1da…` → `ae5d86d1…`, misma fuente); la tabla es la mediana de
  **12 pasadas completas** repartidas en 4 ventanas de 3 (ver §1.2).
- **Artefactos guardados** (gitignored):
  - `target/bench-r10-run1..12.json` y `target/bench-aggregate-r10.json`
    (`sha256 = f8c466cf…`).
  - `target/bench-latest.txt` / `-meta.txt` (pasada del script
    `TOTAL 410.235 ms`; `sha256 = 6a0d9d3c…`).
  - `target/bench-js-r10/` — fixtures temporales de **eventos/timers** (en
    `target/`, no en el repo).
  - `target/bench-net-r10/` — servidor HTTP loopback (`serve.py`) y fixture para
    medir cookies/CORS/gzip; resultados en `target/perf-r10-net.json`.
  - `target/perf-r10-render-stages.json`, `target/perf-r10-js-events.json`.

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §11 (rondas 2–9).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Todas las cargas se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`). La misma `--scale` produce siempre el mismo
flujo de bytes. El *fixture* se construye **fuera** del cronómetro; los
benchmarks de parse incluyen la construcción y liberación del `Document`.

**El crate `bench` y sus generadores no cambiaron en esta ronda.** `bench` solo
depende de `dom/html/css/layout/paint` (y `fonts` opcional): **no enlaza `net`,
`js`, `image` ni `shell`**, no ejecuta scripts ni usa la API de hit-testing.
Las filas de §3 son, por tanto, del motor sin las funcionalidades nuevas; su
coste se mide aparte en §8.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- **La tabla de §3 es la mediana, por fila, de 12 pasadas completas** (4 tandas
  de 3), tomadas entre 17:42 y 17:50. El rango es mínimo–máximo de las 12
  medianas. Se hicieron más pasadas de lo habitual porque el host mostró una
  varianza inusualmente alta en esta ventana (load 0,86–3,53 con otros agentes
  compilando; las filas de layout/paint/raster se movieron ±15–20 % entre
  tandas). La pasada canónica de `./tools/bench.sh` dio
  `TOTAL 410.235 ms` (mediana de las 12: 410.485).
- `setup_ms`: 214,9–247,0 ms (no cronometrado).
- **Medidas de features**: render `--json` (7 ejecuciones, mediana de cada
  etapa); pared de render/shell con 11 ejecuciones intercaladas; cookies/CORS/
  gzip sobre un servidor HTTP loopback (`127.0.0.1`) con 7 ejecuciones por
  variante; eventos/timers con fixtures en `target/` y 7 ejecuciones por modo.
  Todas usan el binario release (sin overhead de `cargo run`).
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` por defecto |
| Carga de fondo | pasadas del bench: 0,86–2,49; ventana global: 0,86–3,53 |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Pasada canónica (compila release + ejecuta + guarda target/bench-latest.txt)
./tools/bench.sh

# Las 12 pasadas de esta baseline (4 tandas de 3)
for i in 1 2 3 4 5 6 7 8 9 10 11 12; do
  target/release/bench --json > "target/bench-r10-run$i.json"
done

# Smoke rápido (~1 s)
./tools/bench.sh --scale 0.1 --iterations 2

# Features nuevas (r10)
target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png \
    --width 1024 --height 768 --json      # incluye hit_regions/cookies/content_encoding
python3 target/bench-net-r10/serve.py 18765 &   # loopback cookies/CORS/gzip
target/release/render http://127.0.0.1:18765/gzip/ /tmp/net.png --json --cookies
target/release/render target/bench-js-r10/events0.html /tmp/ev.png --json
target/release/render target/bench-js-r10/events0.html /tmp/ev.png --json --no-scripting
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
| layout | `layout/realistic-1mb` | `layout_document` sobre el documento realista (28 195 elementos, 88 155 cajas), 800×600. |
| paint | `paint/build-display-list` | Display list: 34 158 items (sin hit regions en el bench). |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` DEFLATE (57 422 B). |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo: parse → styles → layout → display list → raster → PNG. |
| fonts | (`feature fonts`) | No re-medido en r10 (sin cambios en `crates/fonts`). |

**Fuera del bench (medido en §8)**: ejecución JS (scripting, eventos/timers),
hit regions en render, cookies/CORS/gzip sobre HTTP.

---

## 3. Baseline vigente (2026-11-10, HEAD `f10635c`)

Mediana por fila de **12 pasadas completas** (5 iteraciones, 1 warm-up; *heavy*
3). `setup`: 214,9–247,0 ms. La suma de medianas da `TOTAL 410.485 ms`; la
pasada del script dio `TOTAL 410.235 ms`.

| Benchmark | Carga | mediana (ms) | rango 12 pasadas (ms) | throughput |
|---|---|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | **20.305** | 19.465–21.010 | 49,3 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | **3.186** | 3.058–3.293 | 32,9 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | **10.018** | 9.640–10.367 | 33,5 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | **20.396** | 19.603–21.522 | 31,9 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | **12.941** | 12.504–14.588 | — (59 906 start tags descartados) |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | **18.225** | 17.624–19.501 | 5,48 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | **11.713** | 11.265–13.245 | 854 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | **14.921** | 14.526–16.848 | 2,68 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | **10.990** | 10.626–11.620 | 455 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | **14.127** | 13.242–15.635 | 42,4 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | **6.101** | 5.818–6.324 | 328 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | **29.422** | 28.348–32.093 | 339 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | **0.671** | 0.625–1.074 | 59,7 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | **1.260** | 1.198–1.555 | 30,7 M matches/s |
| `layout/realistic-1mb` † | 28 195 elementos, 88 155 cajas, 800×600 | **62.171** | 55.558–65.611 | 453 k elementos/s |
| `paint/build-display-list` † | 34 158 items | **16.128** | 13.613–19.727 | — |
| `paint/rasterize-800x600` † | 34 158 items → pixmap 800×600 | **1.082** | 0.880–1.191 | 444 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (57 422 B) | **13.601** | 12.405–14.699 | 4,02 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | **143.229** | 135.008–153.007 | — |
| **TOTAL (suma)** | | **410.485** | | |

† Filas sensibles a la frecuencia del host en esta ventana: las re-mediciones
filtradas (5 ejecuciones cada una en ventanas separadas) dieron
`layout 55,6–67,7 ms`, `build-display-list 13,6–19,7 ms` y
`rasterize 0,88–1,19 ms`. La mediana de 12 pasadas completa es la que se publica,
pero **la incertidumbre de estas tres filas es ±15–20 %**; no se debe concluir
una regresión o mejora de <20 % en ellas sin una ventana en reposo.

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página.

---

## 4. Complejidad observada

Escalados con el mismo binario, complementarios a §3:

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.19 ms · 32k: 10.02 ms · 64k: 20.40 ms | **Lineal**: 3,2× input → 3,14× y 2× → 2,04×. |
| `html/tag-40k-attrs` | 10k: 2.66 ms · 20k: 6.50 ms · 40k: 14.92 ms | **Superlineal** (~0,37 µs/atributo y creciendo): el camino de 40k atributos empeoró en r10 (§5.3). |
| `css/parse-10k-rules` | 5k: 6.69 ms · 10k: 14.13 ms · 20k: 30.50 ms | **Lineal** (≈2× por cada 2×). |
| `css/resolve-styles-2k/10k` | 2 005 el: 6.10 ms · 10 005 el: 29.42 ms | **Lineal**: 5× elementos → 4,82× tiempo (~339 k el/s). |
| `paint/build-display-list` | 34 158 items: 13,6–19,7 ms | **Plano respecto al nº de items** dentro de la varianza del host (~0,4–0,6 µs/item). |
| `paint/rasterize-800x600` | 34 158 items → 800×600 | **Plano respecto al nº de items**: coste ligado al viewport. |
| `layout/realistic-1mb` | 28 195 elementos → 62,2 ms (55,6–65,6) | Varianza del host; el carril perf reporta 54,5 ms en ventana limpia (§5.2). |

---

## 5. Ronda 10: gran salto de parse HTML, layout y features nuevas

### 5.1 Qué entró en la ronda

- `6a41551 perf(html,layout): cut the 800x600 pipeline from 168 ms to 131 ms`:
  - **HTML**: los caracteres consecutivos del modo in-body se procesan como un
    único run (`append_text(&str)` en vez de trabajo por carácter), el tokenizer
    consume runs de Data directamente (`consume_data_run`), el preprocesado es
    una sola pasada `&str` con line-starts, un hasher multiply-xor acelera los
    mapas del tree-builder y los atributos del tokenizer, `allow_cdata` se
    cachea y el buffer de tokens se reutiliza.
  - **Layout**: elimina clones de `ComputedStyle` en caminos calientes
    (offsets relativos, shrink-to-fit, absolutos, anchos intrínsecos de texto,
    tablas), itera filas/grupos de tabla sin `Box<dyn Iterator>` (1 044 tablas
    en el bench) y reserva el stream de tokens del IFC. El suelo restante es la
    asignación de cajas/estilos: bajar más exige `LayoutBox.style` como
    `Arc<ComputedStyle>` (afecta a paint).
- `f10635c feat(render,shell)`: `render` construye la display list con hit
  regions (misma pasada de pintado, PNG sin cambios) y expone
  `hit_regions`/`cookies_stored`/`content_encoding`; el shell usa las regiones
  para enlaces/navegación; networking avanzado (cookie jar por render,
  CORS con `--origin`, gzip/deflate verificados end to end).
- `7faa431 feat(js): DOM events and a clock-driven timer scheduler (WP-A)`:
  `addEventListener`/`dispatchEvent`, `Event`, y timers con reloj real; `render`
  bombea la cola de timers una vez tras ejecutar los scripts (con presupuesto).

### 5.2 Verificación de la mejora reportada por el carril perf

La ronda anterior terminó con `html/realistic-1mb` 55,3 ms,
`layout/realistic-1mb` 55,1 ms y pipeline 168,1 ms. Verificación con el bench
oficial (12 pasadas, esta ventana):

| Métrica | Reportado por perf | Nuestra medición (mediana 12) | Diferencia |
|---|---:|---:|---:|
| `html/realistic-1mb` | 55,0 → **19,2 ms** | 55,316 → **20,305 ms** (−63,3 %) | +5,8 % vs claim |
| `layout/realistic-1mb` | 56,1 → **54,5 ms** | 55,140 → **62,171 ms** (+12,8 %) | **no reproducido** |
| `pipeline/html-to-png-800x600` | 167,6 → **131,2 ms** | 168,081 → **143,229 ms** (−14,8 %) | +9,2 % vs claim |

**El salto de HTML se confirma** (20,3 ms; también `html/pretty-5k-blocks`
25,9 → 11,0 ms, `massive-end-p` 23,7 → 18,2 ms). La diferencia con el 19,2 ms
del carril es metodológica: ellos reportan una pasada en ventana limpia;
nosotros publicamos la mediana de 12 pasadas con load 0,86–2,49 y el host
mostrando varianza. El **pipeline baja a 143,2 ms** (mejor pasada limpia:
135,0 ms), un −14,8 %.

**Layout no se reprodujo**: medimos 62,2 ms (rango 55,6–65,6; re-mediciones
filtradas 55,6–67,7). El 54,5 ms del carril es alcanzable en una ventana
favorable (nuestra mejor pasada: 55,56 ms), pero la mediana en esta ventana es
~13 % peor que r9. Con la varianza observada (±15–20 % en layout/paint), **no se
puede afirmar ni mejora ni regresión de layout con este host**; se propone un
A/B en ventana en reposo antes de dar el 54,5 ms por bueno (ver §14 y §5.3).

### 5.3 Hallazgos y riesgos de la ronda

- **`html/tag-40k-attrs` empeora +28 %** (11,6 → 14,9 ms; rango 14,5–16,8, todas
  las pasadas por encima de r9): la optimización de HTML acelera el camino
  normal pero el caso de un elemento con 40 000 atributos es ahora más lento
  (posible coste del hasher/contabilidad nueva en el camino peor). **Es una
  regresión reproducible** y el candidato natural a revisar (audit F3: el
  chequeo de duplicados de atributos era cuadrático; si se cambió a hasher,
  medir el peor caso).
- Filas de layout/paint/raster con dispersión ±15–20 % intra-ventana: usar la
  mediana de varias pasadas y re-medir filtradas antes de concluir nada.
- El resto de HTML/CSS está en el rango esperado (±5 %): el motor no tiene
  regresiones de complejidad, y la mejora de parse/procesado es real.

### 5.4 Diferencias metodológicas con los números del carril perf

- Ellos reportan cifras de una ejecución en una ventana presumiblemente en
  reposo; aquí publicamos mediana de 12 pasadas completas con la carga real del
  host (0,86–2,49) y rango.
- El bench de `layout` depende del estado térmico/frecuencia de la CPU: en esta
  máquina hemos visto la misma carga oscilar 55,6–67,7 ms entre tandas.
- Para comparaciones oficiales: misma máquina, `uptime` < 1,5, median de ≥5
  pasadas y re-medición filtrada de layout/paint si cambian >15 %.

---

## 6. Compresión PNG (bench)

| Métrica | r8 | r9 | r10 | Notas |
|---|---:|---:|---:|---|
| PNG del bench (documento realista 800×600) | 57 422 B | 57 422 B | **57 422 B** | sin cambio |
| `paint/render-to-png-800x600` | 13.296 ms | 12.283 ms | **13.601 ms** | dentro de la varianza del host |
| — rasterizar | 0.993 ms | 0.881 ms | **1.082 ms** | ídem |
| — codificar PNG (por diferencia) | ~12,3 ms | ~11,4 ms | ~12,5 ms | sigue siendo el mayor coste de pintado |

---

## 7. Pipeline `render` y shell headless (ronda 10)

### 7.1 Showcase 1024×768 (golden sin cambios)

`target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png
--width 1024 --height 768 --json`; **11 ejecuciones** intercaladas con `basic`
(load 1,03). Golden verificado byte a byte:

- PNG **80 220 B**, **md5 `c95c7eff83e7bd08f815631aa42afdb0`**, 1024×768,
  opaco. PNG sin cambios vs r9 (las hit regions no afectan a los píxeles).
- Metadatos: `@import` 1, hojas externas 1, hojas saltadas por media 1,
  `<img>` 2, fondos 2 (185 B), items 80, **hit regions 121**,
  `document_bytes` 2 383, `css_bytes` 4 163.

| Etapa (mediana de 7 runs, µs) | fetch | decode/parse | styles | images | backgrounds | scripting | layout | paint | raster | **total** |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| r10 showcase | 25 | 230 | 810 | 159 | 54 | 14 | 1 797 | **28** | 10 170 | **32 795** |
| r9 (referencia) | 15 | 200 | 595 | 100 | 33 | 9 | 1 472 | 20 | 9 282 | 28 941 |

| Variante | pared (ms) | rango (ms) | PNG |
|---|---:|---:|---:|
| **showcase 1024×768 (render, 11 runs)** | **33,55** | 30,61–35,55 | 80 220 B (golden) |
| `basic.html` 800×600 | **15,39** | 14,28–16,08 | 21 090 B (`e943565d…`) |

El total de etapas sube ~13 % frente a r9 en esta ventana, de forma homogénea en
todas las etapas (styles/layout/raster), lo que apunta a **frecuencia/térmica
del host** y no a un coste concreto de r10: la única etapa que el código nuevo
toca es `paint`, que pasa de 20 a **28 µs** por las hit regions (+8 µs, §8.1).
El PNG+driver sigue siendo ~59 % del total. Pared: 33,55 ms (r9 31,23).

### 7.2 Shell headless (`megi --screenshot`, mismo pipeline, sin ventana)

| Fixture | pared (ms) | rango | PNG |
|---|---:|---:|---:|
| showcase 1024×768 | **33,33** | 31,43–37,20 | 80 220 B (golden, byte-idéntico a `render`) |
| basic.html 800×600 | **16,12** | 15,28–16,88 | 21 090 B |

El shell ya consume las hit regions (enlaces/navegación) sin una segunda pasada:
las regiones se generan en el mismo pintado que el PNG. No crea ventana ni
`EventLoop` en `--screenshot`.

---

## 8. Coste de las features nuevas (ronda 10)

### 8.1 Hit regions en el pipeline `render`

`render` construye la display list con `paint::build_display_list_with_hit_regions`
(sin flag para desactivarlas; el PNG es idéntico). Coste medido por la etapa
`paint` del JSON (mediana de 7 runs) y recuento de regiones:

| Fixture | items | hit regions | etapa paint r9 | etapa paint r10 | Δ |
|---|---:|---:|---:|---:|---:|
| showcase 1024×768 | 80 | **121** | 20 µs | **28 µs** | **+8 µs** |
| basic 800×600 | 3 | **7** | 4 µs | **8 µs** | **+4 µs** |

Es **<0,03 % de la pared** del render; el commit reporta +0,6 µs en su
micro-medición (solo el bucle de grabación). El bench **no** las usa
(`paint::build_display_list`), así que las filas de §3 no incluyen este coste.
El shell reutiliza las regiones para hit-testing de enlaces (sin pasada extra).

### 8.2 Cookies / CORS / gzip (loopback)

Servidor HTTP local (`target/bench-net-r10/serve.py`, `127.0.0.1:18765`) sirve
el mismo documento en plano y con `Content-Encoding: gzip` + `Set-Cookie`;
7 ejecuciones por variante (400×240):

| Variante | pared (ms) | `content_encoding` | `cookies_stored` |
|---|---:|---|---:|
| fichero local (referencia) | 15,04 | — | 0 |
| HTTP plano | 15,31 | — | 0 |
| HTTP gzip | 14,57 | gzip | 0 |
| HTTP gzip + `--cookies` | 14,64 | gzip | **1** |
| HTTP gzip + `--origin http://cross.example` | 14,28 | gzip | 0 |
| HTTP gzip + cookies + origin | 14,28 | gzip | **1** |

Todas las diferencias caen dentro del ruido de ±1 ms de un render de 15 ms
(**coste despreciable**, <1 %). El gzip se decodifica en la etapa
`decode_parse`; una hoja/subrecurso CORS denegado es no fatal (warning) y solo
afecta a recursos HTTP. Los fixtures locales (`file://`) no pasan por este
camino: el bench no paga nada por estas features.

### 8.3 Eventos y timers JS (WP-A)

Fixtures temporales en `target/bench-js-r10/` (no versionados); 7 ejecuciones
con y sin scripting:

| Fixture | qué hace | pared JS (ms) | pared sin JS (ms) | Δpared | `timings.scripting` | ΔPNG |
|---|---|---:|---:|---:|---:|---:|
| `events` | `addEventListener("click")` + `dispatchEvent` + 2 `setTimeout(…,10)` | 16,14 | 15,73 | +0,41 | 894 µs | +148 B |
| `events0` | igual, timers a **0 ms** | 15,62 | 14,87 | **+0,75** | 730 µs | **+1 110 B** |
| `timers` | solo 2 `setTimeout(…,10)` | 17,37 | 15,40 | +1,97 | 545 µs | 0 B |
| `timers0` | solo 2 `setTimeout(…,0)` | 16,41 | 15,51 | **+0,90** | 684 µs | **+1 178 B** |

Conclusiones:

- **El dispatch de eventos funciona** y es barato (scripting 0,7–0,9 ms con
  re-render incluido en la pared; ~0,55 µs por callback de 1 000 medido aparte).
- **Los timers con retardo >0 no llegan a ejecutarse** antes del screenshot: el
  scheduler es de reloj real y `render` bombea la cola **una sola vez**
  inmediatamente después de los scripts; solo los callbacks ya vencidos (0 ms)
  corren. Un `setTimeout(…,10)` no muta el DOM en esta ruta (ΔPNG = 0). Es una
  limitación conocida, no un fallo de rendimiento: no hay event loop de pared.
- Coste de JS con eventos+timers que mutan el DOM: **≈0,75–0,90 ms de pared**
  (5–6 % de un render de 14–15 ms), coherente con los 0,7–2,0 ms de la matriz
  JS de r9.

### 8.4 Build, CI, binarios y runtime

| Comando | Tiempo | Notas |
|---|---:|---|
| `cargo build --release --workspace` (frío, r9; no re-medido r10) | 37,3 s | Boa + winit + rustls + shell; r10 añade ~poco código |
| `cargo test --workspace` (incremental en esta ventana) | **8,8 s** | **72 suites, 820 tests**, todo OK (r9: 70/793) |
| `cargo test --workspace` (frío, r9) | 31,8 s | referencia de CI |

| Binario | r10 | r9 | Δ |
|---|---:|---:|---:|
| `target/release/render` (con JS + hit regions) | **18 953 968 B** (18,07 MiB) | 18 907 360 B | +46 608 B (+0,25 %) |
| `target/release/megi` (shell) | **22 985 824 B** (21,92 MiB) | 22 955 984 B | +29 840 B (+0,13 %) |
| `target/release/bench` | 4 264 984 B | 4 276–4 279 kB | ≈0 |

**Runtime JS con WP-A** (sonda mínima en `target/`, release):
`JsRuntime::new()` **0,105 ms** (r8: 0,096; +9 % ≈ +9 µs, despreciable),
`eval("1+1")` 0,0018 ms, arranque+primer eval 0,106 ms, y **1 000 callbacks de
listener 0,544 ms** (~0,54 µs/callback). El coste de Fase 6 sigue estando en
build/CI y tamaño (Boa), no en el arranque.

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

Rasterizador software, monohilo, sin GPU. Presupuestos vigentes: **pipeline
≤ 200 ms** y **render showcase ≤ 35 ms**. Con esta ronda:

| Benchmark / medida | Baseline r10 | Presupuesto actual | % | Propuesta |
|---|---:|---:|---:|---|
| `html/realistic-1mb` | 20 ms | ≤ 100 ms | 20 % | mantener |
| `html/nested-div-64k` | 20 ms | ≤ 100 ms | 20 % | mantener |
| `html/massive-end-p` | 18 ms | ≤ 50 ms | 36 % | mantener |
| `html/tag-40k-attrs` | 15 ms | ≤ 50 ms | 30 % | mantener (vigilar regresión §5.3) |
| `css/parse-10k-rules` | 14 ms | ≤ 30 ms | 47 % | mantener |
| `css/resolve-styles-10k-nodes` | 29 ms | ≤ 75 ms | 39 % | mantener |
| `layout/realistic-1mb` | 62 ms (55–66) | ≤ 150 ms | 41 % | mantener |
| `paint/build-display-list` | 16 ms (14–20) | ≤ 20 ms | 81 % | subir a **≤ 25 ms** (varianza) |
| `paint/rasterize-800x600` | 1,08 ms | ≤ 25 ms | 4 % | mantener |
| `paint/render-to-png-800x600` | 13,6 ms | ≤ 50 ms | 27 % | mantener |
| `pipeline/html-to-png-800x600` | **143,2 ms** (mejor 135,0) | **≤ 200 ms** | 72 % | **bajar a ≤ 160 ms** (89 %; alerta >150 ms) |
| `render` showcase 1024×768 | **33,6 ms** | **≤ 35 ms** | **96 %** | **subir a ≤ 40 ms** (84 %; objetivo 33–34) |
| shell headless showcase | **33,3 ms** | **≤ 35 ms** | **95 %** | ídem |
| JS scripting/eventos por página | 0,7–2,0 ms | — | — | documentado §8 |

**Propuesta de presupuesto**: el pipeline ha pasado de 168 a 143 ms, así que
mantener ≤200 ms ya no informa de nada. Se propone **≤160 ms** (el objetivo que
sugiere el carril perf), con la mediana de ≥3 pasadas como gate y un umbral de
alerta en 150 ms; si se quiere un margen del 15 % con la varianza actual,
≤170 ms es más prudente. En cambio, el **render showcase está al 96 % de su
presupuesto de 35 ms**: se propone subirlo a **40 ms** (objetivo 33–34 ms) o
aceptar que el siguiente incremento de funcionalidad lo rompa. `paint/build-display-list`
se acerca al 20 ms con la varianza observada: subir a 25 ms.

---

## 11. Comparación histórica

Nueve baselines en la misma máquina, mismo arnés:

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 (vigente) |
|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | **17:42** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | **`f10635c`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | limpio | docs dirty | limpio | **limpio (12 pasadas)** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | `43f7748d…` | **`a188a1da…`/`ae5d86d1…`** |
| Load (bench) | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | **0,86–2,49** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | **20.305** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | 25.658 | 25.084 | 24.772 | 24.022 | **20.396** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | **29.422** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | **62.171** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | **16.128** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | 0.843 | 0.993 | 0.881 | **1.082** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | **13.601** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | **143.229** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** |

### Ronda 9 → ronda 10, fila a fila

| Benchmark | R9 (ms) | R10 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 55.316 | **20.305** | **−63,3 %** |
| `html/nested-div-10k` | 3.662 | **3.186** | −13,0 % |
| `html/nested-div-32k` | 11.611 | **10.018** | −13,7 % |
| `html/nested-div-64k` | 24.022 | **20.396** | −15,1 % |
| `html/nested-div-64k-default-limits` | 13.963 | **12.941** | −7,3 % |
| `html/massive-end-p` | 23.729 | **18.225** | −23,2 % |
| `html/massive-formatting` | 15.113 | **11.713** | −22,5 % |
| `html/tag-40k-attrs` | 11.645 | **14.921** | **+28,1 %** ⚠️ |
| `html/pretty-5k-blocks` | 25.863 | **10.990** | −57,5 % |
| `css/parse-10k-rules` | 13.698 | **14.127** | +3,1 % |
| `css/resolve-styles-2k-nodes` | 5.940 | **6.101** | +2,7 % |
| `css/resolve-styles-10k-nodes` | 28.241 | **29.422** | +4,2 % |
| `css/selector-match-simple` | 0.666 | **0.671** | +0,8 % |
| `css/selector-match-descendant` | 1.228 | **1.260** | +2,6 % |
| `layout/realistic-1mb` | 55.140 | **62.171** | +12,8 % † |
| `paint/build-display-list` | 14.050 | **16.128** | +14,8 % † |
| `paint/rasterize-800x600` | 0.881 | **1.082** | +22,8 % † |
| `paint/render-to-png-800x600` | 12.283 | **13.601** | +10,7 % † |
| `pipeline/html-to-png-800x600` | 168.081 | **143.229** | **−14,8 %** |
| **TOTAL (suma)** | **485.132** | **410.485** | **−15,4 %** |

† Filas con varianza de host: ver la nota de §3; las re-mediciones filtradas
confirman que paint/build-display-list puede estar en 13,6 ms (≈ r9) y layout en
55,6–67,7 ms. No son conclusiones fiables a ±20 %.

La ronda es un salto real: **HTML parse −63 %, pipeline −15 %, TOTAL −15 %**.
Layout y paint no muestran un cambio atribuible con la varianza actual; el
tag-40k-attrs es la única regresión reproducible y queda flagged.

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
3. Hacer **≥5 pasadas** y usar la mediana si el host está cargado (esta ronda:
   12); para layout/paint/raster, re-medir filtrado antes de concluir.
4. Regresión si la mediana empeora >15 % de forma reproducible; dispersión
   intra-ventana ~1–6 % en reposo y hasta ±20 % en filas sensibles con carga.
5. Para complejidad, usar `--scale 0.5` vs `1.0`: ratios ≈1.
6. Features nuevas: `render --json` expone `hit_regions`, `cookies_stored`,
   `content_encoding`, `scripts_executed`, `timings_us`; para eventos/timers,
   usar los fixtures de `target/bench-js-r10/` (§8.3) y el servidor loopback de
   `target/bench-net-r10/` (§8.2).
7. JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
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

Coste de referencia de CI en esta máquina (20 hilos): build release frío
**≈37 s** y `cargo test --workspace` frío **≈32 s** (r9); en r10 el test
incremental tarda **8,8 s** con **72 suites / 820 tests**. En runners de 2–4
núcleos será varias veces mayor por el paralelismo de Boa. Los números de CI no
son comparables (otra CPU, runners compartidos); el job solo comprueba que
compila y ejecuta, y deja el JSON como artefacto.

---

## 14. Limitaciones y notas

- **Host muy variable en esta ventana**: load 0,86–3,53 con otros agentes
  compilando; las filas de layout/paint/raster oscilaron ±15–20 % entre tandas
  (la misma medición filtrada dio 55,6–67,7 ms de layout). Por eso la baseline
  usa 12 pasadas y publica rangos; las conclusiones de layout/paint deben
  validarse en ventana en reposo.
- **`target/` compartido**: el binario de bench se recompiló una vez con la
  misma fuente (`a188a1da…` → `ae5d86d1…`); los shas de artefactos dependen de
  la build usada en cada tanda.
- **tag-40k-attrs**: regresión reproducible +28 % (40 000 atributos); flagged
  para el carril de parser. El resto de HTML parse mejora con fuerza.
- **Verificación de claims**: el 19,2 ms de HTML se confirma (20,3 ms de
  mediana); el 54,5 ms de layout **no** se reproduce (62,2 ms de mediana,
  55,6–67,7) y el 131,2 ms de pipeline se queda en 143,2 ms (mejor pasada
  135,0). Diferencias metodológicas explicadas en §5.4; se recomienda A/B en
  reposo antes de fijar el layout como mejorado.
- **Hit regions**: opt-in en `paint`; `render` las genera siempre (misma pasada,
  PNG idéntico, +8 µs en el showcase) y el shell las usa para enlazar. El bench
  no las usa.
- **Cookies/CORS/gzip**: medidos en loopback (`file://` no pasa por HTTP); coste
  <1 ms (ruido). Sin TLS ni red externa.
- **Timers**: el scheduler es de reloj real y `render` bombea la cola una sola
  vez tras los scripts; los `setTimeout` con retardo >0 no se ejecutan antes del
  screenshot (documentado, no es coste). `addEventListener`/`dispatchEvent`
  funcionan y cuestan ~0,8 ms con re-render.
- **Presupuestos**: se propone pipeline **≤160 ms** (mediana actual 143,2) y
  render showcase **≤40 ms** (actual 33,6), y `build-display-list` ≤25 ms; ver
  §10.
- **Fixtures de medida**: los de eventos/timers y el servidor loopback viven en
  `target/` (gitignored) y se recrean con los comandos de §1.4; no forman parte
  del repo.
- **Fast path de tablas (r7)**: la comparación 67,2 → 61,4 ms usa el valor
  intermedio documentado por el commit de layout; no hay un commit compilable
  con el camino avanzado sin fast path.
