[net] WS:// en mayúsculas pasa el gate de connect y muere en parse_ws_url

## Resumen
`WebSocket::connect` comprueba el esquema sobre la URL en minúsculas
(`websocket.rs:644` `to_ascii_lowercase().starts_with("ws://")`), pero luego llama a
`parse_ws_url(url)` con la original, que solo despoja `ws://` minúsculas (`:441`).
`"WS://h/x"` supera el gate y cae en `Url::parse` → `InvalidUrl`, un error confuso
para una entrada que el gate dijo aceptar.

## Impacto
P3 — inconsistencia de validación: error engañoso (`InvalidUrl`) en vez de aceptar
o rechazar limpio en el primer gate.

## Pasos para reproducir
1. `WebSocket::connect("WS://h/x")` → `Err(InvalidUrl(...))` en vez de `Ok`-ruta o
   `UnsupportedScheme` claro. (Sin red: el fallo ocurre antes de conectar.)

## Esperado vs observado
Esperado: normalizar una vez y usar la misma forma en ambas etapas.
Observado: `:644` case-insensitive vs `:441` case-sensitive.

## Evidencia literal
Código: `websocket.rs:643-648` vs `:434-444`.

Commit: 3c06db8

## Workaround
Pasar la URL ya en minúsculas.

## Dedup
Dedup: new (rel. #1534 abierta). Mesa A: VERIFIED (código). Mesa B: SOLO NUEVO, P3 mantener.
