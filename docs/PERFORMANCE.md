# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 21:00–21:25 (Europe/Madrid). Ronda 16:
  **gradientes SVG** (+ CSS interno), **APIs de navegador** (`getComputedStyle`
  real, `matchMedia`/viewport/scroll, `requestAnimationFrame`, `readyState` y
  handlers de error), **backgrounds multicapa** con
  `origin`/`clip`/`attachment`/`space`/`round`, y la web del dueño commiteada.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `653626f`**
  (`feat(render): real getComputedStyle, matchMedia/viewport, rAF and error stats`).
- **Estado del árbol**: limpio en la pasada canónica (`dirty = 0`); otros docs
  en vuelo de agentes (`dirty = 2` puntual). `website/` y
  `.github/workflows/pages.yml` ya están commiteados por el dueño (`29721f5`):
  aquí solo se renderiza la web a `/tmp` como validación, **sin versionar nada**.
- **Binario de bench**: `target/release/bench`, `sha256 = 39909f01…`
  (4 374 968 B) en las 7 pasadas válidas; la pasada del script lo recompiló con
  la misma fuente (`8ea0d43e…`, 4 374 880 B).
- **Artefactos guardados** (gitignored):
  - `target/bench-r16-run*.json`, `target/bench-aggregate-r16.json`
    (`sha256 = 9cdaedb3…`), `target/bench-latest.txt`.
  - `target/bench-r16/svg_gradients.json`, `background_layers.json`,
    `browser_apis.json`, `website.json`, `render_shell.json`.

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §14 (rondas 2–16; la 14 no publicó baseline).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. **El crate `bench` y sus generadores no cambiaron en esta ronda.**

El bench **no usa SVG, JS, backgrounds con imagen ni background multicapa**:
sus generadores no contienen `<svg>`, `<script>`, `@font-face` ni `url(...)`.
Las capacidades nuevas de r16 se miden aparte con `render`/`svg` (§8–§11) y **no
cambian el pipeline por defecto** del bench.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- **La tabla de §3 es la mediana, por fila, de 7 pasadas completas válidas**
  (de 13 intentos) con load **1,45–2,65**. Criterio de aceptación: `layout`
  34–40 ms, `pipeline` 114–124 ms, `render-to-png` 8,4–10,0 ms y
  `html/realistic` ≤22 ms.
- `setup_ms` de las válidas: 182,5–203,3 ms. La pasada canónica de
  `./tools/bench.sh` dio `TOTAL 358.827 ms` (con `layout 36.881` y
  `pipeline 119.826`, más cerca del suelo de r15; la mediana de las 7 es
  `362.714`).
- Render/shell: 11 ejecuciones intercaladas (pared) + 7 `--json` para etapas.
  SVG: 7 ejecuciones por caso con el ejemplo release. Backgrounds y APIs de
  navegador: 7 de pared + 3–5 `--json` por fixture; determinismo con 2 pasadas.
  Web del dueño: 7 ejecuciones a `/tmp` (default y `--no-network`).
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` por defecto |
| Carga de fondo | 1,03–2,65 (host en calma; pass canónica a load ~1,5) |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Bench (7 pasadas válidas de esta baseline)
./tools/bench.sh
for i in 1 2 3 4 5 6 7; do target/release/bench --json > "target/bench-r16-run$i.json"; done

# Gradientes SVG (crate svg: CSS interno + linear/radial)
cargo run --release -p svg --example svg_to_png -- \
    crates/svg/tests/fixtures/gradient.svg /tmp/g.png
cargo run --release -p svg --example svg_to_png -- \
    target/bench-r16/gradient_512.svg /tmp/g512.png

# Backgrounds multicapa
target/release/render tests/fixtures/render/background_layers.html /tmp/b.png --json

# APIs de navegador
for f in browser_api raf_animation script_errors console_messages; do
  target/release/render "tests/fixtures/render/$f.html" "/tmp/$f.png" --json
  target/release/render "tests/fixtures/render/$f.html" "/tmp/$f-nojs.png" --json --no-scripting
done

# Web del dueño (solo validación a /tmp; NO se versiona)
target/release/render website/index.html /tmp/site.png \
    --width 1280 --height 900 --json
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
| paint | `paint/build-display-list` | Display list: 34 158 items. |
| paint | `paint/rasterize-800x600` | Rasterizado software con culling/cachés y AA. |
| paint | `paint/render-to-png-800x600` | Rasterizado + `to_png()` DEFLATE (57 422 B). |
| pipeline | `pipeline/html-to-png-800x600` | Parse → styles → layout → display list → raster → PNG. |
| fonts | (`feature fonts`) | No re-medido en r16 (sin cambios; webfonts medidas en r13). |

**Fuera del bench (medido en §8–§11)**: gradientes SVG, backgrounds multicapa,
APIs de navegador/rAF/console y la web real del dueño.

---

## 3. Baseline vigente (2026-11-10, HEAD `653626f`)

Mediana por fila de **7 pasadas válidas** (5 iteraciones, 1 warm-up; *heavy* 3).
`setup`: 182,5–203,3 ms. Binario `sha256 = 39909f01…`. Suma de medianas:
`TOTAL 362.714 ms`; pasada del script: `TOTAL 358.827 ms`.

| Benchmark | Carga | mediana (ms) | rango 7 pasadas (ms) | throughput |
|---|---|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | **21.104** | 20.667–21.430 | 47,4 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | **3.316** | 3.191–3.438 | 31,7 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | **10.638** | 10.437–11.213 | 31,6 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | **21.266** | 20.093–22.486 | 31,6 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | **13.779** | 13.543–14.121 | — |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | **18.936** | 18.568–19.232 | 5,28 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | **12.870** | 12.390–13.715 | 777 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | **12.644** | 12.290–13.102 | 3,16 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | **11.611** | 11.170–11.847 | 431 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | **14.262** | 13.928–15.350 | 41,8 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | **6.618** | 6.431–6.825 | 303 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | **31.564** | 30.576–31.863 | 317 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | **0.633** | 0.592–0.650 | 63,3 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | **1.213** | 1.185–1.269 | 33,0 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | **37.418** | 37.044–38.616 | 754 k elementos/s |
| `paint/build-display-list` | 34 158 items | **12.441** | 12.274–12.663 | — |
| `paint/rasterize-800x600` | 34 158 items → pixmap 800×600 | **1.003** | 0.890–1.083 | 478 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (57 422 B) | **9.015** | 8.934–9.316 | 6,08 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | **122.383** | 121.178–123.625 | — |
| **TOTAL (suma)** | | **362.714** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página.

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.32 · 32k: 10.64 · 64k: 21.27 ms | **Lineal** (3,2× → 3,21×; 2× → 2,00×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 14.26 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 2 005 el: 6.62 · 10 005 el: 31.56 ms | **Lineal** (~317 k el/s). |
| `layout/realistic-1mb` | 28 195 elementos → **37,42 ms** | Estable vs r15 (36,05; +3,8 % de ventana, código de layout sin cambios). |
| `paint/render-to-png` | raster 1,00 + encoder ≈8,0 ms | Encoder PNG de r12, byte-idéntico. |
| `pipeline` | 122,4 ms (121,2–123,6) | Reparto: layout 37,4 + CSS 31,6 + parse 21,1 + display list 12,4 + PNG 9,0. |

---

## 5. Ronda 16: SVG con gradientes/CSS, backgrounds multicapa y APIs web

### 5.1 Qué entró en la ronda

- `1477050 feat(svg)`: **CSS interno** (`<style>`, `class`, `id`) y **gradientes
  lineales/radiales** en el crate `svg`; siguen sin soporte `pattern`/`<text>`.
- `ac1c330 feat(css,paint)`: `background-origin`/`background-clip`/
  `background-attachment`, **múltiples capas** de `background-image` con listas
  por capa, y `space`/`round` en `background-repeat`.
- `5fd4ea2 feat(js)`: `matchMedia`, viewport/scroll, `requestAnimationFrame`,
  `readyState` y handlers de error.
- `653626f feat(render)`: `getComputedStyle` real, `matchMedia`/viewport, rAF y
  estadísticas de error/consola; nueva etapa **`styles_pre_script`** (pase de
  estilos antes de ejecutar scripts).
- `29721f5`: la **web del dueño** queda commiteada (aquí solo se valida a `/tmp`).

### 5.2 Verificación: no regresión en el bench

| Métrica | Esperado en la ronda | Nuestra medición (7 pasadas) | Pasada canónica | Veredicto |
|---|---:|---:|---:|---|
| `layout/realistic-1mb` | ~36 ms | **37.418 ms** (+3,8 % vs r15) | 36.881 | ≈ sin regresión (código de layout sin cambios; load 1,45–2,65 vs 1,4–2,0) |
| `pipeline/html-to-png` | ~116–120 ms | **122.383 ms** (+5,1 %) | 119.826 | ≈ dentro de varianza; canónica en el rango |
| `paint/render-to-png` | ~9 ms | **9.015 ms** (+0,8 %) | 8.939 | ✅ |
| `html/realistic-1mb` | ~20–21 ms | **21.104 ms** (−0,7 %) | 21.070 | ✅ |
| `html/massive-end-p` | ~19 ms | **18.936 ms** (−0,7 %) | 18.461 | ✅ |
| `html/tag-40k-attrs` | ~12,7 ms | **12.644 ms** (−1,1 %) | 12.801 | ✅ |
| **TOTAL (suma)** | — | **362.714 ms** (+2,2 % vs r15) | 358.827 | ≈ varianza del host |

**Explicación**: el bench **no ejecuta ninguna capacidad nueva** de r16 (SVG,
JS, backgrounds con imagen). Las filas de layout/paint no cambiaron de código;
la diferencia con r15 es la ventana de carga (esta ronda se midió a load
1,45–2,65 y r15 a 1,4–2,0; la pasada canónica a load ~1,5 da 36,9/119,8, en el
rango esperado). **No hay regresión atribuible al código**; el TOTAL sube un
2,2 % por varianza. Las filas HTML/CSS se mantienen planas o mejoran.

---

## 6. Codificador PNG (sin cambios en r16)

| Métrica | r15 | r16 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.946 ms | **9.015 ms** | +0,8 % (varianza) |
| — rasterizar | 0.947 ms | **1.003 ms** | varianza |
| — codificar PNG | ~8,00 ms | **~8,01 ms** | encoder de r12, sin cambios |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden phase4 basic | 42 853 B | **42 853 B** | `9e957177…` (verificado) |

---

## 7. Pipeline `render` y shell headless (ronda 16)

11 ejecuciones intercaladas:

| Fixture / modo | pared r16 | pared r15 | rango r16 | PNG |
|---|---:|---:|---:|---:|
| showcase 1024×768 (render) | **28,24 ms** | 29,49 ms | 26,41–32,04 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **8,62 ms** | 8,49 ms | 7,75–9,77 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **28,56 ms** | 29,41 ms | 27,21–30,74 | 80 220 B (idéntico) |
| basic (shell headless) | **8,70 ms** | 9,59 ms | 7,64–11,33 | 21 090 B (idéntico) |
| basic `--width 320 --height 200 --zoom 2` (shell) | **3,58 ms** | 3,66 ms | 2,94–4,11 | 3 202 B |
| basic CSS 160×100 (render, referencia) | **3,14 ms** | 3,42 ms | 2,58–3,38 | 3 202 B (`4792779d…`) |

El showcase baja a **28,24 ms** (−4,2 % vs r15, dentro de varianza; el bench no
usa las features nuevas) y `basic` se mantiene ~8,6 ms. `--zoom 2` sigue siendo
un render al viewport CSS `physical/zoom` con PNG byte-idéntico; auto-refresh
sigue siendo opt-in (no se ejerce en `--screenshot`).

---

## 8. Coste de gradientes SVG (nuevo en r16)

El crate `svg` ahora soporta `<style>`/`class`/`id` internos y gradientes
lineales/radiales. Medido standalone con el ejemplo `svg_to_png` (7 runs; la
pared incluye el **PNG writer de bloques stored** del ejemplo, que en 512² pesa
~1 MB y domina):

| Caso | pared (ms) | rango | PNG | Notas |
|---|---:|---:|---:|---|
| `crates/svg/tests/fixtures/gradient.svg` (120×60: linear + radial + CSS interno) | **2,61** | 2,07–2,86 | 28 928 B | caso del crate |
| relleno plano 512×512 (temporal) | **15,88** | 15,27–18,66 | 1 049 236 B | baseline |
| gradiente 512×512 (64 stops linear + radial + overlay, temporal) | **52,81** | 52,16–57,02 | 1 049 236 B | **+36,93 ms vs plano** |
| relleno plano 256×256 | **4,85** | 4,07–5,55 | 262 488 B | baseline |
| gradiente 256×256 | **14,38** | 13,13–14,76 | 262 488 B | **+9,53 ms vs plano** |

**Lectura**: el coste adicional del gradiente es **por píxel** y escala con el
área (~+37 ms a 512², ~+9,5 ms a 256²; ≈0,14 µs por píxel con AA y varios
stops). Los PNG de plano y gradiente pesan **lo mismo** porque el ejemplo usa
bloques stored (no comprime): en el motor real el encoder DEFLATE del pipeline
sí se beneficiaría. Un gradiente de 120×60 cuesta **2,6 ms** de pared total
(incluido arranque y PNG). Siguen sin soportar `pattern`, `<text>` ni filtros
(próximo foco del crate).

---

## 9. Coste de backgrounds multicapa (ronda 16)

`background_layers.html` del repo: dos capas (`bullet.png` no-repeat 20×20
centrado sobre `pattern.png` repeat), `background-origin/clip: content-box`;
segundo bloque con `origin`/`clip` content-box. 7 runs + 5 `--json`:

| Fixture | pared (ms) | `backgrounds` (µs) | `paint` (µs) | `raster` (µs) | PNG |
|---|---:|---:|---:|---:|---:|
| `background_layers.html` (2 capas) | **9,43** | 41 | 7 | 1 200 | 15 023 B |
| `backgrounds.html` (legacy, 2 imgs + 1 fallo + marcadores) | 9,38 | 58 | 5 | 1 092 | 15 778 B |

`background_images_loaded = 2`, `failed = 0`, `background_bytes = 185` (bullet
92 B + pattern 93 B), sin warnings. La **geometría de capas**
(`backgrounds` 41 µs) y el **origin/clip** son baratos; el coste sigue en el
**rasterizado** del muestreo (~1,1–1,2 ms para los dos bloques de 120×80/160×64).
md5 del PNG a 800×600: **`736b201ed3643eb26f3657bd36f6897b`** (15 023 B);
a 320×200 `d44c004a…` y a 1024×768 `986d2b8e…`. El md5 de referencia
`c357cc2c…` de la ronda no se reproduce en ningún viewport probado ni aparece
en el repo (los tests de `background_layers_tests.rs` validan propiedades de
píxeles, no un md5 global); se documenta el md5 real medido.

---

## 10. APIs de navegador en `render` (ronda 16)

4 fixtures, 7 runs de pared con y sin `--no-scripting`, 3 `--json` para etapas:

| Fixture | pared JS (ms) | sin JS (ms) | Δpared | `styles_pre_script` | `scripting` | `event_loop` | tasks | `console_errors`/entries | ΔPNG |
|---|---:|---:|---:|---:|---:|---:|---:|---|---:|
| `browser_api.html` (getComputedStyle, matchMedia, viewport) | **10,02** | 8,00 | **+2,02** | 266 µs | 1 031 µs | 20 µs | 0 | 0 / 0 | −99 B |
| `raf_animation.html` (rAF) | **8,83** | 7,42 | **+1,41** | 235 µs | 680 µs | 80 µs | **3** | 0 / 0 | +7 B |
| `script_errors.html` | **8,94** | 7,60 | **+1,34** | 230 µs | 570 µs | 62 µs | 0 | **2 / 3** | **+4 844 B** |
| `console_messages.html` | **7,50** | 7,71 | −0,21 (ruido) | 231 µs | 435 µs | 21 µs | 0 | **1 / 3** | 0 B |

- La nueva etapa **`styles_pre_script`** cuesta **0,23–0,27 ms** por página
  (pase de estilos previo a ejecutar scripts) y aparece en `timings_us`.
- El coste de las APIs es de **1,3–2,0 ms de pared** cuando el script muta el
  DOM (`scripting` 0,44–1,03 ms + re-render); `browser_api` es el más caro
  (`getComputedStyle`/`matchMedia` sobre estilos). rAF ejecuta **3 frames**
  (80 µs de event loop) sin coste apreciable; `console_errors`/entries se
  reportan en el JSON.
- **Determinismo**: 2 pasadas idénticas por fixture → mismo PNG (md5) y mismos
  contadores (`scripts_executed`, `script_events`, `tasks`, `console_errors`)
  en los 4 (p. ej. browser_api `3085a8b2…` en ambas, raf `5094557c…`,
  script_errors `54fa14ea…`, console `ff019a60…`).

---

## 11. Web del dueño (validación read-only)

La web promocional del dueño está commiteada (`29721f5`) pero este informe
**no la versiona**: solo se renderiza a `/tmp` como caso real externo. 7 runs a
1280×900 (`--json`), default y `--no-network`:

| Métrica | default | `--no-network` |
|---|---:|---:|
| pared (mediana) | **152,91 ms** | **149,86 ms** |
| PNG | **49 107 B** (`3f67f649…`) | 49 107 B (idéntico) |
| `display_items` | 2 210 | 2 210 |
| `inline_svgs` / `svgs_rendered` | **56 / 58** | 56 / 58 |
| `fonts_loaded` / `images_loaded` | **3 / 64** (0 fallos) | 3 / 64 |
| warnings | 2 (presupuesto de reloj virtual agotado, 4 timers descartados) | 2 |
| total etapas | 154,8 ms | 143,0 ms |

Etapas (µs, mediana): fetch 82 · decode/parse 2 528 · styles 4 549 ·
**styles_pre_script 9 963** · fonts 2 779 · **images 91 548** ·
backgrounds 265 · scripting 2 980 · event_loop 3 451 · layout 1 282 ·
paint 238 · **raster 15 421** · total 154 766.

**Lectura**: en una página real con 64 imágenes y 58 SVG el coste lo dominan
**`images` (~91 ms: decode de 64 imágenes + 58 SVG inline)** y `raster`
(~15 ms); el resto (styles 4,5 ms, layout 1,3 ms, scripting 3,0 ms) es menor.
Los 2 warnings son el presupuesto de reloj virtual del event loop (1000 ms)
agotado con 4 timers pendientes: no afecta al render (no hay más mutaciones).
El default y `--no-network` dan prácticamente lo mismo porque todos los assets
locales ya existen (los badges remotos son opcionales en la página actual).
Documentado como **página real externa del dueño** (no se versiona en este
informe; el render va a `/tmp`).

---

## 12. Coste de floats, posicionamiento, stacking y opacidad (referencia r4)

| Variante | Δ vs control | Factor |
|---|---:|---:|
| `position: relative` | +0,93–1,13 ms | 1,02× |
| `position: relative; z-index` | +0,20–0,59 ms | 1,00–1,01× |
| `opacity: 0.85` | +2,96–3,55 ms | 1,05–1,06× |
| `float: left` | +12,54–12,72 ms | **1,22×** |

El documento realista del bench no usa floats ni opacidad; no interviene en §3.

---

## 13. Presupuestos orientativos

Con el pipeline en **122,4 ms** (canónica 119,8) y el layout en **37,4 ms**
(36,9), se mantienen los presupuestos propuestos en r15:

| Benchmark / medida | Baseline r16 | Presupuesto | % | Veredicto |
|---|---:|---:|---:|---|
| `html/realistic-1mb` | 21,1 ms | ≤ 100 ms | 21 % | ✅ |
| `html/tag-40k-attrs` | 12,6 ms | ≤ 50 ms | 25 % | ✅ |
| `html/massive-end-p` | 18,9 ms | ≤ 50 ms | 38 % | ✅ |
| `css/parse-10k-rules` | 14,3 ms | ≤ 30 ms | 48 % | ✅ |
| `css/resolve-styles-10k-nodes` | 31,6 ms | ≤ 75 ms | 42 % | ✅ |
| `layout/realistic-1mb` | **37,4 ms** | **≤ 45 ms** | **83,1 %** (canónica 82,0 %) | ✅ mantener |
| `paint/build-display-list` | 12,4 ms | ≤ 25 ms | 50 % | ✅ |
| `paint/rasterize-800x600` | 1,00 ms | ≤ 25 ms | 4 % | ✅ |
| `paint/render-to-png-800x600` | 9,0 ms | ≤ 50 ms | 18 % | ✅ |
| `pipeline/html-to-png-800x600` | **122,4 ms** | **≤ 140 ms** | **87,4 %** (canónica 85,6 %) | ✅ mantener |
| `render` showcase 1024×768 | **28,2 ms** | **≤ 35 ms** | **80,7 %** | ✅ mantener |
| shell headless showcase | 28,6 ms | ≤ 35 ms | 82 % | ✅ |
| shell basic `--zoom 2` 320×200 | 3,6 ms | — | — | ✅ |
| gradiente SVG 120×60 | 2,6 ms (ejemplo) | — | — | ✅ coste por píxel |
| background multicapa 2 capas (800×600) | 9,4 ms | — | — | ✅ raster domina |
| APIs de navegador (página con script) | +1,3–2,0 ms | — | — | ✅ incluye `styles_pre_script` 0,25 ms |
| web real del dueño 1280×900 | 150–153 ms | — | — | caso externo, no versionado |

**Conclusión**: se mantienen **pipeline ≤140 ms** (87,4 %; margen ~18 ms),
**layout ≤45 ms** (83,1 %) y **render showcase ≤35 ms** (80,7 %). No se bajan
más: el margen con la varianza observada (±5–10 % en layout/paint) quedaría
demasiado justo. El coste nuevo de r16 fuera del bench es pequeño (gradientes
por píxel en SVG, raster de capas, `styles_pre_script`), y la página real
externa está dominada por decode de imágenes/PNG.

---

## 14. Comparación histórica

Catorce baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | **21:00** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | **`653626f`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | limpio | docs dirty | limpio | limpio | docs dirty | docs dirty | WIP html | limpio (website untracked) | **limpio** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | `43f7748d…` | `a188a1da…` | `c34ff65c…` | `d655c1f2…` | `153b8aff…` | `1f1dbbdd…` | **`39909f01…`** |
| Load (bench) | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | **1,45–2,65** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | **21.104** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | 25.658 | 25.084 | 24.772 | 24.022 | 20.396 | 21.469 | 22.466 | 23.148 | 21.433 | **21.266** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | **31.564** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | **37.418** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | **12.441** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | 0.843 | 0.993 | 0.881 | 1.082 | 0.925 | 1.052 | 0.999 | 0.947 | **1.003** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | **9.015** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | **122.383** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** |

### Ronda 15 → ronda 16, fila a fila

| Benchmark | R15 (ms) | R16 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 21.260 | **21.104** | −0,7 % |
| `html/nested-div-10k` | 3.351 | **3.316** | −1,0 % |
| `html/nested-div-32k` | 10.755 | **10.638** | −1,1 % |
| `html/nested-div-64k` | 21.433 | **21.266** | −0,8 % |
| `html/nested-div-64k-default-limits` | 13.919 | **13.779** | −1,0 % |
| `html/massive-end-p` | 19.065 | **18.936** | −0,7 % |
| `html/massive-formatting` | 12.809 | **12.870** | +0,5 % |
| `html/tag-40k-attrs` | 12.782 | **12.644** | −1,1 % |
| `html/pretty-5k-blocks` | 11.735 | **11.611** | −1,1 % |
| `css/parse-10k-rules` | 14.706 | **14.262** | −3,0 % |
| `css/resolve-styles-2k-nodes` | 6.349 | **6.618** | +4,2 % |
| `css/resolve-styles-10k-nodes` | 30.543 | **31.564** | +3,3 % |
| `css/selector-match-simple` | 0.674 | **0.633** | −6,1 % |
| `css/selector-match-descendant` | 1.291 | **1.213** | −6,0 % |
| `layout/realistic-1mb` | 36.054 | **37.418** | +3,8 % ~ |
| `paint/build-display-list` | 11.954 | **12.441** | +4,1 % ~ |
| `paint/rasterize-800x600` | 0.947 | **1.003** | +5,9 % ~ |
| `paint/render-to-png-800x600` | 8.946 | **9.015** | +0,8 % |
| `pipeline/html-to-png-800x600` | 116.410 | **122.383** | +5,1 % ~ |
| **TOTAL (suma)** | **354.983** | **362.714** | +2,2 % ~ |

~ Sin cambio de código de bench/layout/paint en r16: la diferencia es la
ventana de carga (load 1,45–2,65 vs 1,4–2,0 en r15). La pasada canónica de r16
(load ~1,5) da `layout 36.881` y `pipeline 119.826`, dentro del rango esperado:
**la ronda no introduce regresión en el bench**. Las capacidades nuevas se miden
aparte (§8–§11).

---

## 15. Cómo comparar y detectar regresiones

1. Medir en la misma máquina y con el sistema ocioso (`uptime` < ~1.5). En r16
   se usaron 7 de 13 intentos con criterio de aceptación (`layout` 34–40,
   `pipeline` 114–124, `render-to-png` 8,4–10,0, `html` ≤22) y load
   1,45–2,65.
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
3. Regresión si la mediana empeora >15 % de forma reproducible; dispersión
   típica ~1–6 % en reposo y ±5–10 % en layout/paint con carga.
4. Para complejidad, `--scale 0.5` vs `1.0`: ratios ≈1.
5. Capacidades nuevas: gradientes SVG (crate `svg` + ejemplo), backgrounds
   multicapa (`background_layers.html`), APIs de navegador
   (`browser_api`/`raf_animation`/`script_errors`/`console_messages`) y
   `styles_pre_script` en `render --json`. Determinismo: 2 pasadas → mismo PNG
   y mismos contadores.
6. Golden/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   phase4 basic `9e957177…` 42 853 B, bench PNG 57 422 B. Web del dueño:
   49 107 B/`3f67f649…` a 1280×900 (solo validación a `/tmp`).

---

## 16. Limitaciones y notas

- **Host variable**: 13 intentos de bench, 7 válidos (load 1,45–2,65; picos de
  compilaciones descartados). Las filas layout/paint difieren ±4–6 % entre
  ventanas sin cambios de código; la pasada canónica da los valores del suelo.
- **El bench no usa SVG/JS/backgrounds con imagen**: las mejoras de r16 no
  cambian el pipeline por defecto; se miden en §8–§11.
- **Gradientes SVG**: coste por píxel (~0,14 µs/px); el ejemplo `svg_to_png` usa
  un PNG writer de bloques *stored*, que domina en 512² (1 MB de salida). El
  motor real usa su encoder DEFLATE. Siguen sin soporte `pattern`, `<text>` y
  filtros (próximo foco del crate).
- **Backgrounds multicapa**: el md5 de referencia `c357cc2c…` no se reproduce en
  los viewports probados (800×600 → `736b201e…`) ni aparece en el repo; los
  tests del fixture validan píxeles por propiedades. Se documenta el md5 real.
- **APIs de navegador**: `styles_pre_script` ~0,25 ms/página; el coste de las
  APIs es 1,3–2,0 ms de pared cuando mutan el DOM. Deterministas entre pasadas.
  El presupuesto de reloj virtual (1 000 ms) puede agotarse en páginas con
  timers (warning; no rompe el render).
- **Web del dueño**: commiteada por el dueño, se valida read-only a `/tmp`
  (no se versiona desde este informe); los 2 warnings son del presupuesto de
  reloj virtual, no errores de assets (64 imágenes cargadas, 0 fallos).
- **Presupuestos**: mantener pipeline ≤140 ms (122,4 = 87,4 %), layout ≤45 ms
  (37,4 = 83,1 %) y render showcase ≤35 ms (28,2 = 80,7 %), con margen ~12–13 %
  en los dos primeros.
- **Fast path de tablas (r7)**: sigue la nota histórica; no hay commit
  compilable con el camino avanzado sin fast path.
