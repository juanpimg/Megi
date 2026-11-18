# Revisión final — Ola 2 de auditoría (2026-11-18)

- Revisor: subagente final (sólo lectura; sin editar código ni ledger, sin publicar).
- BASE: `35151bd8e8b3fc9bc33acec9d04a7019c4ac205e`. Cierre de ola: `0114f08`.
  HEAD real en revisión: `b1b3e08` (el usuario avanzó un commit tras el cierre).
- Árbol con cambios sin commitear ajenos a la ola: `M crates/css/src/properties.rs`,
  `?? crates/css/tests/numeric_ranges.rs`.

## Veredicto: CON HALLAZGOS (5: 1 medio-bajo, 3 bajos, 1 informativo)

Ningún hallazgo invalida el fondo de la ola: los 73 issues existen y están
abiertos, los 7 avisos `security` no se publicaron y la cobertura real es total
(0 pendientes). Los hallazgos afectan a la **reproducibilidad del gate estricto
citado en el cierre** y a la **trazabilidad** (plan + 2 informes fuente + filas
del ledger). Detalle en §Hallazgos.

## Tabla de comprobaciones (comando + salida)

| # | Comprobación | Comando | Salida (resumen) | Estado |
|---|---|---|---|---|
| 1 | Cobertura strict | `python3 tools/audit_coverage.py --check --strict` (HEAD) | `error: not in ledger: docs/audit/findings/wave2/07-closeout.md (docs/F3)` · `error: not in ledger: docs/audit/findings/wave2/PUBLISHED.md (docs/F3)` · `error: crates/css/src/properties.rs: 6006 -> 6022 lines, re-read needed` | FALLA (ver H1) |
| 2 | Cobertura report | `python3 tools/audit_coverage.py --report` (HEAD) | `TOTAL 1320 files, 818 covered, 0 pending, 254563 lines tracked, 100%` en toda categoría auditable | OK |
| 3 | 73 issues existen y públicos | `gh issue view <n> --json number,state,title` para 10 muestras (1618, 1625, 1636, 1645, 1647, 1655, 1664, 1667, 1681, 1690) | Los 10 devuelven `"state": "OPEN"` con título del ámbito esperado | OK |
| 4 | 73 issues abiertos, sin bajas | `gh issue list --state open --limit 100 --json number,title` + chequeo programático | Los 73 publicados (`#1618–#1687`, `#1689–#1691`) presentes en la lista open; `#1688` ausente (es el issue de feature del usuario, `feat(iframe)…(#1544)`, ajeno a la ola) | OK |
| 5 | Cuerpos con evidencia+Dedup+base | `gh issue view {1618,1664,1691} --json body` | Los 3 contienen `Dedup` y `Commit:`; `issues/*.md` (73 ficheros) todos con mención `dedup` y sección de evidencia/pasos | OK |
| 6 | Security no publicados | `grep` de los 7 IDs en `PUBLISHED.md` (sólo aparecen en §Avisos privados, ninguno con `→#`) + `gh issue list --state all --limit 200` filtrado por patrones (`Set-Cookie.*PUSH`, `status bogus`, `máscara.*fija`, `amplificación`, `favicon.*file:`, `tmp.*predecible`, IDs `w2-net-001/002/003/d01`, `w2-render-01`, `1173`, `1176`) | 0 coincidencias en el tracker; borradores presentes en `advisories/` (3 ficheros: `w2-render-01`, `w2-st-1173-file-icon`, `w2-st-1176-tmp-writers`; los 4 de net en `meeting-net.md`) | OK |
| 7 | Coherencia actas↔publicados | Conteo includes por acta: core 14 + js 7 + net 20 + browser 2 + f1 8 + assets 9 + render 3 + ci/perf 7 + shell 3 = **73** = ficheros en `issues/` (73) = filas de `PUBLISHED.md` | Cuadra exacto; lp aporta 0 nuevos (20 evaluados: 12 comment + 8 exclude), documentado | OK |
| 8 | Descartes con motivo | `meeting-{core,js,net,render,assets,lp,st,f1,ci,browser}.md` §descartes + cierre §Resultados | Todos los excluidos tienen motivo (convergente, oráculo roto, fuera de ola, needs-repro, YA REPORTADO→comment, advisory privado, mantenimiento). Ver H5 por imprecisión menor de conteo | OK con nota |
| 9 | Sin edición de producto por la ola | `git diff 35151bd..HEAD --stat -- crates/` → 6 ficheros; `git log 35151bd..HEAD -- <esos ficheros>` | `crates/js/{host,runtime}.rs`, `crates/net/{websocket.rs,tests/websocket_tests.rs}`, `crates/render/{lib,scripts}.rs` — todos de commits del usuario/otras sesiones (`#1534` wss/TLS, `#1544` contentDocument, secret-scan, merges). Ningún commit `audit(wave2)` toca `crates/` | OK |
| 10 | Gates del cierre vs logs | `tail /tmp/megi-wave2-final.log` + `/tmp/megi-wave2-final2.log` (3628/3189 líneas, suites `ok`, 0 failed en las líneas inspeccionadas) | Los logs muestran tests pasando, pero no contienen la salida `--check --strict` citada en el cierre; dicha salida no reproduce en `0114f08` ni en HEAD (ver H1) | NO VERIFICABLE (ver H1) |
| 11 | Plan y ledger de la ola | `ls docs/superpowers/plans/` + `git show 0114f08:docs/audit/coverage.tsv \| grep -i wave2` | El plan `2026-11-18-auditoria-linea-por-linea-megi.md` **no existe** (sólo `2026-11-17-honest-progress.md`); el ledger no tiene filas para `wave2/07-closeout.md` ni `wave2/PUBLISHED.md` (la única fila de cierre apunta a `f7/00-closeout.md`, que es el cierre viejo F0–F7 del 17-11) | HALLAZGOS H1/H2 |

## Hallazgos

- **H1 (medio-bajo, proceso): gate `--check --strict` citado en el cierre no
  reproducible.** El cierre (`07-closeout.md`) cita
  `audit ledger ok: 1320 files, 818 covered, 0 pending, …, 0 file(s) to re-read,
  0 unknown file(s)`. Hoy `--strict` falla con 3 errores (tabla, fila 1). Causas:
  (a) `PUBLISHED.md` se añadió en el propio commit de cierre `0114f08` sin fila
  de ledger — el gate sólo pudo estar verde antes de escribirlo, nunca en el
  commit citado; (b) `07-closeout.md` se commiteó después (`b1b3e08`, commit del
  usuario que barrió el fichero) también sin fila; la fila de cierre existente
  apunta a `docs/audit/findings/f7/00-closeout.md` (documento distinto, ola
  anterior); (c) deriva post-cierre sin commitear del usuario en
  `crates/css/src/properties.rs` (6006→6022 líneas) más fichero sin seguimiento
  `crates/css/tests/numeric_ranges.rs`. La cobertura de fondo es correcta
  (`--report`: 0 pendientes, 100 %). Acción: añadir filas de ledger para los dos
  ficheros wave2, corregir la fila `f7→wave2` si procede, y re-marcar
  `properties.rs` tras relectura antes de invocar `--strict` como gate.
- **H2 (bajo, trazabilidad): el plan de la ola no está en el repo.** El ledger
  (línea 1) y esta revisión referencian
  `docs/superpowers/plans/2026-11-18-auditoria-linea-por-linea-megi.md`, que no
  existe (`glob` vacío; en `docs/superpowers/plans/` sólo está
  `2026-11-17-honest-progress.md`). La ejecución es reconstruible vía ledger +
  actas, pero plan↔ejecución no es verificable desde el árbol.
- **H3 (bajo, evidencia): faltan 2 informes fuente de auditores.**
  `docs/audit/findings/wave2/f2-layout-paint.md` y `f5-break-browser.md` no
  existen (tampoco `verifier-template.md`); las actas `meeting-lp.md:5` y
  `meeting-browser.md:10` lo documentan y aplicaron regla conservadora (lp: 0
  includes nuevos; browser: include sólo con re-verificación propia a nivel de
  código). Los 12 comentarios lp llevan reserva explícita de corrección si
  aparece Mesa A. Limita la re-auditoría independiente de esos dos scopes desde
  el repo. (Los `verify-lp-A.md` / `verify-net-A/B.md` ausentes durante las
  mesas sí existen hoy: fueron restaurados según el cierre punto 1.)
- **H4 (informativo, deriva): el árbol siguió mutando tras el cierre.**
  `0114f08 → b1b3e08` (usuario: doc `parse_ws_url` wss) + cambios sin commitear
  en `crates/css/` (ver H1c). No invalida la ola (base auditada fijada en
  `3c06db8` y re-comprobaciones documentadas, p. ej. patrones `websocket.rs`
  tras el cambio a `wss://`), pero cualquier gate futuro exige recalcular la
  cola de relectura.
- **H5 (bajo, precisión del cierre): inexactitudes menores en el cierre.**
  (a) La fila del ledger dice `#1618-#1691`, rango que incluye `#1688`, que es
  el issue de feature del usuario (`#1544`), ajeno a la ola (los publicados son
  73: `#1618–#1687` + `#1689–#1691`). (b) "8 de st (ya corregidos…)" no cuadra
  literalmente con `meeting-st.md` (F18 needs-repro + ~13 verificados-corregidos
  `#1172/#1468/#1200/#1202–#1210/#1168` + `#1201` fuera de scope + 1 falso
  positivo). (c) El cierre no menciona la discrepancia de ledger que sí recoge
  `meeting-lp.md:180-184` (`FINDINGS.md` lista `#1181/#1194/#1182` como
  deferred mientras Mesa B los da por fixed → resincronización pendiente,
  correctamente bloqueada con "no reabrir").

## Notas que NO son hallazgos (verificado, correcto)

- `comment-existing` no publicados como comentarios (12 lp, 13 st, 6 render,
  3 assets, 2 js, 1 browser — conteos cuadran con las actas): decisión explícita
  y documentada para no hacer ruido en issues cerrados; textos listos en actas.
- `w2-render-11` excluido por pertenecer a un commit posterior a la base; honesto
  y correcto.
- `test_harness --strict` con 38 diffs de parse-error y resto de limitaciones
  honestas del cierre (puntos 1–5): coherentes con las actas, sin contradicción
  encontrada.
