# Verificación Mesa A — scope fonts/image/svg (`f2-assets.md`)

- Mesa: A (reproductor independiente). Fecha: 2026-11-18.
- Informe verificado: `docs/audit/findings/wave2/f2-assets.md` (11 hallazgos `w2-assets-01…11` + concerns C1–C6).
- Base de ejecución: worktree limpio `/tmp/opencode/megi-verify`, HEAD `3c06db8`
  (sólo `?? tests/fixtures/render/f2-exp10.html`, `f2-exp12.html` untracked preexistentes;
  `git status` sin WIP en `crates/`).
  Env: `TMPDIR=/home/jp/.cache/megi-tmp`, `CARGO_TARGET_DIR=/home/jp/.cache/f2-target`.
  Repros leídos del árbol principal
  (`docs/audit/findings/wave2/repro-w2-assets/`, 15 ficheros presentes); no se escribió
  nada en el worktree verify ni se borró ningún archivo.
- Sonda: `/home/jp/.cache/megi-probe-assets` (deps por path a `crates/svg` + `crates/image`
  del worktree verify; se ejecutó cada modo 1 vez salvo indicación).
- Resultado: **11/11 VERIFIED**. Suites en verde: `svg` (37+14+13+16+render…),
  `image` (32+34+5+9+8+6+7+21+1), `fonts` (3+19+10+1); 0 failed.

## Tabla de veredictos

| id | veredicto | comando | salida literal (recorte) | nota |
|----|-----------|---------|--------------------------|------|
| w2-assets-01 | VERIFIED | probe `render` (`svg::render_svg` sobre `switch.svg`) | `switch.svg: 100x50 painted=5000 … top=[((0, 0, 255, 255), 2500), ((255, 0, 0, 255), 2500)]` | Ambos hijos pintados completos (rojo 2500 + azul 2500); esperado sólo el azul. Causa en `document.rs:1067` (`"g" \| "a" \| "switch"` comparten rama grupo, `visit_children` sin filtro `systemLanguage`). Idéntico al `painted= 5000` del informe. |
| w2-assets-02 | VERIFIED | probe `textcol` (columna x=100, 20 primeras filas) | `miter.svg: col_x100_first20_alpha=[0, 0, 0, 0, 0, 0, 0, 0, 0, 159, 255, …] topmost_ink_row=Some(5)`; `miter-bevel.svg: […, 0, 0, 0, 0, 128, 255, …] topmost_ink_row=Some(5)` | Miter deja filas 0–8 vacías donde bevel pinta desde la fila 5; el tip falta. Código `raster.rs:1883-1892`: intersección `v + n0·t` con `v + n1·u` (rectas por `v`) con `t ≡ -1` implícito; el chequeo `distance <= miter_limit * half` nunca mide el pico exterior. |
| w2-assets-03 | VERIFIED | probe `render` sobre `namedcolor.svg` + lectura `color.rs:138-163` | `namedcolor.svg: 20x20 painted=0 … top=[((0, 0, 0, 0), 400)]` | `named_color` sólo 16 básicos + `orange` + `transparent`; `rebeccapurple` → `_ => None` → `Paint::None`. Esperado `(102,51,153)` opaco. |
| w2-assets-04 | VERIFIED | probe `render` sobre `hsl.svg` + lectura `color.rs:68-71` | `hsl.svg: 20x20 painted=0 … top=[((0, 0, 0, 0), 400)]` | `parse_color` sólo ramifica `rgb\|rgba` (`_ => None`); `hsl(0,100%,50%)` → `None` → transparente. Esperado rojo `(255,0,0,255)`. |
| w2-assets-05 | VERIFIED | probe `render` sobre `vis-inherit.svg` + lectura `document.rs:613-616` | `vis-inherit.svg: 20x20 painted=0 … top=[((0, 0, 0, 0), 400)]` | `self.visibility = value == "visible"` convierte `inherit` (y cualquier no-`visible`) en oculto; con padre visible debería pintar 400 px rojos. Fix del informe (no tocar en `inherit`) coherente con el código. |
| w2-assets-06 | VERIFIED | probe `textpos` (mín-x de tinta) + lectura `document.rs:2813-2821` | `text-px.svg: ink_min_x=Some(1) ink_count=74`; `text-num.svg: ink_min_x=Some(11) ink_count=74` | `first_number` hace `part.trim().parse::<f32>()` directo: `"10px"` falla → `0.0`; el texto se desplaza al origen con el mismo nº de glifos. Inconsistente con `length_attr` (acepta `px`). La parte `stdDeviation` del título es lectura estática (no ejercitada dinámicamente). |
| w2-assets-07 | VERIFIED | probe `file` (control decimal vs `%`) + lectura `document.rs:419-422` | `root-opacity-dec.svg (opacity="0.5"): px(10,10)=(255,0,0,128)`; `root-opacity-pct.svg (opacity="50%"): px(10,10)=(255,0,0,255)` | Raíz usa `v.parse::<f32>()` en vez de `parse_opacity`; `"50%"` falla → `unwrap_or(1.0)`. El resto de elementos sí acepta `%`. Inconsistencia raíz/resto confirmada con control. |
| w2-assets-08 | VERIFIED | probe `grad` (1 pasada, perfil debug) + lectura `gradient.rs:225-242`, `raster.rs:1583` | `g2-16.svg: 1600x1600 494.97ms painted=2560000`; `g256-16.svg: 1600x1600 1.66s painted=2560000` (≈3,4×) | `evaluate` recorre linealmente `&self.stops[1..]` por píxel; stops ordenados (`sort_by` en `gradient.rs:365`) admiten bisección. Absolutos menores que los del informe (1 pasada vs 3), pero el factor de escala con nº de stops persiste en el mismo perfil debug. Perf, no corrección. |
| w2-assets-09 | VERIFIED | probe `alph` (`megi_image::decode` sobre `alph-f1/2/3.webp` + mutación de bits de filtro del chunk `ALPH`) | `ALPH chunk at file offset 30`; `filter=0: OK 16x16`; `filter=1: OK 16x16`; `filter=2: ERR UnsupportedFeature { feature: "WebP ALPH filtering method" }`; `filter=3: ERR …` | `apply_alpha_plane` (`webp.rs:993-1032`) sólo implementa 0/1 y rechaza 2 (vertical) / 3 (gradient), válidos según el spec del contenedor. Los fixtures `alph-f2/f3.webp` del repro dan el mismo `ERR`. |
| w2-assets-10 | VERIFIED | probe `jpeg` (barrido SOS + `decode`/`decode_scaled`) + lectura `jpeg.rs:8-11` vs `parse_sos` (`jpeg.rs:948-950`) | `SOS scans: [(3, 0, 0, 0, 1), …, (3, 0, 0, 1, 0), (1, 1, 63, 1, 0), …]`; `decode OK 12x8`; `scaled OK 6x4` | El scan `(Ns=3, Ss=0, Se=0, Ah=1, Al=0)` = refine-DC entrelazado **presente** en `progressive.jpg` y decodificado sin error; el doc promete `UnsupportedFeature` para `Ah>0` multi-componente pero `parse_sos` sólo rechaza `len>1 && ss!=0`. Discrepancia doc-vs-código exacta. El comportamiento (decodificar) es el razonable; el fix es de docstring. |
| w2-assets-11 | VERIFIED | lectura estática + `decode` OK de w2-assets-10/probe `alph` (f1 OK) | `lib.rs:10-12` «typed errors for progressive» vs `decode OK 12x8`; `lib.rs:22-24,40-43` «VP8X … stay unsupported» / «animation/alpha-only WebP (VP8X)» vs `alph-f1.webp: OK 16x16`; `png.rs:358` «accumulate per-output-pixel RGBA sums» vs `png.rs:152` «Sampling is nearest-neighbour»; `gif.rs:315` «MSB-first bit reader» vs `gif.rs:323` «least-significant-bit first» | Los tres docstrings contradicen al código del mismo árbol (a y c son auto-evidentes en el propio archivo; b es contradicción interna 358 vs 152). La parte (b)-`decode_png_scaled` doc 149–155 ya dice nearest y es la correcta. Sólo docs, sin cambio de comportamiento. |

Detalle de comandos (todos con `TMPDIR=/home/jp/.cache/megi-tmp CARGO_TARGET_DIR=/home/jp/.cache/f2-target`
desde `/tmp/opencode/megi-verify`):

- `cargo run --manifest-path /home/jp/.cache/megi-probe-assets/Cargo.toml -- render|textcol|alph|jpeg|grad|textpos|concerns|file …`
  (sonda con path-deps al worktree; render in-process vía `svg::render_svg`, ALPH/JPEG vía `megi_image::decode`).
- `cargo test -p svg`, `cargo test -p megi-image`, `cargo test -p megi-fonts` en serie: todo `ok`, 0 failed
  (svg: 37+14+13+16+…; image: 32+34+5+9+8+6+7+21+1; fonts: 3+19+10+1).
- Verificado-fijo del informe (`currentcolor`): re-confirmado —
  `currentcolor.svg: 20x20 painted=400 … ((255, 0, 0, 255), 400)`. No es hallazgo.

## Concerns (para Mesa C)

1. **C1 — EJECUTADO (ya no needs-repro)**: `<text font-size="5000">` renderiza `painted=0`
   (probe `concerns`: `C1 font-size=5000: painted=0`). Causa confirmada en `raster.rs:1016-1018`
   (`size > 4096.0 → return Ok(())`, truncado silencioso). El auditor no pudo ejecutarlo
   (bus lleno); esta mesa sí lo ejecutó. Propuesta: es material de hallazgo P3 (error silencioso;
   considerar `SvgError::Limit` o documentar el tope), no sólo concern.
2. **C2 — needs-repro (sólo código)**: `svg::text::families` (`text.rs:108-123`) parte por `,`
   sin respetar comillas y desnuda genéricos entrecomillados, mientras
   `megi_fonts::parse_font_families` (`db.rs:193-…`) sí respeta comillas (bucle con
   `in_single`/`in_double`/`escaped`). Divergencia confirmada por lectura; sin sonda de
   render diferencial (p. ej. `font-family='"serif", "DejaVu Sans", serif'` con comas dentro
   de comillas). Si Mesa C lo incluye, pedir repro de píxeles o marcar `exclude`.
3. **C3 — needs-repro (sólo código)**: `StreamingInflater::feed` (`inflate.rs:811-815`) sólo
   drena `input` si `position.bit == 0`; con feeds pequeños el buffer retiene bytes ya
   consumidos (memoria O(input) en API streaming). Mecanismo confirmado por lectura; el
   harness RSS del auditor no corrió (disco lleno) y esta mesa tampoco lo ejecutó.
   Si Mesa C lo incluye, pedir medición RSS o marcar `exclude`.
4. **C4/C5/C6 (notas, sin repro de daño)**: no re-investigados más allá de la cobertura del
   informe; nada que objetar a su clasificación como notas. C4 (`identity_digest` O(bytes)
   por render) y C5 (`SYSTEM_FALLBACK_DB` monótono) son observaciones de diseño válidas
   a nivel código; C6 (Huffman over-subscribed leve aceptado) es robustez menor sin
   Panic/OOB. De acuerdo en no publicarlos.
5. **w2-assets-06, segunda mitad (`stdDeviation="2px"` → filtro descartado)**: verificada sólo
   por lectura (`first_number` + `document.rs:2621-2628`); la sonda ejercitó la vía `<text x>`.
   El mecanismo es el mismo (`first_number`), por lo que el veredicto VERIFIED cubre el título;
   si Mesa C crea issue separado para `feGaussianBlur`, pedir repro de píxeles.
6. **w2-assets-08**: 1 pasada por fixture (no 3) y absolutos menores que los del informe; el
   veredicto se apoya en el factor de escala (≈3,4× debug), no en los ms literales. En release
   el factor por píxel persiste aunque el absoluto baje — de acuerdo con el informe.
7. **Dedup**: de acuerdo con el informe — #1270 (gradiente) y #1324 (miter) son diferidos
   cerrados pero vivos → evidencia nueva válida; #1129/#1130 son del crate `css` y no cubren
   a `svg::color` (tablas distintas: `css` 148 colores + hsl vs `svg` 16 + rgb). #1329/#1273
   (switch) likewise vivos.
