# Production Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cerrar el hito M9: llevar todas las barras de aceptación del spec a verde, publicar una alpha/beta/producto real y someter el proyecto a auditoría externa con bus factor > 1.

**Architecture:** Un arnés de aceptación agrega todas las métricas (WPT, Test262, sitios, fuzz, seguridad, rendimiento, distribución, a11y) y falla si falta alguna. Un panel público muestra el estado real. La auditoría externa y la formación de maintainers cierran el proyecto como profesional.

**Tech Stack:** Python 3.11, GitHub Actions, `gh`, generadores de estado.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (§2 y hito M9)

## Global Constraints

- Cero nums estimados: todo se lee de artefactos generados.
- No se declara "producción" hasta que TODA la barra esté verde.
- Auditoría externa con scope y resultados publicados.

## Review Focus

1. **Badge de producción prematuro** → arnés que exige todas las barras.
2. **Bus factor 1** → al menos 3 maintainers con commits de motor en el último trimestre.
3. **Métrica stale** → check de frescura en CI.

---

### Task 1: Arnés de aceptación

**Files:** `tools/production_gate.py`, `docs/status/production.html`
**Steps:**
- [ ] Test que falla: con un subconjunto de barras verdes, el gate falla y lista las faltantes.
- [ ] Implementar agregación de: WPT allowlist=0, Test262 allowlist=0, 100/100 sitios, crash-free≥99,9%, 0 fugas, 3 SO sandbox enforced, Media/WebGL E2E, DevTools/WebDriver suites, rendimiento, distribución firmada.
- [ ] Verificar gate con datos sintéticos: verde solo si todo está verde.
- [ ] Commit.

### Task 2: Corpus de 100 sitios reales

**Files:** `docs/status/real_sites.toml` (crecer a 100), `tools/real_site_check.py`
**Steps:**
- [ ] Definir corpus diverso (noticias, e-commerce, docs, redes, apps, medios, i18n) con snapshots.
- [ ] Cada sitio con criterio PASS explícito y warnings=0.
- [ ] Job de CI que reporta degradaciones y genera WPs.
- [ ] Commit.

### Task 3: Auditoría externa y fuzz masivo

**Files:** `docs/audit/external/*`, workflows de fuzz.
**Steps:**
- [ ] Campaña de fuzz de 1 M ejecuciones por target con resultados publicados.
- [ ] Auditoría de seguridad por terceros con scope, hallazgos y fixes.
- [ ] Publicar informe y cerrar hallazgos con WPs.
- [ ] Commit.

### Task 4: Bus factor y gobernanza

**Files:** `docs/GOVERNANCE.md`, `CODEOWNERS`, `docs/CONTRIBUTING.md`
**Steps:**
- [ ] Al menos 3 maintainers con commits de motor recientes y revisiones registradas.
- [ ] Rotación de revisores y proceso de incorporación.
- [ ] Verificar con script que ningún área tenga un único owner activo.
- [ ] Commit.

### Task 5: Publicación

**Files:** workflows de release, `docs/RELEASE.md`
**Steps:**
- [ ] Tags firmados, artefacts verificados, notas con known issues reales.
- [ ] Canales stable/beta/continuous con la documentación completa.
- [ ] Anuncio con números conseguidos y límites declarados.
- [ ] Commit.
