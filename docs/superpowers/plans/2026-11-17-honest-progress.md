# Estado honesto del proyecto + prueba real de render — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reemplazar el 99 % estimado por una cobertura verificable `implementadas/conocidas` estilo AnyPS5, con inventario de pendientes, y añadir un informe de render en webs reales.

**Architecture:** Un manifiesto TOML declara capacidades con evidencia verificable (gate/test/fixture). Un generador Python valida la evidencia, contrea y emite SVG/HTML/JSON/badge. `update_progress.py` consume el JSON para el titular. Un segundo script genera el informe de render desde instantáneas versionadas.

**Tech Stack:** Python 3.11 stdlib (`tomllib`, `unittest`, `xml.sax.saxutils`), Rust existente (`render --json`), SVG/HTML generados.

**Spec:** `docs/superpowers/specs/2026-11-17-honest-progress-design.md`

## Global Constraints

- Python 3.11+, solo biblioteca estándar; sin red en los comandos `--check`.
- No editar a mano artefactos generados (`PROGRESS.md`, `docs/status/*.svg`, `coverage.json`).
- No tocar los archivos modificados por el usuario: `crates/js/src/host.rs`, `crates/js/src/runtime.rs`.
- `state = "done"` sin evidencia verificable es un error fatal del generador.
- Estilo de mensajes y formato de artefactos coherente con `tools/update_progress.py`.

## Review Focus

- Capacidad `done` con evidencia rota (test renombrado, fixture borrado) → debe fallar el generador.
- Denominador creciente: añadir capacidad `missing` baja el % → test explícito.
- `partial` no debe contar como implementada pero sí aparecer listada.
- Gate PASS con suite ausente → evidencia `gate:` no válida.
- `coverage.json` ausente/obsoleto → `update_progress.py` no debe afirmar un % inventado.

---

### Task 1: Manifiesto y núcleo del generador

**Files:**
- Create: `docs/status/coverage.toml`
- Create: `tools/update_coverage.py`
- Test: `tools/test_update_coverage.py`

**Interfaces:**
- Produces: `load_manifest(path)->dict`, `verify_evidence(cap, root, gate)->(bool,str)`,
  `compute_stats(caps)->dict`, `build_artifacts(root, ...)->dict[str,bytes]`.

- [ ] **Step 1: RED — escribir tests del conteo y la evidencia**
  `tools/test_update_coverage.py` con: `test_overall_is_implemented_over_known`,
  `test_partial_counts_as_missing_and_is_listed`,
  `test_new_capability_grows_denominator_and_lowers_percent`,
  `test_done_requires_verifiable_test_evidence`,
  `test_gate_evidence_requires_suite_and_pass`, `test_none_evidence_forbidden_when_done`.
  Usar directorios temporales con `tempfile` y manifiestos mínimos.
- [ ] **Step 2: verificar que fallan** — `python3 -m unittest tools/test_update_coverage.py -v` → FAIL por módulo ausente.
- [ ] **Step 3: GREEN — implementar `tools/update_coverage.py`** (parseo, verificación de evidencia, conteo, `main` con `--write`/`--check`).
- [ ] **Step 4: GREEN — escribir `docs/status/coverage.toml`** con el inventario real (≈45 capacidades; evidencia `gate:`/`test:`/`fixture:` validada).
- [ ] **Step 5: verificar** — `python3 tools/update_coverage.py --write && python3 tools/update_coverage.py --check` → PASS; tests → PASS.
- [ ] **Step 6: Commit** — `git add docs/status/coverage.toml tools/update_coverage.py tools/test_update_coverage.py && git commit -m "feat(status): verifiable capability coverage manifest and generator"`.

### Task 2: Artefactos visuales y documentación de reglas

**Files:**
- Modify: `tools/update_coverage.py`
- Create: `docs/status/COVERAGE.md`
- Test: `tools/test_update_coverage.py` (ampliar)

**Interfaces:**
- Consumes: `build_artifacts` de Task 1.
- Produces: `docs/status/coverage.svg`, `docs/status/coverage.html`, `docs/status/coverage.json`, `docs/status/badge-coverage.svg`.

- [ ] **Step 1: RED — tests de salida** — `test_svg_has_area_bars`, `test_html_lists_missing_with_issue_links`, `test_json_has_area_breakdown`, `test_rules_doc_exists`.
- [ ] **Step 2: verificar que fallan** — unittest → FAIL (render no implementado).
- [ ] **Step 3: GREEN — implementar renderers** SVG/HTML/JSON/badge + `docs/status/COVERAGE.md`.
- [ ] **Step 4: verificar** — tests PASS; `--write && --check` PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(status): coverage SVG, HTML report and counting rules"`.

### Task 3: Integración en PROGRESS.md, progress.svg, README y CI

**Files:**
- Modify: `tools/update_progress.py`, `README.md`, `.github/workflows/ci.yml`
- Regenerate: `PROGRESS.md`, `docs/status/progress.svg`, badges.

**Interfaces:**
- Consumes: `docs/status/coverage.json` de Task 2 (`overall.percent`, `overall.implemented`, `overall.known`).

- [ ] **Step 1: RED — test de integración** — `test_progress_headline_uses_coverage` en `tools/test_update_coverage.py`: al renderizar con un `coverage.json` de 60 %, el Markdown y el SVG contienen "60%" y no el promedio de fases.
- [ ] **Step 2: verificar que falla** — unittest → FAIL.
- [ ] **Step 3: GREEN — `update_progress.py` lee `coverage.json`** (si existe y es reciente; si no, mantiene el número antiguo pero añade aviso). Añadir enlace a cobertura y aclarar que las barras de fases son checklist histórico.
- [ ] **Step 4: GREEN — README y CI** — badge de cobertura apuntando al informe; claim HTML5 acotado al gate; nuevo paso `--check` en el job Docs freshness.
- [ ] **Step 5: verificar** — `python3 tools/update_progress.py --write --no-gate && --check --no-gate` PASS; `cargo test` no es necesario pero sí `git diff --check`.
- [ ] **Step 6: Commit** — `git commit -m "feat(status): make PROGRESS.md headline reflect measured coverage"`.

### Task 4: Prueba real de render

**Files:**
- Create: `docs/status/real_sites.toml`, `tools/real_site_check.py`, `tools/test_real_site_check.py`
- Create: `tests/fixtures/real-sites/*.html` (instantáneas)
- Generate: `docs/status/real-sites.html`, `docs/status/real-sites.json`, `docs/status/real-sites/*.png`

**Interfaces:**
- Produces: `classify(stats)->str`, `render_report(sites, results)->str`.

- [ ] **Step 1: RED — tests de clasificación y report** — `test_blank_page_when_no_display_items`, `test_degraded_when_warnings`, `test_pass_when_clean`, `test_report_links_issues`.
- [ ] **Step 2: verificar que fallan** — unittest → FAIL.
- [ ] **Step 3: GREEN — implementar `real_site_check.py`** con `--live`/`--offline`, ejecución de `render --json`, guardado de PNG y generación de informe.
- [ ] **Step 4: GREEN — instantáneas y manifiesto** — descargar/guardar 4 páginas reales y crear `real_sites.toml`.
- [ ] **Step 5: verificar** — `python3 tools/real_site_check.py --offline` genera informe con capturas reales; tests PASS.
- [ ] **Step 6: Commit** — `git commit -m "feat(status): real-site render check with screenshots"`.

### Task 5: Verificación final y publicación

- [ ] **Step 1:** `python3 tools/update_coverage.py --check && python3 tools/update_progress.py --check --no-gate`.
- [ ] **Step 2:** `python3 -m unittest tools/test_update_coverage.py tools/test_real_site_check.py -v`.
- [ ] **Step 3:** `git diff --check`, revisar `git status` (los dos ficheros del usuario siguen sin tocar).
- [ ] **Step 4:** revisión final del branch (fresh reviewer si hay subagent; si no, autoinspección declarada).
- [ ] **Step 5:** `git push origin main` (el usuario pidió subirlo al repo).
