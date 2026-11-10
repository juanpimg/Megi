# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 18:51–19:05 (Europe/Madrid). Ronda 12:
  `ComputedStyle` compartido con `Arc` (LayoutBox de 176 B), **codificador PNG
  −30,5 % con bitstream byte a byte idéntico**, JS con eventos/inline handlers y
  fixture real del dueño (`website/`, untracked, en construcción).
- **Repo**: `Megi`, rama `sprint/robustez-fase4`. **HEAD de referencia
  `7d2ddde`** (`perf(paint): PNG encoding 30 % faster with a byte-identical
  bitstream`); a mitad de ventana aterrizó `a799a24`
  (`fix(render): only evaluate the media attribute on stylesheet elements`, que
  **solo toca `crates/render`**, no las crates del bench). El binario del bench
  se compiló de `7d2ddde` (`sha256 d655c1f2…`) y se recompiló después con el
  mismo código de motor (`bc1ea7d2…`).
- **Estado del árbol**: código limpio; `README.md`/`docs/ROADMAP.md` editados por
  otro agente y `website/` untracked (del dueño). Los 6 JSON válidos marcan
  `worktree_dirty = true` por eso.
- **Artefactos guardados** (gitignored):
  - `target/bench-r12-run1..9.json`, `target/bench-aggregate-r12.json`
    (`sha256 = deb9cdce…`), `target/bench-latest.txt` (`cad8580e…`).
  - `target/perf-r12-render.json`, `target/perf-r12-website.json`,
    `target/perf-r12-extra.json` (verificación de goldens PNG).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §11 (rondas 2–11).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. **El crate `bench` y sus generadores no cambiaron en esta ronda.**

Además del bench se mide el **caso real del dueño** `website/index.html`
(1280×900, sin tocar el repo; render a `/tmp`) y los fixtures del pipeline
`render`/`shell` (§7 y §8).

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- **La tabla de §3 es la mediana, por fila, de 6 pasadas completas válidas**
  (3 a load 4,8–5,1 y 3 a load 0,63–0,68). Se hicieron **9 intentos**: las
  pasadas 1–3 se **descartaron** porque el host subió a **load 12,4–12,8** por
  compilaciones de otros agentes y dieron cifras absurdas (p. ej.
  `html/realistic-1mb` 66,5 ms, pipeline 162–169 ms); el criterio de descarte se
  documenta aquí para que la baseline sea reproducible. La pasada canónica de
  `./tools/bench.sh` dio `TOTAL 385.769 ms`.
- `setup_ms`: 194,9–208,2 ms (no cronometrado).
- Render/shell: 11 ejecuciones intercaladas (pared) y 7 para las etapas `--json`.
  Caso real website: 7–9 ejecuciones intercaladas default vs `--no-scripting`,
  con `--no-network` para eliminar el ruido de red (ver §8).
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` por defecto |
| Carga de fondo | pasadas válidas: 0,63–5,1; picos de 12,8 (pasadas descartadas) |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

# Bench
./tools/bench.sh
for i in 1 2 3 4 5 6 7 8 9; do target/release/bench --json > "target/bench-r12-run$i.json"; done

# Render del showcase y del caso real (website del dueño; NO se modifica)
target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png \
    --width 1024 --height 768 --json
target/release/render website/index.html /tmp/site.png \
    --width 1280 --height 900 --no-network --json
#   (sin --no-network intenta descargar los badges shields.io y los assets
#    locales que aún no existen: 12 imágenes fallidas, ~350-420 ms dominados por red)

# Verificación de bitstream PNG byte a byte
target/release/render tests/fixtures/phase4/basic.html /tmp/p4.png --json   # 42 853 B
# showcase: 80 220 B; render/basic: 21 090 B; bench PNG: 57 422 B

# Shell headless
target/release/megi tests/fixtures/render/showcase/index.html \
    --screenshot /tmp/s.png --width 1024 --height 768
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
| fonts | (`feature fonts`) | No re-medido en r12 (sin cambios en `crates/fonts`). |

**Nuevo en r12, fuera del bench**: render del fixture real `website/index.html`
1280×900 (§8); verificación de bitstream PNG (§6); render/shell showcase/basic
con el encoder nuevo (§7).

---

## 3. Baseline vigente (2026-11-10, HEAD `7d2ddde`)

Mediana por fila de **6 pasadas válidas** (5 iteraciones, 1 warm-up; *heavy* 3).
`setup`: 194,9–208,2 ms. Binario `sha256 = d655c1f2…`. La suma de medianas da
`TOTAL 391.748 ms`; la pasada del script dio `TOTAL 385.769 ms` (y el pipeline
`129.849 ms`).

| Benchmark | Carga | mediana (ms) | rango 6 pasadas (ms) | throughput |
|---|---|---:|---:|---:|
| `html/realistic-1mb` | 1.00 MiB, 1 044 secciones, 135 reglas CSS | **21.641** | 21.349–22.493 | 46,3 MiB/s |
| `html/nested-div-10k` | 10 000 divs anidados, 0.10 MiB | **3.446** | 3.379–3.595 | 30,5 MiB/s |
| `html/nested-div-32k` | 32 000 divs anidados, 0.34 MiB | **11.311** | 10.523–12.321 | 29,7 MiB/s |
| `html/nested-div-64k` | 64 000 divs anidados, 0.67 MiB | **22.466** | 22.135–23.633 | 29,9 MiB/s |
| `html/nested-div-64k-default-limits` | 64 000 divs, profundidad máx. 4096 | **14.047** | 13.829–14.467 | — |
| `html/massive-end-p` | 100 000 `</p>`, 0.38 MiB | **21.897** | 21.531–22.619 | 4,57 M tags/s |
| `html/massive-formatting` | 10 000 bloques mal anidados, 0.24 MiB | **12.421** | 12.225–13.164 | 805 k bloques/s |
| `html/tag-40k-attrs` | 40 000 atributos, 0.59 MiB | **12.682** | 12.313–13.839 | 3,15 M attrs/s |
| `html/pretty-5k-blocks` | 5 000 bloques indentados, 0.55 MiB | **11.646** | 11.283–12.236 | 429 k bloques/s |
| `css/parse-10k-rules` | 10 000 reglas, 0.60 MiB | **14.966** | 14.077–15.605 | 39,8 MiB/s |
| `css/resolve-styles-2k-nodes` | 2 005 elementos, 130 reglas | **6.017** | 5.641–6.153 | 333 k elementos/s |
| `css/resolve-styles-10k-nodes` | 10 005 elementos, 130 reglas | **28.807** | 28.638–30.097 | 347 k elementos/s |
| `css/selector-match-simple` | 10 005 el × 4 selectores | **0.671** | 0.610–1.114 | 59,6 M matches/s |
| `css/selector-match-descendant` | 10 005 el × 4 selectores | **1.244** | 1.181–1.262 | 32,2 M matches/s |
| `layout/realistic-1mb` | 28 195 elementos, 88 155 cajas, 800×600 | **51.277** | 50.444–53.590 | 550 k elementos/s |
| `paint/build-display-list` | 34 158 items | **13.648** | 12.394–15.539 | — |
| `paint/rasterize-800x600` | 34 158 items → pixmap 800×600 | **1.052** | 0.895–1.284 | 456 MPix/s |
| `paint/render-to-png-800x600` | raster + PNG (57 422 B) | **9.136** | 8.745–10.518 | 5,99 MiB/s |
| `pipeline/html-to-png-800x600` | parse+styles+layout+dl+raster+PNG | **133.373** | 128.053–139.068 | — |
| **TOTAL (suma)** | | **391.748** | | |

> **Aviso**: el TOTAL suma etapas y pipeline end-to-end (solapados); no es un
> presupuesto de página.

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.45 · 32k: 11.31 · 64k: 22.47 ms | **Lineal** (3,2× input → 3,28×; 2× → 1,99×). |
| `html/tag-40k-attrs` | 40k: 12.68 ms | Estable tras el fix de r11 (±5 % del valor de r11 11,6). |
| `html/massive-end-p` | 21.90 ms | **+2,2 % vs r11**: el flagged de r11 sigue abierto, sin empeorar. |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 14.97 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 2 005 el: 6.02 · 10 005 el: 28.81 ms | **Lineal**: 5× → 4,79× (~347 k el/s). |
| `layout/realistic-1mb` | 28 195 elementos → **51,28 ms** | **−12,8 % vs r11** por el `Arc<ComputedStyle>` y el LayoutBox de 176 B. |
| `paint/render-to-png` | raster 1,05 + encoder ≈8,1 ms | **−30,5 %** solo por el encoder (raster sin cambio de código). |
| `pipeline` | 133,4 ms (128,1–139,1) | Reparto: layout 51,3 + CSS 28,8 + parse 21,6 + display list 13,6 + PNG 9,1. |

---

## 5. Ronda 12: layout con Arc, PNG −30,5 % y caso real

### 5.1 Qué entró en la ronda

- `178290d perf(layout): share ComputedStyle through Arc and shrink LayoutBox to
  176 bytes`: `LayoutBox.style` pasa a `Option<Arc<ComputedStyle>>`; se eliminan
  clones por caja en el árbol de layout (los estilos computados se comparten).
  El suelo que queda es el IFC (≈34 ms de los 51 ms de layout).
- `7d2ddde perf(paint): PNG encoding 30 % faster with a byte-identical
  bitstream`: filtros fused en una pasada, LZ77 con hashes precalculados y
  match a 8 B, `encode_pixels` sin buffer RGBA intermedio y Adler-32
  vectorizable; **el PNG resultante es byte a byte idéntico**.
- `66a2e3b feat(js)`: `currentTarget`/`eventPhase` reales, `once`/`passive`,
  inline handlers y robustez de eventos.
- `a799a24 fix(render)` (a mitad de ventana): solo evalúa `media` en elementos
  `stylesheet`; no afecta a las crates del bench.

### 5.2 Verificación independiente de las mejoras

| Métrica | Claim de la ronda | Nuestra medición | Veredicto |
|---|---:|---:|---|
| `layout/realistic-1mb` | 51–53 ms (−14,6 %) | **51.277 ms** (−12,8 % vs r11) | ✅ |
| `pipeline/html-to-png-800x600` | 134–140 ms | **133.373 ms** (script: 129.849) | ✅ (incluso mejor) |
| `paint/render-to-png-800x600` | ~9,2 ms (−30,5 %) | **9.136 ms** (−30,5 %) | ✅ exacto |
| PNG byte-idénticos | 42 853 / 80 220 B | **42 853 B** (`phase4/basic.html`, md5 `9e957177…`) y **80 220 B** (`showcase`, md5 `c95c7eff…`) | ✅ |
| HTML/CSS sin regresiones | — | HTML +2…+9 %, CSS ±1 % (sin cambios de código; load 0,63–5,1) | ✅ dentro de varianza |
| `paint/rasterize-800x600` | — | 1.052 ms (+13,7 % vs r11 0,925) | ⚠️ varianza del host (código de raster sin cambios; rango 0,90–1,28) |

El **PNG del bench sigue en 57 422 B**, `render/basic` en **21 090 B**
(md5 `e943565d…`) y `tests/fixtures/phase4/basic.html` da **42 853 B**
(md5 `9e95717795744335bc0dfc7cade782ee`), exactamente el golden documentado
desde WP-4.11: el encoder nuevo es byte-idéntico. El TOTAL baja a 391,7 ms
(−3,5 % vs r11).

### 5.3 Nota sobre las pasadas descartadas

El host subió a load 12,8 durante los primeros 3 intentos (compilaciones de
otros agentes): `html/realistic` 24–66 ms, pipeline 151–169 ms, layout 62–86 ms.
Esas pasadas **no** se usan; las 6 válidas (load 0,63–5,1) son consistentes.
En un host compartido, el criterio de aceptación debe ser load < ~2 para las
filas de layout/paint; el bench es barato (~2,2–3,5 s) y se puede reintentar.

---

## 6. Codificador PNG: −30,5 % con bitstream idéntico

| Métrica | r11 | r12 | Δ |
|---|---:|---:|---:|
| `paint/render-to-png-800x600` (total) | 13.146 ms | **9.136 ms** | **−30,5 %** |
| — rasterizar | 0.925 ms | 1.052 ms | +13,7 % (varianza) |
| — codificar PNG (por diferencia) | ~12,22 ms | **~8,08 ms** | **−34 %** |
| PNG del bench | 57 422 B | **57 422 B** | idéntico |
| Golden showcase | 80 220 B | **80 220 B** | idéntico (`c95c7eff…`) |
| Golden phase4 basic | 42 853 B | **42 853 B** | idéntico (`9e957177…`) |

El encoder representa ahora **~6 %** del pipeline del bench (antes ~7,6 %) y
deja de ser el mayor coste de pintado: en el render del showcase el PNG+driver
sigue pesando ~59 % del total (§7), pero en el bench el coste dominante es
layout.

---

## 7. Pipeline `render` y shell headless (ronda 12)

### 7.1 Showcase 1024×768 y `basic` (goldens intactos)

11 ejecuciones intercaladas (load 1,86); etapas con 7 ejecuciones `--json`:

| Fixture | pared r12 | pared r11 | rango r12 | PNG |
|---|---:|---:|---:|---:|
| showcase 1024×768 (render) | **29,49 ms** | 33,69 ms (−12,5 %) | 28,48–32,67 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,64 ms** | 15,40 ms (−37,4 %) | 9,09–11,41 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **29,98 ms** | 32,57 ms | 28,77–31,51 | 80 220 B (idéntico) |
| basic (shell headless) | **9,88 ms** | 15,24 ms | 9,14–10,96 | 21 090 B (idéntico) |

| Etapa (mediana 7 runs, µs) | fetch | decode/parse | styles | images | backgrounds | scripting | layout | paint | raster | **total** |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| showcase r12 | 27 | 233 | 749 | 113 | 39 | 15 | 1 659 | 29 | 9 566 | **27 113** |
| showcase r11 | 23 | 230 | 799 | 161 | 57 | 14 | 1 934 | 29 | 9 840 | 31 413 |
| basic r12 | 19 | 53 | 199 | 1 | 2 | 3 | 722 | 6 | 1 340 | **7 818** |
| basic r11 | 23 | 61 | 252 | 2 | 3 | 4 | 892 | 7 | 1 591 | 14 544 |

El PNG+driver pasa de ~19 ms a ~13,2 ms en el showcase (encoder nuevo) y el
layout baja (Arc). El básico cae de 15,4 a 9,6 ms de pared: **el encoder PNG era
casi la mitad del coste de una página pequeña**.

### 7.2 Shell headless

`megi --screenshot` no crea ventana/`EventLoop`; produce PNG byte-idéntico al
CLI `render`. Pared: showcase **29,98 ms**, basic **9,88 ms** (r11: 32,57/15,24).

---

## 8. Caso real: `website/index.html` (1280×900)

El fixture del dueño (`website/`, **untracked**, en construcción) es una landing
real: 100 595 B de HTML, 2 hojas externas (`site.css` + `fonts.css`, 37 788 B de
CSS), fuentes woff2 y un script inline. **Estado actual del fixture** (no lo
toca este informe; solo se renderiza a `/tmp`):

- CSS presente ya (`assets/css/site.css`), lo que explica que los
  `display_items` hayan pasado de ~472 (estimación anterior sin CSS) a
  **3 134**.
- **Faltan assets**: `assets/js/site.js`, `site.webmanifest` y 6 imágenes
  (`assets/img/showcase.png`, `dag-light.svg`, `dag-dark.svg`,
  `phase4-basic.png`, `images-e2e.png`, `render-fixture.png`); además
  `brand/mark.svg` no es un bitmap decodificable (SVG) y hay 4 badges remotos
  (`img.shields.io`) que el motor no decodifica. Total: **13 warnings**, 12
  imágenes fallidas, `images_loaded = 0`.

### 8.1 Medición offline determinista (`--no-network`)

7–9 ejecuciones intercaladas default vs `--no-scripting`:

| Modo | pared (ms) | rango | PNG | items | hit regions |
|---|---:|---:|---:|---:|---:|
| con scripting (default) | **36,91** | 34,76–39,59 | 46 723 B | 3 134 | 1 632 |
| `--no-scripting` | **34,41** | 33,82–35,36 | 46 723 B | 3 134 | 1 632 |
| **Δ scripting** | **+2,50** | — | 0 B | 0 | 0 |

- Scripting: 1 script inline ejecutado (`scripts_executed = 1`; el externo falta);
  `timings.scripting` **1 941 µs**, `event_loop` 20 µs; el DOM no cambia
  (ΔPNG = 0): el Δpared es el coste de crear Boa + ejecutar el inline.
- Etapas: fetch 104 µs · decode/parse 3 377 · styles **6 733** · images 136
  (fallos locales, sin red) · backgrounds 215 · scripting 1 941 · layout
  **3 477** · paint 351 · raster 2 154 · **total 34 396 µs**.
- **El PNG es el mayor coste (~15,9 ms = 46 %)**: una página 1280×900 genera
  un PNG más grande (46 723 B) y el encoder aún es O(píxeles). Después, CSS
  (6,7 ms) y layout (3,5 ms).
- md5 del PNG: `eef4e5e59dc8f4d120b96f6800e0ae48` (determinista con
  `--no-network`).

### 8.2 Con red (default, sin `--no-network`)

Sin el flag, el render intenta descargar los badges de shields.io y los assets
locales ausentes: la etapa `images` pasa a **311–361 ms** y la pared a
**344–420 ms**, con gran varianza (timeouts). No es una medida estable ni
representativa del motor; **para comparar rendimiento usar `--no-network`** o
completar los assets del fixture. Cuando el dueño termine `website/`, este caso
puede convertirse en el benchmark realista de referencia del pipeline.

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

Con el pipeline en **133,4 ms** (mediana; 129,8 ms en la pasada canónica) y el
render showcase en **29,5 ms**, toca revisar los presupuestos vigentes (160 y
40 ms):

| Benchmark / medida | Baseline r12 | Presupuesto actual | % actual | Propuesta |
|---|---:|---:|---:|---|
| `html/realistic-1mb` | 21,6 ms | ≤ 100 ms | 22 % | mantener |
| `html/tag-40k-attrs` | 12,7 ms | ≤ 50 ms | 25 % | mantener |
| `html/massive-end-p` | 21,9 ms | ≤ 50 ms | 44 % | mantener (flag r11) |
| `css/parse-10k-rules` | 15,0 ms | ≤ 30 ms | 50 % | mantener |
| `css/resolve-styles-10k-nodes` | 28,8 ms | ≤ 75 ms | 38 % | mantener |
| `layout/realistic-1mb` | 51,3 ms | ≤ 150 ms | 34 % | mantener |
| `paint/build-display-list` | 13,6 ms | ≤ 25 ms | 55 % | mantener |
| `paint/rasterize-800x600` | 1,05 ms | ≤ 25 ms | 4 % | mantener |
| `paint/render-to-png-800x600` | 9,1 ms | ≤ 50 ms | 18 % | mantener |
| `pipeline/html-to-png-800x600` | **133,4 ms** | **≤ 160 ms** | **83,4 %** | **bajar a ≤ 150 ms** (88,9 %; canónica 86,5 %) |
| `render` showcase 1024×768 | **29,5 ms** | **≤ 40 ms** | **73,7 %** | **bajar a ≤ 35 ms** (84,3 %) |
| shell headless showcase | 30,0 ms | ≤ 40 ms | 75 % | mantener 40 (o 35) |
| `website` 1280×900 offline (WIP) | 36,9 ms | — | — | sin presupuesto hasta completar assets |
| gate de conformidad (runtime) | 110,3 ms | ≤ 200 ms | 55 % | mantener (r11) |

**Propuesta**: **pipeline ≤150 ms** (actual 133,4 = 88,9 %; la pasada canónica
129,8 = 86,5 %) y **render showcase ≤35 ms** (actual 29,5 = 84,3 %). Mantener
`build-display-list` ≤25 ms. El render básico (9,6 ms) y el shell (9,9 ms) ya
están muy holgados. El website real aún no tiene presupuesto: sus assets están a
medias y el PNG de 1280×900 es el mayor coste (46 %), lo que apunta a **PNG
dinámico/por regiones** como siguiente foco de pintado.

### 10.1 ¿Qué domina ahora?

Reparto del pipeline r12 (mediana, sumando etapas del bench):

| Etapa | ms | % del pipeline |
|---|---:|---:|
| layout (IFC ≈34 + bloque/tablas/absolutos) | 51,3 | **38 %** |
| CSS parse + cascada | 28,8 | 22 % |
| HTML parse | 21,6 | 16 % |
| display list | 13,6 | 10 % |
| PNG encoder + raster | 9,1 | 7 % |
| margen/medición | ~9 | 7 % |

**Siguiente foco**: (1) **IFC ~34 ms** y estilos propios (`Arc<ComputedStyle>`
ya comparte estilo; quedan allocs del IFC); (2) **parse `massive-end-p`**
(21,9 ms, flagged desde r11); (3) **PNG dinámico/regiones** para páginas
grandes (el encoder es ahora barato en 800×600, pero pesa 46 % en la página real
1280×900); (4) completar los assets de `website/` para convertirlo en benchmark
realista estable.

---

## 11. Comparación histórica

Once baselines en la misma máquina, mismo arnés:

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | **18:51** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | **`7d2ddde`** |
| Árbol | dirty (42) | limpio | limpio | limpio | limpio | limpio | docs dirty | limpio | limpio | docs dirty | **docs dirty + website untracked** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | `43f7748d…` | `a188a1da…` | `c34ff65c…` | **`d655c1f2…`** |
| Load (bench) | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | **0,63–5,1** |

Filas clave (mediana, ms):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | **21.641** |
| `html/nested-div-64k` | 21.262 | 23.534 | 24.050 | 24.564 | 25.658 | 25.084 | 24.772 | 24.022 | 20.396 | 21.469 | **22.466** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | **28.807** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | **51.277** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | **13.648** |
| `paint/rasterize-800x600` | 1 484.776 | 0.861 | 0.932 | 0.899 | 0.890 | 0.843 | 0.993 | 0.881 | 1.082 | 0.925 | **1.052** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | **9.136** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | **133.373** |
| **TOTAL (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** |

### Ronda 11 → ronda 12, fila a fila

| Benchmark | R11 (ms) | R12 (ms) | Factor | Δ |
|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 20.361 | **21.641** | 0,94× | +6,3 % |
| `html/nested-div-10k` | 3.343 | **3.446** | 0,97× | +3,1 % |
| `html/nested-div-32k` | 10.800 | **11.311** | 0,95× | +4,7 % |
| `html/nested-div-64k` | 21.469 | **22.466** | 0,96× | +4,6 % |
| `html/nested-div-64k-default-limits` | 13.369 | **14.047** | 0,95× | +5,1 % |
| `html/massive-end-p` | 21.416 | **21.897** | 0,98× | +2,2 % |
| `html/massive-formatting` | 11.979 | **12.421** | 0,96× | +3,7 % |
| `html/tag-40k-attrs` | 11.633 | **12.682** | 0,92× | +9,0 % |
| `html/pretty-5k-blocks` | 11.061 | **11.646** | 0,95× | +5,3 % |
| `css/parse-10k-rules` | 13.893 | **14.966** | 0,93× | +7,7 % |
| `css/resolve-styles-2k-nodes` | 5.969 | **6.017** | 0,99× | +0,8 % |
| `css/resolve-styles-10k-nodes` | 28.797 | **28.807** | 1,00× | +0,0 % |
| `css/selector-match-simple` | 0.680 | **0.671** | 1,01× | −1,3 % |
| `css/selector-match-descendant` | 1.287 | **1.244** | 1,03× | −3,4 % |
| `layout/realistic-1mb` | 58.818 | **51.277** | **1,15×** | **−12,8 %** ✅ |
| `paint/build-display-list` | 15.056 | **13.648** | 1,10× | −9,3 % |
| `paint/rasterize-800x600` | 0.925 | **1.052** | 0,88× | +13,7 % † |
| `paint/render-to-png-800x600` | 13.146 | **9.136** | **1,44×** | **−30,5 %** ✅ |
| `pipeline/html-to-png-800x600` | 141.841 | **133.373** | 1,06× | **−6,0 %** ✅ |
| **TOTAL (suma)** | **405.845** | **391.748** | **1,04×** | **−3,5 %** |

† `rasterize` no cambió de código en r12: la diferencia es varianza del host
(rango 0,90–1,28 ms). Las filas HTML/CSS tampoco cambiaron de código: +1–9 % es
varianza de ventana (r11 se midió con load más bajo en una sub-ventana). Las
mejoras reales de r12 son layout (Arc) y encoder PNG, tal y como se ve en
`render-to-png` y en la pared del render (§7).

---

## 12. Cómo comparar y detectar regresiones

1. Medir antes/después en la misma máquina y con el sistema ocioso
   (`uptime` < ~1.5); misma `--scale` e iteraciones. **Descartar pasadas con
   load > ~2** (en r12 hubo que descartar 3 de 9; ver §5.3).
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
   típica en reposo ~1–6 %, hasta ±10–15 % en layout/paint con carga.
4. Para complejidad, usar `--scale 0.5` vs `1.0`: ratios ≈1.
5. Pipeline: `render --json` (etapas en µs), goldens (`md5`: showcase
   `c95c7eff…`, phase4 basic `9e957177…`, basic `e943565d…`) y bench PNG
   57 422 B. El encoder debe seguir siendo byte-idéntico.
6. Caso real: `render website/index.html 1280×900 --no-network --json`; el Δ
   con/sin scripting y las etapas deben ser estables; sin `--no-network` el
   tiempo lo domina la red y no es comparable.
7. JSON schema (`megi-bench-v1`): `schema`, `generated_unix_ms`, `commit`,
   `worktree_dirty`, `rustc`, `cpu`, `cores`, `scale`, `iterations`, `warmup`,
   `setup_ms`, `total_median_ms`, `results[]`, `skipped[]`.

---

## 13. CI

Se ejecuta a mano (`./tools/bench.sh`); no hay job para no tocar
`.github/workflows/`. Bench sin dependencias externas:

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

El gate de conformidad (r11) sigue enlazando Boa: build release frío de
`test_harness` ≈31,4 s (+25,3 s por Boa vs bench) y runtime de gate 110 ms.
En r12 no cambia el pipeline de CI; el bench completo tarda ~2,2 s + setup
(~0,2 s).

---

## 14. Limitaciones y notas

- **Host variable**: 3 de 9 pasadas descartadas por load 12,4–12,8; las 6
  válidas se tomaron a load 0,63–5,1. En r12 las filas HTML/CSS difieren +1–9 %
  sin cambios de código: es varianza de ventana. Para publicar una baseline,
  usar load < ~2 o mediana de varias ventanas.
- **HEAD**: la medición de referencia es `7d2ddde`; `a799a24` aterrizó a mitad
  de ventana (solo `crates/render`), por lo que no afecta al bench. Los JSON
  llevan `commit = a799a24` porque el bench se recompiló después (mismo código
  de motor).
- **PNG byte-idéntico**: verificado en 4 artefactos (bench 57 422 B, showcase
  80 220 B, phase4 basic 42 853 B, render basic 21 090 B) con md5 exactos; el
  claim de r12 se cumple.
- **`website/` es del dueño y está untracked/en construcción**: solo se
  renderiza a `/tmp`. Faltan `assets/js/site.js`, `site.webmanifest` y 6
  imágenes; `mark.svg` es SVG (no bitmap) y los badges shields.io son remotos.
  Sin `--no-network` el render lo domina la red (350–420 ms); con
  `--no-network` es determinista (36,9 ms). Cuando se completen los assets,
  revisar el presupuesto y añadirlo como caso realista.
- **IFC ~34 ms**: la cifra viene del análisis del carril de layout, no de un
  desglose instrumentado en este informe; el bench solo da el total de layout.
  El siguiente paso natural es instrumentar/optimizar el IFC y los allocs
  restantes (los estilos ya se comparten con `Arc`).
- **`rasterize` +13,7 % vs r11**: sin cambios de código, varianza del host
  (rango 0,90–1,28 ms). El encoder sí mejora de forma reproducible (−30,5 % en
  dos ventanas).
- **Fast path de tablas (r7)**: la comparación 67,2 → 61,4 ms usa el valor
  intermedio documentado por el commit de layout; no hay un commit compilable
  con el camino avanzado sin fast path.
