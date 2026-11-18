[net] Versión del servidor nunca se valida en el handshake

## Resumen
El cliente envía `Sec-WebSocket-Version: 13` (`websocket.rs:79`) pero al validar la
respuesta solo mira status y `Accept` (`:99-120`); nunca exige ni comprueba la versión
del servidor. Un servidor de borrador antiguo con `Accept` coincidente sería aceptado.

## Impacto
P3 — versionado no aplicado: riesgo bajo (el `Accept` ya ata el GUID), pero la
negociación de versión queda sin efecto.

## Pasos para reproducir
1. Servidor local que responde `101` + `Accept` correcto pero sin TimeMachine:
   `connect` → `Ok` sin mirar versión (no hay lectura de `Sec-WebSocket-Version`
   en `:106-120`).

## Esperado vs observado
Esperado: rechazar respuesta sin `Sec-WebSocket-Version: 13` o con otra versión.
Observado: ninguna lectura de ese header en el path de validación.

## Evidencia literal
Código: `websocket.rs:99-120` (solo status + Accept).

Commit: 3c06db8

## Workaround
Ninguno necesario; los servidores modernos devuelven 13.

## Dedup
Dedup: new (rel. #1534 abierta). Mesa A: VERIFIED (código). Mesa B: SOLO NUEVO, P3 mantener.
