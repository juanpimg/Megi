[net] recv_text no puede recibir binario (UnsupportedOpcode en vez de API)

## Resumen
`WebSocket::recv_text` (`websocket.rs:659-672`) devuelve
`Err(UnsupportedOpcode(0x2))` ante un frame Binary. No existe `recv_bytes` simétrico
en el wrapper `WebSocket` (sí en `WebSocketClient::send_bytes`), así que la API
string-oriented no tiene camino para binario entrante.

## Impacto
P3 — gap de API: el binario solo es accesible bajando a `inner.recv()` (campo
privado, imposible desde fuera) o no usando el wrapper.

## Pasos para reproducir
1. Servidor que envía frame Binary; `recv_text()` → `Err(UnsupportedOpcode(2))`.

## Esperado vs observado
Esperado: `recv_bytes()` / `recv()` accesible en `WebSocket`, o documentar la limitación.
Observado: `:669` error; `inner` privado (`:639`), sin escape.

## Evidencia literal
Código: `websocket.rs:637-679` (wrapper sin `recv_bytes`; `inner` privado).

Commit: 3c06db8

## Workaround
Usar `WebSocketClient` directamente en vez del wrapper `WebSocket`.

## Dedup
Dedup: new (rel. #1534 abierta). Mesa A: VERIFIED (código). Mesa B: SOLO NUEVO, P3 mantener.
