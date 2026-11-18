# Verificación Mesa B — scope shell/bench/harness (`f2-shell-tools.md`)

Fiscal de dedup y severidad sobre `docs/audit/findings/wave2/f2-shell-tools.md`
(19 hallazgos `w2-st-*`: F1–F19). No se edita código ni ledger.
Fecha: 2026-11-18. Nota: no existe ningún `verifier-template.md` en el árbol
(`glob **/*verif*.md` solo devuelve los `verify-*.md` de wave2); se usa como
plantilla el formato de `verify-js-B.md` / `verify-ci-B.md` (tabla de veredictos
+ concerns).

## Fuentes de dedup consultadas

- `docs/audit/FINDINGS.md`: filas #1163, #1165, #1166, #1167, #1168, #1169,
  #1171, #1172, #1173, #1174, #1175, #1176, #1177, #1178 (shell),
  #1197, #1198, #1199, #1201 (bench) — todas Deferred en ledger.
- `docs/GAPS.md`: brechas de producto ausente (form submission, live event loop,
  JS APIs, a11y tree…). Ninguna fila cubre los 19 defectos (son defectos de
  código existente, no features ausentes).
- `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json`
  (1000 issues; 5 OPEN: #1544, #1541, #1534, #1519, #1516 — ninguna de scope
  shell/bench/harness). Los 19 issues de referencia están CLOSED.
- `docs/audit/findings/wave2/issues/`: sin ficheros `w2-st-*` (sin colisión con
  borradores existentes).
- Verificación puntual contra el árbol: `favicon.rs:50-129` (sin chequeo de
  esquema, confirma F2), `download.rs:106-161` (tmp predecible + overflow sin
  `exists()`, confirma F9/F18).
- Escala aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass;
  P2 = corrupción visible o contrato roto; P3 = cosmetic/docs/bench.

## Tabla de veredictos

| id | veredicto | sev. propuesta | título concreto | accionable | nota fiscal |
|----|-----------|----------------|-----------------|------------|-------------|
| F1 w2-st-1169-session-tabs | YA REPORTADO (#1169) | P2 (mantener) | sí | sí | Evidencia nueva real (merge solo contempla `input`, `view.tabs` muerta en rama `Some`). Reapertura/comentario en #1169, no issue nuevo. `needs-repro` honesto (requiere display). |
| F2 w2-st-1173-file-icon | YA REPORTADO (#1173) | P2 (mantener) | sí | sí | **security + advisory: NUNCA issue público.** Verificado en código: `fetch` rama `File` lee disco sin comprobar página (líneas 83–99), `for_page` no filtra (116–129). Evidencia nueva fuerte (4 renders + diff 128 px + control `file:`). Reportar por canal privado según `SECURITY.md` (aviso GitHub o email). P1 discutible (página remota lo dispara sola) pero el ancho del oráculo (16×16 px) justifica P2. |
| F3 w2-st-1166-optgroup-rows | YA REPORTADO (#1166) | P2 (mantener) | sí | sí | Evidencia nueva (divergencia exacta de constructores + 3 call sites). Reapertura/comentario en #1166. `needs-repro` honesto. |
| F4 w2-st-1167-find-offset | YA REPORTADO (#1167) | P2 (mantener; P3 admisible) | sí | sí | Doble defecto localizado (offset chrome + índice DOM vs fragmento). Reapertura en #1167. P3 aceptable si P2 se reserva a pérdida de datos. |
| F5 w2-st-1165-reader-url | YA REPORTADO (#1165) | P2 (mantener) | sí | sí | Cadena persistencia→restore documentada. Reapertura en #1165. |
| F6 w2-st-1163-history-push | YA REPORTADO (#1163) | P2 (mantener) | sí | sí | `start_navigation` sin push, localizado. Reapertura en #1163. |
| F7 w2-st-1175-headless-flags | YA REPORTADO (#1175) | P3 (ok) | sí | sí | Estático, verificable con `grep`. Reapertura/comentario en #1175. |
| F8 w2-st-1171-no-fsync | YA REPORTADO (#1171) | P2 (mantener; P3 admisible) | sí | sí | `grep sync_all` → 0, probado. `needs-repro` solo para el evento de pérdida — correcto. Reapertura en #1171. |
| F9 w2-st-1176-tmp-writers | YA REPORTADO (#1176) | P2 (mantener) | sí | sí | **security + advisory: NUNCA issue público.** Verificado: `write_atomic` tmp predecible + `fs::write` (trunca y sigue symlinks, `download.rs:151-152`); 3 call sites siguen vulnerables, solo `session.rs` se migró. Canal privado según `SECURITY.md`. `needs-repro` honesto. |
| F10 w2-st-1174-narrow | YA REPORTADO (#1174) | P3 (ok) | sí | sí | Guard exacto localizado. Reapertura en #1174. |
| F11 w2-st-1197-strip-noop | YA REPORTADO (#1197) | P2 (mantener; P3 admisible) | sí | sí | Diff vacío ejecutado (30 vs 30). Es solo bench (sin impacto a usuario): P3 admisible, mantengo P2 por contrato de la herramienta roto. Reapertura en #1197. |
| F12 w2-st-1198-units | YA REPORTADO (#1198) | P3 (ok) | sí | sí | One-liner (`0.096` → `0.48`). Reapertura en #1198. |
| F13 w2-st-1199-warmup | YA REPORTADO (#1199) | P3 (ok) | sí | sí | Líneas exactas. Reapertura en #1199. |
| F14 w2-st-1177-stale-docs | YA REPORTADO (#1177) | P3 (ok) | sí | sí | 5 contradicciones localizadas. Reapertura en #1177; conviene reescritura única de la sección (ver concern 4). |
| F15 w2-st-1178-find-count | YA REPORTADO (#1178) | P3 (ok) | sí | sí | Descarte en dos niveles. Reapertura en #1178. |
| F16 w2-st-headless-tabs | NUEVO (espejo headless de #1169) | P2 (ok) | sí | sí | md5 idénticos ejecutados en worktree limpio. Opción: issue nuevo con referencia a #1169, o seguimiento de #1169. No hay issue previo headless. |
| F17 w2-st-reader-quadratic | NUEVO | P2 (mantener; P3 admisible) | sí | sí | Ejecución con delta ~3 s en debug; en ventana corre en el event loop (freeze UI). P3 admisible porque el escenario exige página hostil + acción del usuario y el№ en release es menor. Fix concreto (bottom-up una pasada). |
| F18 w2-st-overflow-overwrite | NUEVO | P3 (ok) | sí | sí | Confirmado en código (`download.rs:118` sin `exists()`; contradice "never overwrite" de línea 6). `needs-repro` honesto (requiere 1024 descargas + ventana). |
| F19 w2-st-session-doc-drift | NUEVO | P3 (ok) | sí | sí | Inspección concluyente. Fix = docs o fallback. |

## Concerns

1. **Advisories privados (F2, F9) — nunca issues públicos.** Ambos con evidencia
   sólida en HEAD y verificados contra el código actual. La Mesa C debe
   marcarlos `exclude` del circuito de issues y derivarlos al canal privado de
   `SECURITY.md` (aviso GitHub o email al maintainer). Tampoco deben aparecer
   sus detalles en borradores bajo `issues/`.
2. **Regla de reapertura.** F1–F15 solapan con issues CLOSED/Deferred con
   evidencia genuinamente nueva (repros ejecutados, divergencias exactas, diffs
   de píxeles). La acción correcta es comentario/reapertura del issue original,
   no 15 issues nuevos. Solo F16–F19 son candidatos a issue nuevo (y F16 puede
   ir como seguimiento de #1169).
3. **`needs-repro` en F1, F3–F6, F10, F18.** Etiqueta honesta (rama window sin
   display en CI), pero la evidencia de código es concluyente; no debe leerse
   como debilidad del hallazgo sino como límite del harness.
4. **Deuda de docs (F14).** Coincido con el informe: reescribir la sección
   "What it does not do yet" de `lib.rs` de una vez, no en parches por issue.
5. **#1201 fuera de scope** (crate `layout` no leída): correctamente no
   fiscalizado como hallazgo; queda como concern del informe fuente.
6. **Falso positivo descartado (mayúsculas `site_origin`)**: correctamente no
   reportado (`net` fuera de scope, normalización probable de host).
7. **No se creó ni publicaría ningún issue desde esta fiscalización; no se
   tocó código ni ledger.** Conteo: 19 hallazgos fiscalizados — 15 YA
   REPORTADOS (con evidencia nueva, vía reapertura), 4 NUEVOS (F16–F19),
   de los cuales 2 (F2, F9) son security+advisory por canal privado.
