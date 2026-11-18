[image] ómnibus de docs rancios en image (contrato escrito distinto del código)

## Resumen
Tres docstrings contradicen al código del mismo árbol: (a) `lib.rs` dice "typed errors for progressive" y "VP8X … stay unsupported" cuando ambos se decodifican; (b) `png.rs:358` dice "accumulate per-output-pixel RGBA sums" pero el scaled es nearest (`decode_png_scaled`, doc correcto en 149–155); (c) `gif.rs:315` dice "MSB-first bit reader" pero el LZW es LSB-first (línea 323 lo confirma).

## Impacto
P3 — docs: lectores del contrato esperan errores que ya no existen. Sólo docs, sin cambio de comportamiento.

## Pasos para reproducir
1. Lectura: `crates/image/src/lib.rs:10-12,40-43,122-123` vs éxito de decode progresivo/VP8X (suite verde); `png.rs:358` vs `png.rs:152`; `gif.rs:315` vs `gif.rs:323`.
2. `cargo test -p megi-image -p svg` (verde) + `megi_image::decode` sobre `alph-f1.webp` (OK 16x16, VP8X funcional).

## Esperado vs observado
Esperado: docs = comportamiento.
Observado: (a) y (c) auto-evidentes en el propio archivo; (b) contradicción interna 358 vs 152.

## Evidencia literal
```
lib.rs:10-12 «typed errors for progressive» vs decode OK 12x8
lib.rs:22-24,40-43 «VP8X … stay unsupported» / «animation/alpha-only WebP (VP8X)» vs alph-f1.webp: OK 16x16
png.rs:358 «accumulate per-output-pixel RGBA sums» vs png.rs:152 «Sampling is nearest-neighbour»
gif.rs:315 «MSB-first bit reader» vs gif.rs:323 «least-significant-bit first»
```

Commit: 3c06db8

## Fix sugerido
Actualizar los tres docstrings (checklist de 3 ítems: `lib.rs`, `png.rs:358`, `gif.rs:315`).

## Dedup
Dedup: new, distinto de #1281 (cubría otros temas stale textPath/SMIL/filtros, ya Fixed). Mesa A: VERIFIED (lectura estática + decode OK). Mesa B: SOLO NUEVO, P3 ok.
