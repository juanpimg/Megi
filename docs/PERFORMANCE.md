# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10 23:48 – 2026-11-11 00:05 (Europe/Madrid).
  Ronda 21: **cuatro optimizaciones de carril** (HTML, CSS, layout, paint) que
  recuperan el rendimiento perdido en r18–r20.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`. **Motor medido `a8ce8ba`**
  (merge de la ronda 21); árbol **limpio** (`dirty = 0` en la pasada canónica).
- **Binario de bench**: `sha256 = 4c709a94…` (4 756 256 B) en las 5 pasadas
  válidas; `./tools/bench.sh` lo recompiló con la misma fuente
  (`77f8ac30…`). Incluye las filas E2E de r20 (`pipeline/gradients`, `grid`,
  `css/vars-calc`, `paint/clipping`, `layout/sticky-text`).
- **Ventana de medición**: **load 1,29–1,48**, `powersave` con frecuencia máxima
  muestreada **3,3–4,7 GHz** durante las pasadas y 0,0 % de otros procesos de
  build. Esta es una **ventana apta** (no el suelo absoluto de la máquina, pero
  comparable con r15/r17 y usada para el veredicto de presupuestos).
- **Artefactos** (gitignored): `target/bench-r21-run1..5.json`,
  `target/bench-aggregate-r21.json` (`sha256 = 295b2b75…`),
  `target/bench-r21/ab_r20_r21.json`, `target/bench-r21-render-shell.json`,
  `target/bench-latest.txt`/`-meta.txt` (pasada canónica,
  `TOTAL 534.395 ms`).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–21; la 14 no publicó baseline).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. La matriz incluye las filas E2E de r20 para las features de r18/r19
(§8), con generadores deterministas, sin red ni aleatoriedad.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5; *heavy* 3).
- **La tabla de §3 es la mediana, por fila, de 5 pasadas completas** en la
  ventana de calma (load 1,29–1,35 en las 5 pasadas; `setup_ms` 386–388 ms,
  mayor que antes por las filas E2E). La pasada canónica de `./tools/bench.sh`
  (`TOTAL 534.395 ms`, 27 filas) se tomó con load ~1,5.
- **A/B intercalado prístino** para atribuir las optimizaciones:
  `git archive c6b66b1` (r20, binario en `target/r20-clean-src2`) vs binario
  r21, 5 pares alternando orden, mismo load (§5.2).
- Render/shell: 9 ejecuciones intercaladas (load ~1,3). `--find`: 7 ejecuciones
  con y sin `--find megi`.
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Governor / freq / térmica | `powersave`; freq máx. muestreada 3,3–4,7 GHz; paquete ~50–55 °C |
| Load | **1,29–1,48** en la ventana (picos ajenos de 7–23 antes) |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

./tools/bench.sh                                  # pasada canónica
for i in 1 2 3 4 5; do target/release/bench --json > target/bench-r21-run$i.json; done

# A/B prístino r20 vs r21
git archive c6b66b1 | tar -x -C target/r20-clean-src2
cd target/r20-clean-src2 && CARGO_TARGET_DIR="$PWD/target" cargo build --release -p bench

# Filas E2E (también con --json)
target/release/bench --filter pipeline          # gradients + grid + html-to-png
target/release/bench --filter css/vars-calc
target/release/bench --filter paint/clipping
target/release/bench --filter layout/sticky

# Render/shell/find
target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png \
    --width 1024 --height 768 --json
target/release/megi tests/fixtures/render/find.html --screenshot /tmp/f.png \
    --width 800 --height 600 --find megi
```

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb`, `nested-div-*`, `massive-*`, `tag-40k-attrs`, `pretty-5k-blocks` | Parse/tree construction. |
| css | `css/parse-10k-rules` | Parse de 10 000 reglas. |
| css | `css/resolve-styles-2k/10k-nodes` | Cascada + `ComputedStyle`. |
| css | `css/selector-match-simple/descendant` | Matching de selectores. |
| css | `css/vars-calc-10k` | 10 005 elementos, `:root` + 69 `var()` + 100 `calc()`. |
| layout | `layout/realistic-1mb` | Layout del documento realista, 800×600. |
| layout | `layout/sticky-text-1mb` | 24 156 elementos, 1.00 MiB, sticky headers/cards + `text-transform`. |
| paint | `paint/build-display-list`, `rasterize-800x600`, `render-to-png-800x600` | Display list, raster, PNG. |
| paint | `paint/clipping-800x600` | 48 cajas radius+overflow + `background-clip:text`; display list+raster+PNG. |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo del documento realista. |
| pipeline | `pipeline/gradients-800x600` | 60 cajas linear/radial; pipeline completo. |
| pipeline | `pipeline/grid-800x600` | 3 grids (3×3 `1fr`, auto-fill `minmax`, spans), 29 celdas; pipeline completo. |
| fonts | (`feature fonts`) | No re-medido (sin cambios). |

---

## 3. Baseline vigente (ronda 21, HEAD `a8ce8ba`)

Mediana de **5 pasadas en calma** (load 1,29–1,35). `setup_ms`: 386–388 ms.
**Filas legacy (19)**: `TOTAL 344.199 ms`; **todas (24)**: `TOTAL 527.712 ms`;
pasada canónica: `TOTAL 534.395 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) | throughput |
|---|---:|---:|---:|
| `html/realistic-1mb` | **16.720** | 15.905–17.017 | 59,9 MiB/s |
| `html/nested-div-10k` | **2.761** | 2.658–2.789 | 38,0 MiB/s |
| `html/nested-div-32k` | **8.482** | 8.426–8.604 | 39,6 MiB/s |
| `html/nested-div-64k` | **16.887** | 16.662–17.118 | 39,5 MiB/s |
| `html/nested-div-64k-default-limits` | **10.147** | 9.962–10.182 | — |
| `html/massive-end-p` | **17.306** | 17.015–17.343 | 5,78 M tags/s |
| `html/massive-formatting` | **10.328** | 10.284–10.581 | 968 k bloques/s |
| `html/tag-40k-attrs` | **9.350** | 9.000–9.541 | 4,28 M attrs/s |
| `html/pretty-5k-blocks` | **8.704** | 8.548–8.743 | 575 k bloques/s |
| `css/parse-10k-rules` | **14.133** | 13.404–15.128 | 42,2 MiB/s |
| `css/resolve-styles-2k-nodes` | **7.152** | 6.969–7.270 | 280 k el/s |
| `css/resolve-styles-10k-nodes` | **33.937** | 33.314–34.805 | 295 k el/s |
| `css/selector-match-simple` | **0.655** | 0.640–0.664 | 61,1 M matches/s |
| `css/selector-match-descendant` | **1.211** | 1.195–1.269 | 33,0 M matches/s |
| `layout/realistic-1mb` | **35.492** | 34.830–36.883 | 794 k el/s |
| `paint/build-display-list` | **8.183** | 7.921–8.411 | — |
| `paint/rasterize-800x600` | **0.933** | 0.901–0.935 | 514 MPix/s |
| `paint/render-to-png-800x600` | **8.461** | 8.390–8.577 | 6,48 MiB/s |
| `pipeline/html-to-png-800x600` | **133.357** | 132.980–134.875 | — |
| `layout/sticky-text-1mb` | **30.811** | 30.282–32.097 | 784 k el/s |
| `pipeline/gradients-800x600` | **50.749** | 50.049–51.400 | — |
| `pipeline/grid-800x600` | **5.828** | 5.763–6.000 | — |
| `css/vars-calc-10k` | **46.332** | 46.096–46.685 | 216 k el/s |
| `paint/clipping-800x600` | **49.793** | 49.376–49.840 | — |

Pasada canónica (load ~1,5): html 17.041 · massive-end-p 17.774 · tag-40k
9.594 · css resolve-10k 34.506 · layout 36.436 · display-list 7.523 · raster
0.988 · render-to-png 8.782 · pipeline **135.285** · gradients 51.927 · grid
5.808 · vars-calc 46.466 · clipping 50.190 · sticky 31.991 · TOTAL 534.395.

> **Aviso**: el TOTAL (legacy) suma etapas y pipeline end-to-end (solapados); no
> es un presupuesto de página. En r21 **todos los presupuestos se cumplen**
> (ver §11).

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 2.76 · 32k: 8.48 · 64k: 16.89 ms | **Lineal** (3,2× → 3,07×; 2× → 1,99×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 14.13 · 20k: 30.50 ms | **Lineal** (sin cambio; no era el foco). |
| `css/resolve-styles-2k/10k` | 7.15 / **33.94 ms** | **−13,7 % vs r20**: borrowed-key cascade + interning cache. |
| `layout/realistic-1mb` | 28 195 elementos → **35,49 ms** | **−24,6 %**: intern de `ComputedStyle` + estilos anónimos perezosos. |
| `layout/sticky-text-1mb` | 24 156 elementos → **30,81 ms** | **−26,7 %** por las mismas optimizaciones. |
| `paint/build-display-list` | 34 158 items → **8,18 ms** | **−50,0 %**: fast path de decoraciones + display list reservada. |
| `pipeline` | **133,36 ms** (132,98–134,88) | **−16,7 %, dentro de ≤140**. |

---

## 5. Ronda 21: optimizaciones y verificación

### 5.1 Qué entró

- `26560cc perf(html)`: **fast paths de tag/atributo**, runs de texto
  orientados a bytes y dedupe híbrido → HTML parse −16,6 % (A/B).
- `0ea54b5 perf(css)`: **mapa de cascada con claves prestadas** + **caché de
  interning de `ComputedStyle`** → resolve-10k −11,5 % (A/B).
- `d0249ce perf(layout)`: **interning de instancias `ComputedStyle`** y
  **estilos anónimos perezosos** → layout −21,6 % (A/B).
- `fd2c471 perf(paint)`: **fast path de decoraciones por caja** y **display
  list reservada** → build-display-list −45,8 % (A/B).
- `a8ce8ba`: merge; `aa0ae74` snapshot de estado (sin cambios de motor).

### 5.2 A/B por carril (r20 prístino `c6b66b1` vs r21 `a8ce8ba`)

5 pares intercalados, load ~1,48:

| Fila | r20 (ms) | r21 (ms) | Δ | Claim de la ronda |
|---|---:|---:|---:|---|
| `html/realistic-1mb` | 20,40 | **17,02** | **−16,6 %** | 20,0→16,4 (−18 %) ✅ aprox. |
| `html/tag-40k-attrs` | 11,78 | **9,33** | **−20,8 %** | — |
| `html/massive-end-p` | 18,59 | **17,60** | −5,3 % | — |
| `css/parse-10k-rules` | 13,54 | **14,47** | +6,9 % | — |
| `css/resolve-styles-2k-nodes` | 8,05 | **7,20** | −10,6 % | — |
| `css/resolve-styles-10k-nodes` | 38,89 | **34,42** | **−11,5 %** | 38,84→33,91 (−12,7 %) ✅ aprox. |
| `layout/realistic-1mb` | 45,80 | **35,90** | **−21,6 %** | 46,6→36,4 (−21,9 %) ✅ |
| `paint/build-display-list` | 15,25 | **8,27** | **−45,8 %** | 8,62→7,84 (nota: el r20 del carril era otro corte) |
| `paint/rasterize-800x600` | 0,92 | **0,92** | +0,2 % | — |
| `paint/render-to-png-800x600` | 8,50 | **8,74** | +2,8 % | — |
| `pipeline/html-to-png-800x600` | 154,85 | **135,20** | **−12,7 %** | 154,13→132,39 (−14,1 %) ✅ aprox. |
| suma de filas core | 336,60 | **288,89** | **−14,2 %** | — |

**Veredicto de presupuestos (definitivo)**:

| Medida | Valor r21 | Presupuesto | % | Veredicto |
|---|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **133,36 ms** | **≤ 140 ms** | **95,3 %** | ✅ **CUMPLE** |
| `layout/realistic-1mb` | **35,49 ms** | **≤ 45 ms** | **78,9 %** | ✅ **CUMPLE** |
| `css/resolve-styles-10k-nodes` | **33,94 ms** | **≤ 35 ms** (objetivo) | **97,0 %** | ✅ **CUMPLE** |
| `render` showcase 1024×768 | 27,11 ms | ≤ 35 ms | 77,5 % | ✅ |
| shell headless showcase | 28,13 ms | ≤ 35 ms | 80,4 % | ✅ |
| `paint/render-to-png-800x600` | 8,46 ms | ≤ 50 ms | 16,9 % | ✅ |
| `html/realistic-1mb` | 16,72 ms | ≤ 100 ms | 16,7 % | ✅ |
| `paint/build-display-list` | 8,18 ms | ≤ 25 ms | 32,7 % | ✅ |

Las tres preguntas de la ronda: **pipeline ≤140 → SÍ** (133,36; canónica 135,29,
margen 4,7 %); **layout ≤45 → SÍ** (35,49; margen 21 %); **css resolve ≤35 →
SÍ** (33,94; justo, margen 3 %). Queda poco margen en pipeline (~6,6 ms) y en
css resolve (~1,1 ms): vigilar cualquier regresión futura.

### 5.3 Residuales y siguiente foco

- `css/parse-10k-rules` **+6,9 %** y `css/selector-match-simple` +5,3 % en A/B
  (dentro de ruido, pero el parse es el único carril sin optimizar).
- `paint/render-to-png` **+2,8 %** (raster+PNG; el display list ya bajó −46 %).
- La página real está dominada por **image decode** (~91 ms en la web del
  dueño, ronda 16) y **PNG** (~15 ms 1280×900), fuera del bench legacy.
- **Siguiente foco**: perfilar el residual de `pipeline` (133,4 vs ~131 del
  claim) en parse de CSS y raster/PNG; **imagen decode/PNG por regiones** para
  la página real; fracaso de páginas con muchas imágenes; mantener los
  presupuestos (no subirlos) y añadir E2E como gate de features.

---

## 6. Codificador PNG (sin cambios)

| Métrica | r20 | r21 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.831 | **8.461** | −4,2 % |
| — rasterizar | 0.995 | **0.933** | −6,2 % |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden `--zoom 2` | 3 202 B | **3 202 B** | `4792779d…` |

---

## 7. Pipeline `render` y shell headless (r21)

9 ejecuciones intercaladas (load ~1,3; goldens verificados):

| Fixture / modo | pared r21 | pared r20 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **27,11 ms** | 28,04 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **8,33 ms** | 9,08 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **28,13 ms** | 28,09 | 80 220 B (idéntico) |
| basic (shell headless) | **8,76 ms** | 9,51 | 21 090 B (idéntico) |
| basic `--zoom 2` (320×200) | **3,87 ms** | 3,29 | 3 202 B |
| `find.html` sin `--find` | **9,83 ms** | 10,12 | 30 056 B |
| `find.html` con `--find megi` | **16,98 ms** | 16,17 | 29 950 B (`a818f32a…`) |

El shell con `--find` cuesta **~+7,2 ms** (índice de texto + overlay; 3 hits,
sin re-render al cambiar de hit). Auto-refresh y pestañas no se ejercen en
`--screenshot`.

---

## 8. Filas E2E del bench (activas)

Medianas en calma (misma ventana); las features de r18/r19 ya forman parte de la
suite y se filtran con `--filter`:

| Fila | r21 (ms) | r20 (ms) | Δ | detalle |
|---|---:|---:|---:|---|
| `pipeline/gradients-800x600` | **50.749** | 53.199 | −4,6 % | 60 cajas, 182 items, PNG 248 109 B |
| `pipeline/grid-800x600` | **5.828** | 6.044 | −3,6 % | 3 grids, 29 celdas, PNG 25 280 B |
| `css/vars-calc-10k` | **46.332** | 104.488 | **−55,7 %** | 10 005 elementos, 69 `var()`, 100 `calc()` |
| `paint/clipping-800x600` | **49.793** | 57.670 | −13,7 % | 48 cajas radius+overflow, 1 670 items, PNG 39 412 B |
| `layout/sticky-text-1mb` | **30.811** | 42.010 | −26,7 % | 24 156 elementos, 1.00 MiB, sticky + text-transform |

Cómo medirlas: `--filter pipeline` (gradients+grid), `--filter css/vars-calc`,
`--filter paint/clipping`, `--filter layout/sticky`. Rustdoc en
`crates/bench/src/benches.rs`; un **>15 % reproducible** sobre la referencia de
la misma máquina/load es regresión. Añaden ~388 ms de `setup` y filas extra al
TOTAL (el legacy se publica aparte en §3).

---

## 9. Clipping, `var()`/`calc()`, net, fuzz (r18–r19, resumen)

- **Clipping/`clip:text`**: coste de raster por área; `border-radius` +
  `overflow:hidden` (200×150) ≈ +11,1 ms de raster, con `clip:text` ≈+16,7 ms;
  `background-clip:text` aislado barato. Caps: `MAX_ROUNDED_GROUP_PIXELS =
  16 777 216`, `MAX_TEXT_MASK_RUNS = 4096`, `MAX_SHADOWS = 32`,
  `MAX_SHADOW_BLUR = 64`, `MAX_BACKGROUND_TILES = 4096`,
  `MAX_STACKING_CONTEXT_DEPTH = 256`, `MAX_FLOAT_DEPTH = 64`.
- **`var()`/`calc()`**: el coste estructural de la cascada era el de §5 de r20;
  en r21 el A/B css resolve −11,5 % y la fila E2E vars-calc −55,7 % confirman
  que el interning/borrowed-key resuelve el déficit que dejó el fix parcial de
  r20.
- **Net**: PSL (355 LOC, ≈150 sufijos), Referrer-Policy (8 políticas), Range/206
  y SameSite son módulos puros sin I/O; r19 los integra en `render`. Coste en
  bench ≈0; por petición despreciable.
- **Fuzz**: no entra en el bench; **7 targets**, caché local 1,6 GB; referencia
  **~15 s incremental / 3–5 min frío**; CI job **build-only en PRs** (nightly +
  `rust-cache`, timeout 20 min) → minutos de CI, 0 en el bench.

---

## 10. Coste de floats, posicionamiento, stacking y opacidad (referencia r4)

| Variante | Δ vs control | Factor |
|---|---:|---:|
| `position: relative` | +0,93–1,13 ms | 1,02× |
| `position: relative; z-index` | +0,20–0,59 ms | 1,00–1,01× |
| `opacity: 0.85` | +2,96–3,55 ms | 1,05–1,06× |
| `float: left` | +12,54–12,72 ms | **1,22×** |

El documento realista del bench no usa floats ni opacidad.

---

## 11. Presupuestos: veredicto definitivo

| Medida | Valor r21 | Presupuesto | % | Veredicto |
|---|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **133,36 ms** | **≤ 140 ms** | **95,3 %** | ✅ **CUMPLE** (canónica 135,29) |
| `layout/realistic-1mb` | **35,49 ms** | **≤ 45 ms** | **78,9 %** | ✅ **CUMPLE** |
| `css/resolve-styles-10k-nodes` | **33,94 ms** | **≤ 35 ms** | **97,0 %** | ✅ **CUMPLE** (justo) |
| `render` showcase 1024×768 | 27,11 ms | ≤ 35 ms | 77,5 % | ✅ |
| shell headless showcase | 28,13 ms | ≤ 35 ms | 80,4 % | ✅ |
| `paint/render-to-png-800x600` | 8,46 ms | ≤ 50 ms | 16,9 % | ✅ |
| `html/realistic-1mb` | 16,72 ms | ≤ 100 ms | 16,7 % | ✅ |
| `paint/build-display-list` | 8,18 ms | ≤ 25 ms | 32,7 % | ✅ |
| Filas E2E | §8 | — | — | sin presupuesto; >15 % = regresión |

**No se suben los presupuestos**: se cumplen todos. Margen más justo:
**pipeline 6,6 ms** y **css resolve 1,1 ms**; cualquier regresión futura debe
tratarse con A/B intercalado antes de tocar el presupuesto. El
`paint/rasterize` (0,93 ms) y el PNG (8,46) son los siguientes candidatos si se
quiere más margen en el pipeline; el parse de CSS (+6,9 % en A/B, dentro de
ruido) es el único carril sin optimizar.

---

## 12. Comparación histórica

Diecinueve baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | **23:48** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | **`a8ce8ba`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | **1,29–1,48** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | `43f7748d…` | `a188a1da…` | `c34ff65c…` | `d655c1f2…` | `153b8aff…` | `1f1dbbdd…` | `39909f01…` | `8cfbb715…` | `4185c2de…` | `c315ddf3…` | `ac2740ae…` | **`4c709a94…`** |

Filas clave (mediana, ms; r20/r21 incluyen las filas E2E que no entran en el
TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | **16.720** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | **33.937** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | **35.492** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | **8.183** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | **8.461** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | **133.357** |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** |

### Ronda 20 → ronda 21, fila a fila

| Benchmark | R20 (ms) | R21 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 21.078 | **16.720** | **−20,7 %** |
| `html/nested-div-10k` | 3.268 | **2.761** | −15,5 % |
| `html/nested-div-32k` | 10.360 | **8.482** | −18,1 % |
| `html/nested-div-64k` | 20.771 | **16.887** | −18,7 % |
| `html/nested-div-64k-default-limits` | 13.719 | **10.147** | −26,0 % |
| `html/massive-end-p` | 18.879 | **17.306** | −8,3 % |
| `html/massive-formatting` | 12.113 | **10.328** | −14,7 % |
| `html/tag-40k-attrs` | 12.312 | **9.350** | −24,1 % |
| `html/pretty-5k-blocks` | 11.048 | **8.704** | −21,2 % |
| `css/parse-10k-rules` | 14.069 | **14.133** | +0,5 % |
| `css/resolve-styles-2k-nodes` | 8.252 | **7.152** | −13,3 % |
| `css/resolve-styles-10k-nodes` | 39.347 | **33.937** | **−13,7 %** |
| `css/selector-match-simple` | 0.622 | **0.655** | +5,3 % |
| `css/selector-match-descendant` | 1.223 | **1.211** | −1,0 % |
| `layout/realistic-1mb` | 47.097 | **35.492** | **−24,6 %** |
| `paint/build-display-list` | 16.355 | **8.183** | **−50,0 %** |
| `paint/rasterize-800x600` | 0.995 | **0.933** | −6,2 % |
| `paint/render-to-png-800x600` | 8.831 | **8.461** | −4,2 % |
| `pipeline/html-to-png-800x600` | 160.135 | **133.357** | **−16,7 %** |
| `layout/sticky-text-1mb` | 42.010 | **30.811** | **−26,7 %** |
| `pipeline/gradients-800x600` | 53.199 | **50.749** | −4,6 % |
| `pipeline/grid-800x600` | 6.044 | **5.828** | −3,6 % |
| `css/vars-calc-10k` | 104.488 | **46.332** | **−55,7 %** |
| `paint/clipping-800x600` | 57.670 | **49.793** | −13,7 % |
| **TOTAL legacy** | **420.474** | **344.199** | **−18,1 %** |

---

## 13. Cómo comparar y detectar regresiones

1. Medir con load < 2 y comprobar frecuencia/térmica (en esta ronda: load
   1,29–1,48, freq máx. 3,3–4,7 GHz). Si no hay ventana, marcarlo y no usar la
   pasada para presupuesto.
2. Si el árbol no compila por WIP ajeno, medir desde `git archive <commit>` en
   `target/`.
3. Para atribuir un delta a un commit, compilar dos archivos prístinos
   (antes/después) y ejecutar los binarios **intercalados** (mismo load para
   ambos), como en §5.2.
4. Guardar JSON y comparar medianas:

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
5. Regresión si la mediana empeora >15 % de forma reproducible. En r21 **todos
   los presupuestos se cumplen**, con poco margen en pipeline (6,6 ms) y css
   resolve (1,1 ms): vigilar sobre todo esos dos.
6. Filas E2E: `--filter pipeline` (gradients+grid), `--filter css/vars-calc`,
   `--filter paint/clipping`, `--filter layout/sticky`. Referencias en §8;
   >15 % reproducible = regresión.
7. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   bench PNG 57 422 B, zoom `4792779d…` 3 202 B, find `a818f32a…` 29 950 B.

---

## 14. Limitaciones y notas

- **Ventana buena pero no perfecta**: load 1,29–1,48 y `powersave` con
  frecuencia oscilante (freq máx. muestreada 3,3–4,7 GHz). Es comparable con
  r15/r17 y suficiente para el veredicto, pero no es el suelo absoluto; si se
  quiere afinar, re-medir en una ventana aún más estable.
- **A/B por carril**: las cifras de ronda (html 20→16,4; css 38,84→33,91;
  layout 46,6→36,4; pipeline 154,13→132,39) se confirman en dirección y orden
  de magnitud; el `paint/build-display-list` del claim (8,62→7,84) se midió en
  otro corte del carril, aquí el A/B r20→r21 da 15,25→8,27 (−45,8 %).
- **Filas E2E**: suben `setup` (~388 ms) y el TOTAL del informe; el TOTAL
  legacy se publica aparte. `layout/sticky-text-1mb` quedó activa en r20.
- **Render/shell**: medidos a load ~1,3; `--find` cuesta ~+7,2 ms; pestañas y
  auto-refresh no se ejercen en `--screenshot`.
- **`website/` y `pages.yml`**: del dueño; no se tocan.
- **Fast path de tablas (r7)**: nota histórica; no hay commit compilable con el
  camino avanzado sin fast path.
