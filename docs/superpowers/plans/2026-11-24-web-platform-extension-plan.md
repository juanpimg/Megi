# Web Platform Extension Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Completar las capacidades web que hoy faltan y llevarlas a nivel de producción: Service Workers completos, HTTP/3, storage/capabilities y offline real.

**Architecture:** Cada capacidad se implementa con su propio WPT subset y su WP asociado. Service Workers se apoya en el event loop persistente y el network process. HTTP/3 se añade al stack de `net` con ALPN y fallback. Storage se persiste con cuotas y permisos.

**Tech Stack:** Rust 1.91, `quinn`+`h3`, `net`, `browser/src/sw`, IndexedDB/CacheStorage.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (hitos M4, M8)

## Global Constraints

- Sin permisos de red implícitos; secure context para SW/storage sensibles.
- `#![forbid(unsafe_code)]`; dependencias con FFI en proceso separado y declaradas.
- Todo cambio mueve una métrica WPT o de sitios reales.

## Review Focus

1. **SW con permisos excesivos** → secure context y ámbito por origen.
2. **HTTP/3 sin fallback** → ALPN negocia y cae a h1/h2.
3. **Storage sin cuota** → límites y errores tipados.

---

### Task 1: Service Workers completos

**Files:** `crates/browser/src/sw/*`, `crates/js/src/runtime/gap_apis/sw.rs`
**Steps:**
- [ ] Test que falla: install→activate→fetch interception→update; push y background sync con permiso.
- [ ] Implementar con event loop persistente y network process.
- [ ] WPT `service-workers` subset y fixture offline real.
- [ ] Commit.

### Task 2: Storage y capabilities

**Files:** `crates/js/src/runtime/gap_apis/storage/*`, `crates/browser/src/storage.rs`
**Steps:**
- [ ] IndexedDB subset útil, `CacheStorage` persistente, `localStorage` cuotas, Permissions API.
- [ ] Tests de persistencia tras reinicio y cuota excedida.
- [ ] Commit.

### Task 3: HTTP/3

**Files:** `crates/net/src/h3/*`, Cargo.toml (enmienda ADR)
**Steps:**
- [ ] Spike `quinn`+`h3`: ALPN h3, request/response, fallback h2/h1.
- [ ] Test con servidor local h3; `net` sin regresión.
- [ ] Commit.

### Task 4: Fetch completo

**Files:** `crates/js/src/runtime/gap_apis/fetch/*`
**Steps:**
- [ ] Streaming response body, AbortSignal, methods, credentials, CORS completo.
- [ ] WPT `fetch` subset.
- [ ] Commit.

### Task 5: Otras APIs de plataforma

**Files:** según WPT gaps.
**Steps:**
- [ ] Notifications, Background Sync, Payment Request, Web Locks, BroadcastChannel, SharedWorker, WebRTC subset.
- [ ] Priorizar por WPT + frecuencia en sitios reales; cada una con WP propio y test.
- [ ] Commit por API.
