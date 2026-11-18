# Mesa B — fiscal de dedup y severidad (scope core, `f2-core.md`)

- Informe fiscalizado: `docs/audit/findings/wave2/f2-core.md` (HEAD `3c06db8`).
- Fuentes de dedup: `docs/audit/FINDINGS.md` (228), `docs/GAPS.md`,
  `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json`
  (1000 issues, 5 abiertas: #1544, #1541, #1534, #1519, #1516 — ninguna del scope),
  más `gh`-equivalente vía índice local. Ninguna abierta solapa con core.
- Escala aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass;
  P2 = corrupción visible o contrato roto; P3 = cosmetic/docs.
- Ubicaciones `archivo:línea` verificadas contra el árbol (todas existen salvo nota).

## Tabla de veredictos

| id | veredicto | sev. propuesta | título concreto | acción correcta |
|---|---|---|---|---|
| w2-core-01 | SOLO NUEVO (rel. #1539 cerrada) | P2 (mantener) | sí | validar unidad por función + aridad exacta |
| w2-core-02 | SOLO NUEVO (rel. #1127, #1452) | P3 (mantener) | sí | `eq_ignore_ascii_case` en nombres de función |
| w2-core-03 | SOLO NUEVO (rel. #1128, #1469) | P3 (mantener) | sí | tope 1e6 en `to_length`/`evaluate_*` o documentar excepción |
| w2-core-04 | SOLO NUEVO (rel. #1153, #1445) | P2 (mantener) | sí | `.filter(is_finite)` antes del clamp |
| w2-core-05 | SOLO NUEVO | P2 (mantener) | sí | `is_finite` + `>= 0` en ambas vías |
| w2-core-06 | SOLO NUEVO | P3 (mantener) | sí | inválido total si un componente falla (no `filter_map` parcial) |
| w2-core-07 | SOLO NUEVO | P3 (mantener) | sí | reescribir bloque de comentario UA (líneas 8-9) |
| w2-core-08 | SOLO NUEVO (patrón rel. #1407) | P3 (mantener) | sí | actualizar docstring líneas 1219-1220 |
| w2-core-09 | SOLO NUEVO | P3 (mantener, suelo) | sí | eliminar brazo duplicado (recategorizar: cleanup, no "test") |
| w2-core-10 | SOLO NUEVO CON CORRECCIÓN | P3 (mantener) | parcial (dividir en 2) | ver nota: el claim `is_finite` es falso para `VerticalAlign` |
| w2-core-11 | REQUIERE MÁS EVIDENCIA | — | no evaluable | completar informe (solo existe el resumen de cobertura) |
| w2-core-12 | REQUIERE MÁS EVIDENCIA | — | no evaluable | completar informe (solo existe el resumen de cobertura) |
| w2-core-13 | REQUIERE MÁS EVIDENCIA | — | no evaluable | completar informe; posible rel. #1124 (mismo `line_starts`) |
| w2-core-14 | REQUIERE MÁS EVIDENCIA | — | no evaluable | completar informe; posible rel. #1454/#1468/#1121 |
| latent | REQUIERE MÁS EVIDENCIA | — | no evaluable | la sección latent no existe en el fichero |

## Notas por hallazgo (solo lo que cambia la decisión del chair)

- **01**: `parse_args` (`transform.rs:216-236`) recorta al prefijo numérico y
  `match name` (`:51`) es exacto — defecto confirmado en código. #1539 está
  CERRADA y el parser está cableado (`ComputedStyle.transform`,
  `properties.rs:2468`), así que es defecto nuevo en código vivo, no feature
  pendiente. P2 proporcional: posicionamiento/rotación visiblemente erróneos con
  CSS válido, sin crash → no llega a P1.
- **02**: `split_functions` no pliega (`:170`) y solo `"none"` usa
  `eq_ignore_ascii_case` (`:44`) — confirmado. Distinto de #1127 (índice
  por tag) y #1452 (nombres de grid): citar como relacionadas, no duplicadas.
- **03**: `Length::parse` topa en `|x| > 1e6` (`properties.rs:389-420`) y
  `calc.rs::to_length` (`:46-47`) solo filtra finitud, sin tope — vía de
  escape confirmada. Valores resultantes finitos → P3 proporcional (no P2:
  sin no-finitos ni crash).
- **04**: `.parse::<f32>().ok()` + `.clamp(0,1)` (`properties.rs:3200-3203`):
  `NaN` sobrevive — confirmado. Distinto de #1153 (capa layout, cerrada) y
  #1445 (slice en `Color::parse`). P2: alfa NaN → blending/paint indefinido.
- **05**: `parse_line_height_factor` filtra `>= 0` pero no finitud (`:3408-3414``);
  `parse_line_height` acepta cualquier `f32` (`:3416-3430`) — confirmado. P2.
- **06**: `cycle_property_list` usa `filter_map` (`:4325-4331`); su propio
  docstring promete invalidez total si falta la declaración pero no si un
  componente falla — inconsistencia interna confirmada. P3.
- **07**: el comentario UA (`user_agent.rs:8-9`) afirma que el motor mapea
  `:checked`/`:disabled`/`:focus` a `PseudoClass::Never`, pero `selector.rs`
  los implementa (`:415-426`, `Focus`/`Checked`/`Disabled` en `:359-370` con
  `state.rs`) — contradicción confirmada. P3 docs; fix = reescribir el
  comentario (matizar que la hoja UA usa selectores de atributo, no que el
  motor no los implemente).
- **08**: docstring obsoleto confirmado (`selector.rs:1219-1220`) frente a
  implementación real (`:1691-1694` parsea `is`/`where`/`not`/`has`). P3 docs.
- **09**: brazos duplicados confirmados (`selector.rs:1725-1727`). P3 es el
  suelo; impacto nulo en runtime. Corregir categoría propuesta: es cleanup,
  no "test".
- **10 (corrección)**: la mitad `font-weight` confirmada
  (`:3397-3405`, sin rango 1–1000). La mitad `VerticalAlign` es **inexacta**:
  el código SÍ filtra `is_finite` en ambas ramas (`:1002-1012`); solo falta
  el tope de magnitud 1e6. Acción: dividir en dos issues y corregir el cuerpo
  (quitar el claim `sin is_finite`). P3 para ambas mitades.
- **11–14 + latent**: el fichero termina literalmente a mitad de `w2-core-10`
  (`...[truncated 3880 chars]`, línea 171). No hay cuerpo que fiscalizar
  (título exacto, severidad, spec, fix). Pistas de cobertura para el auditor
  al completar: 11 `color-mix in lab/lch`; 12 `strip_suffix("fr ")` en grid;
  13 coste memoria `line_starts` (solapa parcial con #1124, cerrada, que era
  el cap de posiciones — citar como relacionada); 14 path de pánico en
  `browser.rs` (revisar solape con #1454/#1468/#1121 antes de publicar).

## Concerns para el chair (meeting-core)

1. **El informe está truncado**: 10 de ~14 hallazgos tienen cuerpo y el resto
   (11–14 + latent) no existe. Todo lo no fiscalizable debe ir a `exclude`
   hasta que el auditor complete el fichero. No crear issues 11–14 con el
   resumen de cobertura como único cuerpo.
2. **w2-core-10 necesita corrección antes de `include`**: partir en dos y
   retirar el claim falso sobre `is_finite` en `VerticalAlign`.
3. Ningún hallazgo 01–10 justifica P1/P0 (sin crash/hang/DoS/bypass en los
   cuerpos) ni rebaja a P3 más allá de lo propuesto; severidades del auditor
   proporcionales en los 10 evaluables.
4. Dedup limpio: 0 duplicados exactos en ledger + índice + GAPS; 8 hallazgos
   con issues relacionadas que deben citarse en el cuerpo del issue
   (01→#1539, 02→#1127/#1452, 03→#1128/#1469, 04→#1153, 08→#1407,
   13→#1124, 14→#1454/#1468/#1121).
