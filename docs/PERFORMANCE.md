# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 16:49–17:05 (Europe/Madrid). Ronda 8:
  texto de tablas en paint, `background-image`/`list-style-image` end to end,
  golden showcase 1024×768 actualizado, `net` (cookies/CORS/gzip) y **Boa 0.21**
  embebido en `crates/js` (Fase 6, WP-6.1).
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `31e5139`**
  (`feat(net): cookies, CORS and gzip/deflate with bomb protection`).
- **Estado del árbol**: código limpio en `31e5139` durante toda la ventana de
  medida; otro agente actualizó documentación en paralelo (`README.md`,
  `docs/ARCHITECTURE.md`, `docs/HANDOFF.md`, `docs/ROADMAP.md`,
  `docs/images/showcase.png`). A partir de ~16:59 (ya cerrada la ventana) otros
  agentes empezaron cambios de código sin commitear de la integración de Fase 6
  (`crates/js`, `paint`, `render`, `shell`, `Cargo.lock`): no afectan a las
  cifras, que corresponden a builds de `31e5139`.
- **Binario medido**: `target/release/bench` (los binarios `megi`/`render` se
  comparten con los demás agentes y se re-enlazan durante el trabajo en
  paralelo; las medidas de §7 usaron los presentes en la ventana y su salida se
  validó contra el golden). Compilado con
  `cargo build --release -p bench`. Se recompiló una vez dentro de la ventana
  (mismo código): las 3 pasadas JSON usaron
  `sha256 = ed7006e9265350b6467b31143d8416c7af9d5863163a71abc96985d899f30a6e`
  (4 274 768 B) y la pasada canónica de `./tools/bench.sh` usó
  `sha256 = 4c7eb74bb526dbda1de3140aeb60edc0ef262da3cd477d9501c5c29bda322b6f`
  (4 273 848 B); las cifras difieren <1 % entre ambas builds.
- **Artefactos guardados** (gitignored, no versionados):
  - `target/bench-r8-run1.json` … `target/bench-r8-run3.json` — pasadas
    completas usadas para la tabla (mediana por fila); shas `e9db72b4…`,
    `99bce652…`, `3b05aeef…`.
  - `target/bench-aggregate-r8.json` — medianas agregadas
    (`sha256 = 021db0565535515b01afd677b948637122b34eff2b3590704f59aa879d747922`).
  - `target/bench-latest.txt` / `-meta.txt` — pasada del script
    (`TOTAL 511.102 ms`, `f0832c23…`).
  - `target/perf-r8-render-timings.json`, `target/perf-r8-meta.json`,
    `target/perf-r8-js-builds.json`, `target/perf-r8-showcase-variants.json`,
    `target/perf-r8-cargo-test.json`, `target/perf-r8-cold-build.json` — el
    resto de medidas de la ronda.

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §11 (rondas 2–7).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Todas las cargas se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`). La misma `--scale` produce siempre el mismo
flujo de bytes. El *fixture* se construye **fuera** del cronómetro; los
benchmarks de parse incluyen la construcción y liberación del `Document`.

**El crate `bench` no cambió en esta ronda**, así que todas las diferencias
r7 → r8 son de código del motor. Además, `bench` solo depende de
`dom/html/css/layout/paint` (y `fonts` opcional): **no enlaza `net`, `js` ni
`image`**, y sus generadores no contienen `url(...)`/`background-image`, así que
ni Boa, ni la red, ni las imágenes de fondo intervienen en las cifras de §3
(ver §5 y §8).

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- La tabla de §3 es la **mediana, por fila, de 3 pasadas completas** (cada
  pasada ya es una mediana de 5/3 iteraciones). Rango = mínimo–máximo de las 3
  medianas. Una cuarta pasada canónica con `./tools/bench.sh` dio
  `TOTAL 511.102 ms` (vs 512.440 de la mediana de las 3 pasadas JSON).
- Para cada pasada el propio binario construye y parsea sus fixtures:
  `setup_ms` = 263,9–270,6 ms (no cronometrado).
- Ventana: **load average 2,90–3,25** durante las 3 pasadas del bench (otros
  agentes compilaban en paralelo; picos de 5,9–7,4 en los builds y en la
  ventana de render). Los benchmarks son monohilo y la máquina tiene 20 hilos
  lógicos, por eso las cifras siguen siendo comparables a r7 (±1–5 %); aun así
  las medidas de pared con load alto se tratan como orientativas.
- Las medidas del CLI `render`/`megi` usan cronómetro de pared de Python
  (7–15 ejecuciones intercaladas, mediana; el entorno no tiene `/usr/bin/time`).
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` por defecto (sin `[profile.release]` en el workspace) |
| Carga de fondo | bench: load 2,90–3,25; builds/render: 2,9–7,4 |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Pasada canónica (compila release + ejecuta + guarda target/bench-latest.txt)
./tools/bench.sh

# Las 3 pasadas usadas para la tabla de esta baseline
for i in 1 2 3; do
  target/release/bench --json > "target/bench-r8-run$i.json"
done

# Smoke rápido (~1 s)
./tools/bench.sh --scale 0.1 --iterations 2

# Un grupo o un benchmark concreto
./tools/bench.sh --filter layout
cargo run --release -p bench -- --filter layout/realistic-1mb

# Render del showcase 1024x768 (fixture real con @import, media, <img> y fondos)
cargo run --release -p render -- \
    tests/fixtures/render/showcase/index.html /tmp/showcase.png \
    --width 1024 --height 768 --json

# Shell headless (mismo pipeline, sin ventana)
cargo run --release -p shell -- \
    tests/fixtures/render/showcase/index.html \
    --screenshot /tmp/showcase.png --width 1024 --height 768

# Coste de la Fase 6 (build y tests, ver §8)
cargo build --release --workspace                      # frío: 43,3 s
cargo build --release -p js                            # frío (Boa): 27,9 s
cargo build --release -p js --no-default-features      # frío (stub): 1,5 s
cargo test --workspace                                 # frío: 30,1 s
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
| layout | `layout/realistic-1mb` | `layout_document` sobre el documento realista (28 195 elementos, 88 155 cajas), 800×600. Usa el fast path de tablas. |
| paint | `paint/build-display-list` | Display list: **34 158 items** (r7: 24 762; +9 396 = 1 044 tablas × 9 celdas, texto de tabla). |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` DEFLATE (**57 422 B**; r7: 54 526 B). |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo: parse → styles → layout → display list → raster → PNG. |
| fonts | (`feature fonts`) | No re-medido en r8 (sin cambios en `crates/fonts`; referencia r6: measure 0,456 ms, shape 8,119 ms). |

---

## 3. Baseline vigente (2026-11-10, HEAD `31e5139`)

Mediana por fila de **3 pasadas completas** (5 iteraciones, 1 warm-up; *heavy*
3). `setup`: 263,9–270,6 ms (no cronometrado). Binario
`sha256 = ed7006e9…` (3 pasadas JSON; la pasada del script con `4c7eb74b…` dio
`TOTAL 511.102 ms`). La suma de medianas da `TOTAL 512.440 ms`.

| Benchmark | Carga | mediana (ms) | rango 3 pasadas (ms) | throughput |
|---|---|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | **56.140** | 55.524–56.821 | 17,8 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | **3.811** | 3.776–3.824 | 27,5 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | **12.128** | 12.014–12.350 | 27,7 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | **24.772** | 24.721–24.955 | 27,1 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | **14.949** | 14.808–15.147 | — (59 906 start tags descartados) |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | **23.879** | 23.741–24.648 | 4,19 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | **16.292** | 16.108–16.369 | 614 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | **13.187** | 12.753–13.281 | 3,03 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | **26.800** | 26.751–26.984 | 187 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | **14.941** | 14.759–15.444 | 39,9 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | **6.097** | 6.018–6.400 | 329 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | **28.662** | 28.430–29.029 | 349 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | **0.665** | 0.655–0.668 | 60,1 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | **1.272** | 1.263–1.273 | 31,5 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | **59.767** | 59.020–60.197 | 472 k elementos/s |
| `paint/build-display-list` | 34 158 items | **15.090** | 15.043–15.186 | — |
| `paint/rasterize-800x600` | 34 158 items → pixmap 800×600 | **0.993** | 0.978–1.024 | 484 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (57 422 B) | **13.296** | 13.109–13.563 | 4,12 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | **179.699** | 176.771–181.674 | — |
| **TOTAL (suma)** | | **512.440** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página.

---

## 4. Complejidad observada

Escalados con el mismo binario, complementarios a §3:

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.81 ms · 32k: 12.13 ms · 64k: 24.77 ms | **Lineal**: 3,2× input → 3,18× tiempo; 2× → 2,04×. |
| `html/tag-40k-attrs` | 10k: 2.66 ms · 20k: 6.50 ms · 40k: 12.46 ms | **Lineal** (~0,3 µs/atributo). |
| `css/parse-10k-rules` | 5k: 6.69 ms · 10k: 14.26 ms · 20k: 30.50 ms | **Lineal** (2,13× y 2,14× por cada 2×). |
| `css/resolve-styles-2k/10k` | 2 005 el: 6.10 ms · 10 005 el: 28.66 ms | **Lineal**: 5× elementos → 4,70× tiempo (~349 k el/s). |
| `paint/build-display-list` | 24 762 items (r7): 12.46 ms · 34 158: 15.09 ms | **+21,1 %** por los 9 396 items legítimos de texto de tabla (~0,28 µs/item). |
| `paint/rasterize-800x600` | 24 762 items: 0.843 ms · 34 158: 0.993 ms | **Plano respecto al nº de items**: el coste lo fija el viewport (+0,15 ms por el AA de los glifos de tabla). |
| `layout/realistic-1mb` | 28 195 elementos → 59.77 ms (88 155 cajas) | **−2,6 % respecto a r7** (61,35 ms), dentro del ruido de la ventana. |

---

## 5. Ronda 8: texto de tablas, imágenes y red

### 5.1 Qué entró en la ronda

- `be72a25 feat(paint): background-image/list-style-image rendering and table
  cell content`: pinta `<img>` de fondo estiradas al padding box
  (bilineal/AA, sin repeat/size/position) y marcadores `list-style-image`;
  además corrige la causa real de que faltara el texto de tabla:
  `paint_inline_content` ahora desciende en cajas
  `Table/TableRowGroup/TableRow/TableCell/TableCaption/…` en vez de cortar en
  el brazo genérico. El golden de `paint` (`basic.rgba`, sin tablas) sigue
  byte-idéntico.
- `0f556bd test(layout): pin the table content tree model`: 4 tests que fijan
  el árbol de contenido de tablas (sin cambios de rendimiento).
- `debc1e8 feat(render): background-image and list-style-image end to end` y
  nuevo golden showcase 1024×768 (`c95c7eff…`, 80 220 B).
- `f3d9eef feat(js): embed Boa and the first DOM bindings` (Fase 6) — coste de
  build/CI en §8; **no** interviene en el bench.
- `31e5139 feat(net): cookies, CORS and gzip/deflate`: `net` tampoco es
  dependencia del bench; solo afecta al build del workspace.

### 5.2 Efecto medido en el bench

| Métrica | r7 | r8 | Δ | Lectura |
|---|---:|---:|---:|---|
| items de display list | 24 762 | **34 158** | **+9 396 (+37,9 %)** | exactamente 1 044 tablas × 9 celdas: el texto de celda que antes no se pintaba |
| `paint/build-display-list` | 12.458 ms | **15.090 ms** | **+21,1 %** | trabajo legítimo: ~0,28 µs por item nuevo |
| `paint/rasterize-800x600` | 0.843 ms | **0.993 ms** | **+17,8 %** | AA de los glifos nuevos; sigue en <1 ms |
| `paint/render-to-png-800x600` | 13.045 ms | **13.296 ms** | +1,9 % | el encoder PNG domina (≈12,3 ms) |
| PNG del bench | 54 526 B | **57 422 B** | +5,3 % | ahora se ven los textos de tabla |
| `pipeline/html-to-png-800x600` | 178.791 ms | **179.699 ms** | **+0,5 %** | dentro del ruido y del presupuesto (≤200 ms) |

El crecimiento del display list **no es una regresión de complejidad**: es el
producto correcto (el texto de las tablas ya se pinta). El coste por item se
mantiene lineal y el pipeline end-to-end solo sube 0,9 ms.

Las **imágenes de fondo no aparecen en el bench** (los generadores no usan
`url(...)`; verificado con `grep`): su coste solo se ve en el pipeline `render`
del showcase (§7) y se aísla en §5.3. Igual ocurre con `net` y con Boa: no son
dependencias de `crates/bench`.

### 5.3 Coste aislado de `background-image` (showcase 1024×768)

Variante del fixture copiada a `/tmp` eliminando las líneas `background-image:`
de los CSS (7 ejecuciones intercaladas con el fixture completo; load alto,
útil solo como delta):

| Variante | pared (ms) | items | raster (µs) | PNG |
|---|---:|---:|---:|---:|
| showcase completo | 32,33 | 80 | 9 650 | 80 220 B |
| sin `background-image` | **25,89** | 78 | **2 775** | 77 109 B |
| **coste de 2 fondos** | **+6,4 ms** | +2 items | **+6 875 µs** | +3 111 B |

El rasterizado es el que paga el precio: el muestreo bilineal/AA de los fondos
estirados sobre áreas grandes (hero y stage) multiplica por ~3,5 el coste de la
etapa. El resto del pipeline (parse/styles/layout/paint) no cambia. Es
coherente con lo documentado en el commit: no hay `background-repeat`/`size`/
`position`, se estira al padding box.

---

## 6. Compresión PNG (bench)

| Métrica | r6 | r7 | r8 | Notas |
|---|---:|---:|---:|---|
| PNG del bench (documento realista 800×600) | 54 526 B | 54 526 B | **57 422 B** | +2 896 B por el texto de tabla |
| `paint/render-to-png-800x600` | 13.016 ms | 13.045 ms | **13.296 ms** | estable (+1,9 %) |
| — rasterizar | 0.890 ms | 0.843 ms | **0.993 ms** | +AA de glifos de tabla |
| — codificar PNG (por diferencia) | ~12,1 ms | ~12,2 ms | **~12,3 ms** | sigue siendo el mayor coste de pintado |

---

## 7. Pipeline `render` y shell headless (ronda 8)

### 7.1 Showcase 1024×768 (golden nuevo)

`cargo run --release -p render -- tests/fixtures/render/showcase/index.html
/tmp/s.png --width 1024 --height 768 --json`; **9 ejecuciones** intercaladas
con `basic`. Golden verificado byte a byte:

- PNG **80 220 B**, **md5 `c95c7eff83e7bd08f815631aa42afdb0`**
  (`sha256 0fc70eae…`), 1024×768, opaco. Sustituye al golden r7
  (`570aa554…`, 65 151 B) e incluye el texto de tabla y los dos fondos.
- Metadatos: `@import` 1 (`theme.css`), hojas externas 1, hojas saltadas por
  media 1 (`print`), `<img>` 2, imágenes de fondo 2 (185 B), items 80,
  `document_bytes` 2 383, `css_bytes` 4 163.

| Etapa (single run, µs) | fetch | decode/parse | styles | images | backgrounds | layout | paint | raster | **total** |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| r8 showcase | 15 | 178 | 570 | 98 | 35 | 1 466 | 22 | **9 475** | **29 679** |
| r7 showcase (referencia) | 23 | 256 | 626 | 114 | — | 1 530 | 19 | 3 018 | 23 641 |

| Variante | pared (ms) | rango (ms) | PNG |
|---|---:|---:|---:|
| **showcase 1024×768 (render)** | **30,47** | 29,99–31,96 | 80 220 B (golden) |
| showcase sin fondos (§5.3, ventana aparte) | 25,89 | — | 77 109 B |
| `basic.html` 800×600 | **14,01** | 13,74–16,06 | 21 090 B (`e943565d…`) |
| `images.html` 800×600 | — | — | 15 625 B (2 tags `<img>`: 1 OK, 1 fallback; `1de31699…`) |
| `backgrounds.html` 800×600 | — | — | 15 778 B (2 fondos; `2dbb0832…`) |

El sobrecoste de pared del showcase r7 → r8 (25,18 → 30,47 ms, **+5,3 ms**) es
el muestreo de los fondos (+6,9 ms de raster en la variante aislada, §5.3),
compensado en parte por layout/estilos estables. La **etapa PNG+driver sigue
siendo ~60 %** del total (29,68 ms total − 11,86 ms de etapas medidas ≈ 17,8 ms
de encode/setup/escritura). El presupuesto de render (≤35 ms) queda al **87 %**.

### 7.2 Shell headless (`megi --screenshot`, mismo pipeline, sin ventana)

| Fixture | pared (ms) | rango | PNG |
|---|---:|---:|---:|
| showcase 1024×768 | **31,31** | 29,75–32,12 | 80 220 B (golden, idéntico a `render`) |
| basic.html 800×600 | **14,50** | 14,11–17,28 | 21 090 B |

El shell headless no crea ventana ni `EventLoop`; produce **PNG byte-idéntico**
al CLI `render`. Su sobrecoste es <1 ms (carga del binario + args + escritura).
`megi` no enlaza Boa hoy (shell depende de render/paint/net/winit/softbuffer;
`js` es un crate independiente), así que estas cifras no incluyen JS.

---

## 8. Coste de la Fase 6 (build y runtime)

Boa 0.21 se embebió en `crates/js` detrás de la feature **`js` (activa por
defecto)**; `--no-default-features` compila un stub que devuelve
`JsError::FeatureDisabled`. Medido en esta ventana (20 hilos; load 2,9–7,4):

### 8.1 Build y CI

| Comando | Tiempo | Notas |
|---|---:|---|
| `cargo build --release --workspace` (frío, `CARGO_TARGET_DIR` vacío) | **43,3 s** | incluye Boa, winit, rustls/net, shell y todos los crates |
| `cargo build --release --workspace` (incremental sobre target existente) | **30,8 s** | solo crates pendientes (Boa, shell, net, fonts, render…) |
| `cargo build --release -p js` (frío, feature `js`) | **27,9 s** | Boa + transitivas dominan (~390 MB de target) |
| `cargo build --release -p js --no-default-features` (frío, stub) | **1,5 s** | 5,7 MB de target |
| `cargo test --workspace` (frío, target vacío; debug + tests) | **30,1 s** | 68 suites, 734 tests, todo OK; 3,5 GB de target |
| `cargo test --workspace` (incremental) | **25,7 s** | 68 suites, 734 tests, todo OK |

**Lectura**: la feature de JS añade **≈26 s de compilación en frío** al crate
`js` (27,9 s vs 1,5 s del stub, ~94 % del tiempo del crate) y es la mayor parte
del crecimiento del build del workspace (43,3 s fríos). Un build incremental
del workspace con JS ya compilado baja a pocos segundos (no medido aquí; el
bench incremental fue 0,6 s). Para CI: **≈44 s de build release frío** y
**≈30 s de `cargo test --workspace` frío** en esta máquina de 20 hilos; en
runners de 2–4 núcleos será varias veces mayor por el paralelismo de Boa.

### 8.2 Binarios

| Binario | Tamaño | `sha256` |
|---|---:|---|
| `target/release/megi` (shell) | **9,99 MiB** (10 476 672 B) | `c02d04fd…` |
| `target/release/render` | **6,15 MiB** (6 454 664 B) | `fa0df155…` |
| `target/release/bench` | **4,08 MiB** (4 273 848 B) | `4c7eb74b…` |
| sonda release con Boa (js+dom+html+css) | **12,86 MiB** (13 486 216 B) | — |
| misma sonda sin `js` (stub) | **0,43 MiB** (446 072 B) | — |

**Coste en tamaño de Boa cuando se enlaza: +12,4 MiB** (12,86 vs 0,43 MiB en
una sonda mínima). Hoy `megi`/`render` **no** enlazan `js`, así que sus tamaños
no incluyen Boa; cuando `render`/`shell` ejecuten `<script>` ese será el
sobrecoste esperado de binario (más el de `net`, ya presente).

### 8.3 Runtime JS (arranque y eval)

Sonda mínima release creada **bajo `target/`** (no versionada) con
`js::JsRuntime::new()`/`eval`; 20 arranques y 50 evaluaciones tras warm-up:

| Medida | Mediana | Mínimo |
|---|---:|---:|
| `JsRuntime::new()` (realm Boa + contexto + bindings) | **0,096 ms** | 0,066 ms |
| `eval("1+1")` en realm caliente | **0,002 ms** | 0,001 ms |
| arranque + primer eval | **0,084 ms** | 0,070 ms |
| script con bucle de 10 000 iteraciones (+ suma), caliente | **5,19 ms** | 4,82 ms |

**Lectura**: el coste de arranque del runtime es despreciable (<0,1 ms, dos
órdenes de magnitud por debajo del pipeline de render); el coste de Fase 6 está
en **build/CI y tamaño de binario**, no en runtime de página. El bucle de 10k
iteraciones a ~0,5 µs/iteración refleja los *budgets* anti-DoS instalados por
`eval`; no es representativo de un benchmark JS real (no hay uno todavía).

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

| Benchmark / medida | Baseline r8 | Presupuesto | Estado |
|---|---:|---:|---|
| `html/realistic-1mb` | 56 ms | ≤ 100 ms | ✅ (56 %) |
| `html/nested-div-64k` | 25 ms | ≤ 100 ms | ✅ |
| `html/massive-end-p` | 24 ms | ≤ 50 ms | ✅ |
| `html/tag-40k-attrs` | 13 ms | ≤ 50 ms | ✅ |
| `html/pretty-5k-blocks` | 27 ms | ≤ 75 ms | ✅ |
| `css/parse-10k-rules` | 15 ms | ≤ 30 ms | ✅ |
| `css/resolve-styles-10k-nodes` | 29 ms | ≤ 75 ms | ✅ |
| `layout/realistic-1mb` | 60 ms | ≤ 150 ms | ✅ |
| `paint/build-display-list` | 15,1 ms | ≤ 20 ms | ✅ (**75 %**; +21 % legítimo por texto de tabla) |
| `paint/rasterize-800x600` | 0,99 ms | ≤ 25 ms | ✅ |
| `paint/render-to-png-800x600` | 13,3 ms | ≤ 50 ms | ✅ |
| `pipeline/html-to-png-800x600` | **179,7 ms** | **≤ 200 ms** | ✅ (**89,9 %**, margen 20,3 ms) |
| `render` showcase 1024×768 | **30,5 ms** | **≤ 35 ms** | ✅ (**87,1 %**, margen 4,5 ms) |
| shell headless showcase | **31,3 ms** | **≤ 35 ms** | ✅ (**89,5 %**, margen 3,7 ms) |
| build release workspace frío (con JS) | 43,3 s | — (CI) | ⚠️ coste Fase 6, ver §8 |

Todos los presupuestos se cumplen. El más ajustado pasa a ser el **showcase de
render** (87 %), seguido del pipeline (90 %) y del build-display-list (75 %);
layout (60 ms) y parse HTML (56 ms) siguen siendo el 64 % del pipeline.

---

## 11. Comparación histórica

Siete baselines en la misma máquina, mismo arnés:

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 (vigente) |
|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | **16:49** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | **`31e5139`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | limpio al inicio | **code limpio; docs dirty** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | **`ed7006e9…` / `4c7eb74b…`** |
| Load | — | — | — | — | — | 0,99–1,52 | **2,90–3,25 (bench)** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 |
|---|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | **56.140** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | 25.658 | 25.084 | **24.772** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | **28.662** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | **59.767** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | **15.090** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | 0.843 | **0.993** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | **13.296** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | **179.699** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** |

### Ronda 7 → ronda 8, fila a fila

| Benchmark | R7 (ms) | R8 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 55.926 | **56.140** | +0,4 % |
| `html/nested-div-10k` | 3.897 | **3.811** | −2,2 % |
| `html/nested-div-32k` | 12.492 | **12.128** | −2,9 % |
| `html/nested-div-64k` | 25.084 | **24.772** | −1,2 % |
| `html/nested-div-64k-default-limits` | 14.625 | **14.949** | +2,2 % |
| `html/massive-end-p` | 24.656 | **23.879** | −3,2 % |
| `html/massive-formatting` | 16.382 | **16.292** | −0,5 % |
| `html/tag-40k-attrs` | 12.899 | **13.187** | +2,2 % |
| `html/pretty-5k-blocks` | 26.994 | **26.800** | −0,7 % |
| `css/parse-10k-rules` | 14.470 | **14.941** | +3,3 % |
| `css/resolve-styles-2k-nodes` | 6.253 | **6.097** | −2,5 % |
| `css/resolve-styles-10k-nodes` | 29.853 | **28.662** | −4,0 % |
| `css/selector-match-simple` | 0.718 | **0.665** | −7,4 % |
| `css/selector-match-descendant` | 1.311 | **1.272** | −3,0 % |
| `layout/realistic-1mb` | 61.350 | **59.767** | −2,6 % |
| `paint/build-display-list` | 12.458 | **15.090** | **+21,1 %** (texto de tabla, legítimo) |
| `paint/rasterize-800x600` | 0.843 | **0.993** | **+17,8 %** (AA de glifos) |
| `paint/render-to-png-800x600` | 13.045 | **13.296** | +1,9 % |
| `pipeline/html-to-png-800x600` | 178.791 | **179.699** | +0,5 % |
| **TOTAL (suma)** | **512.047** | **512.440** | **+0,1 %** |

Las subidas de HTML/CSS (±1–7 %) están dentro de la varianza de una máquina
compartida (esta ventana además tuvo load 2,9–3,25, mayor que r7) y no hay
regresiones de complejidad: los escalados siguen siendo lineales (r2 → r8, el
coste por unidad de HTML es incluso menor). Las dos subidas de paint están
explicadas y son producto (texto de tabla y su AA), no deuda.

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
   típica aquí ~1–6 %.
4. Para complejidad, usar `--scale 0.5` vs `1.0`: ratios ≈1.
5. Suite completa ~2,9 s + ~0,27 s de fixtures; smoke
   `--scale 0.1 --iterations 2` ~1 s.
6. Para el pipeline, además del bench: `render --json` (etapas en µs) y el
   golden showcase (`md5 c95c7eff…)`; un cambio de golden exige justificar el
   diff de píxeles en el commit.

JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
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

Coste de referencia de CI en esta máquina (20 hilos, §8.1):
`cargo build --release --workspace` frío **43,3 s**; `cargo test --workspace`
frío **30,1 s** (68 suites, 734 tests). Los números de CI no son comparables
(otra CPU, runners compartidos); el job solo comprueba que compila y ejecuta, y
deja el JSON como artefacto.

---

## 14. Limitaciones y notas

- **Baseline sobre commit limpio de código**: `31e5139`; durante la ventana otro
  agente editó documentación (`README`, `docs/*`, `docs/images/showcase.png`),
  por lo que `worktree_dirty` es `true` en los JSON, pero **no había cambios de
  código sin commitear**. El binario se recompiló una vez (`ed7006e9` →
  `4c7eb74b`, mismo código).
- **Host compartido**: load 2,90–3,25 en las pasadas del bench (frente a
  0,99–1,52 en r7) y 2,9–7,4 en builds/render, por otros agentes compilando en
  paralelo. Variabilidad ±1–6 %; las cifras de pared del pipeline se tomaron con
  load alto y son orientativas. Las cifras absolutas no son comparables con CI.
- **`bench` no cubre todo el motor**: no enlaza `net`, `js` ni `image`, y sus
  generadores no tienen `url(...)`/`background-image`; por eso los fondos, la
  red y Boa se miden aparte (§5.3, §7, §8). Ampliar el catálogo con un
  benchmark de página con imágenes/JS es el siguiente foco natural.
- **Render showcase**: la variante «sin `background-image`» se genera copiando
  el fixture a `/tmp` y borrando las líneas `background-image:` de los CSS; el
  delta se reporta como rango (load alto en esa sub-ventana), no como cifra
  absoluta.
- **Sonda JS**: los micro-benchmarks de arranque/eval se hicieron con un crate
  mínimo bajo `target/` (gitignored) enlazando `crates/js`; las build dirs
  temporales (`target/perf-r8-*`, 4,6 GB) se borraron tras extraer las cifras.
  Los JSON/LOG de medidas se conservan en `target/`.
- **Shell headless**: `--screenshot` no crea ventana/`EventLoop` ni usa el
  worker; mide el proceso completo (carga del binario + render + escritura del
  PNG). El modo ventana (scroll/worker/HiDPI) no se benchmarkea aquí.
- **Artefactos de release compartidos**: `target/release/{bench,megi,render}`
  pueden ser re-enlazados por otros agentes durante la ventana (se observó un
  re-link de `render`); las cifras de §7 se validaron con la salida byte a byte
  contra el golden y el JSON de etapas esperado, y la ventana de medida terminó
  antes de los cambios sin commitear de Fase 6 (~16:59). Los `sha256`
  registrados son una foto al cierre, no una garantía de reproducibilidad
  exacta.
- **Fase 6**: aún no hay página real con `<script>` ejecutándose en el
  pipeline; el coste de Boa en runtime de página no está medido de extremo a
  extremo (solo arranque/eval de la sonda, §8.3). El binario `megi`/`render`
  actuales no enlazan Boa.
- **Fast path de tablas (r7)**: la comparación 67,2 → 61,4 ms usa el valor
  intermedio documentado por el commit de layout; no hay un commit compilable
  con el camino avanzado sin fast path.
