# Work Package Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Construir el sistema que mantiene trabajo constante para los agentes de IA: generadores automáticos de work packages a partir de siete fuentes, priorización, límites de concurrencia y dashboard de productividad.

**Architecture:** Cada fuente tiene un generador que produce WPs con esquema común (ID, fuente, archivo, expected/actual, spec, criterio, comando). Un planificador prioriza por impacto×frecuencia, limita a 10 WPs `in_progress`, y rellena la cola cuando baja de cierto umbral. Un dashboard publica métricas de throughput.

**Tech Stack:** Python 3.11 stdlib, TOML/JSON, GitHub Issues API vía `gh`, YAML de workflows.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (§4.3)

## Global Constraints

- Ningún WP puede modificar el motor sin test de regresión y gate verde.
- Los agentes preparan PRs; nunca fusionan. Revisión humana obligatoria.
- Máximo 10 WPs `in_progress`; relleno automático cuando la cola baje de 20.
- Las métricas del dashboard son **reales** (contadas desde artefactos), nunca estimadas.

## Review Focus

1. **La cola se agota** → relleno automático y siete generadores independientes.
2. **Un WP ambiguo** → esquema obligatorio con criterio y comando; validación al crear.
3. **Demasiados WPs concurrentes** → límite duro de 10 y bloqueo de nuevos inicios.
4. **Un agente fusiona sin revisión** → el planificador solo abre PRs; el merge lo hace el humano.
5. **WPs que no mueven métricas** → cada WP declara qué métrica mueve; cierre sin métrica se rechaza.

---

### Task 1: Esquema de WP y validador

**Files:**
- Create: `tools/wp_schema.py`
- Create: `docs/work-packages/SCHEMA.md`
- Test: `tools/test_wp_schema.py`

**Interfaces:**
- Produces: `WorkPackage` dataclass con `id, source, file, expected, actual, spec, acceptance, verify_cmd, metric, milestone`, y `validate_wp(dict) -> Result<WorkPackage, str>`.

- [x] **Step 1: Test que falla**

`test_wp_missing_verify_cmd_is_rejected`, `test_wp_metric_is_required`, `test_wp_id_format` (`WP-<FUENTE>-<clave>`).

- [x] **Step 2: Verificar que falla**

Run: `python3 -m unittest tools.test_wp_schema -v` → FAIL.

- [x] **Step 3: Implementar**

`wp_schema.py` con el dataclass y validación estricta; `SCHEMA.md` documenta campos y ejemplos.

- [x] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_wp_schema -v` → 3 tests OK.

- [x] **Step 5: Commit**

```bash
git add tools/wp_schema.py tools/test_wp_schema.py docs/work-packages/SCHEMA.md
git commit -m "feat(wp): work package schema and strict validator"
```

### Task 2: Siete generadores

**Files:**
- Create: `tools/wp_generate.py` con subcomandos `wpt, js262, site, fuzz, audit, depth, user`
- Create: `tools/test_wp_generate.py`

**Interfaces:**
- Consumes: `tests/wpt/results/latest.json`, Test262 results, `docs/status/real-sites.json`, `fuzz/artifacts/`, `audit_coverage.py --check`, inventario de subsets, issues con plantillas.
- Produces: archivos `docs/work-packages/queue/<id>.toml`.

- [x] **Step 1: Tests que fallan**

Un test por fuente: dado un artefacto sintético, el generador produce un WP válido con el `metric` correcto (`wpt_passed`, `js262_passed`, `sites_passed`, `fuzz_crashes`, `audit_pending`, `depth_closed`, `user_issues`).

- [x] **Step 2: Verificar que falla**

Run: `python3 -m unittest tools.test_wp_generate -v` → FAIL.

- [x] **Step 3: Implementar**

Cada generador lee su fuente, aplica deduplicación por ID y escribe TOML validado. El generador `depth` consulta un inventario `docs/work-packages/depth-inventory.toml` de subsets con caps pendientes de completar. El generador `user` usa `gh issue list` con las plantillas `crash`/`broken_site`.

- [x] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_wp_generate -v && python3 tools/wp_generate.py --all --dry-run`
Expected: 7 tests OK; salida lista WPs sin escribir.

- [x] **Step 5: Commit**

```bash
git add tools/wp_generate.py tools/test_wp_generate.py docs/work-packages/depth-inventory.toml
git commit -m "feat(wp): seven automatic work package generators"
```

### Task 3: Planificador y límite de concurrencia

**Files:**
- Create: `tools/wp_plan.py`
- Create: `docs/work-packages/BOARD.md` (generado)
- Test: `tools/test_wp_plan.py`

**Interfaces:**
- Consumes: cola de WPs, estado `in_progress` desde PRs abiertos con etiqueta `wp`.
- Produces: selección de hasta 10 WPs,BOARD priorizado, bloqueo de nuevos inicios si hay 10 activos.

- [x] **Step 1: Tests que fallan**

`test_max_ten_in_progress`, `test_priority_is_impact_times_frequency`, `test_refill_when_queue_below_twenty`, `test_agent_cannot_merge` (el planificador no genera comandos de merge).

- [x] **Step 2: Verificar que falla**

Run: `python3 -m unittest tools.test_wp_plan -v` → FAIL.

- [x] **Step 3: Implementar**

`wp_plan.py --select` imprime los WPs a iniciar, priorizados por `impact*frequency` (campos del WP), respetando el límite. `--board` regenera `BOARD.md` con columnas `queued/in_progress/review/done`. Sin comandos de merge.

- [x] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_wp_plan -v && python3 tools/wp_plan.py --board`
Expected: 4 tests OK; BOARD con WPs reales.

- [x] **Step 5: Commit**

```bash
git add tools/wp_plan.py tools/test_wp_plan.py docs/work-packages/BOARD.md
git commit -m "feat(wp): planner with concurrency cap and reflow"
```

### Task 4: Integración con CI y dashboard

**Files:**
- Create: `.github/workflows/wp-refill.yml`
- Modify: `.github/workflows/ci.yml` (unittest nuevos)
- Create: `docs/status/work-packages.html` (generado)
- Test: `tools/test_wp_dashboard.py`

**Interfaces:**
- Consumes: generadores + planificador.
- Produces: ejecución diaria del relleno, dashboard con throughput real.

- [x] **Step 1: Tests que fallan**

`test_workflow_runs_generators`, `test_dashboard_counts_closed_wps_from_commits`, `test_no_estimated_numbers`.

- [x] **Step 2: Verificar que falla**

Run: `python3 -m unittest tools.test_wp_dashboard -v` → FAIL.

- [x] **Step 3: Implementar**

Workflow diario que ejecuta los siete generadores y `wp_plan --board`, abre/actualiza un issue de seguimiento. Dashboard que cuenta WPs cerrados desde mensajes de commit `WP-<id>` y calcula throughput semanal. Sin números estimados.

- [x] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_wp_dashboard -v && python3 tools/check_docs_links.py`
Expected: 3 tests OK; links OK.

- [x] **Step 5: Commit**

```bash
git add .github/workflows/wp-refill.yml .github/workflows/ci.yml docs/status/work-packages.html tools/test_wp_dashboard.py
git commit -m "feat(wp): CI refill and real-throughput dashboard"
```
