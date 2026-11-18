# Mesa B — fiscal de dedup y severidad, REINTENTO (scope core, `f2-core.md` completo)

- Informe fiscalizado: `docs/audit/findings/wave2/f2-core.md` (15 hallazgos 01–09, 10a, 10b, 11–14 + 1 latent; HEAD `3c06db8`; alcance `crates/dom|html|css` intacto según el propio informe).
- Este fichero sustituye a `verify-core-B.md` (aquel fiscalizó 01–10 con cuerpo y marcó 11–14 + latent como no evaluables por truncamiento; el informe ya está completo y aquí se fiscaliza todo). No se borra ningún archivo.
- Escala aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass; P2 = corrupción visible o contrato roto; P3 = cosmetic/docs/perf-no-DoS.
- Fuentes de dedup: `docs/audit/FINDINGS.md` (228 filas), `docs/GAPS.md`, `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json` (1000 issues; 5 abiertas: #1544, #1541, #1534, #1519, #1516 — ninguna del scope), más greps directos al árbol. Ninguna abierta solapa con core.

## Tabla de veredictos

| id | veredicto | sev. propuesta | título concreto | accionable | spec citada |
|---|---|---|---|---|---|
| w2-core-01 | SOLO NUEVO (rel. #1539 cerrada) | P2 (mantener) | sí | sí | sí (CSS Transforms 1 §6/§13) |
| w2-core-02 | SOLO NUEVO (rel. #1127, #1452) | P3 (mantener) | sí | sí | sí (CSS Syntax §4.3.4, ASCII case-insensitive) |
| w2-core-03 | SOLO NUEVO (rel. #1128, #1469) | P3 (mantener) | sí | sí | sí (tope interno 1e6 de `Length::parse`) |
| w2-core-04 | SOLO NUEVO (rel. #1153, #1445) | P2 (mantener) | sí | sí | sí (no-finitos inválidos en CSS) |
| w2-core-05 | SOLO NUEVO | P2 (mantener) | sí | sí | sí (CSS 2.2 §10.8.1) |
| w2-core-06 | SOLO NUEVO | P3 (mantener) | sí | sí | sí (CSS Backgrounds 3 §3.1) |
| w2-core-07 | SOLO NUEVO | P3 (mantener) | sí | sí | n/a (docs; contradicción interna verificada) |
| w2-core-08 | SOLO NUEVO (patrón rel. #1407) | P3 (mantener) | sí | sí | n/a (docs; F13 citado correctamente) |
| w2-core-09 | SOLO NUEVO | P3 (mantener, suelo) | sí | sí | n/a (cleanup; recategorizar: no "test") |
| w2-core-10a | SOLO NUEVO (escisión Mesa B) | P3 (mantener) | sí | sí | sí (CSS Fonts 4 §2.2, rango 1–1000) |
| w2-core-10b | SOLO NUEVO CON CORRECCIÓN | P3 (mantener) | sí | sí | sí (tope interno 1e6; ver corrección) |
| w2-core-11 | SOLO NUEVO | P3 (mantener) | sí | sí | sí (CSS Color 5 §3.2) |
| w2-core-12 | SOLO NUEVO | P3 (mantener, suelo) | sí | sí | n/a (cleanup; código muerto verificado) |
| w2-core-13 | SOLO NUEVO (rel. #1124 cerrada) | P3 (mantener) | sí | sí | n/a (límite de diseño; propuesta documentada) |
| w2-core-14 | REQUIERE MÁS EVIDENCIA | — | parcial (ubicación errónea) | no (sin trigger) | n/a |
| latent | REQUIERE MÁS EVIDENCIA | — | n/a (advisory) | no (sin trigger) | n/a (SECURITY.md si se eleva) |

## Notas por hallazgo (solo lo que cambia la decisión del chair)

- **01**: `parse_args` (`transform.rs:216-236`) recorta al prefijo numérico y descarta la unidad — confirmado en código. #1539 cerrada es el feature, no el defecto. P2 proporcional (posicionamiento/rotación erróneos con CSS válido, sin crash → no P1).
- **02**: `match name` con literales exactos (`transform.rs:51`), `split_functions` no pliega (`:170`), solo `"none"` usa `eq_ignore_ascii_case` (`:44`) — confirmado. Distinto de #1127 (índice por tag) y #1452 (nombres de grid). P3.
- **03**: `Length::parse` topa en `|x| > 1e6` (`properties.rs:389-420`); `calc.rs::to_length` (`:46-47`) solo filtra finitud, sin tope — vía de escape confirmada. Resultados finitos → P3 (no P2: sin no-finitos ni crash).
- **04**: `.parse::<f32>().ok()` + `.clamp(0,1)` (`properties.rs:3200-3203`): `NaN.clamp` devuelve NaN — confirmado. Distinto de #1153 (capa layout, cerrada) y #1445 (slice en `Color::parse`). P2 (alfa NaN → blending indefinido).
- **05**: `parse_line_height_factor` filtra `>= 0` pero no finitud (`:3408-3414`); `parse_line_height` acepta cualquier `f32` (`:3416-3430`) — confirmado. P2.
- **06**: `cycle_property_list` usa `filter_map` (`properties.rs:4315-4335`); su docstring promete invalidez total solo si falta la declaración — inconsistencia interna confirmada. P3.
- **07**: comentario UA (`user_agent.rs:8-9`) afirma mapeo a `PseudoClass::Never`, pero `selector.rs:1694-1699` implementa `Focus`/`Checked`/`Disabled`/`Enabled` — contradicción confirmada. P3 docs.
- **08**: docstring obsoleto confirmado (`selector.rs:1219-1220`) frente a implementación (`:1690-1693` parsea `is`/`where`/`not`/`has`). P3 docs.
- **09**: brazos duplicados exactos confirmados (`selector.rs:1725-1727`). P3 suelo; cleanup, no "test".
- **10a**: `parse_font_weight` (`properties.rs:3397-3405`) sin validación de rango — confirmado. P3.
- **10b (corrección exigida)**: el claim original «`VerticalAlign` sin `is_finite`» es FALSO y el informe ya lo retira: ambas ramas filtran `is_finite` (`properties.rs:1002-1012`, verificado). Solo falta el tope de magnitud 1e6. Negativos SÍ son válidos (CSS 2.1 §10.8.1 permite bajar la caja). Acción chair: publicar 10b con el cuerpo corregido (tope 1e6 en ambas ramas), nunca con el claim retirado. P3.
- **11**: `match space` (`color.rs:586-644`) solo cubre `srgb`/`srgb-linear`/`oklab`/`oklch`; `_ => None` — confirmado. Módulo promete «CSS Color 4/5» (`color.rs:1-8`). Fix accionable: implementar `lab`/`lch`/`xyz-*` o documentar la limitación. P3 (declaraciones válidas caen a inicial; sin crash).
- **12**: `strip_suffix("fr ")` (`grid.rs:623-627`) inalcanzable: `max` viene de `max.trim()` (`:615`) — confirmado. P3 suelo cleanup.
- **13**: `line_starts: Vec<usize>` por línea + `out: Vec<u32>` por codepoint; sonda cuantitativa (2 MiB `\n` → ΔRSS ~24.6 MB, sin violaciones) — evidencia suficiente para P3 perf. #1124 cerrada era el cap de posiciones (revertido): citar como relacionada, no duplica. Fix accionable (`Vec<u32>` o cómputo perezoso + documentar). P3 (amplificación con input permitido pero sin crash/hang → no P1).
- **14 (REQUIERE MÁS EVIDENCIA)**: dos motivos. (a) Ubicación citada errónea: `browser.rs:465-474` no existe (el fichero tiene 398 líneas); el código real está en `tree_builder.rs:4465-4474` (`Err(_) => LimitViolations::default()`), con llamantes en `browser.rs:63/133` y guarda `!violations.is_empty()` en `:77`. (b) Sin trigger reproducible: el propio informe lo marca `needs-repro` y el parser está blindado (clon + `catch_unwind` #1454, budgets). Regla chair: nunca publicar un `needs-repro` como issue. Acción: re-chequear solape con #1454/#1468/#1121 al reproducir.
- **latent (REQUIERE MÁS EVIDENCIA)**: sin trigger conocido desde entrada → no explotable hoy. No es issue público. Solo aviso privado según `SECURITY.md` si el Chair lo eleva; en caso contrario queda como advisory interno ligado a 14.

## Concerns para el chair (meeting-core)

1. **Corrección 10b ya aplicada en el informe**: el claim falso sobre `is_finite` está retirado en `f2-core.md` (§10b + §Concern.2). Verificado contra `properties.rs:1002-1012`: la corrección es correcta. Publicar 10a/10b como dos issues separadas con los cuerpos ya corregidos.
2. **Ubicación de 14 errónea**: el cuerpo del issue (si algún día se reproduce) debe citar `tree_builder.rs:4465-4474` + llamantes `browser.rs:63/133/77`, no `browser.rs:465-474`. Pedir al auditor que corrija la cita al reabrir.
3. **14 + latent van a `exclude`**: sin repro no hay issue ni aviso; la guarda `!violations.is_empty()` + clon de documento (#1454) mitigan el peor caso conocido.
4. **Severidades proporcionales**: ningún P0/P1 en el scope (sin crash/hang/DoS/bypass reproducibles); P2 = 01, 04, 05; resto P3. Dedup limpio: 0 duplicados exactos en ledger + índice + GAPS; relacionadas a citar en cada issue: 01→#1539, 02→#1127/#1452, 03→#1128/#1469, 04→#1153, 08→#1407, 13→#1124, 14→#1454/#1468/#1121.
