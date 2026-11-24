# JS Evolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Llevar el subsistema JavaScript a nivel de producción: corpus Test262 creciente hasta allowlist 0, JS paralelo al render, y **JIT o migración de motor decidida por datos**, sin romper el perfil determinista.

**Architecture:** Un arnés Test262 propio (similar al WPT) mide semántica del motor. En paralelo se instrumenta Boa para identificar cuello de rendimiento. Con esos datos se decide: optimizar/contribuir a Boa, activar un JIT en perfil `performance`, o migrar una vez a rquickjs. El perfil `deterministic` sigue usando el intérprete y es el de CI.

**Tech Stack:** Rust 1.91, Boa 0.22, Test262, rquickjs como alternativa, perfil `performance` feature-gateado.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (hito M8)

## Global Constraints

- Perfil `deterministic` por defecto en CI/goldens; perfil `performance` explícito y documentado.
- `#![forbid(unsafe_code)]` en código propio; un JIT con codegen requiere decisión ADR y perfil separado.
- Toda migración de motor es **una sola vez**, con capa de bindings reutilizable.

## Review Focus

1. **Corpus Test262 maquillado** → denominador = todos los tests importados; allowlist monotónica.
2. **JIT cambia resultados** → lockstep: mismo script en intérprete y JIT debe dar mismo valor ( patrón Ladybird).
3. **Migración prematura** → solo con datos del corpus y gate explícito.

---

### Task 1: Arnés Test262

**Files:**
- Create: `crates/test_harness/src/js262/*`
- Create: `tests/js262/WPT-REVISION.json` equivalente (`TEST262-REVISION.json`), manifiesto y allowlist
- Create: `tools/vendor_test262.py`, `tools/js262_gate.py`
- Test: `crates/test_harness/tests/js262.rs`

**Interfaces:**
- Produces: `megi-test-harness --js262 --report ...` imprimiendo `test262 <sha>: P/T`, con la misma disciplina que WPT.

- [ ] **Step 1: Test que falla**

Script que uses `assert.sameValue` y compruebe que el runner reporta PASS; script con error de semántica reporta FAIL. Ambos fallan hoy.

- [ ] **Step 2: Verificar que falla**

Run: `cargo test -p megi-test-harness --test js262` → FAIL.

- [ ] **Step 3: Implementar**

Vendorizado por SHA, shim de `assert.js`/`sta.js`, ejecución sobre `JsRuntime`, wptreport equivalente, allowlist monotónica y gate.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-test-harness --test js262 && python3 tools/js262_gate.py --check` → OK.

- [ ] **Step 5: Commit**

```bash
git add crates/test_harness/src/js262 tests/js262 tools/vendor_test262.py tools/js262_gate.py
git commit -m "feat(js): Test262 harness with monotonic allowlist"
```

### Task 2: Instrumentación y datos de rendimiento

**Files:**
- Create: `crates/bench/src/benches/js_hot.rs`
- Modify: `docs/PERFORMANCE.md`
- Test: benchmark reproducible.

**Interfaces:**
- Produces: p50/p95 de scripts calientes, % de tiempo en motor vs bindings, identified bottlenecks.

- [ ] **Step 1: Definir métrica**

Test: el benchmark reporta `engine_time_ms` y `binding_time_ms` por separado. Hoy no existe.

- [ ] **Step 2: Verificar que falla**

Run: `cargo bench -p megi-bench js_hot` → FAIL.

- [ ] **Step 3-4: Implementar y verificar**

Añadir benchmarks con scripts reales del corpus; publicar baseline.

- [ ] **Step 5: Commit**

```bash
git add crates/bench docs/PERFORMANCE.md
git commit -m "bench(js): engine vs binding time breakdown"
```

### Task 3: Spike de JIT y decisión

**Files:**
- Create: `docs/adr/2026-11-24-js-jit-decision.md`
- Test: spike descartable (no se conserva en `main`).

**Interfaces:**
- Produces: decisión documentada: (a) contribuir/actualizar Boa, (b) JIT en perfil `performance`, (c) migrar a rquickjs.

- [ ] **Step 1-2: Spike**

Prototipar en worktree desechable: medir ganancia de un JIT sobre Boa y coste de migración a rquickjs con la capa de bindings existente.

- [ ] **Step 3: Decisión**

ADR con datos: ganancia medida, coste, impacto en MSRV/unsafe/determinismo. Si se elige JIT, se implementa solo en perfil `performance`.

- [ ] **Step 4-5: Verificar y commitear**

```bash
git add docs/adr/2026-11-24-js-jit-decision.md
git commit -m "docs(adr): JS engine evolution decision from measured data"
```

### Task 4: Ejecución de la decisión

**Files:** según ADR.
**Interfaces:** acorde a la decisión.
**Steps:** TDD por change; gate en ambos perfiles; publicar benchmarks antes/después; commit.

### Task 5: JS paralelo al estilo/layout

**Files:**
- Modify: `crates/render/src/scripts/`, `crates/browser/src/worker/`
- Test: fixtures con script largo + animación CSS concurrentes.

**Interfaces:**
- Produces: animaciones y `:hover` siguen activos mientras JS bloquea; sin datos de layout obsoletos.

- [ ] **Step 1: Test que falla**

Fixture: script bloquea 500 ms; la animación debe avanzar durante ese tiempo. Hoy se congela.

- [ ] **Step 2: Verificar que falla**

Run: `cargo test -p megi-render js_parallel` → FAIL.

- [ ] **Step 3-4: Implementar y verificar**

Mover estilo/layout/paint a hilo propio con sincronización explícita; `getBoundingClientRect` espera a layout pendiente.

- [ ] **Step 5: Commit**

```bash
git add crates/render/src/scripts crates/browser/src/worker tests/fixtures/render/js_parallel.html
git commit -m "feat(render): style/layout off main thread with deterministic sync"
```
