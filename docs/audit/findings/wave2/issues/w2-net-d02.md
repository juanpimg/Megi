[net] Handshake envía Host con puerto explícito incluso por defecto (:80)

## Resumen
La petición de upgrade usa `Host: {host}:{port}` incondicional (`websocket.rs:77-80`),
así que un `ws://h/x` sale como `Host: h:80`. Servidores virtuales estrictos que
comparan el `Host` literal pueden rechazar el handshake.

## Impacto
P3 — interop: handshake válido pero con forma no canónica del `Host`.

## Pasos para reproducir
1. Servidor local que registra la petición; `WebSocketClient::connect` a `ws://127.0.0.1:<p>/`
   envía `Host: 127.0.0.1:<p>` (correcto con no-default) pero a URL sin puerto
   enviaría `Host: h:80`.

## Esperado vs observado
Esperado: elidir el puerto cuando es el defecto del esquema.
Observado: `format!("...Host: {host}:{port}...")` siempre (`websocket.rs:78`).

## Evidencia literal
Código: `websocket.rs:76-80` (`url.port_or_default().unwrap_or(80)` + formato
incondicional).

Commit: 3c06db8

## Workaround
Ninguno del lado cliente; el servidor suele tolerarlo.

## Dedup
Dedup: new (rel. #1534 abierta). Mesa A: VERIFIED (código). Mesa B: SOLO NUEVO, P3 mantener.
