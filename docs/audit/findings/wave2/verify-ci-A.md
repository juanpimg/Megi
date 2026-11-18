# Verificación Mesa A — CI/tooling + perf (ola 2)

Fecha: 2026-11-18. HEAD: `e87f206`. Verificador (Mesa A, scope tools/CI/perf).

## Resultado

**NEEDS INFO — sin materia que verificar.** Los informes fuente no existen
en el árbol:

- `docs/audit/findings/wave2/f3-tooling-ci-docs.md` — no existe.
- `docs/audit/findings/wave2/f4-perf.md` — no existe.

Búsqueda exhaustiva (`ls` del directorio wave2, `glob **/f3*` / `**/f4*` /
`**/*tooling*ci*`, `grep -rl "w2-ci\|w2-perf"` en `docs/` y
`.superpowers/`): cero coincidencias salvo la fila de planificación en
`.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/progress.md:42`,
que anuncia esos informes como entregables pendientes de otra sesión
(`ses_e119223abffeX4XbRiYp236FVF`). Ningún hallazgo `w2-ci-*` ni `w2-perf-*`
existe en ningún informe, ledger o índice. No hay `repro` que ejecutar;
construir repros desde cero sin descripción de hallazgo sería inventar la
auditoría, no verificarla.

No se ejecutó ningún repro/bench: no hay comando definido por el auditor.
No se editó código ni ledger.

## Tabla

| id | veredicto | comando | salida literal | nota |
|---|---|---|---|---|
| w2-ci-01 (esperado) | NEEDS INFO | — | — | informe fuente `f3-tooling-ci-docs.md` inexistente; hallazgo no publicado por el auditor |
| w2-ci-02 (esperado) | NEEDS INFO | — | — | idem |
| w2-ci-03 (esperado) | NEEDS INFO | — | — | idem |
| w2-ci-04 (esperado) | NEEDS INFO | — | — | idem |
| w2-perf-01 (esperado) | NEEDS INFO | — | — | informe fuente `f4-perf.md` inexistente; hallazgo no publicado por el auditor |
| w2-perf-02 (esperado) | NEEDS INFO | — | — | idem |
| w2-perf-03 (esperado) | NEEDS INFO | — | — | idem |
| w2-perf-04 (esperado) | NEEDS INFO | — | — | idem |

## Concerns

1. La Mesa C no puede dictaminar `include/exclude` sobre este scope hasta que
   la sesión auditora publique `f3-tooling-ci-docs.md` y `f4-perf.md`.
2. Recomendación de proceso: re-dispatch de esta verificación cuando los
   informes existan; los IDs `w2-ci-*`/`w2-perf-*` de la tabla son
   placeholders, no IDs reales del auditor.
