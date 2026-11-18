[net] recv crudo nunca responde pong (solo recv_text lo hace)

## Resumen
El auto-pong vive en `WebSocket::recv_text` (`websocket.rs:666` `Opcode::Ping =>
self.inner.pong()?`), pero `WebSocketClient::recv` (`:175-193`) devuelve los Ping
al llamante sin contestar. Quien use la API cruda incumple el keepalive RFC 6455 §5.5.2
salvo que implemente el pong a mano.

## Impacto
P3 — contrato a medias: dos APIs con distinto comportamiento ante Ping; riesgo de
timeouts por inactividad en consumidores de `recv`.

## Pasos para reproducir
1. Servidor que envía Ping y espera Pong; con `recv` no llega ningún Pong,
   con `recv_text` sí.

## Esperado vs observado
Esperado: documentar que `recv` no auto-responde, o flag de auto-pong.
Observado: diferencia silenciosa entre `:175-193` (sin pong) y `:666` (con pong).

## Evidencia literal
Código: `websocket.rs:175-193` vs `:659-672`.

Commit: 3c06db8

## Workaround
Responder `pong()` manualmente tras cada `Ping` recibido por `recv`.

## Dedup
Dedup: new (rel. #1534 abierta). Mesa A: VERIFIED (código). Mesa B: SOLO NUEVO, P3 mantener.
