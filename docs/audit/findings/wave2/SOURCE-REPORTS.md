# Informes fuente — Ola 2 (2026-11-24)

Alcance y hallazgos no publicados. Cuerpos publicados en GitHub (ver `PUBLISHED.md`).

## f2-assets

# Auditoría ola 2 — F2 assets (`fonts` / `image` / `svg`)

Prefijo IDs: `w2-assets`. Alcance del brief: todos los `.rs` bajo `crates/fonts/src/`,
`crates/image/src/` y `crates/svg/src/`, leídos línea por línea.
Ejecución en worktree limpio `/tmp/opencode/megi-verify` (HEAD `3c06db8`);
informe escrito en el árbol principal. `crates/fonts`, `crates/image` y `crates/svg`
no tienen cambios sin commitear (verificado con `git status`), por lo que ningún
hallazgo depende de WIP del usuario (`excluir: WIP del usuario` no aplica).

## Alcance

29 archivos (20 557 líneas), leídos completos de arriba abajo:

- `crates/image/src/`: `inflate.rs` (1669), `png.rs` (1079), `gif.rs` (467),
  `jpeg.rs` (1737), `exif.rs` (331), `webp.rs` (1437), `vp8.rs` (182),
  `vp8_decoder.rs` (1855), `vp8_tables.rs` (500), `lib.rs` (665), `crc32.rs` (65),
  `error.rs` (93).
- `crates/fonts/src/`: `db.rs` (1256), `font.rs` (388), `types.rs` (183),
  `web.rs` (198), `lib.rs` (81), `error.rs` (65).
- `crates/svg/src/`: `document.rs` (2928), `raster.rs` (2169), `path.rs` (601),
  `transform.rs` (355), `xml.rs` (460), `css.rs` (532), `color.rs` (192),
  `gradient.rs` (494), `pattern.rs` (101), `text.rs` (123), `lib.rs` (351).

Focos del brief verificados: inflate/DEFLATE propio, PNG filters/Adam7/bit depths,
GIF LZW/disposal, JPEG progresivo, WebP VP8L/VP8X/ALPH/ANIM, EXIF orientation,
`decode_scaled`, límites anti-DoS; fuentes WOFF2, alias/generic family order,
fallback DB global, `scan_faces_for_char`, uncached failures; SVG path/arcs,
miter joins (#1324), pattern cycles, clip/mask budgets (#1267), gradient stop
búsqueda O(n) por píxel (#1270), `<switch>`, `currentColor`, text/tspan.

## Cobertura

- `image/inflate.rs` — leído sin hallazgo (límites por escritura, Huffman canónico
  con chequeo de sobresuscripción, streaming con checksum; ver Concern C3).
- `image/png.rs` — leído sin hallazgo publicable (Adam7, filtros 0–4, Paeth,
  sub-byte scaling, tRNS/PLTE validados; comentario rancio en `decode_p

## f2-core

# F2-core — auditoría línea por línea (`crates/core`, `crates/html`, `crates/css`)

- Prefijo: `w2-core`. Alcance: 33 archivos, 38 580 líneas (lista `find … | sort` verificada).
- Base verificada: worktree limpio `/tmp/opencode/megi-verify`, HEAD `3c06db8`, sin WIP.
  Alcance intacto entre `3c06db8` y HEAD actual (diff vacío en `crates/core|html|css`).
  WIP del principal sólo en `crates/js/**`, `crates/render/**`, `crates/net/**`
  (fuera de alcance) → **ningún hallazgo depende de WIP** (`excluir: WIP del usuario` — no aplica).
- `cargo test -p megi-core -p megi-html -p megi-css` (desde verify, `CARGO_TARGET_DIR`/`TMPDIR` en disco
  porque `/tmp` tmpfs estaba lleno): **529 passed, 0 failed** (css lib 164, dom lib 54,
  html lib 36 + 30 suites de integración/doc).
- Dedup: `docs/audit/FINDINGS.md` (228), `docs/GAPS.md`, `docs/audit/findings/f2/00-index.md`,
  índice local de issues (1000 issues; abiertas del scope: ninguna) y `gh issue list`.
  Todo lo publicado abajo es `dedup: new`; las relacionadas se citan como `rel:` (no duplicadas).

## Alcance (33 archivos leídos línea por línea)

`crates/css/src/`: animation.rs (561), animation_runtime.rs (712), calc.rs (442),
cascade.rs (1963), color.rs (694), component_value.rs (41), container.rs (420),
custom.rs (582), flex.rs (393), grid.rs (884), lib.rs (2131), media.rs (634),
parser.rs (861), properties.rs (6006), query.rs (370), rule.rs (332), selector.rs (2145),
shorthand.rs (966), state.rs (242), token.rs (125), tokenizer.rs (570),
transform.rs (271), user_agent.rs (578).
`crates/core/src/`: lib.rs (4011).
`crates/html/src/`: browser.rs (398), entities.rs (2234, tabla de datos WHATWG §13.5),
fast_hash.rs (146), input_stream.rs (225), lib.rs (97), scripting.rs (246),
serializer.rs (688), tokenizer.rs (3576), tree_builder.rs (5036).

## Cobertura

- `css/animation.rs` — leído — sin hallazgo (clamps MAX_TIME_MS/MAX_ITERATIONS; `function_args` con guarda de char-boundary).
- `css/animation_runtime.rs` — leído — sin hall

## f2-js

# F2-js — Auditoría línea por línea de `crates/js/src/` (ola 2)

- Prefijo: `w2-js`. Fecha: 2026-11-18.
- Alcance real: 9 archivos `.rs` bajo `crates/js/src/` (el brief decía ~39/~22k líneas;
  el árbol contiene 9 archivos y ~15.7k líneas totales). Leídos de arriba abajo sin excepción.
- Estado del árbol: **mutó durante la auditoría**. Al inicio `git status` estaba limpio
  para `crates/js`; a mitad de lectura aparecieron modificaciones sin commitear en
  `crates/js/src/host.rs`, `crates/js/src/runtime.rs` (añaden `contentDocument`,
  `frame_*` para iframe #1544, entre otros) y en `crates/render`. Hashes al cerrar
  (árbol principal): `canvas.rs c82ad572…`, `error.rs 8da4462f…`, `events.rs 730e06f1…`,
  `host.rs 757f16af…`, `import_map.rs 89984645…`, `lib.rs 4075e53d…`,
  `runtime.rs b675d156…` (tras el fix del contador 43/43), `websocket.rs 12989d17…`,
  `xhr.rs 2791f113…`.
- Repros: `docs/audit/findings/wave2/repro-f2-js/` (fuentes `r_main.rs`, `r3b.rs`, `r4.rs`)
  + harness externo en `/tmp/opencode/repro-f2-js` (depende por path de `crates/js`;
  no se tocó nada fuera del informe y el subdirectorio de repros).
- NOTA DE RESTAURACIÓN: este informe fue re-escrito desde contexto el 2026-11-18 tras
  el borrado accidental de `/tmp` por otro agente (los `.md` y repros del wave2 se
  perdieron; el contenido se reconstruye íntegro desde la sesión de auditoría).

## Alcance (archivos leídos)

| Archivo | Líneas | Estado |
|---|---|---|
| `crates/js/src/error.rs` | 69 | leído — sin hallazgo |
| `crates/js/src/lib.rs` | 298 | leído — 1 hallazgo (w2-js-008, docs) |
| `crates/js/src/websocket.rs` | 238 | leído — sin hallazgo |
| `crates/js/src/import_map.rs` | 415 | leído — sin hallazgo |
| `crates/js/src/canvas.rs` | 440 | leído — 1 hallazgo (w2-js-001) |
| `crates/js/src/host.rs` | 1033 | leído — sin hallazgo nuevo (#1465 verificado corregido) |
| `crates/js/src/xhr.rs` | 683 | leído — sin hallazgo (ver w2-js-009 sobre duplicidad) |
| `crates/js/src/events.rs` | 1815 | le

## f2-layout-paint

# Ola 2 · F2 layout+paint — informe del auditor

Prefijo: `w2-lp`. Alcance: todos los `.rs` bajo `crates/layout/src/` y
`crates/paint/src/`, leídos línea por línea.

> Nota de alcance real: el brief indica «~68 archivos / ~38k líneas». El
> alcance asignado contiene **22 archivos y 21 108 líneas** (`wc -l` suma
> 21 108). No existen subdirectorios adicionales en `crates/layout/src/` ni
> `crates/paint/src/`. Todos los comandos cargo/test se ejecutaron desde el
> worktree limpio `/tmp/opencode/megi-verify` (HEAD `3c06db8`, sin cambios).
> Lecturas hechas en el árbol principal; `diff -r` confirma que
> `crates/layout`, `crates/paint`, `crates/css`, `crates/core` y
> `crates/image/src` son **byte-idénticos** entre ambos árboles. Los cambios
> WIP del árbol principal (`crates/js`, `crates/render`) no afectan a este
> scope: ningún hallazgo depende de ellos (ninguno marcado
> `excluir: WIP del usuario`).

## Alcance (22 archivos)

`crates/layout/src/`: `bidi.rs` (493), `block.rs` (2310), `box_tree.rs` (313),
`calc.rs` (73), `flex.rs` (822), `geometry.rs` (155), `grid.rs` (1524),
`inline.rs` (2169), `lib.rs` (377), `table.rs` (1608), `text.rs` (310),
`tree_builder.rs` (850).
`crates/paint/src/`: `background.rs` (696), `deflate.rs` (558),
`display_list.rs` (433), `hit_test.rs` (303), `inflate.rs` (329),
`lib.rs` (257), `painter.rs` (2161), `png.rs` (710), `rasterizer.rs` (4454),
`style.rs` (203).

## Cobertura

- `calc.rs`, `geometry.rs`, `lib.rs`, `box_tree.rs`, `text.rs`, `bidi.rs`,
  `tree_builder.rs`, `block.rs`, `flex.rs`, `grid.rs`, `table.rs`,
  `inline.rs`, `style.rs`, `display_list.rs`, `background.rs`, `hit_test.rs`,
  `painter.rs`, `rasterizer.rs`, `png.rs`, `inflate.rs`, `deflate.rs`:
  **leído — con hallazgo** (ver abajo; varios archivos aportan tanto
  hallazgos vivos como verificaciones de fix).
- Ningún archivo quedó sin leer. Lentes aplicadas: aritmética/casts
  (sanitize/NaN guards verificados en `calc`, `block`, `inline`,
  `rasterizer`), panics/indexing

## f2-net

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
- `proxy.rs`

## f2-render

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

- `cargo test -p megi-render --lib` → **ok. 42 passed; 0 failed** (salida literal).
- `cargo test -p megi-render --test iframe_tests` (y resto de targets de
  integración): enlace imposible en esta máquina (`ld` SIGBUS por tmpfs lleno;
  reintentado en target de `/ho

## f2-shell-tools

# Auditoría ola 2 — F2 shell-tools (prefijo `w2-st`)

- Scope: todos los `.rs` bajo `crates/browser/src/`, `crates/bench/src/`, `crates/test_harness/src/` (41 archivos, ~25k líneas), leídos línea por línea.
- HEAD auditado: `3c06db8`. Árbol principal y worktree coinciden en el scope (`cmp` limpio archivo por archivo).
- **Proveniencia de ejecución**: el árbol principal puede tener cambios WIP que afectan a la compilación; **todos** los comandos `cargo`/binario se ejecutaron desde el worktree limpio `/tmp/opencode/megi-verify` (HEAD `3c06db8`, `git status` limpio). Primeros sondeos en el árbol principal se descartaron por sospecha de binario stale (ver concerns). Ningún hallazgo depende de cambios sin commitear → **ninguno lleva `excluir: WIP del usuario`**.
- Issues de referencia verificados en `gh` (todos CLOSED) y en `docs/audit/FINDINGS.md` (marcados Deferred): #1163 #1165 #1166 #1167 #1168 #1169 #1171 #1172 #1173 #1174 #1175 #1176 #1197 #1198 #1199 #1201, más #1177 #1178 y #1468.

## Alcance

Leídos de arriba abajo (tests incluidos):

- `crates/browser/src/`: `a11y.rs` (527), `chrome.rs` (3186), `cli.rs` (771), `convert.rs` (92), `download.rs` (606), `error.rs` (42), `favicon.rs` (347), `find.rs` (692), `forms.rs` (1579), `headless.rs` (409), `history.rs` (391), `i18n.rs` (179), `images.rs` (119), `lib.rs` (274), `links.rs` (491), `main.rs` (203), `navigation.rs` (391), `pages.rs` (625), `present.rs` (255), `reader.rs` (233), `refresh.rs` (206), `scale.rs` (173), `scroll.rs` (249), `session.rs` (1445), `tabs.rs` (473), `title.rs` (370), `window.rs` (3200), `worker.rs` (1121), `zoom.rs` (268).
- `crates/bench/src/`: `benches.rs` (1720), `cli.rs` (183), `generators.rs` (584), `main.rs` (82), `report.rs` (243), `runner.rs` (337).
- `crates/test_harness/src/`: `css_harness.rs` (447), `js_executor.rs` (189), `lib.rs` (181), `main.rs` (513), `serializer_harness.rs` (856), `tokenizer_harness.rs` (469), `tree_harness.rs` (267).

## Cobertura

- `a11y.rs` — leído — sin h

## f3-tooling-ci-docs

# F3 — Tooling, CI, manifiestos y docs (ola 2)

Auditor: wave2 F3/F4 (scope F3 = `w2-ci`, scope F4 = `w2-perf`, este fichero).
HEAD: `3c06db8c186e4f7a19c7816aed4955a0b68f91ef`.
Árbol principal con WIP del usuario en `crates/js/src/host.rs`, `crates/js/src/runtime.rs`,
`crates/render/src/lib.rs`, `crates/render/src/scripts.rs`, `crates/render/tests/iframe_tests.rs`
(leídos, nunca editados). Todos los comandos `cargo`/gates se ejecutaron en el worktree
limpio `/tmp/opencode/megi-verify` (mismo HEAD, `git status` limpio); los hallazgos que
dependen sólo de cambios sin commitear se marcan `excluir: WIP del usuario`.

## Alcance

- `tools/`: 16 `.py` + 6 `.sh` (ver Cobertura), leídos línea por línea.
- `.github/workflows/`: `bench.yml`, `ci.yml`, `coverage.yml`, `dependabot-auto-merge.yml`,
  `docs.yml`, `fuzz-nightly.yml`, `fuzz-soak.yml`, `issue-title-labeler.yml`, `labeler.yml`,
  `release.yml` + `.github/actions/install-system-deps/action.yml`, `.github/CODEOWNERS`,
  `.github/dependabot.yml`, `.github/labeler.yml`, `.github/PULL_REQUEST_TEMPLATE.md`,
  `.github/ISSUE_TEMPLATE/{bug_report,feature_request,security_report}.yml`, `.gitleaks.toml`.
- Manifiestos: `Cargo.toml`, `Cargo.lock` (verificación de pins por grep: 90 KB),
  `deny.toml`, `cliff.toml`, `rust-toolchain.toml`, `.cargo/config.toml`.
- Docs: 37 `.md` bajo `docs/` + 5 raíz (`README.md`, `CONTRIBUTING.md`, `SECURITY.md`,
  `CHANGELOG.md` —cabecera + grep de tooling—, `PROGRESS.md` —generado, verificado vía gate—).
  (El encargo habla de 45 archivos bajo `docs/`; en disco hay 37 `.md`.)
- Pendientes explícitos: `tools/check_fixture_hashes.py`, `docs/audit/findings/f7/00-closeout.md`.

## Cobertura

`leído — sin hallazgo` salvo donde se indica hallazgo.

- tools: `audit_coverage.py`, `audit_recover_issues.py`, `bench_compare.py`, `bench.sh`,
  `check_docs_links.py`, `check_fixture_hashes.py`, `coverage_report.py`, `dev.sh`, `megi.sh`,
  `png_check.py`, `render_fixture.sh`, `soak_fuzz.sh`, `test_audit_covera

## f4-perf

# F4 — Performance (ola 2)

Auditor: wave2 F3/F4 (scope F4 = `w2-perf`, este fichero).
HEAD: `3c06db8c186e4f7a19c7816aed4955a0b68f91ef`.
Mediciones: `cargo run --release -p megi-bench` ejecutado en árbol principal (sucio) y
re-ejecutado en `/tmp/opencode/megi-verify` (limpio, `worktree_dirty:false`).
Constantes del arnés verificadas idénticas en HEAD (`crates/bench` sin WIP).
Metodología: `docs/PERFORMANCE.md` §1/§13/§36 (7 pasadas, mediana por fila, warmup 1 /
iterations 5, heavy 3).

## Alcance

- `crates/bench/src/{benches.rs,runner.rs,report.rs,cli.rs,main.rs,generators.rs}` (leídos
  en lo relevante a #1197/#1198/#1199/#1201; el resto del crate es scope F2/F4-harness).
- `docs/PERFORMANCE.md` (405 líneas, leído entero), `docs/audit/findings/f4/00-perf-baseline.md`,
  `docs/status/bench-baseline.json` (schema `megi-bench-baseline-v1`, round 36, commit
  `a080249`, 81 filas — coherente con PERFORMANCE).
- Fixtures: `tests/fixtures/render/filter_effects.html` (29 líneas),
  `tests/fixtures/render/backdrop_filter.html` (31 líneas).
- Contraste pedido: #1197/#1198/#1199/#1201/#1270 (los 5 cerrados como Deferred con
  "complex fix… Documented for future work"; #1197–#1199 cerrados 2026-11-17T01:03Z).

## Cobertura

- `docs/PERFORMANCE.md` — leído, sin hallazgo nuevo (la cabecera r36 documenta load 8,1–13,0
  y no-comparabilidad 1:1; §13 documenta el comparador automático; presupuestos §11).
- `f4/00-perf-baseline.md` — leído, sin hallazgo (bloqueadores #1197/#1198/#1201 que cita
  siguen vigentes: ver abajo).
- `bench-baseline.json`, `bench_compare.py`, `tools/bench.sh` — sin hallazgo
  (comparador: regresión >20 % y >0,5 ms, exit 1; `bench.sh` reenvía args + `set -euo pipefail`).
- `benches.rs` strip/filter/MPix, `runner.rs` heavy caps, `layout` measurer, `svg/gradient.rs`
  — **hallazgos w2-perf-01…05**.

## Hallazgos

### w2-perf-01 — las filas `filter-fixture*-no-effect` miden CSS byte-idéntico (#1197 sigue vivo)

- `archivo:línea`: `crates/bench/src/benches.rs:8

## f5-break-browser

# F5 — Romper el navegador (auditor ola 2, `w2-browser`)

## Alcance

Batería de rotura del engine vía shell headless + inspección de `crates/js`/`crates/render`/`crates/browser`.
Leídos (línea por línea, extractos relevantes): `crates/browser/src/main.rs:60-183`,
`crates/browser/src/cli.rs` (límites `--find`/tabs/dimensiones), `crates/browser/src/title.rs`
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

## Cobertura (leído — sin hallazgo, con 

