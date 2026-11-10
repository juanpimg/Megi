# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 19:30–19:50 (Europe/Madrid). Ronda 13:
  **crate `svg`** nuevo (parser y rasterizador vectorial con AA), **webfonts**
  (`@font-face` + decodificación WOFF2 a lo largo del pipeline) y **shell** con
  zoom y auto-refresh por timers.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `5234630`**
  (`feat(svg): new from-scratch SVG parser and antialiased vector rasterizer`).
- **Estado del árbol**: la ventana coincidió con trabajo **sin commitear de otro
  agente en `crates/html`** (`fast_hash.rs`, `tree_builder.rs`, a las 19:42) y
  con docs/`.github/workflows/pages.yml`/`website` untracked. Para que la
  baseline no mezcle ese WIP, el bench se compiló y ejecutó desde un **archivo
  prístino de `5234630`** (`git archive` a `target/r13-clean-src`, solo lectura)
  → binario `sha256 = 153b8aff…`. `render`/`shell` se compilaron a las
  19:34–19:35, antes de ese WIP, así que sus medidas son del HEAD.
- **Artefactos guardados** (gitignored):
  - `target/bench-r13-clean-run*.json` (15 intentos, 10 válidos),
    `target/bench-aggregate-r13.json` (`sha256 = 21dfc5af…`).
  - `target/bench-r13/webfonts_summary.json`, `svg_summary.json`,
    `shell_summary.json`; `target/bench-r13-script.log`.
  - `target/r13-clean-src/` (archivo prístino + target temporal, borrable).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–12).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. **El crate `bench` y sus generadores no cambiaron en esta ronda.**

El bench **no depende de `svg` ni de las webfonts**: `bench` solo usa
`dom/html/css/layout/paint` (y `fonts` opcional desactivado). Por eso las
funcionalidades nuevas de r13 se miden aparte (§8 y §9) y **no cambian el
pipeline por defecto** del bench.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- **La tabla de §3 es la mediana, por fila, de 10 pasadas completas válidas**
  de HEAD prístino (de 15 intentos). El host volvió a ser muy variable (load
  3,4–5,3 en las válidas; picos de 14,9 con `rustc` de otros agentes y golpes
  de 20–60 % en filas sueltas). Criterio de aceptación: `layout` 50–56 ms,
  `pipeline` ≤142 ms, `render-to-png` ≤9,9 ms y `html/realistic` ≤26 ms.
- `setup_ms` de las pasadas válidas: 205,7–228,5 ms.
- Render/shell: 11 ejecuciones intercaladas (pared) y 7 para `--json`.
  Webfonts: 7 ejecuciones por fixture + 3 `--json` para las etapas.
  SVG: 7 ejecuciones por caso con el binario del ejemplo release.
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` por defecto |
| Carga de fondo | válidas: 3,4–5,3; picos de 14,9 (pasadas descartadas) |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Bench canónico (repo). Si el árbol tiene WIP de otros agentes, medir desde un
# archivo prístino del HEAD (como en esta ronda):
git archive 5234630 | tar -x -C target/r13-clean-src
cd target/r13-clean-src && CARGO_TARGET_DIR="$PWD/target" \
    cargo build --release -p bench
./target/release/bench --json > target/bench-r13-clean-runN.json

# Webfonts (@font-face WOFF2, render only)
target/release/render tests/fixtures/render/webfont.html /tmp/w.png --json --no-network
target/release/render tests/fixtures/render/webfont-fallback.html /tmp/w.png --json --no-network
target/release/render tests/fixtures/render/webfont-corrupt.html /tmp/w.png --json --no-network
target/release/render tests/fixtures/render/webfont-remote.html /tmp/w.png --json --no-network

# SVG standalone (no cableado en render todavía)
cargo run --release -p svg --example svg_to_png -- \
    crates/svg/tests/fixtures/simple.svg /tmp/s.png
cargo run --release -p svg --example svg_to_png -- \
    target/bench-r13/complex.svg /tmp/c.png          # 512x512
cargo run --release -p svg --example svg_to_png -- \
    target/bench-r13/complex.svg /tmp/c256.png --width 256 --height 256

# Shell headless + zoom
target/release/megi tests/fixtures/render/showcase/index.html \
    --screenshot /tmp/s.png --width 1024 --height 768
target/release/megi tests/fixtures/render/basic.html \
    --screenshot /tmp/z.png --width 320 --height 200 --zoom 2
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
| layout | `layout/realistic-1mb` | `layout_document` (28 195 elementos, 88 155 cajas), 800×600. |
| paint | `paint/build-display-list` | Display list: 34 158 items (sin hit regions: opt-in). |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` DEFLATE (57 422 B). |
| pipeline | `pipeline/html-to-png-800x600` | Parse → styles → layout → display list → raster → PNG. |
| fonts | (`feature fonts`) | No re-medido en r13 (los webfonts se miden en §8 con `render`). |

**Nuevo en r13, fuera del bench**: webfonts (§8), SVG standalone (§9) y shell
con zoom/auto-refresh (§7).

---

## 3. Baseline vigente (2026-11-10, HEAD `5234630`)

Mediana por fila de **10 pasadas válidas** (5 iteraciones, 1 warm-up; *heavy*
3) de HEAD prístino. `setup`: 205,7–228,5 ms. Binario
`sha256 = 153b8aff…` (4 293 864 B). La suma de medianas da
`TOTAL 395.269 ms`.

| Benchmark | Carga | mediana (ms) | rango 10 pasadas (ms) | throughput |
|---|---|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | **22.618** | 20.950–33.004 | 44,3 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | **3.554** | 3.424–5.214 | 29,5 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | **11.334** | 10.593–16.676 | 29,6 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | **23.148** | 21.995–33.867 | 29,0 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | **14.014** | 13.401–20.453 | — |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | **22.312** | 21.499–25.765 | 4,48 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | **12.710** | 12.241–18.530 | 787 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | **12.047** | 11.460–18.209 | 3,32 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | **11.645** | 10.715–12.422 | 429 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | **14.909** | 13.706–16.373 | 40,0 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | **6.399** | 5.936–7.219 | 313 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | **30.311** | 29.960–34.450 | 330 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | **0.682** | 0.640–0.723 | 58,7 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | **1.314** | 1.169–1.746 | 30,5 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | **51.601** | 50.126–55.795 | 546 k elementos/s |
| `paint/build-display-list` | 34 158 items | **12.842** | 12.049–15.105 | — |
| `paint/rasterize-800x600` | 34 158 items → pixmap 800×600 | **0.999** | 0.921–1.321 | 482 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (57 422 B) | **9.200** | 8.697–9.788 | 5,95 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | **133.629** | 129.275–140.963 | — |
| **TOTAL (suma)** | | **395.269** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página. Las filas HTML/CSS están dentro de ±5–10 % de r12 (sin
> cambios de código en esas crates); los picos del rango son golpes de load.

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.55 · 32k: 11.33 · 64k: 23.15 ms | **Lineal** (3,2× → 3,26×; 2× → 2,04×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 14.91 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 2 005 el: 6.40 · 10 005 el: 30.31 ms | **Lineal** (5× → 4,74×; ~330 k el/s). |
| `layout/realistic-1mb` | 28 195 elementos → **51,6 ms** | Estable vs r12 (51,3); el `Arc<ComputedStyle>` sigue siendo el win. |
| `paint/render-to-png` | raster 1,00 + encoder ≈8,2 ms | Encoder PNG de r12 sin cambios (byte-idéntico). |
| `pipeline` | 133,6 ms (129,3–141,0) | Reparto: layout 51,6 + CSS 30,3 + parse 22,6 + display list 12,8 + PNG 9,2. |

---

## 5. Ronda 13: SVG, webfonts y shell — ¿afecta al bench?

### 5.1 Qué entró en la ronda

- `5234630 feat(svg)`: crate **`svg`** nuevo (parser XML/SVG desde cero,
  path-data con arcos, transformaciones, rasterizador AA 4×4, límites
  anti-DoS). No está cableado en `render`/paint todavía: se mide standalone
  (§9). `bench` no lo enlaza.
- `2b97187 feat(webfonts)`: `@font-face` + decodificación **WOFF2** integrada en
  el pipeline `render` (colección de reglas, descarga local/file, registro en
  `fonts::FontDatabase`, `timings.fonts`, `fonts_loaded/failed/bytes`).
- `86be30e feat(shell)`: **zoom** (`--zoom`, Ctrl+±) y **auto-refresh** por
  timers (`--refresh-ms`, opt-in) con títulos/estado ampliados.

### 5.2 Verificación: sin regresiones en el bench

| Métrica | r12 | r13 | Δ | Veredicto |
|---|---:|---:|---:|---|
| `layout/realistic-1mb` | 51.277 | **51.601** | +0,6 % | ✅ sin regresión |
| `pipeline/html-to-png` | 133.373 | **133.629** | +0,2 % | ✅ sin regresión |
| `paint/render-to-png` | 9.136 | **9.200** | +0,7 % | ✅ sin regresión |
| `html/realistic-1mb` | 21.641 | **22.618** | +4,5 % | ~varianza (rango 20,95–33,0) |
| `css/resolve-styles-10k` | 28.807 | **30.311** | +5,2 % | ~varianza / +116 LOC parser @font-face |
| `paint/build-display-list` | 13.648 | **12.842** | −5,9 % | ~varianza |
| **TOTAL (suma)** | 391.748 | **395.269** | +0,9 % | dentro de varianza |

El bench **no cambia por webfonts ni por SVG**: `bench` no enlaza `svg` y no
procesa hojas con `@font-face`; además las fuentes web solo se cargan en el
pipeline `render`. Las diferencias de HTML/CSS (+3–6 %) no tienen cambio de
código asociado en esas crates (`html` sigue igual; en `css` solo se añadió el
parseo de `@font-face`, que no aparece en el fixture del bench) y caen dentro de
la varianza del host (load 3,4–5,3, picos de 14,9). **No hay regresión de
layout/pipeline/PNG**, que era el objetivo de la verificación.

### 5.3 Nota de integridad de la medición

Durante la ventana, otro agente modificó **sin commitear** `crates/html`
(`fast_hash.rs`, `tree_builder.rs`) buscando rendimiento. Para no publicar una
baseline contaminada, el bench de esta ronda se ejecutó desde el **archivo
prístino de `5234630`** en `target/r13-clean-src` (git archive, sin tocar el
repo). `render`/`shell` ya estaban compilados antes de esas ediciones, así que
sus números también son del HEAD.

---

## 6. Codificador PNG (sin cambios en r13)

| Métrica | r12 | r13 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 9.136 ms | **9.200 ms** | +0,7 % (varianza) |
| — rasterizar | 1.052 ms | **0.999 ms** | varianza |
| — codificar PNG (por diferencia) | ~8,08 ms | **~8,20 ms** | sin cambios de código |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden phase4 basic | 42 853 B | **42 853 B** | `9e957177…` (verificado en §7) |

---

## 7. Pipeline `render` y shell headless (ronda 13)

11 ejecuciones intercaladas; `render` y `shell` compilados del HEAD limpio
(19:34–19:35), antes del WIP de `html`.

| Fixture / modo | pared r13 | pared r12 | rango r13 | PNG |
|---|---:|---:|---:|---:|
| showcase 1024×768 (render) | **30,78 ms** | 29,49 ms | 28,98–32,94 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,70 ms** | 9,64 ms | 8,34–11,56 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **30,78 ms** | 29,98 ms | 28,98–32,94 | 80 220 B (idéntico) |
| basic (shell headless) | **9,70 ms** | 9,88 ms | 8,34–11,56 | 21 090 B (idéntico) |
| basic `--width 320 --height 200 --zoom 2` (shell) | **3,85 ms** | — | 3,12–5,04 | 3 202 B |
| basic a viewport CSS 160×100 (render, referencia) | **3,16 ms** | — | 2,65–3,70 | 3 202 B (`4792779d…`) |

- **Zoom**: `--zoom 2` con 320×200 físicos renderiza a **viewport CSS 160×100**
  (`physical / zoom`); el PNG es **byte-idéntico** al de `render --width 160
  --height 100`, así que el zoom headless no añade coste apreciable (la
  diferencia de pared 3,85 vs 3,16 ms es el arranque del shell).
- **Auto-refresh**: `--refresh-ms [MS]` es **opt-in** y solo actúa en modo
  ventana (el bucle llama a `should_refresh` con `scripts_executed` y
  `script_timers_pending`). En `--screenshot` se renderiza una vez, por lo que
  **no se mide aquí** (no hay fixtures con timers en el arnés de shell);
  documentado como coste potencial = 1 render completo por tick.
- Golden showcase intacto (`c95c7eff…`, 80 220 B) y phase4 basic idéntico
  (`9e957177…`, 42 853 B): el cableado de r13 no altera los píxeles.

---

## 8. Coste de las webfonts (`@font-face` + WOFF2)

Fixtures del repo (fuente local `OpenSans-Regular-latin.woff2`, 18 640 B;
variantes sin `@font-face`, corrupta y remota). 7 ejecuciones por caso + 3
`--json` para etapas, con `--no-network` en los casos locales/remotos para
quitar el ruido de red:

| Fixture | pared (ms) | `timings.fonts` | `fonts_loaded` | `fonts_failed` | `font_bytes` | PNG |
|---|---:|---:|---:|---:|---:|---:|
| `webfont.html` | **10,38** | 1 330 µs | **1** | 0 | **18 640** | 23 845 B |
| `webfont.html` + `--no-network` | 10,81 | 1 388 µs | 1 | 0 | 18 640 | 23 845 B |
| `webfont-fallback.html` (sin `@font-face`) | **10,76** | 700 µs | 0 | 0 | 0 | 24 670 B |
| `webfont-corrupt.html` (WOFF2 corrupta) | **9,38** | 1 034 µs | 0 | **1** | 0 | 18 745 B |
| `webfont-remote.html` + `--no-network` | **9,19** | 611 µs | 0 | **1** | 0 | 18 362 B |

| Δ vs fallback | pared | fonts | PNG |
|---|---:|---:|---:|
| `webfont.html` | **−0,38 ms** (dentro del ruido) | **+630 µs** | −825 B (métricas distintas) |
| `webfont-corrupt` | −1,38 ms | +334 µs | −5 925 B (fallback) |
| `webfont-remote` (no-network) | −1,57 ms | −89 µs | −6 308 B (fallback) |

**Lectura**:

- La base de datos de fuentes ya cuesta ~**0,7 ms** por render (inicialización);
  cargar y registrar la fuente web añade **~0,6 ms** (`timings.fonts` de
  0,70 → 1,33 ms). El impacto en pared es **despreciable** frente a los
  ~10 ms del fixture (el cambio de métricas incluso reduce el PNG 825 B).
- Una fuente corrupta o remota falla **con warning y sin crash**, y el texto cae
  al fallback (PNG con métricas del fallback).
- **El bench no paga nada por esto**: `@font-face` solo se procesa en `render`,
  y el bench no enlaza el pipeline de webfonts/red. `fonts` sigue siendo una
  feature opcional del bench (no medida aquí).

---

## 9. Coste SVG standalone (crate `svg`, aún no cableado en render)

`cargo run --release -p svg --example svg_to_png -- <svg> <png> [--width N]`
(la pared incluye arranque del proceso, parse de SVG, raster AA y escritura del
PNG; el ejemplo usa un **PNG writer de bloques stored**, no el encoder DEFLATE
del motor, por lo que el PNG de 512² pesa ~1 MB). 7 ejecuciones, mediana:

| Caso | pared (ms) | rango | PNG | Notas |
|---|---:|---:|---:|---|
| `crates/svg/tests/fixtures/simple.svg` (16×16: rect+circle+path) | **1,44** | 1,18–2,00 | 1 108 B | caso mínimo |
| SVG complejo propio `target/bench-r13/complex.svg` (512×512, 160 círculos + 80 polígonos + 40 paths con stroke/opacidad/transform + 1 fill `url(#grad)` no soportado) | **223,92** | 166,39–280,58 | 1 049 236 B | `render_svg` AA 4×4 + PNG stored de 1 MB |
| mismo complejo con salida 256×256 | **80,99** | 79,30–91,97 | 262 488 B | ~1/4 de píxeles → ~1/2,8 de tiempo |

**Lectura**: el parse+raster del SVG es barato (simple 1,4 ms de pared total);
en el caso complejo, **la mayor parte del tiempo es la escritura del PNG stored
de 1 MB** del ejemplo, no el rasterizado. El rasterizador hace AA 4×4 y los
gradientes (`fill="url(#…)"`) se documentan como **no soportados** (se pintan
sin color): en el SVG complejo la figura con gradiente queda sin relleno, como
se espera. **`svg` no está cableado en `render`/paint todavía** (`<img
src="*.svg">` e inline `<svg>` no se dibujan): wiring es el siguiente foco.

---

## 10. Coste de floats, posicionamiento, stacking y opacidad (referencia r4)

| Variante | Δ vs control | Factor |
|---|---:|---:|
| `position: relative` | +0,93–1,13 ms | 1,02× |
| `position: relative; z-index` | +0,20–0,59 ms | 1,00–1,01× |
| `opacity: 0.85` | +2,96–3,55 ms | 1,05–1,06× |
| `float: left` | +12,54–12,72 ms | **1,22×** |

El documento realista del bench no usa floats ni opacidad; no interviene en §3.

---

## 11. Presupuestos orientativos

Presupuestos vigentes (r12): pipeline **≤150 ms**, render showcase **≤35 ms**,
`build-display-list` **≤25 ms**. Con r13:

| Benchmark / medida | Baseline r13 | Presupuesto | % | Veredicto |
|---|---:|---:|---:|---|
| `html/realistic-1mb` | 22,6 ms | ≤ 100 ms | 23 % | ✅ |
| `html/nested-div-64k` | 23,1 ms | ≤ 100 ms | 23 % | ✅ |
| `html/massive-end-p` | 22,3 ms | ≤ 50 ms | 45 % | ✅ |
| `html/tag-40k-attrs` | 12,0 ms | ≤ 50 ms | 24 % | ✅ |
| `css/parse-10k-rules` | 14,9 ms | ≤ 30 ms | 50 % | ✅ |
| `css/resolve-styles-10k-nodes` | 30,3 ms | ≤ 75 ms | 40 % | ✅ |
| `layout/realistic-1mb` | 51,6 ms | ≤ 150 ms | 34 % | ✅ |
| `paint/build-display-list` | 12,8 ms | ≤ 25 ms | 51 % | ✅ |
| `paint/rasterize-800x600` | 1,00 ms | ≤ 25 ms | 4 % | ✅ |
| `paint/render-to-png-800x600` | 9,2 ms | ≤ 50 ms | 18 % | ✅ |
| `pipeline/html-to-png-800x600` | **133,6 ms** | **≤ 150 ms** | **89,1 %** (min 129,3 = 86,2 %) | ✅ mantener 150 |
| `render` showcase 1024×768 | **30,8 ms** | **≤ 35 ms** | **87,9 %** | ✅ mantener 35 |
| shell headless showcase | 30,8 ms | ≤ 35 ms | 88 % | ✅ |
| shell headless basic | 9,7 ms | — | — | ✅ |
| shell basic `--zoom 2` 320×200 | 3,9 ms | — | — | ✅ (PNG == render 160×100) |
| webfonts (render, por página) | +0,6 ms (`fonts`) | — | — | ✅ despreciable |
| SVG standalone (fuera del pipeline) | 1,4 ms simple / 224 ms complejo 512² (PNG stored) | — | — | sin cablear en render |

**Conclusión**: se mantienen **pipeline ≤150 ms** (actual 133,6 = 89,1 %) y
**render showcase ≤35 ms** (actual 30,8 = 87,9 %). Ambos dejan ~11–12 % de
margen con la varianza actual; el pipeline ya está dominado por layout (51,6)
+ CSS (30,3) + parse (22,6) + display list (12,8) + PNG (9,2). Las features de
r13 (webfonts, SVG) **no consumen** el presupuesto del bench.

---

## 12. Comparación histórica

Doce baselines en la misma máquina, mismo arnés:

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | **19:30** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | **`5234630`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | limpio | docs dirty | limpio | limpio | docs dirty | docs dirty + website | **WIP ajeno en html; bench desde archivo prístino** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | `43f7748d…` | `a188a1da…` | `c34ff65c…` | `d655c1f2…` | **`153b8aff…`** |
| Load (bench) | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | **3,4–5,3 (picos 14,9)** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | **22.618** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | 25.658 | 25.084 | 24.772 | 24.022 | 20.396 | 21.469 | 22.466 | **23.148** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | **30.311** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | **51.601** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | **12.842** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | 0.843 | 0.993 | 0.881 | 1.082 | 0.925 | 1.052 | **0.999** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | **9.200** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | **133.629** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** |

### Ronda 12 → ronda 13, fila a fila

| Benchmark | R12 (ms) | R13 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 21.641 | **22.618** | +4,5 % ~ |
| `html/nested-div-10k` | 3.446 | **3.554** | +3,1 % ~ |
| `html/nested-div-32k` | 11.311 | **11.334** | +0,2 % |
| `html/nested-div-64k` | 22.466 | **23.148** | +3,0 % ~ |
| `html/nested-div-64k-default-limits` | 14.047 | **14.014** | −0,2 % |
| `html/massive-end-p` | 21.897 | **22.312** | +1,9 % |
| `html/massive-formatting` | 12.421 | **12.710** | +2,3 % |
| `html/tag-40k-attrs` | 12.682 | **12.047** | −5,0 % |
| `html/pretty-5k-blocks` | 11.646 | **11.645** | −0,0 % |
| `css/parse-10k-rules` | 14.966 | **14.909** | −0,4 % |
| `css/resolve-styles-2k-nodes` | 6.017 | **6.399** | +6,3 % ~ |
| `css/resolve-styles-10k-nodes` | 28.807 | **30.311** | +5,2 % ~ |
| `css/selector-match-simple` | 0.671 | **0.682** | +1,6 % |
| `css/selector-match-descendant` | 1.244 | **1.314** | +5,6 % |
| `layout/realistic-1mb` | 51.277 | **51.601** | +0,6 % ✅ |
| `paint/build-display-list` | 13.648 | **12.842** | −5,9 % |
| `paint/rasterize-800x600` | 1.052 | **0.999** | −5,0 % |
| `paint/render-to-png-800x600` | 9.136 | **9.200** | +0,7 % ✅ |
| `pipeline/html-to-png-800x600` | 133.373 | **133.629** | +0,2 % ✅ |
| **TOTAL (suma)** | **391.748** | **395.269** | +0,9 % |

~ Filas sin cambio de código en r13: la diferencia es varianza del host
(load 3,4–5,3 con picos de 14,9; el HTML/CSS del bench no procesa `@font-face`).
**Layout, pipeline y render-to-png se mantienen dentro de ±0,7 %**: la ronda no
introduce regresiones en el bench. Las funcionalidades nuevas (SVG, webfonts)
se miden aparte y no forman parte del pipeline por defecto.

---

## 13. Cómo comparar y detectar regresiones

1. Medir en la misma máquina y sistema ocioso (`uptime` < ~1.5). **Si el árbol
   tiene WIP de otro agente**, medir desde un archivo prístino del HEAD
   (`git archive` + build en `target/`), como en esta ronda.
2. Guardar JSON y comparar medianas:

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
3. Criterio de descarte usado en r13: `layout` 50–56 ms, `pipeline` ≤142 ms,
   `render-to-png` ≤9,9 ms, `html/realistic` ≤26 ms; 10 de 15 pasadas válidas.
4. Regresión si la mediana empeora >15 % de forma reproducible.
5. Para complejidad, `--scale 0.5` vs `1.0`: ratios ≈1.
6. Features nuevas: webfonts con `render --json` (`fonts_loaded/failed/bytes`,
   `timings.fonts`), SVG standalone con el ejemplo `svg_to_png`, shell con
   `--zoom`/`--refresh-ms` (auto-refresh solo en modo ventana).
7. JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
   `worktree_dirty`, `rustc`, `cpu`, `cores`, `scale`, `iterations`, `warmup`,
   `setup_ms`, `total_median_ms`, `results[]`, `skipped[]`.

---

## 14. Limitaciones y notas

- **Host muy variable**: 15 intentos de bench, 10 válidos; load 3,4–5,3 en los
  válidos y picos de 14,9 con `rustc` de otros agentes (pasadas con filas de
  33–304 ms descartadas). Las cifras de HTML/CSS de esta ronda deben leerse
  como **sin regresión dentro de varianza**, no como empeoramiento real.
- **Árbol con WIP ajeno en `crates/html`**: la baseline se midió desde un
  archivo prístino `5234630` (`target/r13-clean-src`, borrable). `render` y
  `shell` se compilaron antes de ese WIP, por lo que sus medidas son del HEAD.
- **Webfonts**: el bench no las paga (solo `render`); coste medido ~0,6 ms por
  fuente en un fixture de ~10 ms, y ~0,7 ms de inicialización de la BD aunque
  no haya `@font-face`. Una fuente corrupta/remota falla con warning y sin
  crash.
- **SVG**: el crate está completo pero **no cableado** en `render`/paint (ni
  `<img src="*.svg">` ni inline `<svg>`); la medición standalone incluye el
  PNG writer *stored* del ejemplo, que domina el caso 512². Gradientes no
  soportados (se pintan sin color), documentado en el crate.
- **Shell**: `--zoom` headless equivale a viewport CSS `physical/zoom` y produce
  PNG byte-idéntico; `--refresh-ms` es opt-in y no se ejerce en `--screenshot`
  (un solo render), así que no se mide aquí.
- **Presupuestos**: mantener pipeline ≤150 ms (actual 133,6 = 89,1 %) y render
  showcase ≤35 ms (actual 30,8 = 87,9 %); margen ~11–12 % con la varianza
  actual.
- **Fast path de tablas (r7)**: sigue la nota histórica; no hay commit
  compilable con el camino avanzado sin fast path.
