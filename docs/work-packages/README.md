# Motor de Work Packages — guía operativa

> Implementación mínima disponible hoy: esquema, validación, priorización y tablero. Los parsers concretos de los siete generadores los añade `2026-11-24-work-package-engine-plan.md` (Tasks 1–4). Fuente: spec §4.3.

## Mapa de archivos

| Archivo | Estado | Papel |
|---|---|---|
| `tools/wp_schema.py` | ✅ implementado | `WorkPackage` + `validate_wp` |
| `tools/test_wp_schema.py` | ✅ implementado | 5 tests del esquema |
| `tools/wp_generate.py` | ✅ andamiaje | 7 fuentes registradas, sin artefactos ⇒ 0 WPs |
| `tools/test_wp_generate.py` | ✅ implementado | 3 tests de generadores |
| `tools/wp_plan.py` | ✅ implementado | `select()` con tope de 10 y `render_board()` |
| `tools/test_wp_plan.py` | ✅ implementado | 4 tests del planificador |
| `docs/work-packages/SCHEMA.md` | ✅ implementado | Campos y ciclo del agente |
| `docs/work-packages/BOARD.md` | ✅ generado | Columnas queued/in_progress/review/done |

## Siete generadores

| Fuente | Subcomando | Métrica |
|---|---|---|
| Fallos WPT | `wpt` | `wpt_passed` |
| Fallos Test262 | `js262` | `js262_passed` |
| Sitios reales | `site` | `sites_passed` |
| Crashes de fuzz | `fuzz` | `fuzz_crashes` |
| Deuda de auditoría | `audit` | `audit_pending` |
| Subsets incompletos | `depth` | `depth_closed` |
| Reportes de usuario | `user` | `user_issues` |

## Comandos exactos

```bash
python3 tools/wp_generate.py --all --dry-run
python3 tools/wp_generate.py wpt
python3 tools/wp_plan.py --board
python3 -m unittest tools.test_wp_schema tools.test_wp_generate tools.test_wp_plan
```

## Ciclo del agente

1. `wp_plan --board` → elegir el WP de mayor prioridad (impacto × frecuencia).
2. Leer el plan del hito en `docs/superpowers/plans/`.
3. TDD: test que falla → fix mínimo → test que pasa (AGENTS.md §5).
4. Gate completo de AGENTS.md §6, más los tests del motor de WPs.
5. Commit atómico: `feat(wpt): WP-WPT-… fix …`.
6. El humano revisa y fusiona. Los agentes **nunca** fusionan.

## Priorización

`priority = impact × frequency` (ambos 1–10). Tope duro de 10 WPs `in_progress`. Si la cola baja de 20, el workflow `wp-refill.yml` (planificado) regenera WPs desde las siete fuentes.

## Dashboard

`docs/status/work-packages.html` (generado, pendiente de implementación en el plan) contará WPs cerrados desde commits con el ID y calculará throughput semanal con números reales, nunca estimados.
