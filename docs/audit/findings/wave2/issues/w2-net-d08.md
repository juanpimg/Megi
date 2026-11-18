[net] Longitudes no mínimas aceptadas (16/64 bits para valores pequeños)

## Resumen
`decode_frame` (`websocket.rs:330-348`) acepta codificación extendida no mínima:
`0x7E` con valor <126 o `0x7F` con valor <65536 pasan sin objeción. RFC 6455 §5.2
exige la forma mínima ("MUST use ... the shortest"). Además encapsula un riesgo de
divergencia con peers estrictos.

## Impacto
P3 — contrato roto menor: aceptar lo no-mínimo es tolerante, pero impide detectar
peers no conformes y choca con validadores estrictos.

## Pasos para reproducir
1. `decode_frame(&[0x82, 0x7E, 0x00, 0x05, ...5 bytes...], 1024)` → `Ok`
   (longitud 5 en forma de 16 bits, no mínima).

## Esperado vs observado
Esperado: `Err` ante longitud no mínima.
Observado: `Ok`; `:330-348` sin check de minimalidad.

## Evidencia literal
Código: `websocket.rs:330-348` (ramas 126/127 sin comparar contra el umbral).

Commit: 3c06db8

## Workaround
Ninguno necesario; la tolerancia no corrompe datos.

## Dedup
Dedup: new (rel. #1534 abierta). Mesa A: VERIFIED (código). Mesa B: SOLO NUEVO, P3 mantener.
