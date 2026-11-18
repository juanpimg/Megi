[net] frame_total_len trunca u64 a usize (envuelve en 32 bits)

## Resumen
`frame_total_len` (`websocket.rs:398`) hace `u64::from_be_bytes(array) as usize`.
En plataformas de 32 bits una longitud `u64` gigante envuelve el cálculo de
`offset.checked_add(length)` (`:406`) y el total resultante es pequeño: `decode_frames`
creería tener un frame completo con 1 byte de payload y `decode_frame` leería mal.

## Impacto
P3 — portabilidad/correctitud en 32 bits: truncamiento de longitud con `as`.
En 64 bits no hay efecto (los valores >isize fallan después por límites).

## Pasos para reproducir
1. Lectura: `websocket.rs:392-406`; en target 32 bits, header con len `0x1_0000_0005`
   → `length` truncado a 5.

## Esperado vs observado
Esperado: `usize::try_from(u64)` con `Err` (o `None`) si no cabe.
Observado: `as usize` con truncamiento silencioso.

## Evidencia literal
Código: `websocket.rs:398` (`as usize`).

Commit: 3c06db8

## Workaround
Ninguno; solo afecta a builds 32 bits ante peers maliciosos.

## Dedup
Dedup: new (rel. #1534 abierta). Mesa A: VERIFIED (código). Mesa B: SOLO NUEVO, P3 mantener.
