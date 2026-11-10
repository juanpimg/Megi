# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-11, 00:34–00:55 (Europe/Madrid). Ronda 22:
  **grid completo**, **filtros CSS/blend modes/image-set**, **browser chrome
  del shell** y **HTTP/2 MVP** (net).
- **Repo**: `Megi`, rama `sprint/robustez-fase4`, **HEAD `00d8388`** (engine);
  árbol con solo los cambios de esta tarea en `crates/bench`/`tools/bench.sh`
  (y `Cargo.lock` por añadir `image` al bench), más WIP de otros agentes en docs.
- **Ventana de bench**: **load 1,20–1,23** en las 5 pasadas; governor
  `powersave` con frecuencia máxima muestreada **4,2–4,7 GHz**; `render`/shell
  medidos con load ~3 (valores aproximados, goldens byte-idénticos).
- **Binarios**: bench `0809cb62…` (5 113 608 B, incluye las fixtures de imagen;
  el script lo recompiló con la misma fuente: `3986a66a…`),
  render `fc12b91f…`, shell `dc67bb5b…` (con `--chrome`/`--find`).
- **Artefactos** (gitignored): `target/bench-r22-run1..10.json` (6–10 válidas),
  `target/bench-aggregate-r22.json` (`sha256 = f4b53f1e…`),
  `target/bench-r22/ab_r21_r22.json`, `target/bench-r22-render-shell.json`,
  `target/bench-latest.txt`/`-meta.txt` (pasada canónica, TOTAL 572.504).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §13 (rondas 2–22; la 14 no publicó baseline).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas del bench se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. Ronda 22 añadió las filas de **decodificación de imagen** (§8) con
fixtures commiteadas cargadas con `include_bytes!`; el resto de la matriz sigue
igual (filas E2E de r20 incluidas).

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5; *heavy* 3 — las filas de decode son heavy).
- **La tabla de §3 es la mediana, por fila, de 5 pasadas completas válidas**
  (load 1,20–1,23; `setup_ms` ~390–420 ms por las fixtures E2E/decode). Las
  primeras 5 pasadas del intento inicial (load 4–10) se descartan.
- La pasada canónica de `./tools/bench.sh` (`TOTAL 572.504 ms`, 27 filas) sirve
  de fingerprint (load ~1,3).
- **A/B intercalado prístino**: `git archive a8ce8ba` (r21) vs `00d8388` +
  parche de bench (r22), 5 pares alternando orden, mismo load (§5.2).
- Render/shell: 9 ejecuciones intercaladas; `--find`/`--chrome` 7–9.
- `--scale` multiplica los tamaños (p. ej. `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E), 20 hilos |
| RAM / SO | 15 GiB · CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Governor / térmica | `powersave`; freq máx. 4,2–4,7 GHz; paquete ~50–55 °C |
| Load | **1,20–1,23** bench; ~3 render/shell |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

./tools/bench.sh                                  # pasada canónica
for i in 1 2 3 4 5; do target/release/bench --json > target/bench-r22-run$i.json; done

# Filas nuevas de decodificación
target/release/bench --filter image

# A/B prístino r21 vs r22 (atribución)
git archive a8ce8ba | tar -x -C target/r21-clean-src
cd target/r21-clean-src && CARGO_TARGET_DIR="$PWD/target" cargo build --release -p bench

# Render/shell/chrome/find
target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png --width 1024 --height 768
target/release/megi tests/fixtures/render/showcase/index.html \
    --screenshot /tmp/c.png --width 1024 --height 768 --chrome
target/release/megi tests/fixtures/render/find.html \
    --screenshot /tmp/f.png --width 800 --height 600 --find megi
```

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb`, `nested-div-*`, `massive-*`, `tag-40k-attrs`, `pretty-5k-blocks` | Parse/tree construction. |
| css | `css/parse-10k-rules`, `resolve-styles-2k/10k-nodes`, `selector-match-*`, `vars-calc-10k` | Parse, cascada, matching, custom properties/calc. |
| layout | `layout/realistic-1mb`, `layout/sticky-text-1mb` | Layout; sticky + text-transform. |
| paint | `build-display-list`, `rasterize-800x600`, `render-to-png-800x600`, `clipping-800x600` | Display list, raster, PNG, clipping. |
| pipeline | `html-to-png-800x600`, `gradients-800x600`, `grid-800x600` | Pipeline completo. |
| **image** | **`decode-png-800x600`, `decode-jpeg-800x600`, `decode-gif`** | **Decode completo a RGBA8 (r22).** |
| fonts | (`feature fonts`) | No re-medido. |

---

## 3. Baseline vigente (ronda 22, HEAD `00d8388`)

Mediana de **5 pasadas** en calma (load 1,20–1,23). `setup_ms` ~390–420 ms.
**Filas legacy (19)**: `TOTAL 356.476 ms`; **todas (27)**: `TOTAL 560.251 ms`;
pasada canónica: `TOTAL 572.504 ms` (pipeline 144,955; css resolve 36,924;
decode PNG/JPEG/GIF 6,552/11,110/3,074).

| Benchmark | mediana (ms) | rango 5 pasadas (ms) |
|---|---:|---:|
| `html/realistic-1mb` | **16.847** | 15.99–16.92 |
| `html/nested-div-10k` | **2.781** | 2.66–2.80 |
| `html/nested-div-32k` | **8.597** | 8.43–8.65 |
| `html/nested-div-64k` | **17.083** | 16.89–17.23 |
| `html/nested-div-64k-default-limits` | **10.006** | 9.93–10.13 |
| `html/massive-end-p` | **17.123** | 16.86–17.43 |
| `html/massive-formatting` | **10.325** | 10.16–10.44 |
| `html/tag-40k-attrs` | **9.850** | 9.32–10.19 |
| `html/pretty-5k-blocks` | **8.443** | 8.31–8.55 |
| `css/parse-10k-rules` | **13.461** | 13.30–14.17 |
| `css/resolve-styles-2k-nodes` | **7.638** | 7.35–7.95 |
| `css/resolve-styles-10k-nodes` | **35.805** | 34.75–36.64 |
| `css/selector-match-simple` | **0.659** | 0.62–0.69 |
| `css/selector-match-descendant` | **1.306** | 1.20–1.51 |
| `layout/realistic-1mb` | **37.069** | 36.11–37.75 |
| `paint/build-display-list` | **8.973** | 8.18–9.03 |
| `paint/rasterize-800x600` | **0.962** | 0.94–1.00 |
| `paint/render-to-png-800x600` | **8.354** | 8.31–8.65 |
| `pipeline/html-to-png-800x600` | **141.194** | 136.18–144.55 |
| `layout/sticky-text-1mb` | **31.044** | 30.71–31.44 |
| `pipeline/gradients-800x600` | **49.881** | 49.06–50.38 |
| `pipeline/grid-800x600` | **5.737** | 5.69–5.79 |
| `css/vars-calc-10k` | **48.087** | 47.50–48.36 |
| `paint/clipping-800x600` | **48.775** | 47.51–49.45 |
| `image/decode-png-800x600` | **6.495** | 6.45–6.64 |
| `image/decode-jpeg-800x600` | **10.877** | 10.61–11.00 |
| `image/decode-gif` | **2.879** | 2.87–2.91 |

> **Aviso**: el TOTAL (legacy) suma etapas y pipeline end-to-end (solapados); no
> es un presupuesto. En r22 **pipeline y css resolve exceden ligeramente** sus
> presupuestos (ver §12).

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 2.78 · 32k: 8.60 · 64k: 17.08 ms | **Lineal** (3,2× → 3,09×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 13.46 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 7.64 / **35.81 ms** | +5,5 % vs r21 (filtros/blend/image-set añaden propiedades computadas). |
| `layout/realistic-1mb` | 28 195 elementos → **37,07 ms** | +4,4 % vs r21; ≤45. |
| `image/decode-*` | PNG 6,50 · JPEG 10,88 · GIF 2,88 ms | JPEG domina; GIF es el más barato (paleta). |
| `pipeline` | **141,19 ms** (136,2–144,6) | +5,9 % vs r21; roza el presupuesto. |

---

## 5. Ronda 22: features y verificación de no-regresión

### 5.1 Qué entró

- `b5b7e3c feat(css,layout,paint)`: **grid completo** (áreas/líneas/dense),
  **filtros CSS** (`filter`/`backdrop-filter`), **blend modes** e **image-set**.
- `cc567ec feat(shell)`: **browser chrome** (address bar, tab strip, select).
- `93f2b40 feat(net)`: **HTTP/2 MVP** (HPACK, framing, ALPN, fallback h1).
- `00d8388`: snapshot de estado (sin cambios de motor).

### 5.2 ¿Rompe la ronda 22 los presupuestos? — Parcialmente

A/B intercalado de archivos prístinos r21 (`a8ce8ba`) vs r22 (`00d8388` +
parche de bench), 5 pares, load ~3:

| Fila | r21 (ms) | r22 (ms) | Δ (A/B) | Δ medidos |
|---|---:|---:|---:|---:|
| `html/realistic-1mb` | 17,20 | 17,20 | **+0,0 %** | +0,8 % |
| `css/parse-10k-rules` | 14,30 | 13,41 | −6,2 % | −4,8 % |
| `css/resolve-styles-2k-nodes` | 7,39 | 7,62 | +3,1 % | +6,8 % |
| `css/resolve-styles-10k-nodes` | 35,42 | 36,91 | **+4,2 %** | **+5,5 %** |
| `layout/realistic-1mb` | 36,40 | 37,02 | +1,7 % | +4,4 % |
| `paint/build-display-list` | 7,98 | 9,12 | **+14,4 %** | +9,7 % |
| `paint/rasterize-800x600` | 0,97 | 0,97 | −0,9 % | +3,1 % |
| `paint/render-to-png-800x600` | 8,62 | 8,73 | +1,3 % | −1,3 % |
| `pipeline/html-to-png-800x600` | 137,85 | 145,19 | **+5,3 %** | **+5,9 %** |

**Atribución**: las features nuevas no aparecen en las fixtures del bench, pero
añaden **coste estructural genérico** en la display list (+14,4 %: preparación
de filtros/blend/image-set por caja) y en la cascada (+4,2 %: más propiedades
computadas por nodo). Eso empuja el pipeline +5,3 % y deja **dos presupuestos al
límite o por encima**:

| Presupuesto | r21 | r22 | Límite | % | Veredicto |
|---|---:|---:|---:|---:|---|
| pipeline | 133,36 | **141,19** | ≤140 | **100,9 %** | ❌ **EXCEDIDO (marginal)** |
| css resolve-10k | 33,94 | **35,81** | ≤35 | **102,3 %** | ❌ **EXCEDIDO (marginal)** |
| layout | 35,49 | **37,07** | ≤45 | 82,4 % | ✅ |
| render-to-png | 8,46 | **8,35** | ≤50 | 16,7 % | ✅ |
| html/realistic | 16,72 | **16,85** | ≤100 | 16,8 % | ✅ |
| render showcase | 27,11 | **28,17** | ≤35 | 80,5 % | ✅ |
| display-list | 8,18 | **8,97** | ≤25 | 35,9 % | ✅ |

Con la varianza de ±2–4 % en filas sensibles, el pipeline está en **136–145 ms**
(3 de 5 pasadas por encima de 140) y css resolve en **34,7–36,6 ms** (3 de 5 por
encima de 35): el veredicto honesto es **cumple la mayoría, pero pipeline y css
resolve quedan al límite/por encima** por el coste estructural de r22. No se
suben los presupuestos; el siguiente paso es optimizar el camino genérico
(filtros/blend/image-set en display list y propiedades computadas) y re-medir.

### 5.3 Nota de integridad

El árbol de trabajo tenía solo los cambios de esta tarea (`crates/bench`,
`tools/bench.sh`, `Cargo.lock` por `image`) más WIP ajeno en docs; el motor
medido es `00d8388`. El A/B se hizo con archivos **prístinos** de `a8ce8ba` y
`00d8388` (+ parche de bench) para no mezclar WIP.

---

## 6. Codificador PNG (sin cambios)

| Métrica | r21 | r22 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.461 | **8.354** | −1,3 % |
| — rasterizar | 0.933 | **0.962** | +3,1 % |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden `--zoom 2` | 3 202 B | **3 202 B** | `4792779d…` |

---

## 7. Pipeline `render`, shell, chrome, find y zoom (r22)

9 ejecuciones intercaladas (load ~3; goldens verificados):

| Fixture / modo | pared r22 | pared r21 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **28,17 ms** | 27,11 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,79 ms** | 8,33 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **28,92 ms** | 28,13 | 80 220 B (idéntico) |
| basic (shell headless) | **9,10 ms** | 8,76 | 21 090 B (idéntico) |
| showcase `--chrome` | **45,30 ms** | — | 85 746 B (`cbfc6087…`) |
| basic `--chrome` | **16,75 ms** | — | 23 089 B (`f25f5d93…`) |
| `find.html` sin `--find` | **10,26 ms** | 9,83 | 30 056 B |
| `find.html` con `--find megi` | **19,19 ms** | 16,98 | 29 950 B (`a818f32a…`) |
| basic `--zoom 2` (320×200) | **3,80 ms** | 3,87 | 3 202 B |

El **chrome headless cuesta +17,1 ms** en showcase (address bar + tab strip,
PNG +5,5 KB) y +7,7 ms en basic; `--find` cuesta **+8,9 ms** (índice de texto +
overlay de resaltado; 3 hits, sin re-render al cambiar de hit). Goldens de
render sin chrome siguen byte-idénticos; los PNG de chrome son deterministas.

---

## 8. Filas de decodificación de imagen (nuevas en r22)

Coste real de páginas con imágenes, hasta ahora no medido. Método: fixtures
**commiteadas** cargadas con `include_bytes!` desde el bench (sin red, sin
aleatoriedad); decodificación completa a RGBA8 con los límites por defecto de
`crates/image` y descarte del buffer. Filas *heavy* (3 iteraciones).

| Fila | Fixture | mediana (ms) | rango | throughput |
|---|---|---:|---:|---:|
| `image/decode-png-800x600` | `crates/image/tests/fixtures/png/large_800x600.png` (800×600, 13 947 B) | **6.495** | 6.45–6.64 | 73,9 MPix/s |
| `image/decode-jpeg-800x600` | `crates/bench/fixtures/jpeg-800x600.jpg` (800×600, 76 656 B, baseline Q85 4:2:0) | **10.877** | 10.61–11.00 | 44,1 MPix/s |
| `image/decode-gif` | `crates/bench/fixtures/gif-800x600.gif` (800×600, 84 156 B, 256 colores) | **2.879** | 2.87–2.91 | 166,7 MPix/s |

- PNG: inflate + filtros + un-filter → RGBA; JPEG: baseline con IDCT y upsampling
  (el formato más caro por MPix en este motor); GIF: LZW + paleta (el más barato).
- Las fixtures JPEG/GIF son **deterministas** (generadas con Pillow, receta en
  `crates/bench/fixtures/README.md` y en el rustdoc de `register_image`); el PNG
  es la fixture del propio crate `image`. `--filter image` ejecuta solo estas
  filas; el resto de la suite no las incluye en el TOTAL legacy.

---

## 9. Filas E2E existentes (gradients/grid/vars-calc/clipping/sticky)

| Fila | r22 (ms) | r21 (ms) | Δ |
|---|---:|---:|---:|
| `pipeline/gradients-800x600` | **49.881** | 50.749 | −1,7 % |
| `pipeline/grid-800x600` | **5.737** | 5.828 | −1,6 % |
| `css/vars-calc-10k` | **48.087** | 46.332 | +3,8 % |
| `paint/clipping-800x600` | **48.775** | 49.793 | −2,0 % |
| `layout/sticky-text-1mb` | **31.044** | 30.811 | +0,8 % |

Sin cambios significativos; el aumento de `vars-calc` y pipeline viene del mismo
coste estructural de r22 (§5.2). Filtros: `--filter pipeline`,
`--filter css/vars-calc`, `--filter paint/clipping`, `--filter layout/sticky`,
`--filter image`.

---

## 10. Clipping, `var()`/`calc()`, net y fuzz (resumen)

- **Clipping/`clip:text`**: coste de raster por área; radius+overflow (200×150)
  ≈ +11,1 ms de raster, con `clip:text` ≈+16,7 ms; caps
  `MAX_ROUNDED_GROUP_PIXELS = 16 777 216`, `MAX_TEXT_MASK_RUNS = 4096`,
  `MAX_SHADOWS = 32`, `MAX_SHADOW_BLUR = 64`, `MAX_BACKGROUND_TILES = 4096`,
  `MAX_STACKING_CONTEXT_DEPTH = 256`, `MAX_FLOAT_DEPTH = 64`.
- **`var()`/`calc()`**: `css/vars-calc-10k` en 48,1 ms; el coste estructural de
  la cascada se mide en `css/resolve-*`.
- **Net HTTP/2 MVP** (r22): HPACK completo + framing + ALPN `h2` con fallback
  HTTP/1.1 y tests offline (`cargo test -p net`, 225). Coste en el bench ≈0
  (net no interviene en las fixtures); el limite es no multiplexar/push/flow
  control completo.
- **Fuzz**: no entra en el bench; 7 targets, build-only en PRs (~minutos CI).

---

## 11. Coste de floats, posicionamiento y opacidad (referencia r4)

| Variante | Δ vs control | Factor |
|---|---:|---:|
| `position: relative` | +0,93–1,13 ms | 1,02× |
| `position: relative; z-index` | +0,20–0,59 ms | 1,00–1,01× |
| `opacity: 0.85` | +2,96–3,55 ms | 1,05–1,06× |
| `float: left` | +12,54–12,72 ms | 1,22× |

---

## 12. Presupuestos: veredicto

| Medida | r22 | Presupuesto | % | Veredicto |
|---|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **141,19 ms** | **≤ 140 ms** | **100,9 %** | ❌ **EXCEDIDO (marginal)** |
| `css/resolve-styles-10k-nodes` | **35,81 ms** | **≤ 35 ms** | **102,3 %** | ❌ **EXCEDIDO (marginal)** |
| `layout/realistic-1mb` | **37,07 ms** | **≤ 45 ms** | 82,4 % | ✅ |
| `render` showcase 1024×768 | **28,17 ms** | **≤ 35 ms** | 80,5 % | ✅ |
| shell headless showcase | 28,92 ms | ≤ 35 ms | 82,6 % | ✅ |
| `paint/render-to-png-800x600` | 8,35 ms | ≤ 50 ms | 16,7 % | ✅ |
| `html/realistic-1mb` | 16,85 ms | ≤ 100 ms | 16,8 % | ✅ |
| `paint/build-display-list` | 8,97 ms | ≤ 25 ms | 35,9 % | ✅ |
| `image/decode-*` | 2,88–10,88 ms | — | — | sin presupuesto; referencia §8 |

**No se suben los presupuestos.** El pipeline y css resolve están **por encima
del límite por el coste estructural de r22** (display-list +14,4 %, cascada
+4,2 %, pipeline +5,3 % en A/B). Con la varianza observada, 3 de 5 pasadas
superan el límite y 2 no: es una **violación marginal pero real**. Próximo foco:
(a) evitar trabajo por caja en display list cuando no hay filtros/blend/image-set
(preparación genérica), (b) reducir propiedades computadas por nodo en la
cascada, (c) **decode de imagen en páginas reales** (ahora medido en §8) y
**PNG por regiones** para renders grandes, (d) multiplexado HTTP/2 y perf de
raster; re-medir en calma antes de tocar presupuestos.

---

## 13. Comparación histórica

Veinte baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | 22:52 | 23:48 | **00:34** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | `c6b66b1` | `a8ce8ba` | **`00d8388`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | 0,36–1,8 | 1,29–1,48 | **1,20–1,23** |

Filas clave (mediana, ms; r22 incluye filas E2E/decode que no entran en el TOTAL
legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 | R21 | R22 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | 21.078 | 16.720 | **16.847** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | 39.347 | 33.937 | **35.805** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | 47.097 | 35.492 | **37.069** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | 16.355 | 8.183 | **8.973** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | 8.831 | 8.461 | **8.354** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | 160.135 | 133.357 | **141.194** |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** | **344.199** | **356.476** |

### Ronda 21 → ronda 22, fila a fila

| Benchmark | R21 (ms) | R22 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 16.720 | **16.847** | +0,8 % |
| `html/nested-div-64k` | 16.887 | **17.083** | +1,2 % |
| `html/tag-40k-attrs` | 9.350 | **9.850** | +5,3 % |
| `css/parse-10k-rules` | 14.133 | **13.461** | −4,8 % |
| `css/resolve-styles-2k-nodes` | 7.152 | **7.638** | +6,8 % |
| `css/resolve-styles-10k-nodes` | 33.937 | **35.805** | **+5,5 %** ⚠️ |
| `css/selector-match-descendant` | 1.211 | **1.306** | +7,8 % |
| `layout/realistic-1mb` | 35.492 | **37.069** | +4,4 % |
| `paint/build-display-list` | 8.183 | **8.973** | **+9,7 %** ⚠️ |
| `paint/rasterize-800x600` | 0.933 | **0.962** | +3,1 % |
| `paint/render-to-png-800x600` | 8.461 | **8.354** | −1,3 % |
| `pipeline/html-to-png-800x600` | 133.357 | **141.194** | **+5,9 %** ⚠️ |
| `layout/sticky-text-1mb` | 30.811 | **31.044** | +0,8 % |
| `pipeline/gradients-800x600` | 50.749 | **49.881** | −1,7 % |
| `pipeline/grid-800x600` | 5.828 | **5.737** | −1,6 % |
| `css/vars-calc-10k` | 46.332 | **48.087** | +3,8 % |
| `paint/clipping-800x600` | 49.793 | **48.775** | −2,0 % |
| **TOTAL legacy** | **344.199** | **356.476** | +3,6 % |

---

## 14. Cómo comparar y detectar regresiones

1. Medir con load <2 y comprobar frecuencia (en esta ronda: 1,20–1,23, freq
   máx. 4,2–4,7 GHz). Si no hay ventana, marcarlo.
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
4. Regresión si la mediana empeora >15 % reproducible. En r22 pipeline y css
   resolve están **al límite/por encima** (veredicto §12): vigilarlos.
5. Filas E2E/decode: `--filter pipeline`, `css/vars-calc`, `paint/clipping`,
   `layout/sticky`, `image`. Referencias §8–§9; >15 % = regresión.
6. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   chrome showcase `cbfc6087…` 85 746 B, chrome basic `f25f5d93…` 23 089 B,
   find `a818f32a…` 29 950 B, zoom `4792779d…` 3 202 B.

---

## 15. Limitaciones y notas

- **Ventana buena pero no perfecta**: load 1,20–1,23 en bench (freq 4,2–4,7
  GHz); render/shell a load ~3. El A/B prístino sigue siendo la atribución
  fiable.
- **Veredicto de presupuestos marginal**: pipeline 141,19 (100,9 %) y css
  resolve 35,81 (102,3 %) exceden; 3 de 5 pasadas por encima. Se debe al coste
  estructural de r22 (display-list +14,4 %, cascada +4,2 %) y no a las fixtures,
  que no usan las features nuevas. **No se suben los presupuestos**.
- **Filas de decode**: fixtures deterministas commiteadas (JPEG/GIF generadas
  con Pillow, receta en `crates/bench/fixtures/README.md`; PNG del crate
  `image`), `include_bytes!`, decode completo a RGBA8, heavy (3 iteraciones).
  JPEG es el formato más caro por MPix en este motor.
- **`--chrome`**: añade +17 ms en showcase y +7,7 ms en basic (address bar/tab
  strip); goldens de chrome deterministas; el chrome normal no se ejerce en
  headless sin el flag.
- **`--find`**: +8,9 ms (índice + overlay; 3 hits, sin re-render al cambiar).
- **HTTP/2 MVP** (r22): coste ≈0 en el bench; límites (sin multiplexar/push/
  flow-control completo) documentados en `crates/net`.
- **`website/` y `pages.yml`**: del dueño; no se tocan.
- **Fast path de tablas (r7)**: nota histórica; no hay commit compilable con el
  camino avanzado sin fast path.
