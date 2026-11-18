[net] parse_ws_url acepta con trim silencioso en vez de rechazar

## Resumen
`parse_ws_url` (`websocket.rs:434-435`) hace `input.trim()` y acepta
`"  ws://h/x  "`. Per WHATWG, espacios/tabs/newlines se tratan de forma específica,
pero un trim silencioso total oculta errores del llamante (URL pegada con espacios).

## Impacto
P3 — cosmético/robustez: aceptar entrada con espacios en vez de error tipado.

## Pasos para reproducir
1. `parse_ws_url("  ws://h/x  ")` → `Ok` (por `:435`).

## Esperado vs observado
Esperado: `Err(InvalidResponse)` ante espacios envolventes, o documentar el trim.
Observado: `Ok` silencioso.

## Evidencia literal
Código: `websocket.rs:435` (`let input = input.trim();`).

Commit: 3c06db8

## Workaround
Ninguno; comportamiento benigno en la práctica.

## Dedup
Dedup: new (rel. #1534 abierta). Mesa A: VERIFIED (código). Mesa B: SOLO NUEVO, P3 mantener.
