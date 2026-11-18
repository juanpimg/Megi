# Mesa C — reunión de verificación (scope core, `w2-core-*`)

- Fecha: 2026-11-18. Chair: Mesa C (scope core).
- Commit verificado: `3c06db8` (worktree limpio `/tmp/opencode/megi-verify`).
- Fuentes: informe `f2-core.md` (15 hallazgos 01–09, 10a, 10b, 11–14 + 1 latent;
  sonda `/tmp/probe` T1–T10, L1–L7, O1/O2, H1–H3, W1, B1/B2, Q1/Q2, V1, M1/M2,
  F1, C1/C2, S1 + `mem`, 2 ejecuciones idénticas; `cargo test -p dom -p html -p css`
  → 529 passed, 0 failed; `cargo build -p css` → 0 warnings; `/tmp/dup.rs`),
  `verify-core-A.md` (reproductor independiente), `verify-core-B.md` (fiscal
  inicial 01–10, truncado), `verify-core-B2.md` (fiscal completo, sustituye a B),
  y dedup contra índice local (1000 issues; abiertas: #1544, #1541, #1534, #1519,
  #1516 — ninguna del scope) + `docs/audit/FINDINGS.md` (228) + `docs/GAPS.md`.
- Regla aplicada: include exige A=VERIFIED/PARTIAL y B≠DUPLICADO. B2 deja
  14 SOLO NUEVO (01–09, 10a, 10b, 11, 12, 13) y 2 REQUIERE MÁS EVIDENCIA
  (14 y latent) → exclude.
- Notas de corrección aplicadas al publicar: 10a y 10b como DOS ISSUES SEPARADOS
  (10b sólo el tope 1e6; `VerticalAlign` SÍ filtra `is_finite`, claim retirado;
  negativos válidos por CSS 2.1 §10.8.1); w2-core-06 con el discriminador B1b
  (`no-repeat, bogus`); w2-core-09 como cleanup, no bug de producto.
- `verifier-template.md` no existe en `docs/audit/findings/wave2/` (verificado
  por el chair antes de escribir). No se borró ningún archivo. No se editó
  código ni ledger ni se publicó nada.

## Tabla final

| id | veredicto | sev | área | etiquetas | incl/excl | motivo |
|---|---|---|---|---|---|---|
| w2-core-01 | VERIFIED | P2 | css-transform | public | include | `%`/`rad`/`qpx` aceptados, aridad ignorada (`transform.rs:216-236`); rel. #1539 cerrada |
| w2-core-02 | VERIFIED | P3 | css-transform | public | include | nombres de función case-sensitive (`transform.rs:51`); rel. #1127, #1452 |
| w2-core-03 | VERIFIED | P3 | css-calc | public | include | `calc()` elude tope 1e6 (`calc.rs:260-326` vs `properties.rs:386-424`); rel. #1128, #1469 |
| w2-core-04 | VERIFIED | P2 | css-opacity | public | include | `NaN.clamp` → NaN (`properties.rs:3200-3203`); rel. #1153, #1445 |
| w2-core-05 | VERIFIED | P2 | css-line-height | public | include | inf/NaN/negativo aceptados (`properties.rs:3407-3430`) |
| w2-core-06 | VERIFIED | P3 | css-background | public | include | `filter_map` cicla parcial (`properties.rs:4315-4335`); discriminador B1b |
| w2-core-07 | VERIFIED | P3 | css-docs | public, docs | include | comentario UA (`user_agent.rs:8-9`) contradice `selector.rs:359-370/:415-426` |
| w2-core-08 | VERIFIED | P3 | css-docs | public, docs | include | docstring obsoleto (`selector.rs:1219-1220`) vs impl (`:1690-1693`); patrón rel. #1407 |
| w2-core-09 | VERIFIED | P3 | css-selector | public, cleanup | include | brazo duplicado (`selector.rs:1725-1727`); cleanup, no bug de producto |
| w2-core-10a | PARTIAL (mitad confirmada) | P3 | css-font | public | include | `font-weight: 0/1001` aceptados (`properties.rs:3397-3405`); escisión Mesa B |
| w2-core-10b | PARTIAL (corregido) | P3 | css-vertical-align | public | include | sólo falta tope 1e6 (`properties.rs:989-1018`); `is_finite` SÍ filtra, claim retirado |
| w2-core-11 | VERIFIED | P3 | css-color | public | include | `lab`/`lch`/`xyz-*` rechazados (`color.rs:586-644`) |
| w2-core-12 | VERIFIED | P3 | css-grid | public, cleanup | include | `strip_suffix("fr ")` inalcanzable (`grid.rs:623-627`, `max.trim()` en `:615`) |
| w2-core-13 | VERIFIED | P3 | html-tokenizer | public, perf | include | ~12 B/B (`tokenizer.rs:772-778` + `input_stream.rs:112-201`); rel. #1124 cerrada |
| w2-core-14 | VERIFIED (mecanismo) | — | html-fragment | needs-repro | exclude | sin trigger reproducible; cita errónea (`browser.rs:465-474` no existe; real `tree_builder.rs:4465-4474`); rel. #1454/#1468/#1121 |
| latent | NEEDS INFO | — | html-fragment | advisory | exclude (advisory) | sin trigger conocido → no explotable hoy; sólo aviso privado según `SECURITY.md` si se eleva |

## Incluidos como issue público (14)

w2-core-01, 02, 03, 04, 05, 06, 07, 08, 09, 10a, 10b, 11, 12, 13.
Cuerpos en `docs/audit/findings/wave2/issues/<id>.md` (primera línea
`[css|html|dom]`, con resumen, impacto, pasos, esperado/observado, evidencia
literal, `Commit: 3c06db8`, `Dedup:`).

## Descartes (2)

- w2-core-14: VERIFIED a nivel de mecanismo pero `needs-repro` (sin fixture que
  dispare el pánico; parser blindado con clon + `catch_unwind` #1454 y budgets).
  Regla: nunca publicar un `needs-repro` como issue. Ubicación citada errónea:
  el cuerpo futuro debe citar `tree_builder.rs:4465-4474` + llamantes
  `browser.rs:63/133/77`, no `browser.rs:465-474`.
- latent-security: NEEDS INFO / REQUIERE MÁS EVIDENCIA. Sin trigger conocido desde
  entrada → no explotable hoy. No es issue público; sólo aviso privado según
  `SECURITY.md` si el Chair lo eleva, en caso contrario advisory interno ligado a 14.
