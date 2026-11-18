# Mesa C — reunión de verificación (scope tools/CI/perf, `w2-ci-*` + `w2-perf-*`)

- Fecha: 2026-11-18. Chair: Mesa C (scope tools/CI/perf).
- Commit verificado: `3c06db8` (Mesa A: worktree limpio `/tmp/opencode/megi-verify`;
  `git diff 3c06db8 HEAD --` sobre todos los paths del scope → vacío, vale en HEAD actual).
- Fuentes: `f3-tooling-ci-docs.md` (w2-ci-01…03), `f4-perf.md` (w2-perf-01…05),
  `verify-ci-A.md` (8/8 VERIFIED con salida literal), `verify-ci-B.md`
  (7 REQUIERE MÁS EVIDENCIA + w2-ci-02 mantenimiento).
- Nota de procedimiento: Mesa B fiscalizó sin materia (informes ausentes en ese
  momento, ya restaurados). Dado que A verificó los 8 con salida real y las fuentes
  ya existen, esta mesa incluye los 7 con A=VERIFIED y no-duplicados como issues
  públicos; w2-ci-02 queda como tarea de mantenimiento (no issue). NO se borra nada,
  no se edita código ni ledger, no se publica.

## Tabla final

| id | veredicto | sev | área | etiquetas | incl/excl | motivo |
|---|---|---|---|---|---|---|
| w2-ci-01 | VERIFIED (A) | P2 | tools/gate | public, bug | include | Centinela `"PÁGINA EN BLANK"` vs `"PÁGINA EN BLANCO"` de `classify()`; repro `blank_gate.py` con salida literal. Dedup: new. |
| w2-ci-02 | VERIFIED (A) pero NO ISSUE | P3 máx | ledger/docs | maintenance | exclude (mantenimiento) | Fixes #1308/#1311 existen en HEAD; sólo la fila de FINDINGS está stale por boilerplate de cierre. Arreglo procedural (regenerar/anotar), no defecto de producto. Ver `maintenance` abajo. |
| w2-ci-03 | VERIFIED (A) | P3 | docs/fuzz | public, docs | include | Deriva 26→28 bins, matriz 25+9, corpus 170/258/158 — deriva bajo cualquier número. Rel. #1507 (re-deriva posterior al cierre). |
| w2-perf-01 | VERIFIED (A) | P2 | bench | public, perf | include | Strip 0/29; bench 0,816 vs 0,818 (A: 0,307 vs 0,307). Rel. #1197. |
| w2-perf-02 | VERIFIED (A) | P2 | bench | public, perf | include | 800×600=0,48 MPix vs 0,096 declarado (5×). Rel. #1198. |
| w2-perf-03 | VERIFIED (A) | P3 | bench | public, perf | include | Warmup cap silencioso, `--warmup 0` aceptado, meta con pedidos. Rel. #1199 (parcial). |
| w2-perf-04 | VERIFIED (A) | P2 | bench | public, perf | include | 3 call sites con fallback, producción con measurer real. Rel. #1201. |
| w2-perf-05 | VERIFIED (A) | P2 | svg/perf | public, perf | include | Scan lineal intacto; +41–42 % a 1024², +88 % a 2048². Rel. #1270. |

## Decisión sobre dedups vivos (w2-ci-03, w2-perf-01…05)

- Opción elegida: **abrir como issues nuevos referenciando el cerrado** (no
  comment-existing ni reapertura).
- Justificación: los 6 tienen evidencia NUEVA en HEAD `3c06db8` con verificación
  independiente A (mediciones/conteos/grep + runtime), posterior al cierre como
  Deferred con boilerplate. #1507 es re-deriva (bins #1424/#1426 entraron DESPUÉS
  del cierre); #1197/#1198/#1199/#1201/#1270 siguen vivos en código (parcial #1199
  documentado como tal). Un comentario en un issue cerrado no reabre triage ni deja
  traza de la nueva ronda de medición; un issue nuevo con `Rel. #n` preserva el
  historial y permite cerrar el ciclo con el fix. La regla del plan lo permite con
  evidencia nueva — aquí la hay.
- w2-ci-01 es `new` sin dedup; entra directo.

## Incluidos como issue público (7)

w2-ci-01, w2-ci-03, w2-perf-01…05. Cuerpos en `docs/audit/findings/wave2/issues/<id>.md`.

## Maintenance (1, sin issue público)

- w2-ci-02 (P3 máx, ledger): #1308 (`set -o pipefail` vía `7c81cb4` sobre ambos
  `| tee`, `ci.yml:138,184`) y #1311 (`set -e` vía `0243396` en `coverage.yml`)
  están fixed en HEAD; `FINDINGS.md:108-109` los lista `Deferred` porque
  `audit_recover_issues.py:105-131` clasifica sólo por texto del comentario de
  cierre. Acción para la sesión principal: nuevo comentario de cierre "fixed" +
  regenerar FINDINGS (o anotar excepción); considerar regla que cruce comentario
  ↔ commits `fix(...)(#n)`. Aviso del fiscal: si 2/118 filas "still broken" están
  fixed, puede haber más staleness del mismo origen — conviene re-verificar la
  sección Deferred contra `main` fuera del circuito de issues.

## Descartes (0)

- Ningún hallazgo VERIFIED se descarta; B no marca DUPLICADO en ninguno (sus 7
  REQUIERE MÁS EVIDENCIA eran por informes ausentes, ya restaurados y verificados
  por A con salida real).

## Concerns heredados de A (no bloquean)

1. Corpus w2-ci-03: 170 (A) vs 258 (auditor) vs 158 (docs) — deriva confirmada
   bajo los tres; varianza anotada, posible conteo con artefactos en su momento.
2. Tiempos absolutos w2-perf-01/05 difieren por carga/método, pero deltas y
   dirección reproducen (+41 % auditor vs +42 % A a 1024²).
3. w2-perf-02/03/04 verificados por código+aritmética/CLI sin re-medir baselines
   completas; suficiente (defecto estructural).
