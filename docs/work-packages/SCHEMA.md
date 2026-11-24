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

Opcionales: `impact` (1–10), `frequency` (1–10). Prioridad = `impact × frequency`.

| `profile` | str | `light` o `heavy`, derivado de `verify_cmd`: `cargo`/WPT/Test262/fuzz son `heavy`; Python/docs/git son `light`. Permite repartir trabajo entre una máquina grande y otra con poco disco. |

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

1. `python3 tools/wp_plan.py --board --profile light` (o `heavy`) → elegir el WP de mayor prioridad que quepa en tu máquina.
2. Leer el plan del hito en `docs/superpowers/plans/`.
3. TDD: test que falla → fix mínimo → test que pasa (`AGENTS.md` §5).
4. Gate completo de `AGENTS.md` §6.
5. Commit atómico con el ID del WP: `feat(wpt): WP-WPT-… fix …`.
6. El humano revisa y fusiona. Los agentes **nunca** fusionan.

## Estado de implementación

Los siete generadores tienen parsers y validan cada WP antes de escribirlo. Sus artefactos son `tests/wpt/results/latest.json`, `tests/test262/results/latest.json`, `docs/status/real-sites.json`, `fuzz/artifacts/*.json`, `docs/audit/coverage.tsv`, `docs/work-packages/depth-inventory.toml` y `docs/work-packages/user-reports.json` (o `gh issue list`). Con artefactos ausentes, cada generador devuelve cero WPs: nunca inventan trabajo.
