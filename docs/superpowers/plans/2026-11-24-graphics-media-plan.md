# Graphics & Media Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Completar Canvas 2D, añadir WebGL 1 por software sin FFI propio, y dar soporte real de imágenes modernas, audio y vídeo, llevando los gráficos de Megi a nivel de navegador utilizable.

**Architecture:** Todas las primitivas pasan por `crates/raster`/`crates/paint` compartidas. WebGL se investiga primero con un spike (implementación Rust pura o ejecución WASM de una implementación existente aislada en proceso), sin tocar el perfil `deterministic`. Audio/vídeo corren en procesos separados con códecs Rust; nunca FFmpeg enlazado.

**Tech Stack:** Rust 1.91, `crates/raster`, `symphonia`, `rav1e`/`dav1d` evaluados, WebGL software, WebAssembly como sandbox potencial.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (hito M6)

## Global Constraints

- Perfil `deterministic` sin GPU; WebGL software se ejecuta en el mismo framebuffer CPU o en proceso aislado.
- `#![forbid(unsafe_code)]` en código propio; librerías con FFI van en proceso separado y se declaran en la matriz de dependencias.
- Todo cambio visual conserva goldens salvo declaración explícita.

## Review Focus

1. **WebGL con FFI en el motor** → spike previo y decisión ADR; proceso aislado si hace falta C.
2. **Resultados no deterministas** → perfil `deterministic` separado.
3. **Códecs con licencias incompatible** → `cargo deny check` y auditoría antes de añadir.

---

### Task 1: Spike de WebGL software y ADR

**Files:**
- Create: `docs/adr/2026-11-24-webgl-strategy.md`
- Test: spike en worktree desechable.

**Steps:**
- [ ] Evaluar implementaciones Rust puras y ejecución WASM aislada; medir cobertura de WebGL 1 y rendimiento.
- [ ] Decidir: (a) Rust puro, (b) WASM en proceso, (c) librería C en proceso separado.
- [ ] ADR con licencias, MSRV, unsafe y aislamiento.
- [ ] Commit del ADR.

### Task 2: Canvas 2D completo

**Files:**
- Modify: `crates/js/src/canvas.rs`, `crates/raster/src/*`, `crates/paint/src/*`
- Test: `tests/fixtures/render/canvas_full.html`, WPT canvas subset.

**Steps:**
- [ ] Test que falla: paths, gradientes, `drawImage`, `globalAlpha`, `textMetrics`, `getImageData`.
- [ ] Implementar sobre primitivas compartidas.
- [ ] Verificar WPT subset y md5 estable.
- [ ] Commit.

### Task 3: WebGL 1 (según ADR)

**Files:** según ADR; `crates/js/src/runtime/gap_apis/webgl.rs`, módulo de raster/software.
**Steps:**
- [ ] Test que falla: `getContext('webgl')` devuelve contexto y un triángulo dibuja píxeles esperados.
- [ ] Implementar sin FFI en código propio; proceso aislado si aplica.
- [ ] Verificar WPT WebGL y perfil determinista intacto.
- [ ] Commit.

### Task 4: Imágenes modernas

**Files:** `crates/image/src/*`
**Steps:**
- [ ] AVIF y JPEG XL (decodificadores Rust o proceso aislado), con caps y errores tipados.
- [ ] Fixtures reales y `render --json` sin warnings.
- [ ] Commit.

### Task 5: Audio

**Files:** `crates/media/src/audio.rs`, `crates/js/src/runtime/gap_apis/audio.rs`
**Steps:**
- [ ] `<audio>` con MP3/Opus/Vorbis/AAC vía `symphonia`+`rodio`, proceso separado.
- [ ] Test E2E con archivo local y control de reproducción.
- [ ] Commit.

### Task 6: Vídeo

**Files:** `crates/media/src/video.rs`, proceso de decodificación.
**Steps:**
- [ ] Spike de códecs libres (VP9/AV1) con `dav1d`/Rust puro en proceso aislado.
- [ ] `<video>` con seek, sincronía A/V y compositing en display list.
- [ ] Test E2E con fixture local.
- [ ] Commit.
