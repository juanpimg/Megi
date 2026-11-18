# Mesa B — fiscal de dedup y severidad · F2 layout+paint (w2-lp-01…20)

Fuentes: `docs/audit/FINDINGS.md` (ledger), `docs/GAPS.md`, `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json` (1000 issues), `gh issue view` (etiquetas y estado real). No se editó código ni ledger.

Regla P0–P3 aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass; P2 = corrupción visible o contrato roto; P3 = cosmetic/docs/latente sin efecto visible.

## Veredictos

| id | veredicto | dedup | sev. informe → tracker → propuesta | título / accionabilidad |
|---|---|---|---|---|
| w2-lp-01 | YA REPORTADO #1183 (comentar con evidencia nueva) | FINDINGS deferred #1183, título idéntico | P2 → P2 → **P2** | Título concreto, fix accionable (clip_rect + PushClip, líneas citadas). Spec: CSS Backgrounds 3 existe como documento; nº de sección no verificado en esta mesa. |
| w2-lp-02 | YA REPORTADO #1185 (comentar con evidencia nueva) | FINDINGS deferred #1185, título idéntico | P2 → P3 → **P2** | Título concreto pero de alcance mayor que la evidencia: el informe admite que sólo `none` sobredimensionado está roto (`cover` ya recorta, `contain`/`scale-down` no desbordan). Fix accionable. Upgrade P3→P2 defendible (invasión visible del layout vecino), dejar a chair. |
| w2-lp-03 | YA REPORTADO #1184 (comentar con evidencia nueva) | FINDINGS deferred #1184, título idéntico | P2 → P2 → **P2 con reserva** | Título concreto, fix accionable, pero **sin repro de píxeles** (sólo lectura + suite sin fixture; el propio informe renuncia al claim cuantitativo). Severidad P2 aceptada sólo si mesa A confirma efecto visible; si no, P3. |
| w2-lp-04 | YA REPORTADO #1192 (comentar con evidencia nueva) | FINDINGS deferred #1192, título idéntico | P2 → P3 → **P2** | Repro de píxeles en HEAD, fix accionable (deflactar por border_width). Upgrade P3→P2 defendible (sombra visible donde no debe); dejar a chair. |
| w2-lp-05 | YA REPORTADO #1191 | FINDINGS deferred #1191, título idéntico | P3 → P3 → **P3** | Título concreto, accionable (fix o documentar limitación en display_list.rs). P3 correcta. |
| w2-lp-06 | YA REPORTADO #1179 (comentar con evidencia nueva) | FINDINGS deferred #1179, título idéntico | P2 → P1 → **P2** | El informe rebaja P1→P2 con buen criterio: no hay crash/hang/DoS, sólo ausencia silenciosa de pintura en páginas ≥16.7 Mpx. Apoyo la rebaja. Fix accionable (fallback a fill_rect). |
| w2-lp-07 | YA REPORTADO #1196 (comentar con evidencia nueva) | FINDINGS deferred #1196; **#1211 es duplicado interno del tracker con título idéntico** (ambos CLOSED, ambos P1, mismo título letra por letra incl. `)` final) | P2 → P1 → **P3** | Impacto declarado: "hoy ninguno visible", path inalcanzable. P1 del tracker y P2 del informe están inflados; es trampa latente → P3. Título concreto, fix de una línea accionable. Chair: comentar sólo en #1196 y enlazar/cerrar #1211 como dup, no abrir nada. |
| w2-lp-08 | YA REPORTADO #1188 (comentar con evidencia nueva) | FINDINGS deferred #1188, título idéntico | P2 → P3 → **P2/P3 a criterio de chair** | Sólo afecta a documentos hostiles (>256 niveles). P2 defendible (subárboles invisibles) pero P3 del tracker también razonable. Título concreto, fix accionable. |
| w2-lp-09 | YA REPORTADO #1189 (comentar con evidencia nueva) | FINDINGS deferred #1189 (título tracker habla de "stacking-context depth counter"; el informe aporta evidencia nueva: el contador ya es propio pero la pérdida persiste) | P3 → P3 → **P3** | P3 correcta (hostile-only). Título concreto pero desactualizado respecto a su propio tracker (el mecanismo cambió); el comentario nuevo debe reflejarlo. Accionable (documentar divergencia). |
| w2-lp-10 | YA REPORTADO #1193 | FINDINGS deferred #1193, título idéntico | P3 → P3 → **P3** | Igual reserva probatoria que w2-lp-03: sin repro de píxeles, sólo lectura. P3 correcta. Fix accionable. |
| w2-lp-11 | YA REPORTADO #1187 | FINDINGS deferred #1187, título idéntico | P3 → P3 → **P3** | Título concreto, fix accionable (contador por pase en PaintResources). P3 correcta. |
| w2-lp-12 | YA REPORTADO #107 (comentar con evidencia nueva) | #107 CLOSED (título: "negative underflow with both margins auto snaps to 0 instead of splitting equally"), etiquetado sólo `bug` sin P | P3 → sin P → **P3** | Título concreto, repro en HEAD, fix de una línea. Spec CSS 2.2 §10.3.3 verificada como cita válida. P3 correcta. |
| w2-lp-13 | DUPLICADO (descartar) | #1181 fixed en FINDINGS; informe confirma corrección (Effect anidado en Opacity) | — | No reabrir. |
| w2-lp-14 | DUPLICADO (descartar) | #1194 fixed; confirma cacheo de `u32::MAX` | — | No reabrir. |
| w2-lp-15 | DUPLICADO (descartar) | #1415 fixed; confirma `reserve_shadow` | — | No reabrir. |
| w2-lp-16 | DUPLICADO (descartar) | #1417 fixed/mitigado; confirma cap + tiempos | — | No reabrir. |
| w2-lp-17 | DUPLICADO (descartar) | #1182 fixed; test citado pasa | — | No reabrir. |
| w2-lp-18 | DUPLICADO (descartar) | #1190 fixed; sin guard `is_root` | — | No reabrir. |
| w2-lp-19 | DUPLICADO (descartar) | #1180 fixed; sentinel `None` | — | No reabrir. |
| w2-lp-20 | DUPLICADO (descartar) | #1186 fixed; `is_clipped` en `blend_pixel` | — | No reabrir. |

Notas transversales:

- **Ningún SOLO NUEVO**: los 12 vivos mapean 1:1 a issues deferred/CLOSED conocidos; los 8 restantes mapean a fixed. Cero hallazgos nuevos genuinos.
- **GAPS.md**: sin solape (matriz de features ausentes: transform, iframe, canvas…; nada de estos 20 bugs de paint/layout). Sin conflicto.
- **Accionabilidad general**: los 12 vivos traen archivo:línea + fix concreto. Reserva probatoria sólo en w2-lp-03 y w2-lp-10 (sin fixture de píxeles; mesa A decide).
- **Citas de spec**: CSS 2.2 §10.3.3 (w2-lp-12) válida. Los números de sección de CSS Backgrounds 3 (§2.11.2, §6) y CSS Images 3 (§5.5) no se verificaron en esta mesa; los documentos existen, las secciones exactas debe confirmarlas el chair antes de publicar.

## Concerns

1. **Deuda 100 % deferred, cero novedad**: todo lo vivo ya estaba archivado como deferred. El valor de la ola es la evidencia fresca en HEAD (repros R1/R2/R5/R6, prueba de inalcanzabilidad en w2-lp-07), no hallazgos nuevos. Chair: `include` = comentar evidencia en issues existentes, nunca abrir issues nuevos.
2. **Tracker con severidades infladas y un dup interno**: #1179 (P1 por un skip silencioso), #1196/#1211 (P1 por un path inalcanzable sin efecto visible; además #1211 duplica a #1196 título idéntico). Propuestas: #1179→P2, #1196→P3 y cerrar #1211 como dup. Sin esto, reabrir/comentar perpetúa el ruido de prioridad.
3. **Upgrades propuestos por el auditor (w2-lp-02 P3→P2, w2-lp-04 P3→P2, w2-lp-08 P3→P2)**: defendibles pero son decisión del chair previa a comentar, para no mover etiquetas sin consenso.
