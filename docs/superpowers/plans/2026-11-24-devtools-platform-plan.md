# DevTools & Developer Platform Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hacer de Megi un navegador profesional para desarrolladores: DevTools vía protocolo Firefox, WebDriver BiDi, about: pages y perfilado de red/render.

**Architecture:** Se implementa un servidor de protocolo en el browser que expone dominios (DOM, CSS, Runtime, Network, Performance) sobre el estado real del motor. WebDriver BiDi se apoya en el mismo bus. Las about: pages se renderizan con el propio motor (dogfooding).

**Tech Stack:** Rust 1.91, protocolo DevTools de Firefox, WebDriver BiDi, `serde_json`, `crates/browser`.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (hito M7)

## Global Constraints

- El protocolo no debe permitir ejecutar código arbitrario fuera del sandbox del render.
- Toda respuesta debe derivarse de datos reales del motor, no de simulaciones.
- Sin dependencias con FFI en el motor.

## Review Focus

1. **DevTools inventa estilos** → usar el árbol de estilos real.
2. **WebDriver no controla la ventana real** → integrar con el event loop real.
3. **Perfilado sin datos** → instrumentar `RenderStats` y red reales.

---

### Task 1: Servidor de protocolo y dominio DOM

**Files:** `crates/browser/src/devtools/{server,dom}.rs`
**Steps:**
- [ ] Test que falla: cliente conecta y obtiene el DOM real de una página.
- [ ] Implementar handshake y dominio DOM sobre el documento real.
- [ ] Commit.

### Task 2: Dominios CSS, Runtime y Network

**Files:** `crates/browser/src/devtools/{css,runtime,network}.rs`
**Steps:**
- [ ] Test: estilos aplicados y calculados reales; evaluación en consola; waterfall de red con request/response reales.
- [ ] Implementar sobre `ComputedStyle`, `JsRuntime` y `RenderStats`/net.
- [ ] Commit.

### Task 3: WebDriver BiDi

**Files:** `crates/browser/src/webdriver/*`
**Steps:**
- [ ] Test: launch, navigate, evaluar JS, screenshot, eventos de usuario.
- [ ] Integrar con el event loop real.
- [ ] Suite WebDriver pasa.
- [ ] Commit.

### Task 4: about: pages

**Files:** `crates/browser/src/about/*`
**Steps:**
- [ ] `about:megi`, `about:history`, `about:config`, `about:memory`, `about:performance`, `about:crashes`.
- [ ] Renderizadas por el propio motor; tests de contenido.
- [ ] Commit.

### Task 5: Perfilado y económica de render

**Files:** `crates/browser/src/devtools/performance.rs`, `docs/status/*`
**Steps:**
- [ ] Grabar/reproducir performance, flamegraph de fases, exportar JSON.
- [ ] Test de que los tiempos corresponden a `RenderTimings` reales.
- [ ] Commit.
