# ADR-005 — Decodificación AVIF en `crates/image` (Fase 4)

- **Estado**: **ACEPTADO (ADR-005, 2026-11-24)** — enmienda de la política de dependencias de `docs/ARCHITECTURE.md` §3.1.
- **Contexto**: §3.1 autoriza dependencias externas de bajo nivel para red, ventanas y tipografía, pero **no** para decodificación de imagen: PNG/JPEG/GIF/WebP se decodifican desde cero con `std` (`crates/image`). Sitios reales (issue #1895, `ecosia.org`) sirven imágenes `Content-Encoding`-neutras en formato AVIF (`ftypavif`); `ImageFormat::Unknown` hace que el pipeline las descarte. Un decodificador AV1 desde cero está fuera de alcance (formato muy complejo).

## Decisión

Autorizar **`avif-rust` 0.3** (MIT, Rust puro, `rust-version = 1.88` ≤ MSRV 1.91, sin C/FFI) **únicamente como decodificador de imagen AVIF de bajo nivel** en `crates/image`.

- La dependencia expone `decode_bytes(&[u8]) -> ImageBuffer { width, height, rgba }` y se usa **sólo** para convertir el contenedor HEIF/AVIF + AV1 en píxeles RGBA8.
- Megi conserva la propiedad de: detección por firma (`ftyp` + marca `avif`/`avis`), límites anti-DoS propios (bytes de entrada, píxeles totales, dimensiones), errores tipados (`ImageError`), integración con `RgbaImage`, `detect_format`, el pipeline de `<img>`/`background-image` y el reporte de warnings.
- El resto de formatos siguen deconstruyéndose desde cero; esta ADR **no** autoriza dependencias para PNG/JPEG/GIF/WebP ni para codificación AVIF.
- Feature-gate: ninguno. Es codificación de formato de imagen, no una capacidad opcional del motor.

## Alcance y límites

1. **Dentro**: detección AVIF, decode de imagen fija AVIF a RGBA8, caps y wiring en render.
2. **Fuera**: animación AVIF, HDR/gain maps, codificación AVIF, HEIC/HEIF no-AVIF.
3. Errores del decoder externo se mapean a `ImageError::UnsupportedFeature`; nunca se devuelve una imagen parcial.

## Alternativas consideradas

- **Implementar AV1 desde cero**: coste desproporcionado y riesgo de errores; rechazado.
- **`libavif-sys` / `dav1d` FFI**: incompatible con `#![forbid(unsafe_code)]` y con el perfil de dependencias; rechazado.
- **`zenavif`**: AGPL-3.0 y `rust-version` 1.93 > MSRV 1.91; rechazado.
- **No soportar AVIF**: deja imágenes rotas en sitios reales (#1895); rechazado.

## Consecuencias

- `crates/image/Cargo.toml` gana `avif-rust = "0.3"`; `Cargo.lock` incorpora la dependencia (licencia MIT compatible).
- CI: tests de firma AVIF y de decode con fixture real, clippy, MSRV 1.91 y architecture check. El pipeline determinista y los goldens PNG no cambian mientras no haya imágenes AVIF en los fixtures.
- #1895 puede cerrarse cuando los tests AVIF pasen en CI.
