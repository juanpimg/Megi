# Mesa A — Verificación independiente F2-net (`crates/net`)

- Verificador: Mesa A (reproductor independiente, sin fiarse del auditor).
- Commit verificado: `3c06db8` en worktree limpio `/tmp/opencode/megi-verify`
  (el árbol principal tiene WIP que rompe la compilación; no se usó).
- `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target`.
- Repro reutilizado del informe (sin modificar): copiado a
  `/tmp/opencode/w2net-verify-A` con el `path` de `net` re-apuntado al worktree
  limpio. `cargo run --offline` + `cargo test -p net --offline` ejecutados ahí.
- `cargo test -p net`: todo verde (157 unit + suites de integración, 0 fallos).

## Hallazgos nuevos

| id | veredicto | comando | salida literal recortada | nota |
|----|-----------|---------|--------------------------|------|
| w2-net-001 | VERIFIED | `cd /tmp/opencode/w2net-verify-A && cargo run --offline` (R7) + lectura `loader.rs:509-527` | `R7 blocked-push-cookie-in-jar: true` | `store_pushed` hace `store_from_response` del jar antes del gate `cors::validate_response`; el push bloqueado contamina el jar. El comentario del propio código exige el mismo gate CORS, pero el jar ya quedó escrito. |
| w2-net-002 | VERIFIED | mismo binario (R1, R1b) + lectura `websocket.rs:99-122` | `R1 bogus-status-accepted: true` / `R1b missing-upgrade-headers-accepted: true` | Status validado con `contains(" 101")` (acepta `200 OK ( 101 reasons)`); sólo se exige `Sec-WebSocket-Accept`, nunca `Upgrade`/`Connection`. |
| w2-net-003 | VERIFIED | inspección `websocket.rs:29-80` + `grep -n Origin websocket.rs` (vacío salvo comentario) + R1/R1b como base | request literal sin `Origin`; `TcpStream::connect_timeout` directo; `parse_ws_url` rechaza `wss://` (fail-closed) y acepta `ws://` | Sin `Origin`, sin mixed-content/CSP/proxy/jar en el path WS. Veredicto por inspección + handshake base reproducido; no se montó proxy/MITM real. |
| w2-net-004 | VERIFIED | mismo binario (R4) + lectura `websocket.rs:147-161` | `R4 masks: [12, 34, 56, 78] [12, 34, 56, 78] constant-and-equal: true` | Máscara fija `[0x12,0x34,0x56,0x78]` con comentario que la justifica; dos frames consecutivos usan la misma. |
| w2-net-005 | VERIFIED | mismo binario (R6) + lectura `websocket.rs:175-193` | `R6 recv-after-masked-frame err="HTTP I/O error: Resource temporarily unavailable (os error 11)" after=2.030649539s` | Frame enmascarado del servidor (violación) no devuelve error de protocolo; `recv()` traga el error (`if let Ok`) y bloquea hasta el read-timeout (2 s). |
| w2-net-006 | VERIFIED | mismo binario (R2) + lectura `decode_frame` (`websocket.rs:318-373`) | `R2 rsv-bits-accepted: true` | Ningún chequeo de `bytes[0] & 0x70`; `0xC1` se acepta. |
| w2-net-007 | VERIFIED | mismo binario (R3) + lectura `websocket.rs:318-373,663` | `R3 invalid-utf8-text-accepted: true` | `decode_frame` no valida UTF-8 en Text; `recv_text` usa `from_utf8_lossy` (corrompe a U+FFFD en vez de fallo 1007). |
| w2-net-008 | VERIFIED | mismo binario (R5) + lectura `websocket.rs:200-203` vs `:272-274` | `R5 reserved-close-1005-sent: true` | `close(1005)` se envía por el wire; `is_valid_outgoing` existe pero `close()` no lo llama. |
| w2-net-009 | VERIFIED | mismo binario (R9) + lectura `decode_frame` | `R9 one-byte-close-accepted: true` | Payload de Close de 1 byte (`[0x88,0x01,0x03]`) aceptado; sin chequeo 0-ó-≥2. |
| w2-net-010 | VERIFIED | inspección `websocket.rs:61-63` + `grep -rn write_timeout crates/net/src/` | `set_write_timeout(Some(options.read_timeout))`; `FetchOptions` sin campo `write_timeout` | Escrituras heredan el presupuesto de lectura. Sin repro de timing (enlaces lentos no simulados); el acoplamiento es literal en el código. |
| w2-net-011 | VERIFIED | mismo binario (R8) + lectura `http.rs:955-962`, `auth.rs:93-97` | `R8 default-port-origin-miss: true` | `store.set("http://a.test",..)` jamás casa con el lookup `"http://a.test:80"` (`port_or_default().unwrap_or(0)`); `CredentialStore` es mapa opaco sin normalizar. |
| w2-net-012 | VERIFIED | inspección `websocket.rs:65-74` | `seed = nanos as u64 ^ (pid as u64)<<32 ^ NONCE…`; `key_bytes[8..] = seed.swap_bytes()` | Nonce derivado de tiempo+pid+contador, sin CSRNG. Por inspección; no se intentó predecir claves reales. Severidad la juzga Mesa B. |
| w2-net-013 | VERIFIED | inspección + `grep -rn UpgradeRejected crates/ --include='*.rs'` | sólo def (`:592`) + Display (`:605`); GUID duplicado en `accept_value` (`:463`) y `accept_guid()` (`:582`) | Variante muerta + GUID duplicado, literal. |
| w2-net-014 | VERIFIED | inspección `websocket.rs:147-161,658-671` + suite existente | `send()` siempre `fin:true`; `Continuation` → mensaje completo en `recv_text`; `Close` → `Closed` sin eco | Sin reensamblado ni eco de Close. Por inspección; el test existente sólo cubre unfragmented. |

## Dedups (siguen vivos en HEAD, evidencia nueva)

| id | veredicto | comando / lectura | salida / evidencia | nota |
|----|-----------|-------------------|--------------------|------|
| w2-net-d01 (#1252b) | VERIFIED | lectura `h2/client.rs:1430-1462` | `format!("{scheme}://{authority}{path}")` sin validar esquema/autoridad/`/` inicial antes del `insert` | El gate CORS al servir (`loader.rs:518-526`) no evita ocupación de `pushed_streams` ni el jar (vía 001). |
| w2-net-d02 (#1262) | VERIFIED | lectura `http.rs:746-775` y `:618-624` | `fetch_once(..., Some(&mut *on_data))` hace streaming por chunk y el `cors::validate_response` ocurre al retornar | Orden confirmado en ruta normal y pooled. |
| w2-net-d03 (#1260) | VERIFIED | lectura `h2/client.rs:1781-1800`, `:1744-1843`; `grep validate_request` en `http.rs` | h2c filtra sólo `connection/host/upgrade/http2-settings/keep-alive/transfer-encoding`, no usa el validador `http.rs:1005` | Filtro débil confirmado; el reenvío del request se constata en el rango citado pero no se trazó paquete a paquete. |
| w2-net-d04 (#1286) | VERIFIED | lectura `h2/client.rs:1193-1198` | `HEADERS on unknown or closed stream → Err(h2_error(...))` (error de conexión, no ignorar/RST) | Literal. |
| w2-net-d05 (#1288) | VERIFIED | lectura `loader.rs:884-891` | 304: `policy` sólo del 304 + `updated_entry` sobrescribe `freshness`/`no-cache`; 304 sin `Cache-Control` pierde `no-cache` | Literal. |
| w2-net-d06 (#1290) | VERIFIED | `grep -rn deadline crates/net/src/` (vacío) + `http.rs:105-121` | sin `deadline`; sólo `connect_timeout`/`read_timeout`; cada `read` resetea el presupuesto | Incluye loop WS `websocket.rs:89-98` acotado sólo por 64 KiB. |
| w2-net-d07 (#1293) | VERIFIED | lectura `http.rs:431-660`; `awk … | grep 407/auth_required/validate_request` en rango pooled (sin matches); conteo `validate_request_headers` en pooled = 0 | `Origin` insertada vía `format!` sin validador; sin rama 407 en pooled (el 407 vuelve como `Response` normal; el mapeo a `auth_required_error` sólo existe en `:1729-1780` no-pooled) | Doble sub-claim confirmado por ausencia en el rango pooled. |
| w2-net-d08 (#1294) | VERIFIED | lectura `loader.rs:782-795` | primer índice error real, resto `InvalidResponse(msg)` genérico | Literal. |
| w2-net-d09 (#1297) | VERIFIED | lectura `http.rs:431-432` (doc) vs `:515-520` | doc dice sólo `http:` al pool; el código mete `https` con TLS cuando no hay proxy | Contradicción literal doc/código. |
| w2-net-d10 (#1298) | VERIFIED | lectura `lib.rs:27-32` vs `h2/client.rs` (`get_many`, `enable_server_push`, `send_window`/WINDOW_UPDATE) | doc: "one GET at a time, no multiplexing, no server push, no full flow control" vs código con multiplex/push/flow-control | Doc rancia literal. |

## Concerns de Mesa A

1. Los 9 repros dinámicos (R1–R9) reproducen en `3c06db8` con salida idéntica a la
   del informe; R10 (control sano, ping fragmentado rechazado) también pasa.
   Ningún hallazgo requirió corrección del repro.
2. `w2-net-001/002/003` + PUSH_PROMISE (#1252/d01) tocan jar/cookies y confusión de
   protocolo: coinciden con el aviso del auditor de tratarlos como advisory
   privado, pero esa clasificación la decide Mesa C; Mesa A sólo confirma que el
   fallo existe.
3. Hallazgos por inspección (010/012/013/014 y los 10 dedups) verificados contra
   código HEAD, no contra el commit del informe; las líneas citadas coinciden con
   las del informe salvo desplazamientos menores (p.ej. d02), sin cambios
   semánticos.
