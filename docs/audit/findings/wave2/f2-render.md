# Auditoría ola 2 — F2-render (`crates/render`)

## Alcance

Todos los `.rs` bajo `crates/render/src/` (15 archivos, ~8.9k líneas),
leídos línea por línea de arriba abajo, más `crates/render/tests/iframe_tests.rs`
(190 líneas) como cross-check (sin duplicar hallazgos F1):

- `backgrounds.rs` (579), `browser_host.rs` (258), `cache.rs` (316),
  `charset.rs` (442), `error.rs` (135), `frames.rs` (142),
  `images.rs` (864), `inline_svg.rs` (273), `lib.rs` (2421),
  `main.rs` (357), `scripts.rs` (2005), `stylesheets.rs` (408),
  `svg_assets.rs` (169), `webfonts.rs` (278), `ws_bridge.rs` (117).
- Cambios desde la última lectura verificados: `frames.rs`, `lib.rs`
  (+hunk `frame_documents`, hoy commiteado como e87f206), `scripts.rs`
  (+hunk `set_frame_document`), `ws_bridge.rs` (nuevo, cf388f6),
  `iframe_tests.rs` (+44 en el WIP auditado: `contentDocument` same-origin y
  sandbox nulo — esos tests **no se commitearon**; en HEAD la suite tiene 5
  tests, en verde).

Foco: caps post-descarga (#1213), truncado discovery (#1214/#1222), event-loop
budget (#1215/#1299), cache FNV (#1223), `contains("await")` (#1304), doble
render (#1168/#1471), sandbox de iframes (28e924e), WebSocket bridge,
cookies/CORS/gzip, webfonts, SVG inline.

## Verificación de ejecución (worktree limpio `/tmp/opencode/megi-verify`, HEAD 3c06db8)

El árbol principal estaba bloqueado al inicio (rotura por WIP del usuario en
`crates/js/src/runtime.rs` + `ws_bridge` sin puertas cfg para
`--no-default-features`); verificado en el worktree limpio con
`TMPDIR=/home/jp/.cache/megi-tmp CARGO_INCREMENTAL=0` (el `/tmp` tmpfs se
llenó dos veces durante la sesión: `ld` murió con SIGBUS; reintentado con
`CARGO_TARGET_DIR=/home/jp/.cache/verify-target`):

- `cargo test -p render --lib` → **ok. 42 passed; 0 failed** (salida literal).
- `cargo test -p render --test iframe_tests` (y resto de targets de
  integración): enlace imposible en esta máquina (`ld` SIGBUS por tmpfs lleno;
  reintentado en target de `/home`, ver Concern). No bloquea los hallazgos:
  cada uno tiene repro dedicado ejecutado abajo.
- `cargo run --release -p test_harness -- --summary --strict` (árbol
  principal) → corre; `CSS Syntax Total: 30 / 30 tests passed`;
  **Conformance gate: FAIL** en estricto (38 diferencias documentadas de
  parse-error no toleradas; ámbito html/css, informativo).
- Repros dedicados con el binario `render` (debug, `--json`) y un crate
  auxiliar en `/home/jp/.cache/f2-repro` (solo para límites no expuestos por
  el CLI): salidas literales citadas en cada hallazgo.

## Cobertura (`leído — sin hallazgo` salvo hallazgos)

- `backgrounds.rs` — sin hallazgo (caps coherentes; `continue`+`failed++`).
- `browser_host.rs` — sin hallazgo (snapshot pre-script documentado).
- `cache.rs` — sin hallazgo (#1213 test fetch-cap, #1218, #1223 + test).
- `charset.rs` — sin hallazgo (#1212, #1450 con tests).
- `error.rs` — salvo w2-render-08.
- `frames.rs` — w2-render-01. Sandbox `allow-scripts` (28e924e) vivo.
- `images.rs` — w2-render-10, w2-render-12.
- `inline_svg.rs` — sin hallazgo propio; ver w2-render-09.
- `main.rs` — sin hallazgo (#1219, `--time`, JSON escape).
- `stylesheets.rs` — sin hallazgo (#1214 corregido + test).
- `svg_assets.rs` — sin hallazgo (#655 + test).
- `webfonts.rs` — sin hallazgo (#531/#665/#1300/#1302).
- `ws_bridge.rs` — w2-render-02 (vía `lib.rs:215`), w2-render-13.
- `scripts.rs` — w2-render-03, w2-render-05, w2-render-06. #1217, #1221,
  #1222, #1456, #1470, #1476 verificados corregidos.
- `lib.rs` — w2-render-01/02/04/07/08/09/11. Sin doble ejecución de scripts
  en render (un solo `execute_scripts`): #1168/#1471 sin réplica aquí.
- `iframe_tests.rs` — cross-check (sandbox y `contentDocument`-srcdoc
  cubiertos); ver w2-render-11 para `src` same-origin.

## Hallazgos

### w2-render-01 — [VERIFIED] El cap de iframes es por nivel: 341 renders desde ~0.3 KiB (proyección ~70k)

- `id`: w2-render-01 · `crates/render/src/frames.rs:39-48` + `lib.rs:1223`, `frames.rs:113`
- `lente`: límites y DoS · `severidad`: P1 · `categoría`: perf (DoS sin
  corrupción de memoria — no advisory; ver Concern)
- `title`: `MAX_FRAME_PAGE` se aplica por nivel, no por página
- `evidence` (ejecución real, binario debug, misma máquina):
  - `nest4.html` (4 `<iframe src=self>`, 0.3 KiB) → `real 0m31.5s`
    (341 renders = 1+4+16+64+256), warnings: 0.
  - Control plano `flat4.html` (4 iframes vacíos, 5 renders) → `real 0m0.99s`.
  - Ratio ~32× con el mismo binario; con W=16 la proyección es
    1+16+256+4096+65536 = **69905 renders** (horas/OOM). El depth-cap evita la
    recursión infinita (termina) pero no la amplificación.
- `impact`: página adversa (local o remota) cuelga el render u OOM.
- `expected`: presupuesto global de frames por página (como `MAX_TOTAL_IMPORTS`).
- `fix`: contador global compartido a través de `render_url_inner`/`collect_frames`.
- `repro`: fixtures `nest4.html`/`flat4.html` + `time render …` (ejecutado).
- `dedup`: new · `needs-repro`: no (verificado)

### w2-render-02 — [VERIFIED] El build `--no-default-features` no compila

- `id`: w2-render-02 · `ws_bridge.rs:8-9`, `lib.rs:215`
- `lente`: contrato · `severidad`: P2 · `categoría`: build
- `title`: `use js::...` incondicional rompe la configuración documentada sin `js`
- `evidence`: `cargo check -p render --no-default-features` →
  `error[E0433]: cannot find module or crate 'js' in this scope -->
  crates/render/src/ws_bridge.rs:8:5` (ídem :9). `pub mod ws_bridge`
  incondicional frente a `#[cfg(feature = "js")] mod browser_host`.
- `impact`: configuración documentada incompilable desde cf388f6.
- `fix`: `#[cfg(feature = "js")] pub mod ws_bridge;`.
- `dedup`: new

### w2-render-03 — [VERIFIED] Presupuesto total de loop-iterations cobrado por adelantado (≤10 scripts por defecto)

- `id`: w2-render-03 · `scripts.rs:1298-1308`
- `lente`: aritmética/caps · `severidad`: P2 · `categoría`: bug
- `title`: `remaining_loops -= grant` sin reembolso descarta scripts sanos
- `evidence` (ejecución): página con 12 scripts triviales →
  `"scripts_executed": 10`, warnings:
  `["total JavaScript loop-iteration budget exhausted; skipping remaining scripts"]`.
  Con defaults (100k/1M) el script nº 11+ siempre recibe `grant == 0`;
  `max_scripts` (64) inalcanzable por defecto.
- `impact`: páginas con >10 scripts pequeños pierden silenciosamente el resto.
- `fix`: cargar gasto real o reembolsar `grant - usadas`.
- `repro`: `exp03.html` + `render --json` (ejecutado).
- `dedup`: #1299 (CLOSED; evidencia nueva de que sigue vivo)

### w2-render-04 — [VERIFIED] Navegación directa a SVG usa fuentes del sistema

- `id`: w2-render-04 · `lib.rs:1502` vs `webfonts.rs:143`
- `lente`: contrato · `severidad`: P2 · `categoría`: contract
- `title`: `new_with_bundled_and_system_fonts()` solo en la ruta SVG directa
- `evidence` (ejecución): `FontDatabase::new_with_bundled_fonts().len()` = 2
  caras frente a `new_with_bundled_and_system_fonts().len()` = **1106** caras
  en este host; `identity_digest` difiere
  (`14069339365844523798` vs `8606334732936271880`).
- `impact`: mismo SVG → distinto PNG según el host (métricas de layout).
- `fix`: usar la base bundled también en `render_svg_document`.
- `dedup`: new

### w2-render-05 — [VERIFIED con matiz] `contains("await")` decide el evaluador de módulos

- `id`: w2-render-05 · `scripts.rs:1310` (HEAD 3c06db8; `:1320` en el árbol auditado)
- `lente`: contrato · `severidad`: P2 · `categoría`: bug
- `title`: `await` en comentario/string/identificador cambia la ruta de evaluación
- `evidence` (4 sondas ejecutadas, todas con salida idéntica plain-vs-`//await`):
  timer 0 ms (orden [C,M] en ambas), timer 2000 ms (mismo triple warning de
  virtual-time en ambas), error de sintaxis (mismo mensaje salvo nº de línea,
  desplazada por la línea de comentario). La rama async existe y se toma, pero
  converge con la sync en estos casos.
- `impact`: riesgo latente + maquinaria async innecesaria; el diagnóstico del
  issue sigue siendo válido a nivel de código.
- `fix`: detección sintáctica real o ruta única.
- `dedup`: #1304 (CLOSED/Deferred; sigue vivo en código)

### w2-render-06 — [VERIFIED parcial] Socket WS abierto: +2 s de reloj real; corte de inclusión dependiente del wall-clock

- `id`: w2-render-06 · `scripts.rs:1704-1714` (sleep 5 ms × 20 por drenaje)
- `lente`: concurrencia · `severidad`: P2 · `categoría`: perf
- `title`: `thread::sleep` dentro del drenaje determinista
- `evidence` (ejecución, servidor WS python local):
  - Página con socket abierto e inactivo: `real 2.56s` vs `0.55s` sin socket
    (mismo binario debug).
  - Mensaje a 95 ms → incluido 10/10; a 150 ms → 8/8; a 2000 ms → 0/6. El
    corte de inclusión lo decide el reloj real, no el virtual (aunque en 24
    runs no se observó flip inter-run en el borde: NOT VERIFIED el flip,
    verificado el mecanismo y la latencia).
- `impact`: latencia y ventana de nodeterminismo acoplada a la máquina/carga.
- `fix`: un solo poll sin sleep, o deadline documentado.
- `dedup`: new (relacionado con #1534, OPEN)

### w2-render-07 — [VERIFIED] `external_stylesheets` cuenta intentos, no éxitos

- `id`: w2-render-07 · `lib.rs:1995` (incremento) / `2008` (carga, puede fallar)
- `lente`: contrato · `severidad`: P3 · `categoría`: stats
- `title`: el contador se incrementa antes de saber si la hoja cargó
- `evidence` (ejecución): `<link>` a `missing-xyz.css` →
  `"external_stylesheets": 1` + warning `skipping stylesheet ... file not found`.
- `fix`: mover `+= 1` tras carga exitosa.
- `dedup`: #1305 (CLOSED; sigue vivo)

### w2-render-08 — [VERIFIED estático] PNG vacío → `LayoutFailed`

- `id`: w2-render-08 · `lib.rs:1322` (y 1540, ruta SVG)
- `lente`: contrato · `severidad`: P3 · `categoría`: contract
- `title`: fallo de encoder mapeado a error de layout
- `evidence`: `grep -n 'LayoutFailed' lib.rs` → `1322: return Err(RenderError::LayoutFailed);`
  tras `if png.is_empty()`.
- `fix`: variante `RenderError::PngEncode` (o reutilizar `Output`).
- `dedup`: #1306 (CLOSED; mitad "variante errónea" viva)

### w2-render-09 — [VERIFIED] Fast-path salta el reemplazo de SVG inline si todos los scripts fallaron tras mutar el DOM

- `id`: w2-render-09 · `lib.rs:1134-1135`
- `lente`: contrato · `severidad`: P3 · `categoría`: bug
- `title`: `scripts_executed == 0` no implica "DOM intacto"
- `evidence` (ejecución): script que inserta `<svg>` vía `'<s'+'vg…'` (fuente
  sin literal `<svg`, verificado `grep -c '<svg' = 0`) y luego lanza →
  `executed=0 failed=1 dom_mutated=1 svg_nodes_left=1 inline_svgs=0`.
  Control con literal `<svg` en fuente: `inline_svgs=1` (el mecanismo funciona;
  solo el fast-path lo salta).
- `fix`: exigir también `failed == 0` en la guarda.
- `dedup`: #1220 (CLOSED; sigue vivo)

### w2-render-10 — [VERIFIED] Presupuesto total de imágenes: `break` sin `failed++`, resto abandonado

- `id`: w2-render-10 · `images.rs:694-701`
- `lente`: errores tragados · `severidad`: P3 · `categoría`: bug
- `title`: `break` sordo frente al `continue`+`failed++` de backgrounds
- `evidence` (dos ejecuciones):
  - Límites custom (6 imgs, presupuesto ≈2.5 ficheros): `loaded=2 failed=0`,
    un solo warning `image byte budget exhausted ... (18048 bytes total, max 15040)`;
    4 imágenes ni intentadas ni contadas.
  - CLI con 12 PNGs de ~3.16 MB (38 MB > 32 MB por defecto): `loaded=10
    failed=0`, un warning `... (34816226 bytes total, max 33554432)`;
    img10 e img11 caídas sin rastro individual.
- `fix`: alinear con backgrounds (contar cada omitida).
- `dedup`: new

### w2-render-11 — [VERIFIED] `contentDocument` solo para `srcdoc`

- `id`: w2-render-11 · `lib.rs:1020-1029` (commiteado en e87f206 como #1544; era
  WIP al auditar, hoy es HEAD)
- `lente`: contrato · `severidad`: P3 · `categoría`: contract
- `title`: los iframes `src` same-origin no exponen documento al padre
- `evidence` (ejecución, binario HEAD 2eaab73): padre con
  `<iframe id='f' src='f2-child.html'>` same-origin + script que lee
  `contentDocument` → `executed: 1 failed: 0`,
  warnings: `['console.warn: RESULT:NULL']` (debería exponer el documento).
  Control `srcdoc` (misma sonda con `srcdoc="<body>…from-frame…"`):
  `RESULT:from-frame` (funciona). Asimetría `srcdoc`-sí / `src`-no confirmada
  por ejecución. (Nota: los tests `contentDocument` del WIP auditado no se
  commitearon en e87f206 — solo el código; la suite `iframe_tests` en HEAD
  tiene 5 tests, en verde.)
- `impact`: `iframe[src].contentDocument` same-origin → `null` indebido.
- `fix`: reutilizar el documento hijo de `collect_frames` (además hoy se
  parsea dos veces) o documentar la limitación a `srcdoc`.
- `repro`: `f2-parent.html`/`f2-child.html` + `render --json` (ejecutado).
- `dedup`: new

### w2-render-12 — [VERIFIED] Cap silencioso de 1024 en `collect_responsive_image_sources`

- `id`: w2-render-12 · `images.rs:519-521`
- `lente`: límites · `severidad`: P3 · `categoría`: contract
- `title`: truncado sin warning que contradice las opciones públicas
- `evidence` (ejecución, límites elevados para aislar el cap):
  `loaded=1024 failed=0 warnings=0` con 1100 `<img>`. Cero avisos.
  (Con límites por defecto el presupuesto de 64 Mpx salta antes: 419/1100.)
- `fix`: warning como el resto de caps.
- `dedup`: #1301 (CLOSED; sigue vivo)

### w2-render-13 — [NO VERIFICADO] WS: binario→texto lossy e `is_open` ignora cierre remoto

- `id`: w2-render-13 · `ws_bridge.rs:53-57`, `:97-99`
- `lente`: texto/contrato · `severidad`: P3 · `categoría`: contract
- `title`: `Opcode::Binary → String::from_utf8_lossy`; `is_open` solo mira `stop`
- `evidence`: lectura directa (código). Sin sonda ejecutable vía CLI
  (`is_open` es diagnóstico interno, no observable en la salida del render).
- `fix`: variante `Message(Vec<u8>)` o documentar; `stop.store(true)` al
  recibir `Close`.
- `dedup`: new (#1534 OPEN) · `needs-repro` (test unitario)

## Comandos ejecutados (salidas literales)

1. `wc`/`git log`/`git status`: alcance y WIP identificados.
2. `cargo test -p render` (árbol principal, con WIP): `error[E0308] ...
   runtime.rs:1454 ... expected 44, found 43` (bloqueo inicial, hoy resuelto
   en HEAD e87f206).
3. `cargo check -p render --no-default-features`: `error[E0433] ...
   ws_bridge.rs:8:5` (w2-render-02).
4. `cargo test -p render --lib` (worktree limpio): `ok. 42 passed; 0 failed`.
5. Targets de integración: enlace imposible en tmpfs lleno (`ld` SIGBUS);
   reintentado con `CARGO_TARGET_DIR=/home/jp/.cache/verify-target`:
   `iframe_tests` → **ok. 5 passed; 0 failed** (worktree 3c06db8); mismo
   resultado en el árbol principal HEAD (`cargo test -p render --test
   iframe_tests` → 5 passed). Resto de targets pendientes de `/tmp` sano.
6. `test_harness --summary --strict`: `CSS Syntax Total: 30 / 30`; gate FAIL
   (38 parse-error differences).
7. Repros (salidas arriba): exp03, exp05×4, exp05b, exp05c, exp05d, exp06
   (latencia + 10/8/6 runs), exp07, exp09/09b, exp10 (crate + CLI), exp12.
8. `gh issue view`: CLOSED = 1168, 1213, 1214, 1215, 1217, 1218, 1220, 1221,
   1222, 1223, 1299, 1300, 1301, 1302, 1303, 1304, 1305, 1306, 1471, 1526,
   1527, 1528, 1530, 1537, 1538, 1543, 1545, 465, 524, 654, 660, 665, 670, 462,
   1212, 1216, 1456, 1470, 1476; OPEN = 1544, 1534.

## Concern

1. **DoS remoto (w2-render-01, P1)**: 31.5 s para 0.3 KiB con W=4; proyección
   ~70k renders con W=16. Si el shell renderiza contenido no confiable,
   valorar advisory según `SECURITY.md` (sin corrupción de memoria: hang/OOM).
2. **Infra de verificación**: `/tmp` tmpfs (7.7 G) se llenó dos veces por
   actividad ajena; `ld` muere con SIGBUS y los `cargo test` de integración no
   enlazan ahí. Repros hechos con binario CLI + `CARGO_TARGET_DIR` en `/home`.
   La suite de integración completa queda pendiente de una máquina con `/tmp`
   sano.
3. **w2-render-11** es ya código de HEAD (e87f206 #1544), no WIP: queda como
   hallazgo con ejecución pendiente (sonda trivial vía CLI cuando el binario
  de verify-target esté listo).
4. **test_harness strict FAIL** (38 diferencias): ámbito parser, informativo.
5. Riesgo visto fuera de alcance (para F1 js/host): el module loader
   (`with_allow_file` sin confinamiento a directorio) permite a un `import`
   estático leer cualquier `file://` con documento `file:`.
