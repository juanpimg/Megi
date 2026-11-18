# F1 — Tests, fuzz targets, fixtures (ola 2, scope F1)

Auditor scope F1. Prefijo: `w2-t`. Fecha: 2026-11-18.
Nota de entorno: el árbol principal sufrió ediciones concurrentes durante la
auditoría (cambios WIP del usuario, staged de otros agentes, HEAD movido de
`3c06db8` a `e87f206`). Toda compilación/test/fuzz se ejecutó desde el worktree
limpio y desacoplado `/tmp/opencode/megi-verify` (HEAD `3c06db8`, sin WIP).
Lectura de archivos y este informe, en el árbol principal.

## Alcance

44 archivos (lista de `awk -F'\t' '$7=="pending" && $3 ~ /^(test|fuzz|fixture)$/'`
sobre `docs/audit/coverage.tsv`): los 5 de `crates/css/tests/`, los 2
generadores `crates/image/tests/fixtures/generate_exif*.py`, los 10 de
`crates/js/tests/`, los 4 de `crates/layout/tests/`, los 4 de
`crates/net/tests/`, `crates/paint/tests/dialog_top_layer.rs`, los 12 de
`crates/render/tests/` (incluidos `websocket_e2e_tests.rs` e
`iframe_tests.rs`), `crates/svg/tests/scanline_budget.rs`, los 3 fuzz targets
(`charset.rs`, `compression.rs`, `http_head.rs`) y los 2 fixtures
(`animation_time.html`, `js_fetch_loader_error.html`).
Deduplicación contra `docs/audit/FINDINGS.md` (228), `docs/audit/findings/f7/00-closeout.md`
y `docs/audit/findings/f1/*`.

## Cobertura

- `crates/css/tests/cascade_core.rs` — hallazgo w2-t-03; resto leído sin hallazgo.
- `crates/css/tests/dialog.rs` — leído — sin hallazgo.
- `crates/css/tests/pseudo_elements.rs` — leído — sin hallazgo.
- `crates/css/tests/selector_escapes.rs` — leído — sin hallazgo.
- `crates/css/tests/transform.rs` — leído — sin hallazgo.
- `crates/image/tests/fixtures/generate_exif.py` + `generate_exif_orientations.py` — hallazgo w2-t-06.
- `crates/js/tests/canvas_tests.rs` — leído — sin hallazgo.
- `crates/js/tests/cookie_tests.rs` — leído — sin hallazgo.
- `crates/js/tests/custom_elements_tests.rs` — leído — sin hallazgo.
- `crates/js/tests/dialog_tests.rs` — leído — sin hallazgo.
- `crates/js/tests/form_tests.rs` — leído — sin hallazgo.
- `crates/js/tests/intl_tests.rs` — leído — sin hallazgo.
- `crates/js/tests/shadow_dom_tests.rs` — leído — sin hallazgo.
- `crates/js/tests/slot_tests.rs` — leído — sin hallazgo (3 tests pasan).
- `crates/js/tests/websocket_api_tests.rs` — hallazgo w2-t-02.
- `crates/js/tests/xhr_tests.rs` — leído — sin hallazgo (el más completo del scope).
- `crates/layout/tests/dialog.rs` — leído — sin hallazgo.
- `crates/layout/tests/float_budget.rs` — leído — sin hallazgo (incl. duplicación
  de assert en líneas 87/90, cosmético, no se reporta).
- `crates/layout/tests/pseudo_elements.rs` — leído — sin hallazgo.
- `crates/layout/tests/transform.rs` — leído — sin hallazgo.
- `crates/net/tests/auth_tests.rs` — hallazgo w2-t-05.
- `crates/net/tests/hsts_tests.rs` — leído — sin hallazgo.
- `crates/net/tests/redirect_tests.rs` — leído — sin hallazgo.
- `crates/net/tests/websocket_tests.rs` — leído — sin hallazgo en producto;
  el staged posterior al read es cosmético (ver Concern).
- `crates/paint/tests/dialog_top_layer.rs` — hallazgo w2-t-04.
- `crates/render/tests/canvas_tests.rs` — leído — sin hallazgo.
- `crates/render/tests/csp_tests.rs` — leído — sin hallazgo.
- `crates/render/tests/dialog_tests.rs` — leído — sin hallazgo.
- `crates/render/tests/document_cookie_tests.rs` — leído — sin hallazgo.
- `crates/render/tests/filter_fixture_tests.rs` — leído — sin hallazgo.
- `crates/render/tests/fixture_smoke.rs` — leído — sin hallazgo.
- `crates/render/tests/form_post_tests.rs` — hallazgo w2-t-08 (latente).
- `crates/render/tests/iframe_tests.rs` — leído (versión 190 líneas con WIP y
  versión HEAD 146 líneas) — sin hallazgo publicable; ver Concern.
- `crates/render/tests/mixed_content_tests.rs` — leído — sin hallazgo
  (`Box::leak` en línea 230 acotado a 3 peticiones de test, no se reporta).
- `crates/render/tests/pseudo_elements_tests.rs` — leído — sin hallazgo.
- `crates/render/tests/responsive_images_tests.rs` — leído — sin hallazgo.
- `crates/render/tests/sri_tests.rs` — leído — sin hallazgo.
- `crates/render/tests/websocket_e2e_tests.rs` — hallazgo w2-t-07.
- `crates/svg/tests/scanline_budget.rs` — leído — sin hallazgo.
- `fuzz/fuzz_targets/charset.rs` — hallazgo w2-t-01.
- `fuzz/fuzz_targets/compression.rs` — leído — sin hallazgo (límites 64 KiB/1000:1).
- `fuzz/fuzz_targets/http_head.rs` — leído — sin hallazgo (sin oráculo, sólo no-panic).
- `tests/fixtures/render/animation_time.html` — leído — sin hallazgo.
- `tests/fixtures/render/js_fetch_loader_error.html` — leído — sin hallazgo
  (consumido por `crates/render/tests/fetch_tests.rs:87`).

## Hallazgos

### w2-t-01 — el oráculo del fuzz target `charset` es falso positivo y aborta cada campaña

- `archivo:línea`: `fuzz/fuzz_targets/charset.rs:44-49`
- `lente`: contrato vs documentación/especificación
- `severidad`: P2
- `categoría`: test
- `title`: el assert "BOM ⇒ sin U+FFFD" contradice la decodificación lossy documentada y hace crashear `cargo fuzz run charset` con entradas válidas según spec
- `evidence`: campaña `cargo +nightly fuzz run charset -- -max_len=4096 -runs=5000`
  aborta con `exit status: 77`; replay en HEAD limpio (`/tmp/opencode/megi-verify`,
  `3c06db8`) con `-runs=0` sobre el mismo input reproduce el mismo crash:
  `thread '<unnamed>' panicked at fuzz_targets/charset.rs:45:17: a UTF-8 BOM must
  not decode to a replacement character` (2 ejecuciones, mismo input, mismo panic).
  Input: 12 bytes `EF BB BF FF FE 3C 00 00 00 00 00 38`
  (base64: `77u///48AAAAAAA4`).
- `impact`: la campaña de fuzz de `charset` no puede completarse: aborta al primer
  input con BOM + bytes inválidos posteriores y enmascara bugs reales del
  decodificador (clase del ya sufrido #1212, prescan panic).
- `expected`: `crates/render/src/charset.rs:32-39` implementa lo correcto por
  WHATWG Encoding §13.2: el BOM UTF-8 se pela y gana a cualquier declaración, y
  el resto se decodifica lossy (`String::from_utf8_lossy`). `FF FE…` tras el BOM
  es UTF-8 inválido → U+FFFD inicial es el resultado *correcto*; el BOM sí fue
  pelado (el texto no empieza por U+FEFF). El oráculo confunde "BOM pelado" con
  "cero replacements".
- `fix`: relajar el oráculo a `!decoded.text.starts_with('\u{FEFF}')`
  (BOM efectivamente pelado), y si se quiere verificar ausencia de FFFD,
  exigirlo sólo cuando `rest` sea UTF-8 válido.
- `repro`: `fuzz/artifacts/charset/crash-5a2968725140a3cde1c5b0a59e09264264704c99`
  (generado por la campaña; 12 bytes arriba) + `cd fuzz && cargo +nightly fuzz
  run charset -- -max_len=4096 -runs=5000`.
- `dedup`: new (FINDINGS.md sólo cita charset en #1212 prescan-panic, ya fixed;
  F1 no cubre este target).

### w2-t-02 — `websocket_send_and_constants` no verifica que `send()` transmita nada

- `archivo:línea`: `crates/js/tests/websocket_api_tests.rs:27-42,69-83`
- `lente`: errores tragados
- `severidad`: P3
- `categoría`: test
- `title`: el `Rc` de enviados nunca se cablea al `MockTransport`; el test de `send` sólo comprueba que no lanza
- `evidence`: `runtime_with` crea `sent: Rc<RefCell<Vec<String>>>`, clona a
  `seen`, y acto seguido `let _ = seen;` (línea 40). El `provider` construye
  `MockTransport { sent: Vec::new() }` propio e inaccesible. El test
  `websocket_send_and_constants` ejecuta `ws.send('ping')` / `ws.close()` sin
  ningún assert posterior (líneas 80-82). Suite verde: `cargo test -p megi-js --tests`
  31/31 ok — el test pasa sin comprobar lo que su nombre afirma.
- `impact`: regresión en `WebSocketTransport::send` (p. ej. mensaje tragado o nunca
  entregado al transporte) no la detecta ningún test.
- `expected`: un test llamado `send_and_constants` debería afirmar el payload
  entregado al transporte.
- `fix`: mover `seen` dentro del provider (`events` ya se clona por invocación;
  hacer lo mismo con un `Rc<RefCell<Vec<String>>>` compartido) y
  `assert_eq!(sent…, ["ping"])`.
- `repro`: `cargo test -p megi-js --test websocket_api_tests` (pasa en verde).
- `dedup`: new (F1 cita `let _` muertos en otros archivos, nunca en este).

### w2-t-03 — `style_collection_records_rules_for_each_group` no comprueba la colección

- `archivo:línea`: `crates/css/tests/cascade_core.rs:107-114`
- `lente`: contrato vs documentación/especificación
- `severidad`: P3
- `categoría`: test
- `title`: el test titulado "records rules for each group" sólo aserta el parseo del stylesheet y descarta la colección con `let _`
- `evidence`: líneas 111-113: `assert_eq!(stylesheet.rules.len(), 1);` sobre el
  valor devuelto por `parse_stylesheet`, seguido de `let _ = &collection;`.
  Ningún assert toca `collection` (ni nº de grupos, ni reglas por grupo).
- `impact`: una regresión que vacíe/duplique grupos en `StyleCollection` pasa
  este test en verde.
- `expected`: el test debería afirmar contenido de `collection` (p. ej. reglas
  registradas por grupo) coherente con su título.
- `fix`: exponer/consultar los grupos de `collection` y asertar 2 reglas en sus
  grupos esperados, o renombrar el test a lo que realmente comprueba.
- `repro`: `cargo test -p megi-css --test cascade_core` (pasa).
- `dedup`: new (patrón similar a F1 css-svg-html.md #5, pero archivo y test distintos).

### w2-t-04 — el test top-layer calcula el nº de display items y lo tira

- `archivo:línea`: `crates/paint/tests/dialog_top_layer.rs:116-120`
- `lente`: errores tragados
- `severidad`: P3
- `categoría`: test
- `title`: cómputo `filter(SolidColor).count()` descartado con `let _`; el orden de pintado top-layer carece de assert sobre la display list
- `evidence`: líneas 116-120: `let _ = list.items.iter().filter(...).count();`.
  Los asserts reales son sólo 2 píxeles + hit-test; nada verifica que el diálogo
  pinte *encima* del overlay z-99999 a nivel de display list (el título del test
  lo promete: "paint … above high z-index").
- `impact`: una regresión de ordenación que aún tiña los 2 píxeles muestreados
  (p. ej. por el backdrop) pasaría el test.
- `expected`: asertar posición relativa del item del diálogo vs overlay en `list`.
- `fix`: capturar el `count` (o los índices) y asertar
  `dialog_item_index > overlay_item_index`.
- `repro`: `cargo test -p megi-paint --test dialog_top_layer` (pasa).
- `dedup`: new (F1 paint-image.md #5 es sobre `robustness.rs`, no este archivo).

### w2-t-05 — `eprintln!("DEBUG …")` commiteado en `auth_tests.rs`

- `archivo:línea`: `crates/net/tests/auth_tests.rs:69`
- `lente`: contrato vs documentación/especificación
- `severidad`: P3
- `categoría`: test
- `title`: resto de depuración `eprintln!("DEBUG status={}", …)` en el árbol commiteado ensucia la salida de `cargo test`
- `evidence`: `grep -n eprintln crates/net/tests/auth_tests.rs` → línea 69;
  presente en HEAD `e87f206` (no es WIP). Suite verde
  (`cargo test -p megi-net --tests`, todo ok).
- `impact`: ruido en logs de CI; en suites paralelas dificulta atribuir salidas.
- `expected`: los tests no deben emitir por stderr en el camino verde
  (usar el valor en el assert o `dbg!` eliminado).
- `fix`: borrar la línea (el `assert_eq!(response.status, 200)` de la línea 70
  ya cubre la comprobación).
- `repro`: `cargo test -p megi-net --test auth_tests -- --nocapture 2>&1 | grep DEBUG`.
- `dedup`: new.

### w2-t-06 — dos generadores EXIF escriben los mismos `exif_o1..8.jpg` con distinta geometría

- `archivo:línea`: `crates/image/tests/fixtures/generate_exif.py:27,121-126` y
  `crates/image/tests/fixtures/generate_exif_orientations.py:17,53-64`
- `lente`: límites y DoS (no hermeticidad de fixtures, no DoS)
- `severidad`: P3
- `categoría`: test (fixture)
- `title`: ambos scripts generan `crates/image/tests/fixtures/jpeg/exif_o{1..8}.jpg`; uno en 12x8 y otro en 16x16: el último en ejecutarse gana en silencio
- `evidence`: `OUT_DIR = …/fixtures/jpeg` + bucle `for orientation in range(1, 9)`
  con `path = OUT_DIR / f"exif_o{orientation}.jpg"` (12x8 vía `make_reference`);
  frente a `FIXTURES = "crates/image/tests/fixtures/jpeg"` + mismo bucle con
  `W, H = 16, 16`. Ningún test fija la geometría esperada; `image/tests/exif.rs`
  consumía fixtures sólo por tag (antecedente F1 paint-image.md #4: los 8 jpeg
  diferían sólo en el tag).
- `impact`: regenerar fixtures con un script u otro cambia silenciosamente las
  dimensiones; tests futuros sensibles a geometría fallarían/pasarían según qué
  script se corriera por último.
- `expected`: un único generador canónico (o salidas con distinto nombre) y un
  test que fije dimensiones por orientación.
- `fix`: eliminar uno de los dos (el de 16x16 documenta mejor la inversión, el
  de 12x8 además sincroniza `tests/fixtures/render` y `fuzz/corpus`) y fijar el
  superviviente como canónico con assert de geometría en `exif.rs`.
- `repro`: `python3 crates/image/tests/fixtures/generate_exif.py &&
  python3 crates/image/tests/fixtures/generate_exif_orientations.py && ls -l
  crates/image/tests/fixtures/jpeg/exif_o*.jpg` (requiere Pillow).
- `dedup`: new (relacionado con #1364/#1555 ya cerrados, pero el conflicto entre
  generadores es posterior y distinto).

### w2-t-07 — el servidor echo del e2e de WebSocket no acota ni implementa el framing que asume

- `archivo:línea`: `crates/render/tests/websocket_e2e_tests.rs:19-24,51-61,86-90`
- `lente`: límites y DoS
- `severidad`: P3
- `categoría`: test
- `title`: helper de test con `head` sin acotar, sin soporte de longitud extendida (`len as u8` trunca) y nombre de fixture temporal constante
- `evidence`: bucle `while !head.ends_with(b"\r\n\r\n")` sin cota (líneas 20-25;
  compárese con el cap 64 KiB de `crates/net/tests/auth_tests.rs:16`);
  `let len = (header[1] & 0x7F) as usize` + echo `vec![0x81, len as u8]`
  (líneas 51,61) trunca payloads ≥126; `AtomicUsize::new(0).fetch_add(...)`
  siempre devuelve 0 → el temporal es siempre `megi-ws-e2e-{pid}-0.html`
  (línea 89); sin guardia de borrado (sólo se borra tras asserts en verde,
  línea 107) y `server.join()` tras el assert (línea 108) deja el hilo colgado
  si el test falla.
- `impact`: hoy el payload es de 15 bytes y el test pasa, pero el helper no
  sirve para payloads reales, colisiona si el test se repite en el proceso y
  deja hilos/temporales en rojo.
- `expected`: helpers de test con las mismas cotas que el código que ejercitan
  (cap de cabecera, framing 16/64-bit o rechazo explícito, nombre único por
  test y limpieza con guardia).
- `fix`: cap 64 KiB al leer `head`, manejar/extender longitudes o documentar el
  límite de 125 bytes, contador `static` para el nombre temporal y limpieza
  RAII antes del `join`.
- `repro`: `cargo test -p megi-render --test websocket_e2e_tests` (pasa; el defecto
  es latente, visible por lectura).
- `dedup`: new.

### w2-t-08 — `spawn_echo` de form-post parsea `Content-Length` con case sensible (latente)

- `archivo:línea`: `crates/render/tests/form_post_tests.rs:34-40`
- `lente`: errores tragados
- `severidad`: P3
- `categoría`: test
- `title`: el primer servidor usa `strip_prefix("Content-Length: ")` exacto mientras su gemelo en el mismo archivo parsea case-insensitive; si el cliente minisculiza, `content_length=0` y el body puede no haberse leído al asertar
- `evidence`: líneas 34-40 (`line.strip_prefix("Content-Length: ")`) frente a
  líneas 121-127 del mismo archivo (`k.trim().eq_ignore_ascii_case(...)`).
  Hoy pasa porque `crates/net/src/http.rs:1688` emite `"Content-Length"`
  capitalizado (verificado en HEAD limpio) y cliente/servidor suelen ir en el
  mismo segmento; el `break` tras cabeceras (línea 41-43) no garantiza el body.
- `impact`: flake latente ante cualquier cambio de capitalización o
  segmentación TCP; el assert `request.ends_with(b"user=alice&remember=1")`
  (línea 91) es el que caería.
- `expected`: ambos helpers con el mismo parseo robusto (reutilizar el
  case-insensitive del segundo).
- `fix`: copiar el parseo de líneas 119-127 al primer servidor.
- `repro`: `cargo test -p megi-render --test form_post_tests` (pasa; latente).
- `dedup`: new.

## Comandos ejecutados

Desde `/tmp/opencode/megi-verify` (HEAD `3c06db8` limpio), con
`CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-fuzz` (el tmpfs de `/tmp`,
7.7G, estaba al 100%):

- `cargo +nightly fuzz run charset -- -max_len=4096 -runs=5000` →
  crash `exit status: 77`, `panicked at fuzz_targets/charset.rs:45:17`
  (input `EF BB BF FF FE 3C 00 00 00 00 00 38`). Artefacto:
  `fuzz/artifacts/charset/crash-5a2968725140a3cde1c5b0a59e09264264704c99`.
- Replay `-runs=0` del mismo input en verify → idéntico crash (2ª ejecución,
  confirma HEAD limpio, no WIP).
- `cargo +nightly fuzz run compression -- -max_len=4096 -runs=5000` →
  `Done 5000 runs`, sin crash.
- `cargo +nightly fuzz run http_head -- -max_len=4096 -runs=5000` →
  `Done 5000 runs`, sin crash.
- Limpieza: `git clean -f -- fuzz/` en verify (263 ficheros generados por mis
  campañas); el árbol quedó como se encontró.
- `grep -rn -i "content-length" crates/net/src/*.rs` → `http.rs:1688` emite
  `"Content-Length"` capitalizado (sostiene w2-t-08 como latente, no activo).

Desde el árbol principal (antes del aviso de contención; con WIP ajeno
conviviendo, resultados todos en verde):

- `cargo test -p megi-css/layout/net/svg/paint/image --tests` → todo ok, 0 fallos.
- `cargo test -p megi-js --tests` → 31 suites ok, 0 fallos (incl. `slot_tests` 3/3).
- `cargo test -p megi-render --tests` → todas las suites ok (incl. `iframe_tests`
  7/7 con los tests WIP de `contentDocument`, luego revertidos).

## Concern

1. **Edición concurrente en el worktree principal**: durante la auditoría otro
   agente modificó `crates/js/src/runtime.rs` bajo mis lecturas (observé un
   `E0308` transitorio: array declarado de 44 con 43 elementos, que impedía
   compilar el crate `js`; una segunda lectura mostró 43 y compilaba). Una sola
   observación, estado ya revertido/commiteado (`e87f206`): no cumple el listón
   de 2 reproducciones → no se publica, queda como concern de proceso.
2. **Tests WIP observados y revertidos** (`iframe contentDocument`,
   `eprintln!("WARN2")`): pasaban (7/7) pero desaparecieron del árbol;
   `excluir: WIP del usuario` — no se reportan.
3. **Staged ajeno en `crates/net/tests/websocket_tests.rs` +
   `crates/net/src/websocket.rs`**: sólo cosmético (refactor de parseo de
   cabecera equivalente + split de string para el secret scanner). Sin efecto
   en este informe.
4. **Espacio en `/tmp`**: tmpfs lleno al 100% (5.7G del `target/` del propio
   worktree verify + cachés ajenas); el fuzz sólo fue posible redirigiendo
   `CARGO_TARGET_DIR` a `/home`. Riesgo de contención entre auditores
   concurrentes.
