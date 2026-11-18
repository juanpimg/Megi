# Mesa C — reunión de verificación (scope fonts/image/svg, `w2-assets-*`)

- Fecha: 2026-11-18. Chair: Mesa C (scope fonts/image/svg).
- Commit verificado: `3c06db8` (worktree limpio `/tmp/opencode/megi-verify` según informe F2 y Mesa A; HEAD actual `d82118c` sin cambios en los archivos citados — `git log 3c06db8..HEAD` vacío en `document.rs`, `raster.rs`, `color.rs`, `gradient.rs`, `webp.rs`, `jpeg.rs`).
- Fuentes: informe `docs/audit/findings/wave2/f2-assets.md` (11 hallazgos `w2-assets-01…11` + concerns C1–C6), `verify-assets-A.md` (Mesa A, 11/11 VERIFIED + C1 ejecutado), `verify-assets-B.md` (Mesa B, fiscal dedup/severidad), spot-check propio del chair (C1 en HEAD actual).
- Nota de procedimiento: `verifier-template.md` (Mesa C) no existe en el repo (Mesa B ya lo constató por glob sin resultados); formato tomado de `meeting-net.md` (tabla final + avisos/incluidos/descartes).
- Regla de inclusión: `include` exige A=VERIFIED **y** B≠DUPLICADO (`YA REPORTADO` → `comment-existing`, no issue nuevo; `SOLO NUEVO` → publicar). No se edita código ni ledger. No se borra ningún archivo.

## Tabla final

| id | veredicto | sev | área | etiquetas | incl/excl | motivo |
|----|-----------|-----|------|-----------|-----------|--------|
| w2-assets-01 | VERIFIED (A) / YA REPORTADO #1329 (B; #1273 duplicado previo) | P2 | svg switch | public, spec-s5.8 | exclude (comment-existing) | Rama `"g" \| "a" \| "switch"` como grupo (`document.rs:1067`); ambos hijos pintados (`painted= 5000`, rojo 2500 + azul 2500). Evidencia nueva en HEAD, pero match exacto con #1329 (P2 coincidente). |
| w2-assets-02 | VERIFIED (A) / YA REPORTADO #1324 (B) | P1 | svg miter | public, spec-s11.4 | exclude (comment-existing) | Intersección `v+t·n0` vs `v+u·n1` con `t ≡ -1` implícito (`raster.rs:1883-1920`); tip (100,1.97) vacío. Evidencia nueva fuerte, pero #1324 es el mismo defecto P1. |
| w2-assets-03 | VERIFIED (A) / SOLO NUEVO (B; rel #1129 otro crate) | P2 | svg color | public, css-color-3 | include | `named_color` (`color.rs:138-163`) sólo 16+2; `rebeccapurple` → `painted=0` (esperado `(102,51,153)`). #1129 es del crate `css`. Cuerpo en `issues/w2-assets-03.md`. |
| w2-assets-04 | VERIFIED (A) / SOLO NUEVO (B; rel #1130 otro crate) | P2 | svg color | public, css-color-3 | include | `parse_color` (`color.rs:68-71`) sin `hsl()`; `painted=0` (esperado rojo). Cuerpo en `issues/w2-assets-04.md`. |
| w2-assets-05 | VERIFIED (A) / SOLO NUEVO (B) | P3 | svg visibility | public, spec-s5.3 | include | `value == "visible"` (`document.rs:613-616`) convierte `inherit` en oculto; `painted=0` (esperado 400 px). Cuerpo en `issues/w2-assets-05.md`. |
| w2-assets-06 | VERIFIED (A) / SOLO NUEVO (B) | P2 | svg text/unidades | public, spec-s4 | include | `first_number` (`document.rs:2813-2821`) sin unidades: `x="10px"` → 0 (`ink_min_x` 1 vs 11); además `stdDeviation="2px"` → `feGaussianBlur` descartado (lectura). Cuerpo en `issues/w2-assets-06.md`. |
| w2-assets-07 | VERIFIED (A) / SOLO NUEVO (B; distinto #1190/#1268) | P3 | svg opacity | public, spec-s12 | include | Raíz `parse::<f32>()` (`document.rs:419-422`): `"50%"` → 1.0 (`(255,0,0,255)` vs esperado 128); resto sí acepta `%`. Cuerpo en `issues/w2-assets-07.md`. |
| w2-assets-08 | VERIFIED (A) / YA REPORTADO #1270 (B; #1416 otro crate) | P2 | svg gradient | public, perf | exclude (comment-existing) | Scan lineal por píxel (`gradient.rs:225-242`); g2 ~1.0-1.3s vs g256 ~1.8-2.6s debug (≈3,4× en sonda A de 1 pasada). Coherente con #1270. |
| w2-assets-09 | VERIFIED (A) / SOLO NUEVO (B) | P2 | image webp | public, webp-spec | include | `apply_alpha_plane` (`webp.rs:1025-1029`) rechaza filtros 2/3 válidos; imagen entera falla. Cuerpo en `issues/w2-assets-09.md`. |
| w2-assets-10 | VERIFIED (A) / SOLO NUEVO (B) | P3 | image jpeg | public, docs | include | Doc (`jpeg.rs:8-11`) promete rechazar refines entrelazados; `parse_sos` sólo rechaza `ss!=0` y decodifica `(Ns=3,Ss=0,Ah=1)` OK. Fix de docstring. Cuerpo en `issues/w2-assets-10.md`. |
| w2-assets-11 | VERIFIED (A) / SOLO NUEVO (B; distinto #1281) | P3 | image docs | public, docs | include | Ómnibus 3 docstrings (`lib.rs`, `png.rs:358`, `gif.rs:315`) contradichos por el propio código. Cuerpo en `issues/w2-assets-11.md`. |
| w2-assets-C1 | VERIFIED (A ejecutado; elevado de concern C1) / nuevo (B posterior) | P3 | svg text | public, error-silencioso | include | `<text font-size="5000">` → `painted=0` con `Ok(())` (`raster.rs:1016-1018`, `size > 4096 → return Ok`); código sigue en HEAD `d82118c`. Cuerpo en `issues/w2-assets-C1.md`. |

## Comment-existing (texto listo, NO issue nuevo)

- **w2-assets-01 → comentar en #1329** (vincular #1273 como duplicado previo del mismo síntoma):
  > Evidencia nueva en HEAD `3c06db8`: `<switch>` con hijo 1 `systemLanguage="xx-invalid"` (rojo) + hijo 2 sin tests (azul) pinta ambos completos (`painted= 5000`, rojo 2500 + azul 2500; esperado sólo el azul). Causa: `crates/svg/src/document.rs:1067` trata `"g" | "a" | "switch"` como grupo (`visit_children` sin filtro `systemLanguage`). Repro: `docs/audit/findings/wave2/repro-w2-assets/switch.svg` + `cargo run -p megi-svg --example svg_to_png`. Mesa A: VERIFIED; Mesa B: YA REPORTADO (#1329, P2 coincidente). Detalle: `f2-assets.md` w2-assets-01, `verify-assets-A.md` fila 01.
- **w2-assets-02 → comentar en #1324**:
  > Evidencia nueva en HEAD `3c06db8`: polilínea 10,90→100,10→190,90 (width 12, miterlimit 10), columna x=100 con alfa 0 en filas 0–8 (`[0×9, 159, 255, …]`, topmost ink fila 5) mientras `miter-bevel.svg` pinta desde la fila 5; tip geométrico correcto (100, 1.97) vacío. Causa: `crates/svg/src/raster.rs:1883-1892` intersecta `{v + t·n0}` con `{v + u·n1}` (ambas por `v`) → `t ≡ -1` siempre; el `miter_limit` nunca mide el pico exterior. Derivación re-chequeada por fiscal vía Cramer. Repro: `repro-w2-assets/miter.svg`, `miter-bevel.svg` + columna x=100. Mesa A: VERIFIED; Mesa B: YA REPORTADO (#1324, P1 coincidente). Detalle: `f2-assets.md` w2-assets-02.
- **w2-assets-08 → comentar en #1270** (notar #1416 es de otro crate):
  > Evidencia nueva en HEAD `3c06db8`: rect 1600×1600 debug, 2 stops 1049–1315 ms vs 256 stops 1848–2567 ms (informe, 3 pasadas); sonda independiente 1 pasada 495 ms vs 1.66 s (≈3,4×). Causa: `evaluate` (`crates/svg/src/gradient.rs:225-242`, llamado por píxel en `raster.rs:1583`) recorre linealmente hasta 256 stops ordenados (`sort_by` en `gradient.rs:365`, admiten `partition_point`, precedente `raster.rs:951`). Repro: `repro-w2-assets/g2-16.svg`, `g256-16.svg`. P2 coherente con #1270 (P3 admisible bajo política estricta, es sólo perf). Mesa A: VERIFIED; Mesa B: YA REPORTADO (#1270). Detalle: `f2-assets.md` w2-assets-08.

## Incluidos como issue público (9)

`w2-assets-03`, `w2-assets-04`, `w2-assets-05`, `w2-assets-06`, `w2-assets-07`, `w2-assets-09`, `w2-assets-10`, `w2-assets-11` (los 8 SOLO NUEVO) + `w2-assets-C1` (elevado de concern C1, código verificado en HEAD). Cuerpos en `docs/audit/findings/wave2/issues/<id>.md` (primera línea `[image|svg|fonts] <título>`, resumen, impacto, pasos, esperado/observado, evidencia literal, `Commit: 3c06db8`, `Dedup:`).

## Descartes (0)

- Ningún hallazgo VERIFIED se descarta como inválido; los 3 YA REPORTADO (01, 02, 08) van a `comment-existing`, no a issue nuevo, por regla procesal (no por falta de mérito).
- Concerns C2/C3 (needs-repro, sólo código) y notas C4/C5/C6: correctamente no publicados (de acuerdo Mesas A/B). C1 queda promovido a `w2-assets-C1` por ejecución de Mesa A + persistencia en HEAD.
- Verificado-fijo (`currentColor` #1276/#1331): corroborado (Mesa A re-confirmó 400 px rojos); correcto no publicarlo.
