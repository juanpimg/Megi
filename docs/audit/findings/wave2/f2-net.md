# Ola 2 — Auditoría F2-net (`crates/net`)

Prefijo: `w2-net`. Scope: todos los `.rs` bajo `crates/net/src/` (30 archivos, ~27k
líneas con tests), leídos línea por línea, con foco en el nuevo bridge WebSocket
(`websocket.rs` 678 líneas, `websocket_tests.rs` 100 líneas), orden CORS/cookies,
HPACK/h2 framing, flow control, PUSH_PROMISE, TLS rustls, caché RFC 9111, PSL,
CONNECT/SOCKS5/PAC, gzip bomb caps, deadlines y `auth.rs`, `csp.rs`, `hsts.rs`,
`integrity.rs`, `mixed_content.rs`.

## Alcance

Archivos leídos (`crates/net/src/`):

- `lib.rs` (199), `error.rs` (281), `body.rs` (106), `url.rs` (858), `mime.rs` (277),
  `data.rs` (249), `file.rs` (161), `range.rs` (240), `referrer.rs` (274)
- `http.rs` (2814), `loader.rs` (1300), `cookies.rs` (1267), `cors.rs` (661),
  `cache.rs` (353), `compression.rs` (651), `proxy.rs` (416), `socks.rs` (243),
  `pac.rs` (224), `psl.rs` (303)
- `auth.rs` (165), `csp.rs` (498), `hsts.rs` (396), `integrity.rs` (619),
  `mixed_content.rs` (147), `websocket.rs` (678)
- `h2/mod.rs` (65), `h2/frame.rs` (842), `h2/hpack.rs` (706), `h2/client.rs` (1865)
- Tests leídos: `websocket_tests.rs` (100), `auth_tests.rs` (99); resto de
  `crates/net/tests/` revisados por grep dirigido (nombres/casos).

## Cobertura

- `lib.rs` — leído — sin hallazgo (docs h2 "sin push/multiplex" desactualizadas → ver
  `w2-net-d11` abajo, dedup #1298).
- `error.rs`, `body.rs`, `mime.rs`, `data.rs`, `file.rs`, `range.rs`, `referrer.rs` —
  leído — sin hallazgo.
- `url.rs` — leído — sin hallazgo.
- `cookies.rs` — leído — sin hallazgo (PSL/super-cookie/Secure OK; `unwrap` sólo en tests).
- `cors.rs` — leído — sin hallazgo (`*`+credenciales rechazado, `allow-credentials`
  exigido; lógica correcta).
- `cache.rs` — leído — sin hallazgo directo (el bug 304 vive en `loader.rs`, ver
  `w2-net-d06`).
- `compression.rs` — leído — sin hallazgo (FHC magic+reservados rechazados, CRC/ISIZE
  verificados, caps 64 MiB / ratio 1000:1; FHCRC ya se rechaza, #1265 fixed OK).
- `proxy.rs`, `socks.rs`, `pac.rs`, `psl.rs` — leído — sin hallazgo (SOCKS5 sólo
  CONNECT+no-auth/user-pass; PAC sólo primera directiva PROXY; `Display` redacta
  credenciales, #1255 fixed OK).
- `auth.rs` — leído — sin hallazgo (sólo Basic; desconocidos nunca fabrican cabecera).
- `csp.rs` — leído — sin hallazgo (intersección multi-policy, `unsafe-inline` ignorado
  con nonce/hash, wildcards con chequeo de longitud previo al slice).
- `hsts.rs` — leído — sin hallazgo (IP literales ignoradas, `max-age` duplicado/`+`/
  signo rechazados, cap 4096 con evicción).
- `integrity.rs` — leído — sin hallazgo (tier más fuerte por SRI §3.3.3, sha NIST
  verificados por tests).
- `mixed_content.rs` — leído — sin hallazgo (upgrade conserva puerto explícito y
  falla cerrado; `ws://` no cubierto → ver `w2-net-003`).
- `http.rs` rutas `fetch`/`fetch_with_method` cookie-antes-de-CORS — leído — sin
  hallazgo (espec Fetch: el network-fetch guarda cookies y luego el main-fetch filtra
  por CORS; coincide con `http.rs:756-758/773-775`, `1088-1090/1105-1107`). El bug es
  sólo la ruta *pushed* (`w2-net-001`) y la pooled mantiene el mismo orden
  espec-conforme.
- `h2/frame.rs`, `h2/hpack.rs`, `h2/mod.rs` — leído — sin hallazgo (payload alloc tras
  `validate_frame(max_frame_size)`; `decode_integer` con guarda `shift>=28` +
  `checked_add`; casts `as` restantes son estrechamientos seguros o con máscara).
- `h2/client.rs` salvo PUSH_PROMISE/autoridad/407/deadline — leído — sin hallazgo
  nuevo (flow-control con `send_window`, RST CANCEL en descarte, HPACK resync en
  pushes descartados).
- `websocket_tests.rs`, `auth_tests.rs` — leído — sin hallazgo (el test
  `pushed_responses_respect_cors_and_store_cookies` NO cubre cookie-en-push-bloqueado;
  ver `w2-net-001`).

## Hallazgos nuevos (con repro ejecutable)

### w2-net-001 — `store_pushed` guarda `Set-Cookie` antes del chequeo CORS
- `archivo:línea`: `crates/net/src/loader.rs:513-517` (store) vs `:518-526` (CORS).
- `lente`: contrato vs especificación. `severidad`: P1. `categoría`: security.
- `title`: un push cross-origin sin ACAO deja su `Set-Cookie` en el jar aunque el
  push se descarta.
- `evidence`: `loader.rs:513-517` hace `store_from_response` incondicional; el
  `validate_response` de `:521` sólo decide descarte del caché, no revierte el jar.
  Repro `R7` (`repro-f2-net/src/main.rs`): push de `https://evil.test/` con
  `set-cookie: evil=1` y `origin=https://app.test` → `blocked-push-cookie-in-jar: true`.
- `impact`: un servidor h2 malicioso planta cookies (sesión, tracking) en el jar del
  usuario sin pasar ningún gate CORS.
- `expected`: Fetch/CORS: una respuesta que no pasa el chequeo no debe tener efectos
  laterales persistentes (juzgado ya defecto en #1252 para esta ruta).
- `fix`: validar CORS **antes** de `store_from_response`, o guardar cookies sólo
  para pushes que pasen el gate (igual que la rama permitida).
- `repro`: `docs/audit/findings/wave2/repro-f2-net/src/main.rs` (R7) + `cargo test -p megi-net`.
- `dedup`: **#1252** (evidencia nueva: el test existente no cubre cookie-en-bloqueado;
  el jar sí se contamina en HEAD). `security` + `advisory`: NO publicar como issue.

### w2-net-002 — handshake WS acepta status bogus y no valida `Upgrade`/`Connection`
- `archivo:línea`: `crates/net/src/websocket.rs:100-104` (`contains(" 101")`),
  `:106-120` (sólo comprueba `Sec-WebSocket-Accept`).
- `lente`: contrato vs especificación. `severidad`: P1. `categoría`: security.
- `title`: `HTTP/1.1 200 OK ( 101 reasons)` completa el "handshake"; una respuesta
  101 sin cabeceras `Upgrade`/`Connection` también es aceptada.
- `evidence`: repro `R1` → `bogus-status-accepted: true`; `R1b` (101 sin
  Upgrade/Connection) → `missing-upgrade-headers-accepted: true`. RFC 6455 §4.1 exige
  validar status 101 + `Upgrade: websocket` + `Connection: Upgrade` + accept.
- `impact`: un MITM/proxy que inyecta un 200 con accept predecible es tratado como
  túnel WS; confusión de protocolo (response smuggling hacia el parser de frames).
- `expected`: parsear `status == 101` exacto y exigir las tres cabeceras (RFC 6455 §4.1,
  pasos 4-5).
- `fix`: `parse_head`-like estricto + comprobación case-insensitive de
  `Upgrade`/`Connection`; mapear a `UpgradeRejected(status)` (variante hoy muerta).
- `repro`: `repro-f2-net/src/main.rs` (R1, R1b). `dedup`: new.
  `security` + `advisory`: NO publicar como issue.

### w2-net-003 — handshake WS sin `Origin`, sin mixed-content/CSP/proxy/cookies
- `archivo:línea`: `crates/net/src/websocket.rs:29-80` (request en `:77-80`),
  `parse_ws_url:434-454`.
- `lente`: contrato vs especificación. `severidad`: P1. `categoría`: security.
- `title`: el bridge abre TCP directo sin `Origin`, sin chequeo mixed-content
  (`ws://` desde documento `https:`), sin `connect-src` CSP, sin jar ni proxy.
- `evidence`: el literal del request (`:77-80`) no contiene `Origin`; `grep Origin
  websocket.rs` vacío; `connect` usa `TcpStream` directo ignorando
  `FetchOptions::proxy`/`cookie_jar`/`mixed_content`; `wss://` fail-closed pero
  `ws://` siempre permitido.
- `impact`: un script puede abrir `ws://` plaintext desde contexto seguro (filtra
  IP destino, elude inspección del proxy corporativo); el servidor no recibe `Origin`
  y no puede validar el initiator.
- `expected`: Fetch/WebSocket: enviar `Origin`, bloquear `ws://` en documento
  `https:` salvo upgrade, enrutar por proxy/CSP como `fetch()`.
- `fix`: añadir `Origin`, aplicar `MixedContentPolicy` + `allows_connect` y el proxy
  antes de `TcpStream::connect`; documentar `wss://` pendiente.
- `repro`: inspección + `R1/R1b` (misma base). `dedup`: new.
  `security` + `advisory`: NO publicar como issue.

### w2-net-004 — máscara de cliente fija `12 34 56 78`
- `archivo:línea`: `crates/net/src/websocket.rs:151`.
- `lente`: contrato vs especificación. `severidad`: P2. `categoría`: contract.
- `title`: todos los frames de cliente usan la misma máscara (RFC 6455 §5.3 exige
  impredictibilidad por frame).
- `evidence`: repro `R4` captura dos frames: máscaras `[12,34,56,78] == [12,34,56,78]`,
  `constant-and-equal: true`.
- `impact`: un intermediario malicioso puede predecir el wire-format y envenenar
  cachés (el ataque que el masking pretende impedir).
- `expected`: 4 bytes frescos por frame (CSPRNG o al menos `SystemTime`+contador
  mezclado, como `handshake_key`).
- `fix`: generar máscara aleatoria por `send()` (p.ej. dos `AtomicU64` + tiempo).
- `repro`: `repro-f2-net/src/main.rs` (R4). `dedup`: new.

### w2-net-005 — `recv()` traga errores de protocolo y bloquea hasta el timeout
- `archivo:línea`: `crates/net/src/websocket.rs:175-193` (`if let Ok(...)` en `:179`).
- `lente`: errores tragados. `severidad`: P2. `categoría`: bug.
- `title`: un frame de servidor enmascarado (violación RFC) no devuelve error; el
  cliente espera hasta el read-timeout y reporta `Io` genérico.
- `evidence`: repro `R6`: servidor envía frame enmascarado y calla →
  `err="HTTP I/O error: Resource temporarily unavailable" after=2.0s` (debió ser
  `server frame must not be masked` inmediato).
- `impact`: diagnóstico erróneo + hang de 2 s por defecto; un peer malicioso mantiene
  el hilo ocupado; oculta ataques de framing.
- `expected`: propagar el error de `decode_frames` salvo truncamiento real
  (distinguir "header parcial" de "frame completo inválido").
- `fix`: si hay bytes suficientes para `frame_total_len` y `decode_frame` falla,
  retornar el error en vez de seguir leyendo.
- `repro`: `repro-f2-net/src/main.rs` (R6). `dedup`: new.

### w2-net-006 — frames con bits RSV aceptados sin extensión negociada
- `archivo:línea`: `crates/net/src/websocket.rs:318-323` (no mira `bytes[0] & 0x70`).
- `lente`: contrato vs especificación. `severidad`: P3. `categoría`: contract.
- `title`: `RSV1/2/3` deben fallar el handshake de frame (RFC 6455 §5.2) y se aceptan.
- `evidence`: repro `R2` → `rsv-bits-accepted: true`.
- `impact`: un peer puede activar compresión/extensiones no negociadas y el cliente
  interpretará basura como datos.
- `expected`: `Err(protocol("reserved RSV bits"))` si `bytes[0] & 0x70 != 0`.
- `fix`: chequeo de 1 línea en `decode_frame`. `repro`: R2. `dedup`: new.

### w2-net-007 — frames de texto no-UTF-8 aceptados; `recv_text` usa `lossy`
- `archivo:línea`: `crates/net/src/websocket.rs:318-373` (sin validación),
  `:663` (`from_utf8_lossy`).
- `lente`: UTF-8/texto. `severidad`: P2. `categoría`: contract.
- `title`: RFC 6455 §5.6 exige texto UTF-8 válido (fallo 1007); el cliente lo acepta
  y lo corrompe silenciosamente a U+FFFD.
- `evidence`: repro `R3` → `invalid-utf8-text-accepted: true`.
- `impact`: corrupción silenciosa de mensajes; interop rota con servidores estrictos
  que esperan close 1007.
- `expected`: validar UTF-8 en opcode Text (y razón del Close) o cerrar 1007.
- `fix`: `str::from_utf8` en `decode_frame` para Text / en `recv_text` con error
  `InvalidFrame`. `repro`: R3. `dedup`: new.

### w2-net-008 — `close(code)` envía códigos reservados sin validar
- `archivo:línea`: `crates/net/src/websocket.rs:200-203` vs validador `:272-274`.
- `lente`: contrato vs especificación. `severidad`: P3. `categoría`: contract.
- `title`: `close(1005/1006/1015)` se envía por el wire aunque
  `is_valid_outgoing` los prohíbe (RFC 6455 §7.4); el validador sólo se usa en tests.
- `evidence`: repro `R5` → `reserved-close-1005-sent: true`; `grep is_valid_outgoing
  crates/` → cero usos fuera de `websocket.rs`+tests.
- `impact`: frames de cierre malformados; el peer puede abortar la sesión.
- `expected`: rechazar códigos inválidos antes de enviar.
- `fix`: `if !is_valid_outgoing(code) { return Err(protocol(...)) }`.
- `repro`: R5. `dedup`: new.

### w2-net-009 — payload de Close de 1 byte aceptado
- `archivo:línea`: `crates/net/src/websocket.rs:314-373` (sin chequeo de close).
- `lente`: contrato vs especificación. `severidad`: P3. `categoría`: contract.
- `title`: el cuerpo de Close debe ser vacío o ≥2 bytes con código válido (§5.5.1);
  1 byte se acepta.
- `evidence`: repro `R9` → `one-byte-close-accepted: true` (`[0x88,0x01,0x03]`).
- `impact`: close codes truncados interpretados como `UnexpectedEof`-like silencioso.
- `expected`: rechazar payloads de 1 byte y códigos no válidos.
- `fix`: validar en `decode_frame` para opcode Close. `repro`: R9. `dedup`: new.

### w2-net-010 — timeout de escritura heredado del de lectura
- `archivo:línea`: `crates/net/src/websocket.rs:61-63`.
- `lente`: contrato vs documentación. `severidad`: P3. `categoría`: bug.
- `title`: `set_write_timeout(Some(options.read_timeout))` — corta/larga escrituras
  con el presupuesto de lectura; `set_read_timeout` posterior (`:130`) no lo corrige.
- `evidence`: cita literal; `FetchOptions` no tiene `write_timeout` (grep vacío).
- `impact`: en enlaces lentos, `send` de mensajes grandes falla espuriamente con el
  timeout de lectura.
- `expected`: campo `write_timeout` propio o misma semántica documentada.
- `fix`: añadir `FetchOptions::write_timeout` o documentar el acoplamiento.
- `repro`: inspección + `cargo test -p megi-net` (ningún test distingue ambos).
- `dedup`: new.

### w2-net-011 — credenciales con origen sin puerto nunca hacen match
- `archivo:línea`: `crates/net/src/http.rs:955-962` (lookup
  `scheme://host:puerto`) vs `crates/net/src/auth.rs:93-97` (clave opaca).
- `lente`: contrato vs documentación. `severidad`: P2. `categoría`: contract.
- `title`: el doc promete clave "forma `origin()`" pero no existe `Url::origin()`;
  `store.set("http://a.test",..)` jamás casa con el lookup `"http://a.test:80"`.
- `evidence`: repro `R8` → `default-port-origin-miss: true` (mecanismo idéntico al
  lookup de `fetch`: `port_or_default().unwrap_or(0)`); el test `auth_tests.rs:63`
  sólo pasa porque incluye el puerto explícito.
- `impact`: el reintento Basic 401 silenciosamente no ocurre; el usuario recibe 401
  pese a tener credenciales.
- `expected`: normalizar (puerto por defecto) en `set` y en el lookup, o exponer
  `Url::origin()`.
- `fix`: normalizar ambos lados con `port_or_default`.
- `repro`: R8. `dedup`: new.

### w2-net-012 — `Sec-WebSocket-Key` predecible (tiempo+pid+contador)
- `archivo:línea`: `crates/net/src/websocket.rs:65-74`.
- `lente`: estado global/concurrencia. `severidad`: P3. `categoría`: security.
- `title`: 16 bytes de nonce derivados de `nanos ^ pid<<32 ^ contador`; sin CSRNG.
- `evidence`: cita; `as_nanos() as u64` trunca a 64 bits y el "swap" no añade
  entropía.
- `impact`: clave adivinable por un atacante local/observador (debilita el accept
  como prueba de MITM).
- `expected`: RFC 6455 §4.1: nonce de 16 bytes aleatorios.
- `fix`: CSRNG (`getrandom` ya es dependencia transitiva vía rustls) o al menos
  mezclar con direcciones/estado del socket. `repro`: inspección. `dedup`: new.
  Nota: se reporta como hallazgo normal (no crítico); si el equipo lo considera
  explotable, tratar como advisory.

### w2-net-013 — variante `UpgradeRejected` y `accept_guid` inconsistentes
- `archivo:línea`: `crates/net/src/websocket.rs:592,605` (definida y mostrada, jamás
  construida), `:582-584` (GUID duplicado con `:463`).
- `lente`: errores tragados. `severidad`: P3. `categoría`: contract.
- `title`: el rechazo de upgrade nunca tipa el status (siempre `InvalidResponse`
  genérico) y el GUID existe dos veces.
- `evidence`: `grep -rn UpgradeRejected crates/ --include=*.rs` → sólo def+Display;
  `cargo test -p megi-net` pasa sin cubrirla.
- `fix`: construir `UpgradeRejected(status)` en el handshake + deduplicar GUID.
- `repro`: inspección. `dedup`: new.

### w2-net-014 — sin reensamblado/fragmentación ni eco de Close
- `archivo:línea`: `crates/net/src/websocket.rs:147-161` (`fin:true` siempre),
  `:658-671` (`Continuation` tratado como mensaje completo), `:660-668` (Close →
  `Closed` sin responder).
- `lente`: contrato vs especificación. `severidad`: P3. `categoría`: contract.
- `title`: mensajes fragmentados se entregan por fragmentos; el Close del peer no
  recibe eco (RFC 6455 §5.4, §7.1.5).
- `evidence`: lectura + test `ws_handshake_and_text_roundtrip` (sólo unfragmented).
- `fix`: buffer de reensamblado por opcode inicial + envío de Close de vuelta.
- `repro`: inspección + suite existente. `dedup`: new.

## Hallazgos dedup (siguen vivos en HEAD, evidencia nueva)

### w2-net-d01 — #1252b: URLs de PUSH_PROMISE sin validar
- `archivo:línea`: `crates/net/src/h2/client.rs:1442-1462`
  (`format!("{scheme}://{authority}{path}")` sin checks).
- `title`: el promised-URL acepta cualquier `:scheme` (incluido distinto al de la
  conexión), `:authority` ajena al origen y `:path` sin `/` inicial.
- `evidence`: entre el parse (`:1432-1447`) y el `insert` (`:1455`) no hay ninguna
  validación de esquema/autoridad; `loader.rs:518-526` filtra por CORS al servir,
  pero el `Sec-Cookie` ya quedó (w2-net-001) y el push ocupa `pushed_streams`.
- `dedup`: **#1252**. `security`+`advisory` (hereda la clasificación del brief).

### w2-net-d02 — #1262: streaming entrega body antes de la decisión CORS
- `archivo:línea`: `crates/net/src/http.rs:746-755` (`fetch_once` con `on_data`) vs
  `:773-775` (CORS después).
- `evidence`: `stream_encoded_body` (`:2220-2307`) llama `on_data(&decoded)` por
  chunk dentro de `fetch_once`; la validación CORS ocurre al retornar. El modo
  pooled (`:618-624`) tiene el mismo orden.
- `dedup`: **#1262**.

### w2-net-d03 — #1260: h2c envía cabeceras raw y reenvía el request
- `archivo:línea`: `crates/net/src/h2/client.rs:1784-1794` (request raw con
  `extra_headers` sin el validador de `http.rs:1002-1017`), `:1744-1843`.
- `dedup`: **#1260**.

### w2-net-d04 — #1286: HEADERS en stream cerrado → error de conexión
- `archivo:línea`: `crates/net/src/h2/client.rs:1193-1198` (`Err(h2_error(...))` =
  `FetchError::Http2`, aborta todo el exchange en vez de ignorar/RST).
- `dedup`: **#1286**.

### w2-net-d05 — #1288: 304 pela la policy almacenada
- `archivo:línea`: `crates/net/src/loader.rs:884-891` (policy sólo del 304) +
  `updated_entry` (sobrescribe `freshness`/`no_cache` incondicionalmente; 304 sin
  `Cache-Control` → freshness heurística, pierde `no-cache`).
- `dedup`: **#1288**.

### w2-net-d06 — #1290: sin deadline acumulada
- `archivo:línea`: `crates/net/src/http.rs:105-121` (`FetchOptions` sólo
  `connect_timeout`/`read_timeout`); `grep -rn deadline crates/net/src/` vacío.
- `evidence`: cada `read` resetea el presupuesto; un servidor de goteo retiene el
  hilo indefinidamente (también `websocket.rs:89-98`, loop acotado sólo por 64 KiB).
- `dedup`: **#1290**.

### w2-net-d07 — #1293: pooled no valida outbound ni mapea 407
- `archivo:línea`: `crates/net/src/http.rs:528-564` (`format!` directo con
  `options.origin` controlado por el llamante, sin pasar por el validador
  `:1002-1017`); sin rama 407 en `:580-652` (el 407 vuelve como `Response` normal
  en vez de `auth_required_error` como en `:1729-1740`).
- `dedup`: **#1293**.

### w2-net-d08 — #1294: error de `on_data` hunde todo el batch
- `archivo:línea`: `crates/net/src/loader.rs:782-795` (primer índice error real,
  resto `InvalidResponse(msg)` genérico).
- `dedup`: **#1294**.

### w2-net-d09 — #1297: docs dicen "sólo http GETs" pero https va al pool
- `archivo:línea`: `crates/net/src/http.rs:431-432` vs `:515-520` (TLS pooled cuando
  no hay proxy) y `:455-462` (condición real de bypass).
- `dedup`: **#1297**.

### w2-net-d10 — #1298: docs h2 rancias
- `archivo:línea`: `crates/net/src/lib.rs:27-32` ("one GET at a time, no
  multiplexing, no server push, no full flow control") vs `h2/client.rs` (multiplex
  en `get_many`, `Limits::enable_server_push`, `send_window`/WINDOW_UPDATE).
- `dedup`: **#1298**.

## Comandos ejecutados

- `cargo test -p megi-net` → **todo verde** (157 unit + 33 suites de integración, 0
  fallos; websocket 2/2, auth 2/2). Salida completa en el log de sesión.
  Re-ejecutado desde `/tmp/opencode/megi-verify` (HEAD `3c06db8` limpio): mismos
  conteos, 0 fallos.
- Repros propios (`cargo run --offline`; copia persistente en
  `docs/audit/findings/wave2/repro-f2-net/`, re-ejecutados contra el worktree
  limpio con salida idéntica):

```text
R1 bogus-status-accepted: true
R1b missing-upgrade-headers-accepted: true
R2 rsv-bits-accepted: true
R3 invalid-utf8-text-accepted: true
R4 masks: [12, 34, 56, 78] [12, 34, 56, 78] constant-and-equal: true
R5 reserved-close-1005-sent: true
R6 recv-after-masked-frame err="HTTP I/O error: Resource temporarily unavailable" after=2.0s
R7 blocked-push-cookie-in-jar: true
R8 default-port-origin-miss: true
R9 one-byte-close-accepted: true
R10 fragmented-ping-rejected: true
```

- Greps de verificación: `deadline` vacío en `net/src`; `UpgradeRejected` sólo
  def+Display; `Origin` ausente en `websocket.rs`; `danger|NoVerifier` vacío
  (TLS rustls estándar, SNI+webpki OK).

## Verificación en worktree limpio (2026-11-18)

- Todos los comandos cargo/test y sondeos de red se re-ejecutaron desde
  `/tmp/opencode/megi-verify` (HEAD `3c06db8`, `git status` limpio), con
  `CARGO_TARGET_DIR`/`TMPDIR` bajo `/home/jp/.cache` (tmpfs de `/tmp` lleno).
- `diff -rq crates/net` entre árbol principal y worktree limpio: **sin
  diferencias**. El WIP del usuario sólo toca `crates/js` y `crates/render`;
  ningún hallazgo depende de cambios sin commitear → **ningún hallazgo marcado
  `excluir: WIP del usuario`**.
- `cargo test -p megi-net` en limpio: todo verde (157 unit + suites, 0 fallos;
  mismos conteos que en el árbol principal).
- Repros `R1–R10` contra el árbol limpio (crate en
  `/tmp/opencode/w2net-repro-verify`, path-dep al worktree): salida idéntica a
  la de arriba (los 9 comportamientos bug presentes, R10 correcto).

## Concern

1. **El bridge WebSocket nació sin controles de navegador** (sin `Origin`,
   mixed-content, CSP `connect-src`, cookies, proxy ni validación de handshake
   RFC 6455 §4.1) a la vez que usa primitivas débiles (máscara fija, nonce
   predecible, status `contains`). Cada item aislado es P2/P3, pero el conjunto es
   una superficie de confusión de protocolo y exfiltración: recomiendo tratar
   `w2-net-001/002/003` (+ PUSH_PROMISE de #1252) como **advisory privado**, no
   issues públicos.
2. **Deuda diferida intacta**: los 10 dedups de net (#1252, #1260, #1262, #1286,
   #1288, #1290, #1293, #1294, #1297, #1298) siguen reproducibles en HEAD; la
   tabla de "deferred" es precisa y no se encontró ninguna fix silenciosa.
3. **Riesgo de regresión**: `is_valid_outgoing`, `UpgradeRejected` y el gate CORS
   de pushes existen pero no están cableados al flujo real; futuros cambios pueden
   asumir que protegen. Cablearlos (w2-net-002/008) es barato y elimina la falsa
   sensación de seguridad.
