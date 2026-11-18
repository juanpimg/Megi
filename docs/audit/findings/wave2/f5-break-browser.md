# F5 — Romper el navegador (auditor ola 2, `w2-browser`)

## Alcance

Batería de rotura del engine vía shell headless + inspección de `crates/js`/`crates/render`/`crates/shell`.
Leídos (línea por línea, extractos relevantes): `crates/shell/src/main.rs:60-183`,
`crates/shell/src/cli.rs` (límites `--find`/tabs/dimensiones), `crates/shell/src/title.rs`
(fuente del título de ventana = elemento `<title>` del DOM), `crates/js/src/runtime.rs`
`document_title_get/set:3776-3793`, `node_layout_rect:7801-7811`,
`node_get_bounding_client_rect:7824-7856`, `worker_handle:8217-8228`,
`worker_constructor:8230-8290`, `crates/render/src/scripts.rs:1000-1053`
(proveedores WebSocket/worker/layout), `tools/png_check.py` (validador PNG).
Nota: el árbol principal no compila por cambios WIP sin commitear del usuario
(`crates/js/src/host.rs`, `runtime.rs`, `crates/render/src/scripts.rs|lib.rs`);
**toda** la evidencia de ejecución de este informe se obtuvo en el worktree limpio
`/tmp/opencode/megi-verify` (HEAD `3c06db8`, `git status` vacío) con
`CARGO_TARGET_DIR=/home/jp/.cache/cargo-targets/megi-verify` (el primer build en
`/tmp` murió por `No space left on device`, tmpfs 7.7G lleno por `target/`).

Repros (ruta absoluta `/home/jp/Escritorio/Proyectos/Megi/docs/audit/repros/browser/`):
`control.html`, `nest-50k.html`, `selectors-100k.html`, `fontface-flood.html`,
`svg-use-cycle.html`, `img-corrupt.html` + `bad.{png,jpg,webp,gif}`, `utf8-bidi.html`,
`docwrite-loop.html`, `docwrite-nasty.html`, `worker-tamper.html`, `worker-forge.html`,
`worker-forge2.html` + `w2-worker-real.js`, `iframe-sandbox.html`, `iframe-plain.html`,
`sandbox-{empty,allowscripts,jsurl}.html`, `iframe-contentdoc.html`,
`iframe-sandbox-remove.html`, `ws-nohandshake.html`, `ws-nohandshake2.html`,
`calc-viewport.html`, `calc-edge.html`, `calc-measure.html`, `gbr-mutate.html`,
`gbr-edge.html`, `find-target.html`, `title-set.html`, `title-roundtrip.html`,
`session-page.html`.

## Cobertura (leído — sin hallazgo, con evidencia)

- **Control sano**: `cargo run --release -p shell -- <RP>/control.html --screenshot
  /tmp/v-control.png --width 1024 --height 768 --no-session` → `exit=0`,
  `wrote /tmp/v-control.png (1024x768, 23655 bytes…)`, `png_check.py: OK`.
- **100k selectores**: `exit=0`, PNG válido. **`@font-face` flood (2000 reglas)**:
  `exit=0`, 8 primeros `cannot register … unknown magic`, luego
  `skipping @font-face "F8": max web fonts per render (8) reached` (cap efectivo).
- **SVG `<use>` cíclico** (`a↔b`): `exit=0`, sin cuelgue.
- **Imágenes corruptas** (PNG/JPEG/WebP/GIF truncadas): `exit=0`, 4 `warning: skipping
  image … truncated image data …` / `limit exceeded for GIF dimension`, PNG válido.
- **UTF-8/bidi inválido** (`\xff\xfe`, overlong, bidi isolates, lone surrogate):
  `exit=0`, sin crash.
- **`document.write` en bucle** (10 000 escrituras): `exit=0`, `--save-html` contiene
  10001 `<p>`; variante `docwrite-nasty.html`: `exit=0`, sin cuelgue.
- **Worker `this.__megi_worker` manipulado**: `exit=0`; forja fuerte
  (`worker-forge2.html`: `Object.defineProperty(w,"__megi_worker",…)` →
  `redefine-err`, reasignación silenciosa, `postMessage`/`terminate` operativos,
  cap `too many Web Workers (limit 8)` al 9.º) → blindado, fix #1465 vigente.
  `dedup: #1465` (verificado vivo como *fixed*).
- **iframe `sandbox`** (sin allow-scripts, vacía, `allow-scripts`,
  `src=javascript:`): `exit=0`, `<body>` serializado sin atributos `data-pwned*` en
  los 4 casos → sin bypass. Remover `sandbox` en runtime no expone nada (la API no
  existe en HEAD). Sin `security`.
- **WS a servidor sin handshake** (HTTP 200 plano en 127.0.0.1:8766):
  `constructed` síncrono + `constructed|ERROR` asíncrono, `exit=0`, sin cuelgue.
- **`calc(50vw)`** (`calc-measure.html`, viewport 800): `a=410` (400+10),
  `b=49.20000076293945`, `c=25`, `d=20` → viewport real usado; fix #1469 vigente.
  `calc-edge.html` (`/0`, `nonsense`, incompleto): `exit=0`.
- **`getBoundingClientRect()` tras mutar DOM — variante display:none/desatado**:
  ambos `{"x":0,…,"width":0…}` → correcto (CSSOM, sin cajas).
- **find-in-page**: `--find hello` → `exit=0` PNG válido; sin coincidencias → `exit=0`;
  `--find` de 5000 B → `megi: --find value too long (max 4096 bytes)`, `exit=2`;
  `--find ""` → `exit=0`.
- **Sesión corrupta** (`not json{{{`, `{"tabs":[broken` en `--session-file`):
  `exit=0` en ambos, renderiza (tolerante). Sin crash.
- **Chrome real**: NO disponible en esta sesión (`browser.disconnected`; relay
  `tools.browse.status()` inalcanzable `127.0.0.1:19988`). Comportamientos esperados
  tomados del texto de spec (CSSOM View §5, HTML §document.title), no de Chrome.

## Hallazgos

### w2-browser-01 — `getBoundingClientRect()` devuelve geometría pre-script tras mutar el estilo (stale)
- `id`: `w2-browser-01` · `archivo:línea`: `crates/render/src/scripts.rs:1046-1053`
  (proveedor = snapshot pre-script) + `crates/js/src/runtime.rs:7801-7811`
- `lente`: contrato vs spec · `categoría`: contract · `severidad`: **P2**
- `title`: layout cache pre-script nunca se invalida; la mutación de estilo es invisible a la geometría
- `evidence` (3/3 en HEAD limpio, comando exacto):
  `cargo run --release -p shell -- <RP>/gbr-mutate.html --screenshot … --save-html … --no-session`
  (`gbr-mutate.html`: lee rect, `el.style.width="300px"`, añade nodo, relee):
  `<p id="r">{"x":8,…,"width":100,…}|{"x":8,…,"width":100,…}</p>` — el segundo
  rect sigue en 100 (esperado 300). Runs 1–3 idénticos, `exit=0`.
- `impact`: cualquier página que mida tras mutar (tooltips, virtualización, tests) recibe
  dimensiones falsas; indistinguible de un layout roto.
- `expected`: CSSOM View §5 (`getBoundingClientRect` fuerza reflow y devuelve cajas
  actualizadas). El comentario en `scripts.rs:1049-1051` admite que el proveedor "es el
  snapshot pre-script del pipeline".
- `fix`: re-ejecutar layout tras `execute_scripts` cuando el DOM/estilo mutó, o marcar
  el proveedor como sucio y re-resolver bajo demanda.
- `repro`: `docs/audit/repros/browser/gbr-mutate.html`
- `dedup`: **#1470** (cerrado como fixed: se eliminó el `OnceLock`, pero la evidencia
  nueva muestra que el congelado persiste vía snapshot pre-script).

### w2-browser-02 — `document.title=` no llega al DOM ni al título de ventana
- `id`: `w2-browser-02` · `archivo:línea`: `crates/js/src/runtime.rs:3785-3793`
  (setter escribe solo `host.title`) + `crates/shell/src/title.rs:8-12` (ventana lee el
  elemento `<title>` del DOM); `host.title` no tiene más lectores (`grep host\.title`
  solo `runtime.rs:634,3781,3791`).
- `lente`: contrato vs spec · `categoría`: contract · `severidad`: **P2**
- `title`: el setter de `document.title` es un no-op visible (solo round-trip en el host)
- `evidence` (3/3 en HEAD limpio):
  `cargo run --release -p shell -- <RP>/title-roundtrip.html --screenshot … --save-html … --no-session`
  (`document.title="CHANGED123";…get…`): getter → `<p id="out">CHANGED123</p>` pero el
  DOM conserva `<title>orig</title>`. Runs 1–3 idénticos, `exit=0`.
  Publicado también en `docs/audit/findings/wave2/issues/w2-browser-02.md`.
- `impact`: título de pestaña/ventana (`Ctrl+D`, chrome) nunca refleja cambios por
  script; SPAs y notificaciones por título rotas.
- `expected`: HTML Standard §`document.title` (setter → actualiza el elemento `title`).
- `fix`: sincronizar el setter con el elemento `<title>` del documento (crear si falta).
- `repro`: `docs/audit/repros/browser/title-roundtrip.html`
- `dedup`: **new** (sin entradas de título en `FINDINGS.md`, `GAPS.md`, `f2/00-index.md`).

### w2-browser-03 — mensaje de error de Worker filtra la ruta absoluta del servidor
- `id`: `w2-browser-03` · `archivo:línea`: `crates/render/src/scripts.rs:760-`
  (`load_worker_source` incrusta `{url}` resuelta) + `crates/js/src/runtime.rs:8017-8024`
  (`worker_constructor` entrega el texto sin sanear)
- `lente`: errores tragados/información expuesta · `categoría`: security (menor,
  publicable; **no** `advisory`: no es remoto ni bypass) · `severidad`: **P3**
- `title`: `new Worker("data:…")`/`"nonexistent.js"` vuelca `file:///<abs-path>/…` al script
- `evidence` (HEAD limpio):
  `cargo run --release -p shell -- <RP>/worker-tamper.html --screenshot … --save-html …`
  → `<p id="done">worker-err:TypeError: JavaScript error: worker script
  file:///home/jp/Escritorio/Proyectos/Megi/docs/audit/repros/browser/data:text/javascript,postMessage(1):
  file not found: /home/jp/…/browser/data:text/javascript,postMessage(1)</p>`.
  (Además: los `data:` URLs no se soportan como workers — se resuelven como fichero.)
  Publicado también en `docs/audit/findings/wave2/issues/w2-browser-03.md`.
- `impact`: fingerprinting de layout de disco en páginas `file://`; ruido para depurar.
- `expected`: mensajes sin base local (solo nombre/URL solicitada).
- `fix`: sanear `resolved` en el error del worker loader (mostrar basename o la URL
  original).
- `repro`: `docs/audit/repros/browser/worker-tamper.html`
- `dedup`: **new** (workers en ledger: #1221, #1138–#1140, todos distintos).

### w2-browser-04 — `contentDocument.querySelectorAll` usa tag-name matching (`excluir: WIP del usuario`)
- `id`: `w2-browser-04` · `archivo:línea`: WIP sin commitear
  `crates/js/src/runtime.rs` (`frame_query_selector_all` usa
  `d.get_elements_by_tag_name(&selector)`) — **no existe en HEAD `3c06db8`**
  (`grep contentDocument` vacío en el worktree limpio).
- `lente`: contrato vs spec · `categoría`: contract · `severidad`: **P2 si el WIP aterriza**
- `title`: `qSA(".x")` devuelve 0 y `qSA("p")` devuelve 2 sobre el mismo subdocumento
- `evidence` (solo binario del árbol principal con WIP):
  `iframe-contentdoc.html` → `<p id="out">title=InnerTitle|one|cls:0|tag:2</p>`
  (esperado `cls:2`). En HEAD limpio el script falla (`contentDocument` undefined) →
  el hallazgo **no aplica a HEAD**.
- `impact` (si aterriza): selectores de clase/atributo rotos en subdocumentos.
- `expected`: DOM Standard §`querySelectorAll` (selectores CSS completos).
- `fix`: enrutar por el motor de selectores CSS real en vez de `get_elements_by_tag_name`.
- `repro`: `docs/audit/repros/browser/iframe-contentdoc.html`
- `dedup`: **new** · `excluir: WIP del usuario`.

## Comandos ejecutados

Todos desde `/tmp/opencode/megi-verify` (`export CARGO_TARGET_DIR=…`,
`timeout 60s cargo run --release -p shell -- <ruta-abs-repro> … --no-session`):
control `exit=0` PNG OK · nest-50k `exit=1` 45906 violations (×3: idéntico) ·
selectors/fontface/svguse/imgcorrupt/utf8/docwrite/worker/sandbox/ws/calc `exit=0`
PNG OK · find `0` / long `2` / vacío `0` · sesión-corrupta ×2 `0` · ws-anti-handshake
(`ws-nohandshake2.html`) `constructed|ERROR` `exit=0` · `gbr-mutate` stale ×3 ·
`title-roundtrip` desync ×3 · `calc-measure` `a=410|b=49.2|c=25|d=20` (valida fix #1469) ·
`worker-forge2` `ctor-ok|redefine-err|assign-done|post-ok|term-ok|w2-ok|cap@8…` ·
`iframe-sandbox-remove` en HEAD: `EXPOSED-before|EXPOSED-after:ERR` (API inexistente,
`undefined !== null`; sin bypass real) · `docwrite-loop` 10001 `<p>` ·
`worker-tamper` path-leak (arriba). `python3 tools/png_check.py` OK en todos los PNG
producidos (incluido el `nest`, en blanco pero válido: 100 % white).

## Concern

1. El congelado de geometría (#1470) se cerró pero sobrevive por otra vía (snapshot
   pre-script): los fixes de caché global que cambian el mecanismo sin re-evaluar el
   invariante tienden a reaparecer — revisar #1473/#1461/#1474/#1475 con la misma lente.
2. `nest-50k` escribe un PNG válido y a la vez `exit=1 I/O error on <png>`: la señal de
   "degradado" (#1457) usa un mensaje indistinguible de un fallo de escritura real.
3. Sin pata de Chrome real en esta sesión (sin desktop app, sin relay); los
   `expected` son citas de spec, no A/B contra Chrome.
4. Los `bad.*` y `w2-worker-real.js` son fixtures de este informe, no del ledger.
