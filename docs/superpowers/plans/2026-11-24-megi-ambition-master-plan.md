# Megi Ambition — Master Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ejecutar la estrategia de 12 meses del spec de ambición: medir con WPT real, cerrar explotos, profundizar el motor y convertir Megi en una alpha descargable y verificable, defendiendo cinco ejes ganables frente a Ladybird.

**Architecture:** Cuatro planes independientes (WPT, seguridad, motor, release/producto) que producen software verificable por separado. Este índice fija el orden, las dependencias entre planes y la regla de parada. Cada plan tiene sus propios tests y gate; ninguno se fusiona con CI roja.

**Tech Stack:** Rust 1.91 / 17 crates / Boa 0.22; Python 3.11 stdlib; GitHub Actions; cosign keyless; Landlock/rustix; wasmi/unicode-* (con enmienda ADR).

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md`

## Global Constraints

- Todo lo del spec: `#![forbid(unsafe_code)]` en 17 crates, MSRV 1.91, toolchain CI 1.99.1, sin JIT propio, sin WebGL, sin vídeo FFmpeg, sin motor JS propio, sin fundación en 12 meses.
- Semántica de sitio = eTLD+1 con la PSL existente; sandbox real solo en Linux; reportes a borrador de GitHub Issue, sin backend.
- **Regla de parada**: no iniciar la fase Q(n+1) de un eje hasta que el criterio de aceptación de Q(n) esté verde. Si Q1 del motor se retrasa, desplazar Q3 (SW/WASM), nunca Q2 (lo que el usuario ve).
- Cada tarea termina con commit y gate verde; las ramas se limpian al merge con `tools/git_hygiene.sh` (sin `-D`).

## Review Focus

1. **Cualquier plan publica una métrica autorreferencial como si fuera conformidad** → el plan WPT separa badge WPT de capability coverage y prohíbe agregarlos.
2. **Un plan de seguridad se salta el orden F0→F1→F2→F3** → este índice bloquea F2/F3 hasta que T1/T2/T5/T9 estén verdes.
3. **El motor abre demasiados frentes a la vez** → límite de 10 WPs activos y regla de parada Q1→Q2→Q3→Q4.
4. **Release publica sin verificar o con artefacto no reproducible** → el plan de release añade job `verify` y test de doble build.
5. **La gobernanza pierde trabajo en worktrees/ramas** → higiene que solo lista, nunca borra sin revisión.

---

## Mapa de planes

| Plan | Entregable verificable | Q1 | Q2 | Q3 | Q4 |
|---|---|---|---|---|---|
| `2026-11-24-wpt-harness-plan.md` | `WPT <sha>: 1.700/2.500` → `17.000/25.000` | Tasks 1–4 | Task 7 | — | rebaseline |
| `2026-11-24-security-isolation-plan.md` | 0 fugas T1/T2, watchdog <6 s, sandbox Linux | Tasks 1–3 | Task 5 | Task 6 | Task 7 |
| `2026-11-24-engine-depth-plan.md` | real-sites 4 PASS, replay 20/20 | Tasks 1–4 | Tasks 5–6 | Tasks 7–8 | Task 9 |
| `2026-11-24-release-product-plan.md` | release verificada + canales + feedback | Tasks 1–3 | Tasks 4–5 | Task 6 | Task 7 |

## Secuencia de ejecución

### Ola 1 — Semanas 1–4 (BLOQUEANTE: medir y no publicar basura)

1. **Seguridad Task 1** (docs que mienten) y **Task 2/3** (T1/T2/T3/T5/T9). Escapa de Q1 y es requisito de credibilidad.
2. **WPT Tasks 1–4** (pin, shim, runner, gate). Sin esto seguimos midiendo con 30 checks caseros.
3. **Release Tasks 1–2** (verify + firma). Antes de cualquier publicación.

Gate de ola: `cargo test --workspace`, harness PASS, `wpt_gate --check`, 0 fugas T1/T2, `update_progress --check`.

### Ola 2 — Meses 2–3

4. **Motor Tasks 1–4** (corpus JS, event loop, formularios, SSE).
5. **Release Task 3** (canales continuous/beta).

Gate de ola: `real-sites` sin console_errors en example.com/HN; POST loopback 200; continuous publicado ≥90 % de días.

### Ola 3 — Meses 4–6

6. **WPT Task 7** (época 2, puente getComputedStyle).
7. **Seguridad Task 5** (sandbox Linux).
8. **Motor Tasks 5–6** (bidi/CJK, Canvas 2D).
9. **Release Tasks 4–5** (feedback y welcome page).

Gate de ola: WPT 6.000/9.000; 12 pruebas negativas de sandbox; fixtures bidi/canvas con md5 estable.

### Ola 4 — Meses 7–9

10. **Motor Tasks 7–8** (Shadow DOM, SW fase 1, WASM).
11. **Seguridad Task 6** (network process).

Gate de ola: ≥100 subtests shadow-dom; `sw_offline` funciona con red cortada; site-a no puede pedir site-b.

### Ola 5 — Meses 10–12

12. **Motor Task 9** (replay, huella, aceptación final).
13. **Seguridad Task 7** (SiteInstance y matriz).
14. **Release Task 6–7** (gobernanza y formatos, con puerta de tracción).

Gate final: G1–G5 del spec verdes.

## Métricas de éxito globales (del spec)

- WPT `17.000/25.000` con SHA pegado y allowlist decreciente.
- `real-sites.html`: 4 PASS / 0 DEGRADED, 0 errores JS, <2,5 s.
- Replay 20/20; `unsafe_lines = 0` en `megi-*`; binario ≤40 MB; RSS ≤100 MB.
- 31 targets de fuzz; 0 fugas cross-site; watchdog <6 s.
- Release con checksum + cosign + attestation verificados; canales activos; SLA de triaje 72 h.

## Autose revisión del plan

- **Cobertura del spec**: G1 (WPT+real-sites) → WPT 1–7 y motor 9; G2 (tareas reales) → motor 1–4; G3 (seguridad) → seguridad 1–7; G4 (determinismo) → motor 9; G5 (producto) → release 1–7. Cubierto.
- **Preguntas del builder check**: T1–T10 en spec §3; denominador por época en spec §4.1; eTLD+1 en §4.3; destino de reportes en §4.4; sandbox por plataforma en §4.3. Ninguna queda sin respuesta.
- **Consistencia de tipos**: `subtest_key`, `WptReport`, `Validated<T>`, `CookiePolicyError`, `FileGrantTable`, `SiteInstance`, `PersistentEventLoop`, `FormBody` se definen una sola vez en el plan que los introduce y se consumen por su nombre.
- **Proporción**: el plan es más largo que el spec porque descompone cuatro subsistemas en tareas ejecutables; cada tarea conserva solo firmas, tests y comandos, no transcribe el código.
