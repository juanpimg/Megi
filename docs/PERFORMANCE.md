# Rendimiento de Megi — Metodología y Baseline

Este documento describe el arnés de benchmarks del proyecto (`crates/bench`) y
publica la **baseline de rendimiento vigente**, para que cualquier regresión de
complejidad sea detectable comparando números en lugar de discutir impresiones.

- **Fecha de medición**: 2026-11-10, 22:52–23:20 (Europe/Madrid). Ronda 20:
  **fix de rendimiento de custom properties** (lazy/COW + fast path),
  **conic-gradients**, color hints/spaces, **calc mixto**, **modelo sticky +
  `text-transform`** y shell con pestañas; además se **activan las filas E2E**
  añadidas al bench en r20.
- **Repo**: `Megi`, rama `sprint/robustez-fase4`. Motor medido **`c6b66b1`**
  (test: filas E2E del bench); a mitad de ventana aterrizó `27b806b` (**solo
  docs**, no cambia el motor). Binario de bench `ac2740ae…`, 4 705 600 B
  (incluye la fila `layout/sticky-text-1mb` activada para esta medición,
  cambio local en `crates/bench`).
- **Ventana de medición**: **load 0,36–1,8** (host en calma tras esperar ~10 min
  a que bajaran compilaciones ajenas); governor `powersave`, con frecuencia
  máxima muestreada durante el bench **mediana 4,1 GHz** (mín. 1,6 / máx. 4,7),
  package 54 °C. No es el suelo absoluto (frecuencia oscila por `powersave`),
  pero es una ventana apta y comparable con r15/r17.
- **Artefactos** (gitignored): `target/bench-r20-run1..5.json`,
  `target/bench-aggregate-r20.json` (`sha256 = 4e1568a4…`),
  `target/bench-r20/ab_r19_r20.json`, `target/bench-r20-render-shell.json`,
  `target/bench-latest.txt`/`-meta.txt` (pasada canónica).

> Las baselines anteriores quedan **reemplazadas**; sus cifras se conservan en
> la comparación histórica de §12 (rondas 2–20; la 14 no publicó baseline).

---

## 1. Metodología

### 1.1 Cargas sintéticas deterministas

Las cargas se generan en memoria con funciones puras
(`crates/bench/src/generators.rs`); el *fixture* se construye **fuera** del
cronómetro. La ronda 20 añadió al bench filas E2E para las features de r18/r19
(§8) con generadores deterministas, sin red ni aleatoriedad.

### 1.2 Medición

- Reloj: `std::time::Instant`; por benchmark `--warmup` (1) + `--iterations`
  (5, *heavy* 3).
- **La tabla de §3 es la mediana, por fila, de 5 pasadas completas** en la
  ventana de calma (load 0,36–0,86 en las 5 pasadas; `setup_ms` 455–484 ms por
  las filas E2E nuevas). La pasada canónica de `./tools/bench.sh`
  (`TOTAL 662.498 ms`, 27 filas) se tomó con load ~1,8 y sirve de fingerprint.
- **A/B intercalado prístino** para atribuir el fix: `git archive d9e07e0`
  (r19, binario en `target/r19-clean-src`) vs binario r20, 5 pares alternando
  orden (§5.2). El load afecta a ambos; el Δ es fiable.
- Render/shell: 9 ejecuciones intercaladas (load ~1,3–1,8). `--find`: 7
  ejecuciones con y sin `--find megi`.
- `--scale` multiplica los tamaños (por ejemplo `0.25` para smoke tests).

### 1.3 Máquina y condiciones

| Campo | Valor |
|---|---|
| CPU | 12th Gen Intel(R) Core(TM) i7-12700H (híbrida P/E) |
| CPU visibles | `nproc` = **20 hilos lógicos** (afinidad 0-19) |
| RAM | 15 GiB |
| SO / kernel | CachyOS Linux, kernel `7.3.1-1-cachyos`, x86_64 |
| Compilador | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, perfil `release` |
| Governor / térmica | `powersave`; freq mediana 4,1 GHz durante bench; 54 °C |
| Load | 0,36–1,8 en la ventana; picos ajenos de 7–19 antes/después |

### 1.4 Comandos

```bash
cd /home/jp/Escritorio/Proyectos/Megi

./tools/bench.sh                                  # pasada canónica
for i in 1 2 3 4 5; do target/release/bench --json > target/bench-r20-run$i.json; done

# A/B prístino r19 vs r20
git archive d9e07e0 | tar -x -C target/r19-clean-src
cd target/r19-clean-src && CARGO_TARGET_DIR="$PWD/target" cargo build --release -p bench

# Filas E2E nuevas (también con --json)
target/release/bench --filter pipeline          # gradients + grid + html-to-png
target/release/bench --filter css/vars-calc
target/release/bench --filter paint/clipping
target/release/bench --filter layout/sticky

# Render/shell
target/release/render tests/fixtures/render/showcase/index.html /tmp/s.png \
    --width 1024 --height 768 --json
target/release/megi tests/fixtures/render/find.html --screenshot /tmp/f.png \
    --width 800 --height 600 --find megi
```

---

## 2. Catálogo de benchmarks

| Grupo | Benchmark | Qué mide |
|---|---|---|
| html | `html/realistic-1mb`, `html/nested-div-*`, `html/massive-*`, `html/tag-40k-attrs`, `html/pretty-5k-blocks` | Parse/tree construction. |
| css | `css/parse-10k-rules` | Parse de 10 000 reglas. |
| css | `css/resolve-styles-2k/10k-nodes` | Cascada + `ComputedStyle`. |
| css | `css/selector-match-simple/descendant` | Matching de selectores. |
| css | **`css/vars-calc-10k`** (r20) | 10 005 elementos, `:root` + 69 `var()` + 100 `calc()`. |
| layout | `layout/realistic-1mb` | Layout del documento realista, 800×600. |
| layout | **`layout/sticky-text-1mb`** (r20) | 24 156 elementos, 1.00 MiB, sticky headers/cards + `text-transform`. |
| paint | `paint/build-display-list`, `paint/rasterize-800x600`, `paint/render-to-png-800x600` | Display list, raster, PNG. |
| paint | **`paint/clipping-800x600`** (r20) | 48 cajas radius+overflow + 6 `background-clip:text`; display list+raster+PNG. |
| pipeline | `pipeline/html-to-png-800x600` | Pipeline completo del documento realista. |
| pipeline | **`pipeline/gradients-800x600`** (r20) | 60 cajas linear/radial gradients; pipeline completo. |
| pipeline | **`pipeline/grid-800x600`** (r20) | 3 grids (3×3 `1fr`, auto-fill `minmax`, spans), 29 celdas; pipeline completo. |
| fonts | (`feature fonts`) | No re-medido (sin cambios). |

---

## 3. Baseline vigente (ronda 20, HEAD `c6b66b1`/`27b806b`)

Mediana de **5 pasadas** en calma (load 0,36–0,86). `setup_ms`: 455–484 ms por
las filas E2E. **Filas legacy (19)**: `TOTAL 420.474 ms`; **todas (24 filas)**:
`TOTAL 683.885 ms`; pasada canónica: `TOTAL 662.498 ms`.

| Benchmark | mediana (ms) | rango 5 pasadas (ms) | throughput |
|---|---:|---:|---:|
| `html/realistic-1mb` | **21.078** | 20.496–21.459 | 47,5 MiB/s |
| `html/nested-div-10k` | **3.268** | 3.217–3.496 | 32,1 MiB/s |
| `html/nested-div-32k` | **10.360** | 10.012–10.940 | 32,4 MiB/s |
| `html/nested-div-64k` | **20.771** | 20.026–21.543 | 32,3 MiB/s |
| `html/nested-div-64k-default-limits` | **13.719** | 13.325–13.961 | — |
| `html/massive-end-p` | **18.879** | 18.531–19.352 | 5,30 M tags/s |
| `html/massive-formatting` | **12.113** | 11.753–12.403 | 826 k bloques/s |
| `html/tag-40k-attrs` | **12.312** | 12.213–12.816 | 3,25 M attrs/s |
| `html/pretty-5k-blocks` | **11.048** | 10.912–11.275 | 453 k bloques/s |
| `css/parse-10k-rules` | **14.069** | 13.384–14.785 | 42,4 MiB/s |
| `css/resolve-styles-2k-nodes` | **8.252** | 7.627–8.463 | 243 k el/s |
| `css/resolve-styles-10k-nodes` | **39.347** | 38.762–40.233 | 254 k el/s |
| `css/selector-match-simple` | **0.622** | 0.593–0.636 | 64,3 M matches/s |
| `css/selector-match-descendant` | **1.223** | 1.192–1.237 | 32,7 M matches/s |
| `layout/realistic-1mb` | **47.097** | 44.267–50.120 | 599 k el/s |
| `paint/build-display-list` | **16.355** | 15.242–17.779 | — |
| `paint/rasterize-800x600` | **0.995** | 0.898–1.179 | 482 MPix/s |
| `paint/render-to-png-800x600` | **8.831** | 8.590–9.285 | 6,20 MiB/s |
| `pipeline/html-to-png-800x600` | **160.135** | 157.220–176.483 | — |
| `layout/sticky-text-1mb` (nueva) | **42.010** | 40.249–44.252 | 575 k el/s |
| `pipeline/gradients-800x600` (nueva) | **53.199** | 52.176–53.637 | — |
| `pipeline/grid-800x600` (nueva) | **6.044** | 5.940–7.457 | — |
| `css/vars-calc-10k` (nueva) | **104.488** | 101.779–105.169 | 96 k el/s |
| `paint/clipping-800x600` (nueva) | **57.670** | 56.422–59.237 | — |

Pasada canónica (load ~1,8): css resolve-10k 39.221 · layout 44.170 · sticky
39.232 · render-to-png 8.512 · pipeline **157.589** · gradients 50.995 · grid
5.767 · vars-calc 101.699 · clipping 55.845 · TOTAL 662.498.

> **Aviso**: el TOTAL (legacy) suma etapas y pipeline end-to-end (solapados); no
> es un presupuesto de página. **Pipeline y layout superan sus presupuestos**
> (ver §11).

---

## 4. Complejidad observada

| Serie | Muestras | Lectura |
|---|---|---|
| `html/nested-div-*` | 10k: 3.27 · 32k: 10.36 · 64k: 20.77 ms | **Lineal** (3,2× → 3,18×; 2× → 2,01×). |
| `css/parse-10k-rules` | 5k: 6.69 · 10k: 14.07 · 20k: 30.50 ms | **Lineal**. |
| `css/resolve-styles-2k/10k` | 8.25 / **39.35 ms** | Casi lineal; **+20 % vs r17** pese al fix de customs (ver §5.2). |
| `layout/realistic-1mb` | 28 195 elementos → **47,10 ms** | +6,1 % vs r19 (sticky/text-transform/typography/calc). |
| `layout/sticky-text-1mb` | 24 156 elementos → **42,01 ms** | Nuevo fixture sticky+text-transform. |
| `paint/build-display-list` | 34 158 items → **16,36 ms** | +5,2 % vs r19 (preparación conic/clipping/typography). |
| `pipeline` | **160,1 ms** (157,2–176,5) | +2,9 % vs r19 medido; +5,2 % en A/B. |

---

## 5. Ronda 20: fix de custom properties, features y verificación

### 5.1 Qué entró

- `a7e32d5 feat(css,paint)`: **fix del coste de custom properties** —
  `ComputedStyle.custom_properties` pasa a
  `Option<Arc<HashMap<…>>>` con copy-on-write y un fast path
  (`custom::needs_resolution` sobre bytes) que **salta `resolve_declarations`**
  cuando el nodo/ancestros no declaran `--*`; además conic-gradients, color
  hints/spaces, `calc()` y typografía.
- `feade5c feat(layout)`: **calc mixto**, **modelo sticky**, **`text-transform`**
  aplicado al shaping y spacing.
- `43d62e3 feat(shell)`: pestañas con estado, atajos y `--tabs`.
- `27b806b` (docs): sincronización de documentación r20 (sin cambios de motor).

### 5.2 ¿Vuelve css resolve a ~34 ms y pipeline a ≤140 ms? — **NO**

A/B intercalado de archivos prístinos r19 (`d9e07e0`) vs r20 (`c6b66b1`),
5 pares, load 1,78 (mismo load para ambos binarios):

| Fila | r19 (ms) | r20 (ms) | Δ |
|---|---:|---:|---:|
| `css/resolve-styles-2k-nodes` | 8,32 | 8,12 | **−2,4 %** |
| `css/resolve-styles-10k-nodes` | 40,24 | 39,18 | **−2,6 %** |
| `css/parse-10k-rules` | 13,67 | 14,26 | +4,3 % |
| `layout/realistic-1mb` | 40,81 | 44,72 | **+9,6 %** |
| `paint/build-display-list` | 14,60 | 15,58 | **+6,7 %** |
| `paint/render-to-png-800x600` | 8,84 | 8,63 | −2,4 % |
| `pipeline/html-to-png-800x600` | 149,12 | 156,89 | **+5,2 %** |

En la ventana de calma la mediana es **css resolve-10k 39,35 ms** y
**pipeline 160,14 ms**: el fix recupera solo **~2,5 %** de la cascada, no los
~19 % que había metido r18. Los objetivos de la ronda **no se cumplen**:
`~34 ms` y `≤140 ms` quedan lejos. El r20 añade además **+9,6 % de layout** y
**+6,7 % de display-list** (sticky/typography/calc y preparación de
conic/clipping), de modo que el pipeline empeora en términos netos.

**Causa residual**: el fast path elimina la asignación/expansión de `--*` en
nodos sin custom properties, pero el coste por elemento que introdujo r18
**no era solo** ese: quedan guardas de estado en el matching, reglas UA
extra y más propiedades computadas/derivadas por nodo. Con `130` reglas de
autor y `10 005` elementos el coste sigue ~**+20 % vs r17** (32,77 ms).

### 5.3 Nota de integridad

El motor medido es `c6b66b1`; `27b806b` (docs) llegó a mitad y no cambia
código. La fila `layout/sticky-text-1mb` se activó para esta medición (antes
SKIP): el motor ya aplica `text-transform` y tiene modelo sticky; el cambio
vive solo en `crates/bench` (scope permitido) y el generador es determinista.

---

## 6. Codificador PNG (sin cambios)

| Métrica | r19 | r20 | Notas |
|---|---:|---:|---|
| `paint/render-to-png-800x600` | 8.892 | **8.831** | −0,7 % |
| — rasterizar | 0.956 | **0.995** | +4,1 % (varianza) |
| PNG del bench | 57 422 B | **57 422 B** | byte-idéntico |
| Golden showcase | 80 220 B | **80 220 B** | `c95c7eff…` |
| Golden basic | 21 090 B | **21 090 B** | `e943565d…` |
| Golden `--zoom 2` | 3 202 B | **3 202 B** | `4792779d…` |

---

## 7. Pipeline `render` y shell headless (r20)

9 ejecuciones intercaladas (load ~1,3–1,8; goldens verificados):

| Fixture / modo | pared r20 | pared r19 | PNG |
|---|---:|---:|---:|
| showcase 1024×768 (render) | **28,04 ms** | 28,79 | 80 220 B (`c95c7eff…`) |
| basic 800×600 (render) | **9,08 ms** | 9,68 | 21 090 B (`e943565d…`) |
| showcase (shell headless) | **28,09 ms** | 29,65 | 80 220 B (idéntico) |
| basic (shell headless) | **9,51 ms** | 9,14 | 21 090 B (idéntico) |
| basic `--zoom 2` (320×200) | **3,29 ms** | 3,46 | 3 202 B |
| `find.html` sin `--find` | **10,12 ms** | — | 30 056 B (`19c35c18…`) |
| `find.html` con `--find megi` | **16,17 ms** | 16,89 | 29 950 B (`a818f32a…`) |

El shell con `--find` sigue costando **~+6,0 ms** (índice de texto + overlay;
3 hits, sin re-render al cambiar de hit). Showcase/basic y goldens sin cambios
funcionales. Auto-refresh y pestañas del shell no se ejercen en `--screenshot`.

---

## 8. Filas E2E del bench (nuevas en r20)

El bench mide ahora las features de r18/r19 junto al resto (generadores
deterministas; fixture fuera del cronómetro; rustdoc en `crates/bench/src/benches.rs`
con notas y referencia). Medianas en calma:

| Fila | mediana (ms) | rango | detalle |
|---|---:|---:|---|
| `pipeline/gradients-800x600` | **53.199** | 52.18–53.64 | 60 cajas, 182 items, PNG 248 109 B |
| `pipeline/grid-800x600` | **6.044** | 5.94–7.46 | 3 grids, 29 celdas, PNG 25 280 B |
| `css/vars-calc-10k` | **104.488** | 101.78–105.17 | 10 005 elementos, 69 `var()`, 100 `calc()` |
| `paint/clipping-800x600` | **57.670** | 56.42–59.24 | 48 cajas radius+overflow, 1 670 items, PNG 39 412 B |
| `layout/sticky-text-1mb` | **42.010** | 40.25–44.25 | 24 156 elementos, 1.00 MiB, sticky + text-transform |

Cómo medirlas: `target/release/bench --filter pipeline` (gradients+grid),
`--filter css/vars-calc`, `--filter paint/clipping`, `--filter layout/sticky`.
Los presupuestos no se tocan en el bench: el rustdoc documenta que un
**>15 % reproducible** sobre estas referencias (misma máquina/load) es una
regresión. Estas filas suben el `setup` (~455–484 ms) y el TOTAL del informe
porque añaden benchmarks (el TOTAL legacy sigue aparte en §3).

---

## 9. Clipping, `var()`/`calc()`, net, fuzz y features r18/r19 (resumen)

- **Clipping/`clip:text`**: coste de **raster por área**; `border-radius` +
  `overflow:hidden` (200×150) ≈ **+11,1 ms de raster**, con `clip:text`
  ≈+16,7 ms; `background-clip:text` aislado es barato (−5,3 ms de pared).
  Caps: `MAX_ROUNDED_GROUP_PIXELS = 16 777 216`, `MAX_TEXT_MASK_RUNS = 4096`,
  `MAX_SHADOWS = 32`, `MAX_SHADOW_BLUR = 64`, `MAX_BACKGROUND_TILES = 4096`,
  `MAX_STACKING_CONTEXT_DEPTH = 256`, `MAX_FLOAT_DEPTH = 64`.
- **`var()`/`calc()`**: etapa `styles` +18 % de la etapa en un fixture pequeño;
  el coste estructural de la cascada es el descrito en §5.2.
- **Net**: PSL (355 LOC, ≈150 sufijos), Referrer-Policy (270 LOC, 8 políticas),
  Range/206 (239 LOC) y SameSite (~1 020 LOC) son módulos **puros sin I/O**;
  r19 los integra en `render` (referrer/SameSite en subrecursos). Coste en bench
  ≈0; coste real por petición despreciable.
- **Fuzz**: no entra en el bench; **7 targets** (`html_parse`, `css_parse`,
  `selectors`, `net_url`, `render_pipeline`, `js_eval`, `svg_parse`), caché
  local 1,6 GB; referencia **~15 s incremental / 3–5 min frío**; CI job
  **build-only en PRs** (nightly + `rust-cache`, timeout 20 min) → minutos de
  CI, 0 en el bench.
- **Gradientes CSS** (external, r19): raster +0,8–1,3 ms en fixtures de 6
  bloques; caps `MAX_GRADIENT_STOPS = 64`, `MAX_GRADIENTS = 256`.
- **Grid** (external): layout +2–6 µs para 3–6 hijos; ahora medido E2E en §8.
- **Estado/referrer**: `StateContext` sin coste medible en estilos; Seguridad:
  referrer recortado y cookies SameSite retenidas cross-site.

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

## 11. Presupuestos: veredicto (sin subirlos)

| Medida r20 | Valor | Presupuesto | % | Veredicto |
|---|---:|---:|---:|---|
| `pipeline/html-to-png-800x600` | **160,1 ms** | **≤ 140 ms** | **114,4 %** | ❌ **EXCEDIDO** |
| `layout/realistic-1mb` | **47,1 ms** | **≤ 45 ms** | **104,7 %** | ❌ **EXCEDIDO** |
| `render` showcase 1024×768 | **28,0 ms** | ≤ 35 ms | 80,1 % | ✅ |
| shell headless showcase | 28,1 ms | ≤ 35 ms | 80,3 % | ✅ |
| `paint/render-to-png-800x600` | 8,8 ms | ≤ 50 ms | 17,7 % | ✅ |
| `css/resolve-styles-10k-nodes` | 39,3 ms | ≤ 75 ms | 52,4 % | ✅ formal (pero +20 % vs r17) |
| `html/realistic-1mb` | 21,1 ms | ≤ 100 ms | 21,1 % | ✅ |
| Filas E2E (referencia) | §8 | — | — | sin presupuesto; >15 % = regresión |

**No se suben los presupuestos.** El pipeline queda **114 % del presupuesto** y
el layout **105 %**, así que la ronda 20 **no cumple el gate**. La diferencia
con la expectativa (~34 ms css resolve, ≤140 pipeline) se explica por:
1. El fast path de custom properties solo recupera **~2,5 %**; el residuo de la
   cascada (~+20 % vs r17) está en guardas de estado/reglas UA/propiedades
   computadas, no en `--*`.
2. r20 añade **+9,6 % layout** (sticky/typography/calc) y **+6,7 % display
   list** (conic/clipping/typography), comiéndose el margen.
3. Fuera del bench: `render`/shell se mantienen (~28/9 ms) y el PNG no cambia;
   el coste no está en el PNG del bench.

**Siguiente foco (sin tocar presupuestos)**: (a) **base de estilos/cascada** —
perfilar el residuo por nodo (guardas de estado, UA forms, propiedades
computadas; evitar trabajo por regla cuando no hay pseudo-clases), (b) **layout**
— sticky/text-transform/typography/`calc` por caja, (c) **display list** —
preparación genérica de conic/clipping, (d) **image decode / PNG por regiones**
para la página real, (e) medir pestañas/auto-refresh del shell en ventana.

---

## 12. Comparación histórica

Dieciocho baselines en la misma máquina, mismo arnés (la ronda 14 no publicó
baseline):

| | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 (vigente) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Fecha | 09:45 | 14:09 | 14:41 | 15:15 | 15:43 | 16:18 | 16:49 | 17:12 | 17:42 | 18:21 | 18:51 | 19:30 | 20:15 | 21:00 | 21:37 | 22:04 | 22:27 | **22:52** |
| HEAD | `9cca2e1` | `532bb6a` | `3cc8449` | `49a2792` | `13e9945` | `c002ec0` | `31e5139` | `c86682c` | `f10635c` | `13f3f21` | `7d2ddde` | `5234630` | `b808d7b` | `653626f` | `67f7c57` | `718d075` | `d9e07e0` | **`c6b66b1`** |
| Load | — | — | — | — | — | 0,99–1,52 | 2,90–3,25 | 2,91–3,35 | 0,86–2,49 | 2,16–4,23 | 0,63–5,1 | 3,4–5,3 | 1,4–2,0 | 1,45–2,65 | 0,44–0,69 | 1,4–2,0 | 4,0–5,35 | **0,36–1,8** |
| Binario | `0fd520aa…` | `9936527c…` | `d752a856…` | `e9fceca…` | `f2ea6d77…` | `687627e0…` | `ed7006e9…` | `43f7748d…` | `a188a1da…` | `c34ff65c…` | `d655c1f2…` | `153b8aff…` | `1f1dbbdd…` | `39909f01…` | `8cfbb715…` | `4185c2de…` | `c315ddf3…` | **`ac2740ae…`** |

Filas clave (mediana, ms; r20 incluye las filas E2E nuevas, que no entran en el
TOTAL legacy):

| Benchmark | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R15 | R16 | R17 | R18 | R19 | R20 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `html/realistic-1mb` | 51.179 | 52.530 | 55.569 | 57.234 | 58.738 | 55.926 | 56.140 | 55.316 | 20.305 | 20.361 | 21.641 | 22.618 | 21.260 | 21.104 | 20.581 | 20.440 | 20.910 | **21.078** |
| `css/resolve-styles-10k-nodes` | 61.908 | 27.037 | 27.604 | 28.199 | 29.595 | 29.853 | 28.662 | 28.241 | 29.422 | 28.797 | 28.807 | 30.311 | 30.543 | 31.564 | 32.774 | 39.300 | 40.876 | **39.347** |
| `layout/realistic-1mb` | 49.506 | 63.541 | 69.248 | 84.958 | 57.742 | 61.350 | 59.767 | 55.140 | 62.171 | 58.818 | 51.277 | 51.601 | 36.054 | 37.418 | 37.085 | 38.389 | 44.406 | **47.097** |
| `paint/build-display-list` | 3.356 | 3.802 | 8.108 | 12.237 | 12.745 | 12.458 | 15.090 | 14.050 | 16.128 | 15.056 | 13.648 | 12.842 | 11.954 | 12.441 | 13.101 | 13.848 | 15.546 | **16.355** |
| `paint/render-to-png-800x600` | 1 513.860 | 5.194 | 5.852 | 12.964 | 13.016 | 13.045 | 13.296 | 12.283 | 13.601 | 13.146 | 9.136 | 9.200 | 8.946 | 9.015 | 8.604 | 8.437 | 8.892 | **8.831** |
| `pipeline/html-to-png-800x600` | 1 766.298 | 152.519 | 168.612 | 214.159 | 173.305 | 178.791 | 179.699 | 168.081 | 143.229 | 141.841 | 133.373 | 133.629 | 116.410 | 122.383 | 124.575 | 142.178 | 155.633 | **160.135** |
| **TOTAL legacy (suma)** | **5 075.985** | **452.445** | **489.190** | **570.064** | **508.408** | **512.047** | **512.440** | **485.132** | **410.485** | **405.845** | **391.748** | **395.269** | **354.983** | **362.714** | **360.410** | **387.685** | **417.010** | **420.474** |

### Ronda 19 → ronda 20, fila a fila

| Benchmark | R19 (ms) | R20 (ms) | Δ |
|---|---:|---:|---:|
| `html/realistic-1mb` | 20.910 | **21.078** | +0,8 % |
| `html/nested-div-10k` | 3.311 | **3.268** | −1,3 % |
| `html/nested-div-32k` | 10.465 | **10.360** | −1,0 % |
| `html/nested-div-64k` | 21.349 | **20.771** | −2,7 % |
| `html/nested-div-64k-default-limits` | 13.813 | **13.719** | −0,7 % |
| `html/massive-end-p` | 19.053 | **18.879** | −0,9 % |
| `html/massive-formatting` | 12.790 | **12.113** | −5,3 % |
| `html/tag-40k-attrs` | 12.468 | **12.312** | −1,3 % |
| `html/pretty-5k-blocks` | 11.560 | **11.048** | −4,4 % |
| `css/parse-10k-rules` | 14.494 | **14.069** | −2,9 % |
| `css/resolve-styles-2k-nodes` | 8.617 | **8.252** | −4,2 % |
| `css/resolve-styles-10k-nodes` | 40.876 | **39.347** | −3,7 % |
| `css/selector-match-simple` | 0.655 | **0.622** | −5,0 % |
| `css/selector-match-descendant` | 1.218 | **1.223** | +0,4 % |
| `layout/realistic-1mb` | 44.406 | **47.097** | **+6,1 %** ⚠️ |
| `paint/build-display-list` | 15.546 | **16.355** | **+5,2 %** ⚠️ |
| `paint/rasterize-800x600` | 0.956 | **0.995** | +4,1 % |
| `paint/render-to-png-800x600` | 8.892 | **8.831** | −0,7 % |
| `pipeline/html-to-png-800x600` | 155.633 | **160.135** | **+2,9 %** ⚠️ |
| **TOTAL legacy** | **417.010** | **420.474** | **+0,8 %** |

(Medido con load distinta en r19 —4,0–5,35— y r20 —0,36–1,8—; la atribución
fiable es el **A/B intercalado de §5.2**: css resolve −2,5 %, layout +9,6 %,
display-list +6,7 %, pipeline +5,2 %.)

---

## 13. Cómo comparar y detectar regresiones

1. Medir con load < 2 y comprobar frecuencia/térmica (en esta ronda: load
   0,36–1,8, freq mediana 4,1 GHz, 54 °C). Si no hay ventana, marcarlo y no
   usar la pasada para presupuesto.
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
5. Regresión si la mediana empeora >15 % de forma reproducible. En r20 el
   pipeline y el layout **exceden** sus presupuestos (bloqueante).
6. Filas E2E: `--filter pipeline` (gradients+grid), `--filter css/vars-calc`,
   `--filter paint/clipping`, `--filter layout/sticky`. Referencias en §8;
   >15 % reproducible sobre ellas = regresión.
7. Goldens/bitstream: showcase `c95c7eff…` 80 220 B, basic `e943565d…` 21 090 B,
   bench PNG 57 422 B, zoom `4792779d…` 3 202 B, find `a818f32a…` 29 950 B.

---

## 14. Limitaciones y notas

- **Ventana no perfecta**: load 0,36–1,8 y governor `powersave` con frecuencia
  oscilante; se verificó freq mediana 4,1 GHz durante el bench. No es el suelo
  absoluto de la máquina, pero es comparable con r15/r17. Re-medir en una
  ventana más estable si se quiere afinar el presupuesto.
- **Fix de custom properties parcial**: el fast path afecta solo a nodos sin
  `--*`; el residuo de la cascada (~+20 % vs r17) sigue en guardas de
  estado/reglas UA/propiedades computadas. No se suben presupuestos.
- **Filas E2E nuevas**: suben `setup` (~455–484 ms) y el TOTAL del informe; el
  TOTAL legacy se publica aparte. `layout/sticky-text-1mb` se activó en esta
  ronda (motor con sticky + `text-transform` ya implementados).
- **r20 cabe en el histórico con load distinto a r19**: usar el A/B de §5.2 para
  atribuir; los absolutos de r19 estaban medidos bajo carga 4,0–5,35.
- **Render/shell**: medidos a load ~1,3–1,8; `--find` cuesta ~+6 ms; pestañas y
  auto-refresh no se ejercen en `--screenshot`.
- **`website/` y `pages.yml`**: del dueño; no se tocan.
- **Fast path de tablas (r7)**: nota histórica; no hay commit compilable con el
  camino avanzado sin fast path.
