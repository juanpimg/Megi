# Extended Platform Implementation Plan (M10)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Completar la plataforma: WebGPU, WebXR, WebRTC completo, sistema de extensiones, sincronización de cuenta y versiones mobile de Android e iOS.

**Architecture:** Cada capacidad entra con spike y ADR antes de código, reutiliza la frontera de permisos de M3 y el perfil `performance` para GPU/hilos. Extensiones y sync se aíslan por permisos y cifrado. Mobile comparte el motor (`crates/webview`) con frontends nativos.

**Tech Stack:** Rust 1.91, `wgpu`, WebXR device API, WebRTC (ICE/STUN/TURN/DTLS/SRTP), extension manifest, sync protocol con cifrado E2E, `winit`/`android-activity`/`objc2`.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (M10)

## Global Constraints

- GPU y XR solo en perfil `performance`; perfil `deterministic` intacto para CI/goldens.
- `#![forbid(unsafe_code)]` en código propio; FFI de backend en proceso/crate aislado y declarado.
- Toda capacidad nueva mueve una métrica WPT o E2E; sin excepción.
- Las extensiones no pueden escalar permisos del sandbox de M3.

## Review Focus

1. **GPU sin backend disponible** → degradación explícita y WPT "no soportado" conforme.
2. **Extensiones con permisos excesivos** → manifiesto validado y modelo de permisos heredado de M3.
3. **Sync sin cifrado** → E2E obligatorio u opt-in local sin servidor.
4. **Mobile que no comparte motor** → mismo `crates/webview` y mismos goldens.

---

### Task 1: Spike y ADR de WebGPU/WebXR

**Files:** `docs/adr/2026-11-24-webgpu-xr.md`
**Steps:**
- [ ] Evaluar `wgpu` con backend software (llvmpipe/Vulkan software) y adaptadores WebXR.
- [ ] Medir cobertura WPT WebGPU y coste de FFI.
- [ ] ADR con decisión, licencias, perfiles y degrade.
- [ ] Commit.

### Task 2: WebGPU

**Files:** `crates/js/src/runtime/gap_apis/webgpu/*`, `crates/browser/src/gpu/*`
**Steps:**
- [ ] Test que falla: `navigator.gpu.requestAdapter()` y un compute shader simple.
- [ ] Implementar sobre `wgpu` en perfil `performance`, con fallback explícito.
- [ ] WPT WebGPU subset verde.
- [ ] Commit.

### Task 3: WebXR

**Files:** `crates/js/src/runtime/gap_apis/xr/*`, adaptadores por dispositivo.
**Steps:**
- [ ] Test: `navigator.xr.isSessionSupported`, sesión inline, frame loop.
- [ ] Implementar con degradación "unsupported" verificable.
- [ ] WPT WebXR subset.
- [ ] Commit.

### Task 4: WebRTC completo

**Files:** `crates/net/src/rtc/*`, `crates/js/src/runtime/gap_apis/rtc/*`
**Steps:**
- [ ] Test: señalización, ICE con servidor STUN local, DTLS/SRTP, audio/vídeo entre dos instancias.
- [ ] E2E con dos clientes y captura de媒体.
- [ ] Commit.

### Task 5: Sistema de extensiones

**Files:** `crates/browser/src/extensions/*`
**Steps:**
- [ ] Test: cargar extensión con manifiesto válido, permisos limitados, contenido inyectado.
- [ ] Validación de manifiesto y sandbox de permisos reutilizando M3.
- [ ] E2E: extensión no puede leer cookies de otro sitio.
- [ ] Commit.

### Task 6: Sincronización de cuenta

**Files:** `crates/browser/src/sync/*`
**Steps:**
- [ ] Test: marcadores/historial/contraseñas cifrados E2E, sync entre dos perfiles, resolución de conflictos.
- [ ] Protocolo abierto, sin servidor obligatorio (sync local por archivo/QR).
- [ ] Commit.

### Task 7: Mobile (Android e iOS)

**Files:** `crates/browser-mobile/*`, `build-aux/android/*`, `build-aux/ios/*`
**Steps:**
- [ ] Android: `android-activity`, surface, input, ciclo de vida, APK firmado.
- [ ] iOS: `objc2`, UIKit, ciclo de vida, IPA.
- [ ] Mismos goldens de render que escritorio; tests de inputs táctiles.
- [ ] Commit por plataforma.
