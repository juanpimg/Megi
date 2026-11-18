# Mesa C — reunión de verificación (scope net, `w2-net-*`)

- Fecha: 2026-11-18. Chair: Mesa C (reintento; el chair anterior no escribió archivos).
- Commit verificado: `3c06db8` (árbol limpio `/tmp/opencode/megi-verify`).
- Fuentes: informe F2-net (harness repro `w2net-verify-A`, idéntico a `w2net-repro`),
  re-ejecución independiente por el chair en `3c06db8`, lectura directa de
  `crates/net/src/websocket.rs`, `loader.rs:497-557`, `auth.rs:25-108`,
  y dedup contra índice local (1000 issues, abiertas net: solo #1534) + FINDINGS/GAPS.
- Nota de procedimiento: los ficheros `f2-net.md`, `verify-net-A.md` y `verify-net-B.md`
  no existen en `docs/audit/findings/wave2/` (verificado por el chair antes de escribir).
  La verificación se reconstruyó ejecutando el harness repro en HEAD `3c06db8`
  (salidas literales abajo) + inspección de código + chequeo de dedup.
  Ningún hallazgo `needs-repro`: todos tienen repro ejecutable o cita exacta de código.
- Comando: `cd /tmp/opencode/w2net-verify-A && cargo run --offline` → salidas:
  `R1 bogus-status-accepted: true` · `R1b missing-upgrade-headers-accepted: true` ·
  `R2 rsv-bits-accepted: true` · `R3 invalid-utf8-text-accepted: true` ·
  `R4 masks: [12, 34, 56, 78] [12, 34, 56, 78] constant-and-equal: true` ·
  `R5 reserved-close-1005-sent: true` ·
  `R6 recv-after-masked-frame err="HTTP I/O error: Resource temporarily unavailable (os error 11)" after=2.016392856s` ·
  `R7 blocked-push-cookie-in-jar: true` · `R8 default-port-origin-miss: true` ·
  `R9 one-byte-close-accepted: true` · `R10 fragmented-ping-rejected: true` (control negativo: correcto).

## Tabla final

| id | veredicto | sev | área | etiquetas | incl/excl | motivo |
|---|---|---|---|---|---|---|
| w2-net-001 | VERIFIED | P1 | http-push | security, advisory-privado | exclude (advisory) | `store_pushed` guarda `Set-Cookie` antes del gate CORS (`loader.rs:513-526`); R7 `true`. Explotable entre orígenes → aviso privado, nunca issue público. |
| w2-net-002 | VERIFIED | P1 | ws-handshake | security, advisory-privado | exclude (advisory) | `status_line.contains(" 101")` (`websocket.rs:101`); R1 `true`. Acepta `200 OK ( 101 reasons)` como upgrade → aviso privado. |
| w2-net-003 | VERIFIED | P2 | ws-framing | security, advisory-privado | exclude (advisory) | Máscara cliente fija `[0x12,0x34,0x56,0x78]` (`websocket.rs:151`); R4 máscaras idénticas. Debilita la protección anti-cache-poisoning de RFC 6455 §10.3 → aviso privado. |
| w2-net-d01 | VERIFIED | P2 | ws-handshake | security, advisory-privado | exclude (advisory) | Sin validación de `Upgrade`/`Connection` en la respuesta (`websocket.rs:99-120`); R1b `true`. Acepta respuestas no-WebSocket como sesión → aviso privado. |
| w2-net-004 | VERIFIED | P2 | ws-framing | public, rfc6455-s5.2 | include | RSV1-3 ignorados en `decode_frame` (nunca se lee `bytes[0] & 0x70`); R2 `true`. |
| w2-net-005 | VERIFIED | P2 | ws-framing | public, rfc6455-s5.6 | include | Texto sin validación UTF-8 en `decode_frame`; R3 `true`. Además `recv_text` usa `from_utf8_lossy` (`websocket.rs:664`). |
| w2-net-006 | VERIFIED | P2 | ws-api | public, rfc6455-s7.4 | include | `close(code)` envía cualquier código sin `is_valid_outgoing` (`websocket.rs:200-203`); R5 `1005-sent: true`. El validador existe (`:272`) pero no se usa. |
| w2-net-007 | VERIFIED | P1 | ws-api | public, hang | include | `recv` traga el error de protocolo (`if let Ok(...)`, `websocket.rs:179`) y bloquea hasta el timeout; R6 tarda 2.01s y devuelve `Io` en vez de error de protocolo inmediato. Hang/DoS. |
| w2-net-008 | VERIFIED | P2 | auth | public, credenciales | include | `CredentialStore` clavea por string crudo sin normalizar (`auth.rs:83-102`); R8 `default-port-origin-miss: true`. Auth no se reenvía. |
| w2-net-009 | VERIFIED | P2 | ws-framing | public, rfc6455-s5.5 | include | Close de 1 byte aceptado (0 o ≥2 con código válido exige la RFC); R9 `true`. Sin validación de payload de Close en `decode_frame`. |
| w2-net-010 | VERIFIED | P2 | ws-framing | public, rfc6455-s7.4 | include | Códigos de cierre nunca validados en recepción (1005/1006/1015 aceptados en payload); `decode_frame` (`websocket.rs:318-373`) no inspecciona el payload de Close. |
| w2-net-011 | VERIFIED | P2 | ws-api | public, fragmentación | include | Sin reensamblado de mensajes: `recv` devuelve el primer frame suelto (`websocket.rs:175-193`), `Continuation` expuesto en crudo al llamante. |
| w2-net-012 | VERIFIED | P3 | ws-handshake | public, aleatoriedad | include | Clave de handshake predecible (`now+pid+contador`, `websocket.rs:65-74`) y comparación de `Accept` con `!=` no constante (`:118`). |
| w2-net-013 | VERIFIED | P3 | auth | public, parsing | include | `parse_challenge` corta por `,` (`auth.rs:26`) y rompe `realm="a,b"`; además solo usa el primer challenge (documentado, pero el corte es dentro de comillas). |
| w2-net-014 | VERIFIED | P3 | ws-handshake | public, timeouts | include | `set_write_timeout(Some(options.read_timeout))` (`websocket.rs:61-63`): el timeout de escritura sale del de lectura. |
| w2-net-d02 | VERIFIED | P3 | ws-handshake | public, interop | include | `Host: {host}:{port}` siempre con puerto explícito (`websocket.rs:78`), incluso `:80` por defecto. |
| w2-net-d03 | VERIFIED | P3 | ws-framing | public, api | include | `encode_frame` ignora `frame.masked` y siempre pone el bit MASK (`websocket.rs:284-299`); asimetría encode/decode admitida en el propio test (`:509-513`). |
| w2-net-d04 | VERIFIED | P3 | ws-api | public, ping-pong | include | Ping/pong solo se contestan en `recv_text` (`websocket.rs:666`); `recv` crudo nunca responde pong automáticamente. |
| w2-net-d05 | VERIFIED | P3 | ws-api | public, api-gap | include | `recv_text` falla con `UnsupportedOpcode(0x2)` ante Binary (`websocket.rs:669`): la API string no puede recibir binario. |
| w2-net-d06 | VERIFIED | P3 | ws-handshake | public, versionado | include | La versión del servidor nunca se valida en la respuesta (se envía `Sec-WebSocket-Version: 13` pero no se comprueba eco; `websocket.rs:99-120`). |
| w2-net-d07 | VERIFIED | P3 | ws-api | public, parsing | include | `parse_ws_url` hace `trim()` silencioso (`websocket.rs:435`): acepta `"  ws://h/x  "` en vez de rechazar. |
| w2-net-d08 | VERIFIED | P3 | ws-framing | public, rfc6455-s5.2 | include | Longitudes no mínimas aceptadas (16 bits para <126, 64 bits para <65536; `websocket.rs:330-348` sin check de minimalidad). |
| w2-net-d09 | VERIFIED | P3 | ws-framing | public, portabilidad | include | `frame_total_len` trunca `u64 as usize` (`websocket.rs:398`): en 32 bits una longitud gigante envuelve el cálculo. |
| w2-net-d10 | VERIFIED | P3 | ws-api | public, inconsistency | include | `WebSocket::connect` acepta prefijo case-insensitive (`:644`) pero `parse_ws_url` solo despoja `ws://` minúsculas (`:441`): `"WS://h/x"` pasa el gate y muere en `InvalidUrl`. |

## Avisos privados (4, sin issue público)

- w2-net-001 (P1): `Set-Cookie` de push bloqueado por CORS ya plantado en el jar.
- w2-net-002 (P1): handshake acepta status falso que contenga `" 101"`.
- w2-net-003 (P2): máscara cliente constante.
- w2-net-d01 (P2): handshake sin exigir `Upgrade`/`Connection`.

## Incluidos como issue público (20)

w2-net-004…014 (11) + w2-net-d02…d10 (9). Cuerpos en `docs/audit/findings/wave2/issues/<id>.md`.

## Descartes (0 + 1 control)

- Ningún hallazgo verificado se descarta; B no marca DUPLICADO en ninguno de los 24
  (dedup contra índice: solo #1534 abierta en el scope, relacionada pero no duplicada;
  resto de net cerradas #1526-#1537/#1531/#1530 son gaps ya cerrados, citados como rel.).
- R10 (`fragmented-ping-rejected: true`) es control negativo: el rechazo ya es correcto, no genera issue.
