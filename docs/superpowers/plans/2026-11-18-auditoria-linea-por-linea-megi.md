# Auditoría línea por línea de Megi (ola 2) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Auditar el 100 % del código rastreado de `juanpimg/Megi` de arriba a abajo y línea por línea, ejecutar la suite completa, intentar romper el navegador con entradas hostiles y publicar issues accionables con evidencia reproducible, sin reabrir hallazgos ya dispositionados salvo prueba nueva.

**Architecture:** Se reutiliza el ledger `docs/audit/coverage.tsv` como prueba mecánica de cobertura (`tools/audit_coverage.py --generate/--mark/--report/--check --strict`). Cada archivo se lee con las siete lentes de riesgo, se contrasta con los 228 hallazgos de `docs/audit/FINDINGS.md` y `docs/GAPS.md`, y toda evidencia se guarda en `docs/audit/findings/wave2/` antes de proponer un issue. Cada hallazgo pasa por una mesa de tres subagentes (A reproductor, B fiscal, C chair). Los issues se publican tras verificación; los defectos de seguridad van a aviso privado según `SECURITY.md`.

**Tech Stack:** Rust 1.99.1 (`rust-toolchain.toml`, MSRV declarado 1.91), workspace de 14 crates, `cargo test`/`clippy`/`fmt`/`cargo-fuzz`, `python3 tools/audit_coverage.py`, `gh` CLI, Chrome real vía MCP `browser`, `python3 -m http.server`.

**Spec:** `docs/audit/README.md`, `docs/audit/FINDINGS.md`, `docs/audit/findings/f2/00-index.md`, `docs/ARCHITECTURE.md`, `SECURITY.md`, `docs/FUZZING.md`, `docs/GAPS.md`.

> Nota de reconstrucción: este plan fue borrado por una limpieza concurrente de `/tmp` durante la ejecución; se reescribió al cierre desde el ledger y las actas. Los pasos están marcados `[x]` porque ya se ejecutaron.

## Global Constraints

- El ledger es la única prueba de cobertura: se edita **sólo** con `tools/audit_coverage.py`; `--check --strict` debe quedar en verde al cierre.
- La ola es de **sólo lectura** sobre `crates/*/src`; los arreglos se proponen como issues/PRs separados. Reproducores en `docs/audit/repros/` y `docs/audit/findings/wave2/repro-*`.
- No tocar cambios sin commitear del usuario: ejecutar `git status --short` al inicio de cada tarea.
- No editar artefactos generados (`PROGRESS.md`, `docs/status/*`, `docs/audit/FINDINGS.md`). Hallazgos nuevos en `docs/audit/findings/wave2/`.
- Todo hallazgo necesita evidencia reproducible (comando + salida) y dedup contra 228 hallazgos, `GAPS.md` e issues. Sin evidencia → `needs-repro`, no publicar.
- Seguridad → aviso privado, nunca issue público.
- Publicación autorizada por el usuario previa verificación en mesa.

## Review Focus

- Entrada hostil al parser → panic/hang/OOM (Tareas 2–4, 9).
- Presupuestos anti-DoS contados tarde o reiniciados (Tareas 5, 6, 8, 9).
- Scripts/event loop: ejecución duplicada, sandbox, WebSocket bridge (Tareas 6, 9).
- Red: orden CORS/cookies, handshake WS, TLS (Tareas 5, 9).
- Integridad de CI/métricas: `| tee` sin `pipefail`, coverage floor, release (Tareas 7, 11).

---

### Tareas (todas completadas)

- [x] **Task 1 — F0: baseline e inventario.** Capturar HEAD, `--report`, `--check --strict`; `--generate`; verificar `cargo`/`gh`; congelar cola de relectura. → `00-baseline.md`.
- [x] **Task 2 — F1: tests, fuzz y fixtures pendientes.** Leer 43 archivos; ejecutar suites; hallazgos `w2-t-*`. → `f1-tests-fuzz-fixtures.md`.
- [x] **Task 3 — F2a: `dom`, `html`, `css`.** Lectura línea por línea con 7 lentes; hallazgos `w2-core-*`. → `f2-core.md`.
- [x] **Task 4 — F2b: `fonts`, `image`, `svg`, `paint`, `layout`.** → `f2-assets.md`, `f2-layout-paint.md`.
- [x] **Task 5 — F2c: `net`, `js`.** Incluye archivos con deriva. → `f2-net.md`, `f2-js.md`.
- [x] **Task 6 — F2d: `render`, `shell`, `bench`, `test_harness`.** → `f2-render.md`, `f2-shell-tools.md`.
- [x] **Task 7 — F3: tools, CI, manifiestos, docs.** Gates reales + falla inyectada. → `f3-tooling-ci-docs.md`.
- [x] **Task 8 — F4: rendimiento medido.** Baseline bench y contraste de hotspots. → `f4-perf.md`.
- [x] **Task 9 — F5: romper el navegador.** Repros headless con `timeout` y Chrome real. → `f5-break-browser.md`.
- [x] **Task 10 — F6: triaje, mesas de verificación y publicación.** A/B/C por scope; 73 issues públicos, 7 avisos privados. → `meeting-*.md`, `issues/`, `PUBLISHED.md`.
- [x] **Task 11 — F7: cierre verificable.** `--report`/`--check --strict`, suite completa, disposiciones. → `07-closeout.md`, `FINAL-REVIEW.md`.
