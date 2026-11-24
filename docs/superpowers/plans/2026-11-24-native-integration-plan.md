# Native Integration Implementation Plan (M11)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrar Megi con el sistema operativo de forma completa y segura: selectores de archivo, gestores de credenciales, notificaciones, permisos, deep links e impresión nativa en Linux, macOS, Windows, Android e iOS.

**Architecture:** Una capa de abstracción `crates/browser/src/native/` con implementaciones por SO y una matriz generada que declara `enforced | best-effort | unsupported` por capacidad. Todo paso nativo pasa por el broker de permisos de M3; el renderer nunca llama directamente al SO.

**Tech Stack:** Rust 1.91, `rfd` (selectores), `keyring` (credenciales), `notify-rust`/UNUserNotificationCenter/Windows Toast, `winit`, `windows-sys`, `objc2`, D-Bus/Linux.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (M11)

## Global Constraints

- `#![forbid(unsafe_code)]` en código propio; FFI encapsulado en crates declarados y auditados.
- Matriz generada, nunca afirmación manual: `tools/update_native_matrix.py`.
- Degradación visible en UI y `--json`.

## Review Focus

1. **Permiso concedido sin consentimiento** → flujo explícito y auditado.
2. **Matriz que miente** → generada desde tests por SO.
3. **Credenciales filtradas al renderer** → gestor de credenciales solo en el proceso privilegiado.

---

### Task 1: Capa de abstracción y matriz

**Files:** `crates/browser/src/native/{mod,permissions}.rs`, `tools/update_native_matrix.py`, `docs/status/native.html`
**Steps:**
- [ ] Test que falla: `Capabilities` enum con `file_picker, credentials, notifications, permissions, deep_links, printing` y estado por SO desde tests reales.
- [ ] Implementar broker de permisos con consentimiento explícito.
- [ ] Generar matriz y verificar frescura en CI.
- [ ] Commit.

### Task 2: Selectores de archivo y descargas nativas

**Files:** `crates/browser/src/native/files.rs`
**Steps:**
- [ ] `<input type=file>` con `rfd` en 3 SO; cancelación y multiple; E2E con archivo temporal.
- [ ] Deep links `megi://` y asociación de extensiones `.html`.
- [ ] Commit.

### Task 3: Gestor de credenciales

**Files:** `crates/browser/src/native/credentials.rs`
**Steps:**
- [ ] Guardado/lectura con `keyring`, solo en proceso privilegiado; renderer recibe tokens opacos.
- [ ] Test: renderer no puede leer la contraseña en claro.
- [ ] Commit.

### Task 4: Notificaciones y permisos

**Files:** `crates/browser/src/native/notifications.rs`
**Steps:**
- [ ] Notificaciones por SO con permiso de usuario; Tests por plataforma.
- [ ] Permissions API del motor conectada al broker real.
- [ ] Commit.

### Task 5: Impresión nativa

**Files:** `crates/browser/src/native/printing.rs`
**Steps:**
- [ ] `window.print()` con diálogo nativo y salida PDF determinista en headless.
- [ ] Test de PDF con fixture.
- [ ] Commit.
