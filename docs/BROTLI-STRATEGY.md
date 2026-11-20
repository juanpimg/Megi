# ADR-003 — Decodificación Brotli en `crates/net` (Fase 5)

- **Estado**: **ACEPTADO (ADR-003, 2026-11-20)** — decisión de arquitectura registrada para el módulo de red.
- **Contexto**: Megi negocia `Content-Encoding` con `Accept-Encoding: gzip` y decodifica `gzip`/`deflate` desde cero en `crates/net/src/compression.rs` (con `megi_image::inflate`). Los sitios modernos servidos por CDN (p. ej. `https://www.hola.com/`, issue #1918) responden sus bundles JavaScript con `Content-Encoding: br` (RFC 7932 / Brotli); al no existir decodificador, esos scripts se descartan y la página no se inicializa. La matriz §3.1 de `docs/ARCHITECTURE.md` no autorizaba explícitamente una dependencia de compresión para `net`.

## Decisión

Autorizar **`brotli` 9.x** (Rust puro, licencia MIT/Apache-2.0, sin C/FFI) como dependencia de **codificación de bajo nivel** de `crates/net`, junto a la política de compresión existente (`gzip`/`deflate`).

- La dependencia se usa **sólo** para inflar el cuerpo `br` (`brotli::DecompressorReader`/`brotli::decompress`), nunca para producir codificaciones ni para lógica de navegador.
- Megi conserva la propiedad de: negociar `Accept-Encoding`, parsear `Content-Encoding`, aplicar los límites anti-bomb (`CompressionLimits`: `max_output` y `max_ratio`), el cap `max_response_bytes`, los errores tipados (`FetchError::Decompression`, `DecompressionLimit`, `ResponseTooLarge`) y la distinción entre respuesta completa y streaming.
- Se anuncia `Accept-Encoding: gzip, deflate, br` para que el servidor pueda elegir Brotli, que ahora se decodifica; los servidores que elijan `gzip`/`deflate` siguen por la ruta existente.
- La ruta de streaming (`fetch_streaming`) buffered el cuerpo `br` y lo decodifica con los mismos límites antes de entregarlo al callback, conservando la semántica anti-DoS.

## Alcance y límites

1. **Dentro**: `ContentEncoding::Brotli` (`br`, case-insensitive), decodificación de cuerpo completo y streaming, y los mismos límites que `gzip`/`deflate`.
2. **Fuera de alcance**: codificaciones apiladas (`gzip, br`), Brotli en fuentes WOFF2 (ya cubierto por `woff2-patched`), y Brotli en imágenes (los decoders propios de `crates/image` no lo usan).
3. **No afecta** al pipeline determinista headless ni a los goldens PNG: la negociación y el decode ocurren en `net`, aguas arriba del render.

## Alternativas consideradas

- **No anunciar `br` en `Accept-Encoding`**: mitigación parcial — los CDN pueden ignorar la cabecera o el bundle se sirve con `br` de todos modos; no arregla la causa (issue #1918). Descartada como único fix.
- **Implementar Brotli desde cero**:几个月 de trabajo y riesgo de errores en un formato complejo; `brotli` es Rust puro sin FFI y ya es el estándar de la industria. Descartado.
- **FFI a libbrotli**: incompatible con `#![forbid(unsafe_code)]` y con la política de dependencias. Descartado.

## Consecuencias

- `crates/net/Cargo.toml` gana `brotli = "9"` (no feature-gateado: es codificación de transporte, no una capacidad opcional del motor).
- `Cargo.lock` incorpora la dependencia (licencias compatibles).
- CI: tests de fixture `br` (cuerpo completo y streaming) + clippy + architecture check. El build determinista y los goldens no cambian.
- #1918 puede cerrarse cuando los tests `br` pasen en CI.
