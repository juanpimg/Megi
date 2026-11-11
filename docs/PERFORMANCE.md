# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-11, 02:07–02:35 (Europe/Madrid). Ronda 24:
  `object-fit`/`object-position`, `backdrop-filter`, `aspect-ratio` en layout,
  JPEG progresivo completo (color), WebP VP8L subset + wiring en render, sesión
  persistente y descargas del shell, APIs JS y targets de fuzz.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `0b3adee`**. Árbol con
  WIP ajeno solo en docs (`CHANGELOG.md`, `CONTRIBUTING.md`, `README.md`,
  `SECURITY.md`, `ARCHITECTURE.md`, `FUZZING.md`); motor medido `0b3adee`.
- **Ventana de bench**: **load 1,32–1,38** en las 5 pasadas válidas, `powersave`
  con frecuencia máxima muestreada **3,56–4,65 GHz**; render/shell a load ~1,4.
- **Binarios**: bench `37724ea3…` (5 172 240 B), render `c15b4b9e…` (20 669 880
  B), shell `420c4bc4…` (24 929 576 B, con `--chrome`/`--find`/`--save-png`).
- **Artefactos**: `target/bench-r24-run1..5.json`,
  `target/bench-aggregate-r24.json` (`sha256 = 03d1c180…`),
  `target/bench-r24/ab_r23_r24.json`, `target/bench-r24-feature-costs.json`,
  `target/bench-r24-render-shell.json`, `target/bench-latest.txt`/`-meta.txt`
  (canónica, TOTAL 490.951).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–24; la 14 no publicó baseline).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. La matriz incluye las filas E2E de r20 (gradients/grid/vars-calc/
clipping/sticky) y las de **decodificación de imagen** de r22
(PNG/JPEG/GIF 800×600 con fixtures commiteadas e `include_bytes!`).

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5; *heavy* 3 — las filas de decode son heavy).
- **La tabla de §3 es la mediana, por fila, de 5 pasadas completas válidas**
  (load 1,32–1,38; `setup_ms` ~400–430 ms por E2E/decode). La pasada canónica de
  `./tools/bench.sh` (`TOTAL 490.951 ms`, 27 filas) sirve de fingerprint.
- **A/B intercalado prístino**: `git archive d1f3121` (r23) + bench de esa ronda
  (`target/r23-pristine`) vs binario r24, 5 pares alternando orden, load
  1,41–1,57 (§5.2). El primer intento de A/B se descartó por un pico de load
  ajeno (9,8–24,8).
- Render/shell: 7 ejecuciones intercaladas; también las fixtures nuevas
  `aspect_ratio.html`, `object_fit.html`, `backdrop_filter.html` frente a sus
  equivalentes **sin las propiedades** (temporales en `target/bench-r24/`).
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E), 20 hilos |
| RAM / SO | 15 GiB · CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Governor / térmica | `powersave`; freq máx. 3,56–4,65 GHz; paquete ~50–55 °C |
| Load | **1,32–1,38** bench; ~1,4 render/shell |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

./tools/bench.sh                                  # pasada canónica
for i in 1 2 3 4 5; do target/release/bench --json > target/bench-r24-run$i.json; done

# A/B prístino r23 vs r24
git archive d1f3121 | tar -x -C target/r23-pristine
cd target/r23-pristine && CARGO_TARGET_DIR="$PWD/target" cargo build --release -p bench

# Coste de las fixtures nuevas (con y sin las propiedades)
target/release/render tests/fixtures/render/aspect_ratio.html   /tmp/a.png --width 800 --height 600 --json
target/release/render target/bench-r24/aspect_ratio_noprops.html /tmp/b.png --width 800 --height 600 --json

# Render/shell/chrome/find/save-png
target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png --width 1024 --height 768
target/release/megi tests/fixtures/render/showcase/index.html \
    --screenshot /tmp/c.png --width 1024 --height 768 --chrome
target/release/megi tests/fixtures/render/find.html \
    --screenshot /tmp/f.png --width 800 --height 600 --find megi
target/release/megi tests/fixtures/render/showcase/index.html \
    --screenshot /tmp/sp.png --save-png /tmp/saved.png --width 1024 --height 768
```

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb`, `nested-div-*`, `massive-*`, `tag-40k-attrs`, `pretty-5k-blocks` | Parse/tree construction. |
| css | `parse-10k-rules`, `resolve-styles-2k/10k-nodes`, `selector-match-*`, `vars-calc-10k` | Parse, cascada, matching, custom properties/calc. |
| layout | `layout/realistic-1mb`, `layout/sticky-text-1mb` | Layout; sticky + text-transform. |
| paint | `build-display-list`, `rasterize-800x600`, `render-to-png-800x600`, `clipping-800x600` | Display list, raster, PNG, clipping. |
| pipeline | `html-to-png-800x600`, `gradients-800x600`, `grid-800x600` | Pipeline completo. |
| image | `decode-png-800x600`, `decode-jpeg-800x600`, `decode-gif` | Decode completo a RGBA8. |
| fonts | (`feature fonts`) | No re-medido. |

---

## 3. Baseline vigente (ronda 24, HEAD `0b3adee`)

Mediana de **5 pasadas** en calma (load 1,32–1,38). `setup_ms` ~400–430 ms.
**Filas legacy (19)**: `TOTAL 307.687 ms`; **todas (27)**: `TOTAL 485.837 ms`;
pasada canónica: `TOTAL 490.951 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `html/realistic-1mb` | **16.171** | 15.636–16.620 |
| `html/nested-div-10k` | **2.653** | 2.443–2.704 |
| `html/nested-div-32k` | **8.405** | 8.264–8.469 |
| `html/nested-div-64k` | **16.666** | 15.761–16.846 |
| `html/nested-div-64k-default-limits` | **9.891** | 9.782–9.927 |
| `html/massive-end-p` | **17.123** | 16.521–17.420 |
| `html/massive-formatting` | **10.335** | 10.117–10.377 |
| `html/tag-40k-attrs` | **9.338** | 8.979–9.431 |
| `html/pretty-5k-blocks` | **8.628** | 8.308–8.736 |
| `css/parse-10k-rules` | **13.258** | 12.671–13.379 |
| `css/resolve-styles-2k-nodes` | **5.602** | 5.373–5.719 |
| `css/resolve-styles-10k-nodes` | **27.085** | 26.300–27.189 |
| `css/selector-match-simple` | **0.674** | 0.651–0.680 |
| `css/selector-match-descendant` | **1.278** | 1.253–1.288 |
| `layout/realistic-1mb` | **34.900** | 34.227–35.247 |
| `paint/build-display-list` | **7.982** | 7.937–8.009 |
| `paint/rasterize-800x600` | **0.949** | 0.880–1.107 |
| `paint/render-to-png-800x600` | **8.775** | 8.464–8.866 |
| `pipeline/html-to-png-800x600` | **107.974** | 106.302–109.349 |
| `layout/sticky-text-1mb` | **31.387** | 31.196–31.562 |
| `pipeline/gradients-800x600` | **51.455** | 49.291–51.968 |
| `pipeline/grid-800x600` | **5.731** | 5.728–5.830 |
| `css/vars-calc-10k` | **20.559** | 20.293–20.942 |
| `paint/clipping-800x600` | **48.512** | 46.826–49.134 |
| `image/decode-png-800x600` | **6.524** | 6.454–6.584 |
| `image/decode-jpeg-800x600` | **11.014** | 10.284–11.128 |
| `image/decode-gif` | **2.968** | 2.830–3.044 |

> **Aviso**: el TOTAL (legacy) suma etapas y pipeline end-to-end (solapados); no
> es un presupuesto. En r24 **todos los presupuestos se cumplen con margen**
> (ver §11).

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 2.65 · 32k: 8.41 · 64k: 16.67 ms | **Lineal** (3,17× → 3,15×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 13.26 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 5.60 / **27.09 ms** | +6,3 % vs r23 (coste estructural de las features r24; ver §5.2). |
| `layout/realistic-1mb` | 28 195 elementos → **34.90 ms** | −0,4 % vs r23; estable. |
| `image/decode-*` | PNG 6,52 · JPEG 11,01 · GIF 2,97 ms | Estables (±2 %). |
| `pipeline` | **107,97 ms** (106,3–109,3) | +1,4 % vs r23; margen 32,0 ms bajo ≤140. |

---

## 5. Ronda 24: features y verificación

### 5.1 Qué entró

- `9ad3391 feat(layout)`: **aspect-ratio** consumido en block/replaced/flex/grid
  (geometría; ver `crates/layout/tests/aspect_ratio.rs`).
- `c444e96 feat(image,render)`: **JPEG progresivo color completo**, **WebP VP8L
  subset** y wiring de imágenes en render.
- `0f3f080 feat(shell)`: **sesión persistente** y **descargas PNG/HTML**.
- `1f58102 test(fuzz)`: targets de **imagen** (JPEG/GIF/PNG/WebP/EXIF) y **HTTP/2**
  (HPACK/frames).
- `0b3adee`: snapshot de estado. Además `object-fit`/`backdrop-filter` y APIs JS
  entran vía css/paint/js (fixtures nuevas en `tests/fixtures/render/`).

### 5.2 Verificación: A/B por carril y presupuestos

A/B intercalado de archivos prístinos r23 (`d1f3121`) vs r24 (`0b3adee`),
5 pares, load 1,41–1,57:

| Fila | r23 (ms) | r24 (ms) | Δ (A/B) |
|---|---:|---:|---:|
| `html/realistic-1mb` | 16,76 | 16,68 | **−0,5 %** |
| `css/parse-10k-rules` | 12,83 | 13,19 | +2,8 % |
| `css/resolve-styles-2k-nodes` | 5,35 | 5,55 | **+3,8 %** |
| `css/resolve-styles-10k-nodes` | 25,16 | 26,85 | **+6,7 %** |
| `layout/realistic-1mb` | 33,86 | 34,16 | +0,9 % |
| `paint/build-display-list` | 7,66 | 7,56 | −1,3 % |
| `paint/rasterize-800x600` | 0,92 | 0,93 | +1,6 % |
| `paint/render-to-png-800x600` | 8,51 | 8,50 | **−0,1 %** |
| `pipeline/html-to-png-800x600` | 105,42 | 108,49 | **+2,9 %** |

**Lectura**: las features r24 no añaden coste estructural apreciable salvo un
**+6,7 % en css resolve** (y +2,9 % en pipeline) atribuible al trabajo genérico
de las propiedades/APIs nuevas sobre cada nodo; layout, display list y raster
están planos. Aun así, los presupuestos conservan márgenes amplios. **No se
suben los presupuestos.**

| Medida | r24 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **107,97 ms** | **≤ 140 ms** | 77,1 % | 32,0 ms | ✅ |
| `layout/realistic-1mb` | **34,90 ms** | **≤ 45 ms** | 77,6 % | 10,1 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **27,09 ms** | **≤ 35 ms** | 77,4 % | 7,9 ms | ✅ |
| `render` showcase 1024×768 | **27,72 ms** | **≤ 35 ms** | 79,2 % | 7,3 ms | ✅ |
| shell headless showcase | 28,22 ms | ≤ 35 ms | 80,6 % | 6,8 ms | ✅ |
| `paint/render-to-png-800x600` | **8,78 ms** | (ref. ~8,5) | — | +0,3 ms | ✅ |
| `paint/build-display-list` | **7,98 ms** | **≤ 25 ms** | 31,9 % | 17,0 ms | ✅ |
| `html/realistic-1mb` | 16,17 ms | ≤ 100 ms | 16,2 % | 83,8 ms | ✅ |

### 5.3 Nota de integridad

El primer A/B y la primera pasada canónica se descartaron por picos de load
ajenos (9,8–24,8 y 14,9). Todos los números publicados son de la ventana
1,32–1,57. Árbol con WIP ajeno solo en docs; motor medido `0b3adee`.

---

## 6. Coste de las fixtures nuevas (r24)

Wall time del binario `render` (7 ejecuciones, load ~1,4) frente a la misma
fixture **sin las propiedades** (temporales en `target/bench-r24/*_noprops.html`,
mismos assets por symlink/copia):

| Fixture | con props | sin props | Δ | display items | notas |
|---|---:|---:|---:|---:|---|
| `aspect_ratio.html` | **7,90 ms** | 7,27 ms | **+0,63 ms (+8,7 %)** | 7 / 7 | ratio aplica geometría sin ítems extra. |
| `object_fit.html` | **9,08 ms** | 9,40 ms | **−0,32 ms (−3,4 %)** | 28 / 16 | dentro de ruido; 7 imágenes, `object-fit` añade ítems de pintado. |
| `backdrop_filter.html` | **11,15 ms** | 9,74 ms | **+1,41 ms (+14,5 %)** | 122 / 120 | blur + brightness/grayscale sobre fondo con imagen. |

Coste absoluto pequeño (<1,5 ms) y fuera de la ruta caliente de los benchmarks
core; no afecta a los presupuestos. `--save-png` (shell): **66,05 ms** frente a
28,22 ms sin el flag (**+37,8 ms**), PNG idéntico (`c95c7eff…`); es coste del
guardado adicional, anotado para revisar si se abarata.

---

## 7. Codificador PNG (sin cambios)

| Métrica | r23 | r24 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.626 | **8.775** | +1,7 % (varianza) |
| — rasterizar | 0.912 | **0.949** | +4,1 % |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden phase4 PNG | 42 853 B | **42 853 B** | `9e957177…` |
| Golden `basic.rgba` | — | **1 920 000 B** | `60f8984e…` |

---

## 8. Pipeline `render`, shell, chrome, find y zoom (r24)

7 ejecuciones intercaladas (load ~1,4; goldens verificados):

| Fixture / modo | pared r24 | pared r23 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **27,72 ms** | 27,27 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,59 ms** | 9,00 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **28,22 ms** | 29,24 | 80 220 B (idéntico) |
| showcase `--chrome` | **47,28 ms** | 44,26 | 85 746 B (`cbfc6087…`) |
| `find.html` con `--find megi` | **18,04 ms** | 17,31 | 29 950 B (`a818f32a…`) |
| showcase `--save-png` (además de `--screenshot`) | **66,05 ms** | — | 80 220 B (idéntico) |

El chrome headless cuesta **+19,1 ms** en showcase; `--find` queda en 18,04 ms;
`--save-png` añade **+37,8 ms** (segunda escritura/encode del PNG). Los goldens
sin chrome siguen byte-idénticos.

---

## 9. Filas E2E y decode

| Fila | r24 (ms) | r23 (ms) | Δ |
|---|---:|---:|---:|
| `pipeline/gradients-800x600` | **51.455** | 51.336 | +0,2 % |
| `pipeline/grid-800x600` | **5.731** | 5.818 | −1,5 % |
| `css/vars-calc-10k` | **20.559** | 19.110 | +7,6 % |
| `paint/clipping-800x600` | **48.512** | 48.732 | −0,5 % |
| `layout/sticky-text-1mb` | **31.387** | 31.285 | +0,3 % |
| `image/decode-png-800x600` | **6.524** | 6.506 | +0,3 % |
| `image/decode-jpeg-800x600` | **11.014** | 10.950 | +0,6 % |
| `image/decode-gif` | **2.968** | 2.911 | +2,0 % |

Decode estable; `vars-calc` sube +7,6 % por el mismo coste de cascada que §5.2.
Filtros: `--filter pipeline`, `--filter css/vars-calc`, `--filter paint/clipping`,
`--filter layout/sticky`, `--filter image`.

---

## 10. Clipping, `var()`/`calc()`, net, imagen y fuzz (resumen)

- **Clipping/`clip:text`**: caps `MAX_ROUNDED_GROUP_PIXELS = 16 777 216`,
  `MAX_TEXT_MASK_RUNS = 4096`, `MAX_SHADOWS = 32`, `MAX_SHADOW_BLUR = 64`,
  `MAX_BACKGROUND_TILES = 4096`, `MAX_STACKING_CONTEXT_DEPTH = 256`,
  `MAX_FLOAT_DEPTH = 64`.
- **Net HTTP/2**: MVP + flow control/multiplexación; coste ≈0 en el bench;
  pendiente validar contra un servidor real.
- **Imagen**: EXIF orientation, JPEG progresivo **completo (color)** y WebP
  VP8L **subset** + wiring en render; VP8 lossy y VP8L completo pendientes.
- **Fuzz**: 7 targets (incluye imagen y HTTP/2 nuevos); build-only en PRs;
  siguiente paso: campañas con corpus y `cargo fuzz` en CI.

---

## 11. Presupuestos: veredicto

| Medida | r24 | Presupuesto | % | Margen | Veredicto |
|---|---:|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **107,97 ms** | **≤ 140 ms** | 77,1 % | 32,0 ms | ✅ |
| `layout/realistic-1mb` | **34,90 ms** | **≤ 45 ms** | 77,6 % | 10,1 ms | ✅ |
| `css/resolve-styles-10k-nodes` | **27,09 ms** | **≤ 35 ms** | 77,4 % | 7,9 ms | ✅ |
| `render` showcase 1024×768 | **27,72 ms** | **≤ 35 ms** | 79,2 % | 7,3 ms | ✅ |
| shell headless showcase | 28,22 ms | ≤ 35 ms | 80,6 % | 6,8 ms | ✅ |
| `paint/render-to-png-800x600` | 8,78 ms | ref. ~8,5 | — | +0,3 ms | ✅ |
| `paint/build-display-list` | **7,98 ms** | **≤ 25 ms** | 31,9 % | 17,0 ms | ✅ |
| `html/realistic-1mb` | 16,17 ms | ≤ 100 ms | 16,2 % | 83,8 ms | ✅ |
| `image/decode-*` | 2,97–11,01 ms | — | — | — | referencia §9 |

**No se suben los presupuestos.** Todos se cumplen con márgenes cómodos; el
menor margen relativo sigue en css resolve (22,6 %) y showcase (20,8 %), el
menor absoluto en render-to-png (+0,3 ms frente a la referencia). El coste
estructural de r24 es pequeño (+6,7 % css resolve, +2,9 % pipeline) y está
cuantificado en §5.2; las fixtures nuevas están en §6. Siguiente foco:
**cablear providers JS en render**, **WebP VP8L completo**, **ES modules**,
**PNG por regiones** y **campañas de fuzz**; mantener A/B intercalado antes de
cualquier cambio de presupuesto.

---

## 12. Comparación histórica

Veintidós baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | 00:34 | 01:18 | **02:07** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | `00d8388` | `d1f3121` | **`0b3adee`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | 1,20–1,23 | 1,76–1,86 | **1,32–1,38** |

Filas clave (mediana, ms; r22–r24 incluyen filas E2E/decode que no entran en el
TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 | R23 | R24 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | 16.847 | 16.566 | **16.171** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | 35.805 | 25.477 | **27.085** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | 37.069 | 35.043 | **34.900** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | 8.973 | 7.940 | **7.982** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | 8.354 | 8.626 | **8.775** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | 141.194 | 106.440 | **107.974** |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** | **305.461** | **307.687** |

### Ronda 23 → ronda 24, fila a fila

| Benchmark | R23 (ms) | R24 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 16.566 | **16.171** | −2,4 % |
| `html/nested-div-10k` | 2.683 | **2.653** | −1,1 % |
| `html/nested-div-32k` | 8.584 | **8.405** | −2,1 % |
| `html/nested-div-64k` | 17.162 | **16.666** | −2,9 % |
| `html/nested-div-64k-default-limits` | 10.167 | **9.891** | −2,7 % |
| `html/massive-end-p` | 17.058 | **17.123** | +0,4 % |
| `html/massive-formatting` | 10.665 | **10.335** | −3,1 % |
| `html/tag-40k-attrs` | 9.304 | **9.338** | +0,4 % |
| `html/pretty-5k-blocks` | 8.581 | **8.628** | +0,5 % |
| `css/parse-10k-rules` | 13.130 | **13.258** | +1,0 % |
| `css/resolve-styles-2k-nodes` | 5.276 | **5.602** | +6,2 % |
| `css/resolve-styles-10k-nodes` | 25.477 | **27.085** | **+6,3 %** |
| `css/selector-match-simple` | 0.639 | **0.674** | +5,5 % |
| `css/selector-match-descendant` | 1.208 | **1.278** | +5,8 % |
| `layout/realistic-1mb` | 35.043 | **34.900** | −0,4 % |
| `paint/build-display-list` | 7.940 | **7.982** | +0,5 % |
| `paint/rasterize-800x600` | 0.912 | **0.949** | +4,1 % |
| `paint/render-to-png-800x600` | 8.626 | **8.775** | +1,7 % |
| `pipeline/html-to-png-800x600` | 106.440 | **107.974** | +1,4 % |
| `pipeline/gradients-800x600` | 51.336 | **51.455** | +0,2 % |
| `pipeline/grid-800x600` | 5.818 | **5.731** | −1,5 % |
| `css/vars-calc-10k` | 19.110 | **20.559** | +7,6 % |
| `paint/clipping-800x600` | 48.732 | **48.512** | −0,5 % |
| `layout/sticky-text-1mb` | 31.285 | **31.387** | +0,3 % |
| `image/decode-png-800x600` | 6.506 | **6.524** | +0,3 % |
| `image/decode-jpeg-800x600` | 10.950 | **11.014** | +0,6 % |
| `image/decode-gif` | 2.911 | **2.968** | +2,0 % |
| **TOTAL legacy** | **305.461** | **307.687** | **+0,7 %** |

---

## 13. Cómo comparar y detectar regresiones

1. Medir con load < 2 y comprobar frecuencia (en esta ronda: 1,32–1,38, freq
   máx. 3,56–4,65 GHz). Si no hay ventana, marcarlo.
2. Para atribuir un delta a un commit, compilar dos archivos prístinos
   (antes/después) y ejecutar los binarios **intercalados** (mismo load para
   ambos), como en §5.2.
3. Guardar JSON y comparar medianas:

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
4. Regresión si la mediana empeora >15 % reproducible. En r24 todos los
   presupuestos se cumplen con margen (§11); el coste nuevo está en §5–§6.
5. Filas E2E/decode: `--filter pipeline`, `css/vars-calc`, `paint/clipping`,
   `layout/sticky`, `image`. Referencias §6 y §9.
6. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   phase4 `9e957177…` 42 853 B, `basic.rgba` `60f8984e…` 1 920 000 B, chrome
   `cbfc6087…` 85 746 B, find `a818f32a…` 29 950 B.

---

## 14. Limitaciones y notas

- **Ventana buena** (load 1,32–1,38, freq 3,56–4,65 GHz; render/shell ~1,4); dos
  intentos previos descartados por picos de load ajenos (9,8–24,8).
- **WIP ajeno en docs**; el motor medido es `0b3adee`. El árbol tenía 6 docs
  modificados por otros agentes, ninguno del motor.
- **Coste de features r24**: cuantificado en §6 con fixtures propias; el
  `--save-png` del shell cuesta +37,8 ms (anotado para revisar).
- **Filas de decode**: fixtures deterministas commiteadas con `include_bytes!`,
  decode completo a RGBA8, heavy (3 iteraciones).
- **Imagen r24**: JPEG progresivo color completo y WebP VP8L subset; VP8 lossy y
  VP8L completo pendientes. `--chrome` del shell +19,1 ms en showcase.
- **`website/` y `pages.yml`**: del dueño; no se tocan.
- **Fast path de tablas (r7)**: nota histórica; no hay commit compilable con el
  camino avanzado sin fast path.
