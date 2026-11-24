# Media & Licensing Implementation Plan (M12)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Alcanzar la máxima compatibilidad media legalmente posible y decidir con datos qué hacer con DRM/EME y codecs propietarios, publicando siempre la decisión y sus límites.

**Architecture:** Un ADR evalúa tres vías para EME: (a) integrar un CDM licensed (Widevine/PlayReady/FairPlay), (b) usar un servicio externo bajo licencia, (c) no soportar DRM y documentar alternativas. En paralelo se maximiza el soporte de codecs libres y se mide la cobertura real sobre un corpus de streams.

**Tech Stack:** Rust 1.91, EME/CDM, AV1/VP9/opus, `symphonia`, proceso aislado para media.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (M12)

## Global Constraints

- NINGUNA promesa de DRM sin licencia firmada. El ADR es obligatorio antes de tocar código.
- Codecs con patentes se evalúan por estado legal, no por disponibilidad técnica.
- Todo límite se publica en docs, welcome page y `--json`; nunca se oculta.

## Review Focus

1. **Prometer Netflix sin licencia** → ADR y verificación legal antes de cualquier claim.
2. **Codec con licencia incompatible con el proyecto** → `cargo deny check licenses`.
3. **Límite oculto** → matriz de soporte visible y verificada.

---

### Task 1: Corpus de media y medición

**Files:** `docs/status/media-matrix.toml`, `tools/media_check.py`
**Steps:**
- [ ] Corpus de streams libres (AV1/VP9/Opus) y protegidos (Widevine/PlayReady/FairPlay) con resultados esperados.
- [ ] Herramienta que reporta `playable / blocked-by-license / unsupported`.
- [ ] Commit.

### Task 2: ADR de DRM/EME

**Files:** `docs/adr/2026-11-24-drm-eme-decision.md`
**Steps:**
- [ ] Evaluar coste, requisitos legales, compatibilidad con `forbid(unsafe_code)` y modelo de proceso.
- [ ] Decidir (a), (b) o (c) con datos y aprobación del dueño.
- [ ] Publicar ADR con límites y alternativas.
- [ ] Commit.

### Task 3: Ejecución de la decisión

**Files:** según ADR.
**Steps:**
- [ ] Si (a)/(b): integrar CDM en proceso aislado, sin unsafe propio; E2E con stream protegido; verificación de licencia.
- [ ] Si (c): bloquear EME con error tipado y mensaje claro; matriz publicada; WPs de alternativas.
- [ ] Cualquier decisión: `cargo deny check licenses` y tests.
- [ ] Commit.

### Task 4: Maximizar codecs libres

**Files:** `crates/media/src/*`
**Steps:**
- [ ] Añadir AVIF/JPEG XL en imágenes; AV1/VP9/Opus en audio/vídeo; HDR y subtítulos.
- [ ] Corpus de media E2E verde; `cargo deny` limpio.
- [ ] Commit.

### Task 5: Matriz de soporte pública

**Files:** `docs/status/media.html`, `tools/update_media_matrix.py`
**Steps:**
- [ ] Generar matriz desde resultados reales; integrar en welcome page y `--json`.
- [ ] Check de frescura en CI.
- [ ] Commit.
