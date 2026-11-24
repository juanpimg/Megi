# Motor de Work Packages — guía operativa

> Implementación disponible hoy: esquema y validación, siete generadores con parsers de artefactos, planificador con tope de concurrencia, recompletado diario y dashboard de cierres reales, cola y distribución light/heavy. Fuente: spec §4.3 y `2026-11-24-work-package-engine-plan.md` (Tasks 1–4).

## Mapa de archivos

| Archivo | Estado | Papel |
|---|---|---|
| `tools/wp_schema.py` | ✅ implementado | `WorkPackage`, `validate_wp`, `classify_profile` y métricas permitidas |
| `tools/test_wp_schema.py` | ✅ implementado | tests del esquema |
| `tools/wp_generate.py` | ✅ implementado | siete generadores; sin artefactos ⇒ 0 WPs |
| `tools/test_wp_generate.py` | ✅ implementado | tests de generadores |
| `tools/wp_plan.py` | ✅ implementado | `select()`, `needs_refill()`, `claim()`, `done()` y `render_board()` |
| `tools/test_wp_plan.py` | ✅ implementado | tests del planificador |
| `tools/wp_dashboard.py` | ✅ implementado | dashboard de cierres reales, cola y distribución light/heavy |
| `tools/test_wp_dashboard.py` | ✅ implementado | tests del dashboard |
| `tools/check_milestone.py` | ✅ implementado | verifica criterios M0–M12 contra `progress.toml` |
| `tools/test_check_milestone.py` | ✅ implementado | tests del verificador de hitos |
| `.github/workflows/wp-refill.yml` | ✅ implementado | recompletado diario de la cola |
| `docs/work-packages/SCHEMA.md` | ✅ implementado | campos, perfiles y ciclo del agente |
| `docs/work-packages/depth-inventory.toml` | ✅ implementado | inventario real de subsets pendientes |
| `docs/work-packages/queue/` | ✅ generado | WPs validados, uno por archivo TOML |
| `docs/work-packages/BOARD.md` | ✅ generado | columnas queued/in_progress/review/done |
| `docs/status/work-packages.html` | ✅ generado | dashboard con métricas reales de cierre |

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

## Contratos de artefactos

Los generadores sólo leen artefactos reales; si faltan, devuelven cero WPs. Cada WP se valida con `validate_wp` antes de escribirse en la cola; las entradas inválidas se descartan y los IDs se deduplican.

| Fuente | Artefacto | Entradas |
|---|---|---|
| `wpt` | `tests/wpt/results/latest.json` | `failures[]` |
| `js262` | `tests/test262/results/latest.json` | `failures[]` |
| `site` | `docs/status/real-sites.json` | `results[]` (se omiten los sitios `PASS`) |
| `fuzz` | `fuzz/artifacts/*.json` | `crashes[]` |
| `audit` | `docs/audit/coverage.tsv` | filas con `status = pending` |
| `depth` | `docs/work-packages/depth-inventory.toml` | `[[entry]]` con criterios M0–M12 pendientes |
| `user` | `docs/work-packages/user-reports.json` o issues abiertas con etiqueta `crash`/`broken_site` vía `gh` | cuerpo del reporte |

El campo `profile` se deriva del `verify_cmd`: `cargo`/WPT/Test262/fuzz son `heavy`; Python/docs/git son `light`. Así una máquina con poco disco puede trabajar sólo con WPs ligeros.

## Comandos exactos

```bash
python3 tools/wp_generate.py --all --dry-run
python3 tools/wp_generate.py wpt
python3 tools/wp_plan.py --board --profile light
python3 tools/wp_plan.py --select --profile light
python3 tools/wp_plan.py --claim WP-DEPTH-m0-01
python3 tools/wp_plan.py --done WP-DEPTH-m0-01 --metric "criterio M0#4 cubierto"
python3 tools/wp_dashboard.py --write
python3 tools/wp_dashboard.py --check
python3 -m unittest tools.test_wp_schema tools.test_wp_generate tools.test_wp_plan tools.test_wp_dashboard tools.test_check_milestone
```

Cuando se combinan acciones en un mismo comando, `--board` y `--select` se evalúan **después** de `--claim`/`--done`: el tablero muestra el WP recién reservado en `in_progress` o el recién cerrado en `done`.

## Ciclo del agente

1. `wp_plan --board --profile light` (o `heavy`) → elegir el WP de mayor prioridad que quepa en tu máquina.
2. Leer el plan del hito en `docs/superpowers/plans/`.
3. TDD: test que falla → fix mínimo → test que pasa (AGENTS.md §5).
4. Gate completo de AGENTS.md §6, más los tests del motor de WPs.
5. Reservar con `wp_plan --claim WP-…` antes de empezar (tope duro de 10 en curso).
6. Commit atómico: `feat(wpt): WP-WPT-… fix …`.
7. Cerrar con `wp_plan --done WP-… --metric "…"`: el dashboard cuenta sólo cierres con métrica declarada y commit que mencione el ID.
8. El humano revisa y fusiona. Los agentes **nunca** fusionan.

## Priorización y recompletado

`priority = impact × frequency` (ambos 1–10). Tope duro de 10 WPs `in_progress`. Si la cola baja de 20, `needs_refill()` lo señala y el workflow `wp-refill.yml` regenera WPs desde las siete fuentes, actualiza el tablero y el dashboard, y abre un issue de seguimiento.

## Dashboard

`docs/status/work-packages.html` (generado por `wp_dashboard.py --write`) cuenta cierres desde `closed.toml`, exige commit y métrica declarados, y muestra la cola y su distribución en perfiles light/heavy con números reales, nunca estimados. `wp_dashboard.py --check` verifica que el artefacto generado esté al día.
