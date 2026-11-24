# Work Package Schema

> Fuente: `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` §4.3.
> Validador: `tools/wp_schema.py` (`validate_wp`).

Un **work package (WP)** es la unidad atómica de trabajo para los agentes de IA. Todo WP debe mover una métrica al cerrarse; los que no la mueven se rechazan.

## Campos obligatorios

| Campo | Tipo | Regla |
|---|---|---|
| `id` | str | `WP-<FUENTE>-<clave>`, p. ej. `WP-WPT-css-syntax-003` |
| `source` | str | `wpt`, `js262`, `site`, `fuzz`, `audit`, `depth` o `user` |
| `file` | str | ruta exacta al archivo a modificar |
| `expected` | str | comportamiento esperado (según spec) |
| `actual` | str | comportamiento observado (según artefacto) |
| `spec` | str | sección de especificación o barra del roadmap |
| `acceptance` | str | no vacío: qué significa "terminado" |
| `verify_cmd` | str | no vacío: comando que demuestra el cierre |
| `metric` | str | `wpt_passed`, `js262_passed`, `sites_passed`, `fuzz_crashes`, `audit_pending`, `depth_closed` o `user_issues` |
| `milestone` | str | hito M0–M12 |
| `profile` | str | `light` o `heavy`, derivado de `verify_cmd`: `cargo`/WPT/Test262/fuzz son `heavy`; Python/docs/git son `light`. Permite repartir trabajo entre una máquina grande y otra con poco disco. |

Opcionales: `impact` (1–10), `frequency` (1–10). Prioridad = `impact × frequency`.

## Fuentes y generadores

| Fuente | Generador | Métrica |
|---|---|---|
| Fallos WPT | `tools/wp_generate.py wpt` | `wpt_passed` |
| Fallos Test262 | `... js262` | `js262_passed` |
| Sitios reales | `... site` | `sites_passed` |
| Crashes de fuzz | `... fuzz` | `fuzz_crashes` |
| Deuda de auditoría | `... audit` | `audit_pending` |
| Subsets incompletos | `... depth` | `depth_closed` |
| Reportes de usuario | `... user` | `user_issues` |

## Ciclo del agente

1. `python3 tools/wp_plan.py --board --profile light` (o `heavy`) → elegir el WP de mayor prioridad que quepa en tu máquina. `python3 tools/wp_plan.py --select --profile light` imprime hasta 10 WPs **descontando los ya en curso** (tope duro de 10).
2. Leer el plan del hito en `docs/superpowers/plans/`.
3. TDD: test que falla → fix mínimo → test que pasa (`AGENTS.md` §5).
4. Gate completo de `AGENTS.md` §6.
5. Reservar antes de empezar: `python3 tools/wp_plan.py --claim WP-…` (falla si ya hay 10 en curso). Esto evita que dos agentes hagan el mismo WP.
6. Commit atómico con el ID del WP: `feat(wpt): WP-WPT-… fix …`. El comando de verificación debe pasar **después** del cambio; los WPs `depth` usan `python3 tools/check_milestone.py <M> <n>`, que exige mover el criterio de `pending` a `items` con evidencia en `progress.toml`.
7. Cerrar: `python3 tools/wp_plan.py --done WP-… --metric "qué métrica se movió"`. Esto saca el WP de la cola y lo registra en `closed.toml`; el dashboard cuenta sólo cierres con métrica declarada y commit que mencione el ID. Un simple mensaje de commit menciona un ID no cierra nada.
8. El humano revisa y fusiona. Los agentes **nunca** fusionan.

## Estado de implementación

Los siete generadores tienen parsers y validan cada WP antes de escribirlo. Sus artefactos son `docs/status/wpt.json` (informe real `megi-wpt-report/1`, con `tests/wpt/results/latest.json` como feed heredado), `tests/test262/results/latest.json`, `docs/status/real-sites.json`, `fuzz/artifacts/*.json`, `docs/audit/coverage.tsv`, `docs/work-packages/depth-inventory.toml` y `docs/work-packages/user-reports.json` (o `gh issue list`). Los WPT usan la clave canónica del subtest y deduplican por ID; su `verify_cmd` es `tools/wpt_gate.py --check` (rojo con un fallo no incluido en la allowlist, verde al pasarlo). Con artefactos ausentes, cada generador devuelve cero WPs: nunca inventan trabajo.
