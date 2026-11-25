# Megi Ambition — Master Implementation Plan (v2, sin calendarización)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Llevar Megi a un navegador 100 % utilizable en producción mediante hitos de capacidad encadenados y un motor de work packages que mantiene ocupados a los agentes de IA de forma continua.

**Architecture:** Trece hitos (M0–M12) organizados por dependencias lógicas, sin fechas. Cada hito se cierra con una barra de aceptación medible. Nueve planes de subsistema producen software verificable de forma independiente. El motor de WPs regenera la cola de trabajo a partir de siete fuentes automáticas.

**Tech Stack:** Rust 1.91 / 17+ crates / Boa 0.22; Python 3.11 stdlib; GitHub Actions; cosign; Landlock/rustix; wasmi; quinn/h3; symphonia; software WebGL; DevTools protocol.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (v2)

## Global Constraints

- `#![forbid(unsafe_code)]` en todos los crates `megi-*`; FFI solo en workspace separado con revisión dedicada.
- MSRV 1.91, toolchain CI 1.99.1. Sin JIT/GPU en el perfil `deterministic` (por defecto en CI); perfil `performance` explícito cuando se active JIT/GPU.
- Barra de producción del spec §2 es el destino; ningún plan puede rebajarla.
- Máximo 10 WPs `in_progress`; el motor de WPs regenera la cola cuando una fuente se agota.
- Cada tarea termina con commit y gate verde; nunca se fusiona con CI roja.
- Los números publicados son **conseguidos**, nunca planeados.

## Review Focus

1. **Los agentes se quedan sin trabajo** → motor de WPs con siete fuentes y regla de relleno automático (plan `work-package-engine`).
2. **La ambición se convierte en dispersión** → hitos con barra de aceptación y límite de 10 WPs activos.
3. **JIT/GPU/hilos rompen los goldens** → perfiles `deterministic`/`performance` con tests en ambos.
4. **Se reabren anti-objetivos de la v1 sin condición** → cada carrera diferida (JIT, WebGL, vídeo, SW completo, mobile) exige spike y entrada por hito.
5. **Se publica una meta como logro** → separación estricta entre badges conseguidos y roadmap.

---

## Mapa de planes

| Plan | Cubre hitos | Entregable verificable |
|---|---|---|
| `2026-11-24-wpt-harness-plan.md` | M0, continuo en M2/M4/M9 | WPT `P/T` por época, allowlist monotónica, badge separado |
| `2026-11-24-security-isolation-plan.md` | M0, M3, M5 | 0 fugas T1/T2, sandbox por SO, network/image process |
| `2026-11-24-engine-depth-plan.md` | M1, M2, M4 | E2E de tareas reales, bidi/CJK, Canvas 2D, SW fase 1, WASM |
| `2026-11-24-release-product-plan.md` | M0, M5, M9 | release verificada, canales, feedback, gobernanza |
| `2026-11-24-js-evolution-plan.md` | M8 | corpus Test262, JIT o migración, paralelismo JS |
| `2026-11-24-graphics-media-plan.md` | M6 | Canvas 2D completo, WebGL software, imagen/vídeo/audio |
| `2026-11-24-web-platform-extension-plan.md` | M4, M8 | SW completo, HTTP/3, storage/capabilities, offline |
| `2026-11-24-devtools-platform-plan.md` | M7 | DevTools, WebDriver BiDi, about: pages, perfilado |
| `2026-11-24-work-package-engine-plan.md` | continuo | cola auto-regenerada, generadores, priorización, dashboard |
| `2026-11-25-zen-inspired-ui-plan.md` | M5 (shell, sin tocar el motor) | rail vertical opcional, modo compacto, tokens y atajos con paridad a11y |

## Encadenamiento de hitos

```
M0 (verdad y cimiento)
 ├─ WPT Tasks 1-4 ────────────────┐
 ├─ Seguridad Tasks 1-3 ─────────┤
 └─ Release Tasks 1-2 ───────────┤
                                 ▼
M1 (interactividad) ◄─ Motor Tasks 1-4 + Release Task 3
                                 ▼
M2 (profundidad) ◄─ WPT Task 7 + Motor Tasks 5-6
                                 ▼
M3 (frontera seguridad) ◄─ Seguridad Tasks 4-6
                                 ▼
M4 (componentes) ◄─ Motor Tasks 7-8 + Web Platform Tasks 1-3
                                 ▼
M5 (plataforma nativa) ◄─ Seguridad Task 7 + Release Tasks 4-6
                                 ▼
M6 (gráficos/media) ◄─ Graphics-Media Tasks 1-6
                                 ▼
M7 (herramientas) ◄─ DevTools Tasks 1-5
                                 ▼
M8 (motor ofensivo) ◄─ JS Evolution + Web Platform Tasks 4-6
                                 ▼
M9 (producción) ◄─ Production readiness + auditoría externa
                                 ▼
M10 (plataforma extendida) ◄─ Extended platform (WebGPU/WebXR/WebRTC, extensiones, sync, mobile)
                                 ▼
M11 (integración nativa) ◄─ Native integration (perfiles por SO)
                                 ▼
M12 (media/legal) ◄─ Media licensing (DRM/EME con datos, o límite documentado)

El motor de WPs corre EN PARALELO desde el primer día y nunca se detiene.
```

**Regla de dependencia**: un hito puede iniciarse antes de cerrar el anterior **solo si no comparte archivos y su barra de aceptación no depende del anterior**. Si M1 se retrasa, M4 puede avanzar; M9 no, porque agrega todas las barras.

---

## Barras de cierre por hito

| Hito | Cierre (todo verde) |
|---|---|
| M0 | WPT report publicado con SHA; 0 fugas T1/T2; watchdog <6 s; release con verify + cosign |
| M1 | 10 fixtures E2E pasan; corpus JS sin fallos de binding no clasificados; real-sites sin console_errors |
| M2 | WPT época 2 con allowlist reducida; fixtures bidi/canvas/md5 estables; solapes conocidos cerrados |
| M3 | 0 fugas cross-site; 31+ targets fuzz; 12 pruebas negativas de sandbox; site-a no puede pedir site-b |
| M4 | 100+ subtests shadow-dom; offline demo con red cortada; WASM export correcto |
| M5 | a11y auditada; sandbox enforced en Linux/macOS/Windows; impresión y descargas E2E |
| M6 | corpus de media E2E; WebGL WPT; AVIF/JPEG XL; audio/vídeo reproducibles |
| M7 | suites DevTools y WebDriver BiDi pasan; about: pages; perfilador usable |
| M8 | benchmarks publicados; JIT o motor alternativo decidido por datos; sin regresión no declarada |
| M9 | barra completa del spec §2; 100/100 sitios; allowlists a 0; auditoría externa; bus factor >1 |
| M10 | WebGPU/WebXR/WebRTC completos, extensiones, sync E2E y mobile publicado |
| M11 | matriz `enforced` por SO para selectores, credenciales, notificaciones y permisos |
| M12 | DRM/EME decidido con ADR público: funciona, o límite documentado con alternativas |

---

## Autorevisión

- **Cobertura del spec**: §2 (barra de producción) → M9 + todos los planes; §4.1 (hitos) → tabla de encadenamiento; §4.3 (motor de WPs) → plan dedicado; §4.2 (carreras diferidas) → planes js-evolution, graphics-media, web-platform-extension, devtools-platform; §5 (profesionalización) → release-product + gobernanza. Cubierto.
- **Sin calendarización**: no hay meses ni fechas en los hitos; solo dependencias lógicas. Correcto.
- **Sin topes funcionales**: 25.000 subtests pasa a ser piso, no techo; anti-objetivos v1 pasan a hitos M5–M8, y M10–M12 cubren WebGPU/WebXR/WebRTC, extensiones, sync, mobile, integración nativa y la decisión legal sobre DRM. Correcto.
- **Consistencia**: los mismos nombres de tipos y barras se usan en los planes de subsistema. Correcto.
- **Proporción**: el master es un índice; los detalles están en los nueve planes. Correcto.
