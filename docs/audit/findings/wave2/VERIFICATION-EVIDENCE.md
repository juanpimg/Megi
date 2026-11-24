# Evidencia de verificación — Ola 2 (2026-11-24)

Consolidación de salidas literales de sondas A/B/C. Detalle en `git log -- docs/audit/findings/wave2/verify-*.md`.

## verify-assets-A

# Verificación Mesa A — scope fonts/image/svg (`f2-assets.md`)

- Mesa: A (reproductor independiente). Fecha: 2026-11-18.
- Informe verificado: `docs/audit/findings/wave2/f2-assets.md` (11 hallazgos `w2-assets-01…11` + concerns C1–C6).
- Base de ejecución: worktree limpio `/tmp/opencode/megi-verify`, HEAD `3c06db8`
  (sólo `?? tests/fixtures/render/f2-exp10.html`, `f2-exp12.html` untracked preexistentes;
  `git status` sin WIP en `crates/`).
  Env: `TMPDIR=/home/jp/.cache/megi-tmp`, `CARGO_TARGET_DIR=/home/jp/.cache/f2-target`.
  Repros leídos del árbol principal
  (`docs/audit/findings/wave2/repro-w2-assets/`, 15 ficheros presentes); no se escribió
  nada en el worktree verify ni se borró ningún archivo.
- Sonda: `/home/jp/.cache/megi-probe-assets` (deps por path a `crates/svg` + `crates/image`
  del worktree verify; se ejecutó cada modo 1 vez salvo indicación).
- Resultado: **11/11 VERIFIED**. Suites en verde: `svg` (37+14+13+16+render…),
  `image` (32+34+5+9+8+6+7+21+1), `fonts` (3+19+10+1); 0 failed.

## Tabla de veredictos

| id | veredicto | comando | salida literal (recorte) | nota |
|----|-----------|---------|--------------------------|------|
| w2-assets-01 | VERIFIED | probe `render` (`megi_svg::render_svg` sobre `switch.svg`) | `switch.svg: 100x50 painted=5000 … top=[((0, 0, 255, 255), 2500), ((255, 0, 0, 255), 2500)]` | Ambos hijos pintados completos (rojo 2500 + azul 2500); esperado sólo el azul. Causa en `document.rs:1067` (`"g" \| "a" \| "switch"` comparten ra

## verify-assets-B

# Verificación Mesa B — scope F2-assets (`crates/fonts` / `crates/image` / `crates/svg`)

Fiscal de dedup y severidad sobre `docs/audit/findings/wave2/f2-assets.md` (11 hallazgos `w2-assets-01..11`).
No se edita código ni ledger. Fecha: 2026-11-18. HEAD auditado por el informe: `3c06db8`;
HEAD actual al fiscalizar: `d82118c`. Nota: no existe `verifier-template.md` en el repo
(búsqueda por glob sin resultados); se sigue el formato de los informes hermanos de Mesa B
(`verify-js-B.md`, `verify-browser-B.md`).
Método: lectura íntegra del informe + `FINDINGS.md` (ledger, 254 líneas) + estado vía
`gh issue view` + spot-checks de código en el árbol actual.

## Fuentes de dedup consultadas

- `docs/audit/FINDINGS.md`: hits relevantes — #1270 (svg gradient linear scan, Deferred),
  #1273 (svg switch renders every child, P3), #1275 (gradient stop style/sort, otro defecto),
  #1276 (currentColor→black, P3), #1324 (miter bowtie, P1), #1329 (switch first-passing, P2),
  #1331 (currentColor hardcode + color no parseado, P2), #1129/#1130 (css named-colors/hsl,
  Fixed on main — otro crate), #1190 (paint root opacity ignored, Fixed),
  #1268 (svg root presentation attrs/opacity, Fixed), #1281 (svg stale docs
  textPath/SMIL/filtros, Fixed — temas distintos de w2-assets-11), #1298 (net stale docs,
  otro crate). Sin filas sobre colores con nombre en `svg`, `hsl()` en `svg`,
  `visibility="inherit"`, `first_number`/unidades, opacidad raíz porcentual, ALPH 2/3
  ni doc JPEG progresivo.
- `gh is

## verify-browser-A

# Verificación Mesa A — browser (`w2-browser`, informe F5)

Verificador independiente. No se editó código de producto ni ledger.
Solo se crea este fichero.

- Worktree de verificación: `/tmp/opencode/megi-verify`, HEAD
  `3c06db8c186e4f7a19c7816aed4955a0b68f91ef` (`git status`: solo 2
  untracked ajenos `tests/fixtures/render/f2-exp10.html`,
  `f2-exp12.html`, resto limpio).
- `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target` (build release
  propia de esta mesa; el target original del auditor ya no se reutiliza).
- AVISO DE PROCEDENCIA: entre la lectura inicial y la ejecución, los
  repros originales (`docs/audit/repros/browser/*`, untracked en el árbol
  principal) y el propio `f5-break-browser.md` desaparecieron del árbol
  (otra sesión los eliminó/movió; `find` no los localiza en todo el repo).
  Los 4 repros usados aquí son reconstrucciones byte-fieles del contenido
  leído íntegro por esta mesa antes del borrado, guardadas en
  `/tmp/opencode/browser-repro/` (fuera del repo). El veredicto se basa en
  esas copias + el texto del informe (conservado en contexto).
- Comando base (×3 cada repro, `timeout 60s`, CWD `/tmp/opencode/megi-verify`):
  `cargo run -q --release -p megi-browser -- <repro> --screenshot /tmp/v-<id>-<i>.png --save-html /tmp/v-<id>-<i>.html --no-session`

| id | veredicto | comando | salida literal (recortada) | nota |
|----|-----------|---------|----------------------------|------|
| w2-browser-01 | VERIFIED | `… -- gbr-mutate.html --screenshot … --

## verify-browser-B

# Mesa B — fiscal de dedup y severidad: `f5-break-browser.md`

Fecha: 2026-11-18 · HEAD auditado por el informe: `3c06db8` · HEAD actual al fiscalizar: `e87f206`.
Fuentes: `docs/audit/FINDINGS.md` (ledger, 228), `docs/GAPS.md`, `issues-index.json` (1000),
`gh issue list` (open: #1544, #1541, #1534, #1519, #1516).

## Veredictos

| id | veredicto | severidad | título | accionable | spec | motivo |
|---|---|---|---|---|---|---|
| w2-browser-01 | YA REPORTADO #1470 (comentar con evidencia nueva) | P2 — mantener | concreto | sí (re-layout tras scripts o proveedor sucio bajo demanda; `scripts.rs:1046-1053`) | existe (CSSOM View §5) | #1470 cerrado como fixed describe el mismo síntoma (geometría congelada pre-script); la evidencia nueva muestra que persiste por otra vía (snapshot pre-script del pipeline, no `OnceLock`). No es issue nuevo: comentar/reabrir #1470. P2 proporcionada (contrato roto, geometría falsa visible). |
| w2-browser-02 | SOLO NUEVO | P2 — mantener | concreto | sí (sincronizar setter con el elemento `<title>`, crear si falta; `runtime.rs:3785-3793`, `title.rs:8-12`) | existe (HTML Standard §`document.title`) | Sin entradas en FINDINGS (grep `title` solo da `document_write` #1125), GAPS ni issues abiertos. `host.title` sin más lectores (`runtime.rs:634,3781,3791`) confirma el no-op. P2 proporcionada (contrato roto, título de ventana/pestaña nunca se actualiza). |
| w2-browser-03 | SOLO NUEVO | P3 — mantener | concreto | sí (sanear `resolved` en el error del worker 

## verify-ci-A

# Verificación Mesa A — CI/tooling + perf (ola 2)

Fecha: 2026-11-18. Verificador (Mesa A, scope tools/CI/perf).
Base: worktree limpio `/tmp/opencode/megi-verify` @ `3c06db8`
(`git status` limpio salvo 2 fixtures untracked ajenos al scope:
`tests/fixtures/render/f2-exp10.html`, `f2-exp12.html` — no se tocan).
Env: `TMPDIR=/home/jp/.cache/megi-tmp`,
`CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target`.
Chequeo HEAD actual (`d82118c`): `git diff 3c06db8 HEAD --` sobre
`tools/real_site_check.py`, `.github/workflows/ci.yml`,
`.github/workflows/coverage.yml`, `docs/audit/FINDINGS.md`,
`docs/FUZZING.md`, `docs/TOOLING.md`, `SECURITY.md`, `CONTRIBUTING.md`,
`fuzz/Cargo.toml`, `crates/bench`, `crates/svg`, `crates/layout`,
`tools/bench.sh`, `tools/bench_compare.py`, `docs/PERFORMANCE.md` →
**vacío**. Todo lo verificado vale también en HEAD actual.

## Tabla

| id | veredicto | comando | salida literal | nota |
|---|---|---|---|---|
| w2-ci-01 | VERIFIED | `python3 docs/audit/findings/wave2/repro-w2-ci/blank_gate.py` (desde verify; `sys.path` → `tools` del worktree) | `classify blank -> 'PÁGINA EN BLANCO'` / `failure_reasons -> []` / `exit=1` | `classify()` (`tools/real_site_check.py:54`) devuelve `"PÁGINA EN BLANCO"` pero `failure_reasons()` (`:192`) compara contra `"PÁGINA EN BLANK"`. El gate nunca falla por página en blanco, contra su docstring (hard failure, issue #1485). |
| w2-ci-02 | VERIFIED | `git blame -L 135,140 -L 181,186 .github/workflows/ci.yml` + demos `bash -c '(exit 

## verify-ci-B

# Mesa B — fiscal de dedup y severidad (scope tools/CI/perf, `f3-tooling-ci-docs.md` + `f4-perf.md`)

- Informes fiscalizados: `docs/audit/findings/wave2/f3-tooling-ci-docs.md` y
  `docs/audit/findings/wave2/f4-perf.md` — **inexistentes** en el árbol
  (confirmado por Mesa A en `verify-ci-A.md` y re-verificado: `ls` de wave2,
  `glob **/f3*` / `**/f4*`, `grep -rn "w2-ci\|w2-perf"` en `docs/` y
  `.superpowers/` — cero coincidencias salvo la fila de planificación en
  `progress.md:42`, entregables pendientes de `ses_e119223abffeX4XbRiYp236FVF`).
- Fuentes de dedup: `docs/audit/FINDINGS.md` (228: 118 deferred, 107 fixed,
  3 unclassified), `docs/GAPS.md` (sin entradas #1308/#1311),
  `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json`
  (1000 issues; 5 OPEN: #1544, #1541, #1534, #1519, #1516 — ninguna de scope
  tools/CI/perf), más verificación directa contra el árbol (`HEAD` actual).
- Escala aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass;
  P2 = corrupción visible o contrato roto; P3 = cosmetic/docs.

## Tabla de veredictos

| id | veredicto | sev. propuesta | título concreto | acción correcta |
|---|---|---|---|---|
| w2-ci-01 (esperado) | REQUIERE MÁS EVIDENCIA | — | no evaluable | informe fuente inexistente; sin descripción no hay nada que deduplicar ni severizar |
| w2-ci-02 (ledger #1308/#1311) | CLAIM FACTUAL CORRECTO, pero NO ES ISSUE — tarea de mantenimiento del ledger | P3 máximo (docs/ledger), no P0–P2 | n/a (no es defecto de 

## verify-core-A

# Verificación Mesa A — scope core (`f2-core.md`)

- Mesa: A (reproductor independiente). Fecha: 2026-11-18.
- Informe verificado: `docs/audit/findings/wave2/f2-core.md` (untracked, sin commit).
- Base de ejecución: worktree limpio `/tmp/opencode/megi-verify`, HEAD `3c06db8`
  (sólo `?? fuzz/corpus/...` untracked; `git status` sin WIP en `crates/`).
  Env: `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target`,
  `TMPDIR=/home/jp/.cache/megi-tmp`. Cada sonda se ejecutó 2 veces (`diff` → idénticas).
- Sonda: `/home/jp/.cache/megi-probe-A` (deps → crates del worktree verify) + sonda
  del auditor `/tmp/probe` (modo `mem`) + `/tmp/dup.rs` + `cargo build -p megi-css`.
- ⚠️ El informe está **truncado a mitad de `w2-core-10`** (termina en el literal
  `...[truncated 3880 chars]`): los cuerpos de `w2-core-11/12/13/14` y
  `latent-security` **no existen** en el fichero. Esos 4 (+latent) se verificaron
  contra su línea de cobertura + reconstrucción del repro; se marca [T] = cuerpo truncado.
- ⚠️ Conteo: el brief pide 12 hallazgos pero el informe referencia `w2-core-01…14`
  (14). Se verifican los 14 referenciados + `latent-security`.

## Tabla de veredictos

| id | veredicto | comando | salida literal (recortada) | nota |
|----|-----------|---------|----------------------------|------|
| w2-core-01 | VERIFIED | `cargo run --manifest-path /home/jp/.cache/megi-probe-A/Cargo.toml` (T2/T3/T5/T6/T8) | `T2 translate pct: Some(Transform { e: 10.0, .. })`; `T3 rotate rad: Some(Transform { a: 

## verify-core-B

# Mesa B — fiscal de dedup y severidad (scope core, `f2-core.md`)

- Informe fiscalizado: `docs/audit/findings/wave2/f2-core.md` (HEAD `3c06db8`).
- Fuentes de dedup: `docs/audit/FINDINGS.md` (228), `docs/GAPS.md`,
  `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json`
  (1000 issues, 5 abiertas: #1544, #1541, #1534, #1519, #1516 — ninguna del scope),
  más `gh`-equivalente vía índice local. Ninguna abierta solapa con core.
- Escala aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass;
  P2 = corrupción visible o contrato roto; P3 = cosmetic/docs.
- Ubicaciones `archivo:línea` verificadas contra el árbol (todas existen salvo nota).

## Tabla de veredictos

| id | veredicto | sev. propuesta | título concreto | acción correcta |
|---|---|---|---|---|
| w2-core-01 | SOLO NUEVO (rel. #1539 cerrada) | P2 (mantener) | sí | validar unidad por función + aridad exacta |
| w2-core-02 | SOLO NUEVO (rel. #1127, #1452) | P3 (mantener) | sí | `eq_ignore_ascii_case` en nombres de función |
| w2-core-03 | SOLO NUEVO (rel. #1128, #1469) | P3 (mantener) | sí | tope 1e6 en `to_length`/`evaluate_*` o documentar excepción |
| w2-core-04 | SOLO NUEVO (rel. #1153, #1445) | P2 (mantener) | sí | `.filter(is_finite)` antes del clamp |
| w2-core-05 | SOLO NUEVO | P2 (mantener) | sí | `is_finite` + `>= 0` en ambas vías |
| w2-core-06 | SOLO NUEVO | P3 (mantener) | sí | inválido total si un componente falla (no `filter_map` parcial) |
| w2-core-07 | SOLO NUEVO | P3 (mante

## verify-core-B2

# Mesa B — fiscal de dedup y severidad, REINTENTO (scope core, `f2-core.md` completo)

- Informe fiscalizado: `docs/audit/findings/wave2/f2-core.md` (15 hallazgos 01–09, 10a, 10b, 11–14 + 1 latent; HEAD `3c06db8`; alcance `crates/core|html|css` intacto según el propio informe).
- Este fichero sustituye a `verify-core-B.md` (aquel fiscalizó 01–10 con cuerpo y marcó 11–14 + latent como no evaluables por truncamiento; el informe ya está completo y aquí se fiscaliza todo). No se borra ningún archivo.
- Escala aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass; P2 = corrupción visible o contrato roto; P3 = cosmetic/docs/perf-no-DoS.
- Fuentes de dedup: `docs/audit/FINDINGS.md` (228 filas), `docs/GAPS.md`, `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json` (1000 issues; 5 abiertas: #1544, #1541, #1534, #1519, #1516 — ninguna del scope), más greps directos al árbol. Ninguna abierta solapa con core.

## Tabla de veredictos

| id | veredicto | sev. propuesta | título concreto | accionable | spec citada |
|---|---|---|---|---|---|
| w2-core-01 | SOLO NUEVO (rel. #1539 cerrada) | P2 (mantener) | sí | sí | sí (CSS Transforms 1 §6/§13) |
| w2-core-02 | SOLO NUEVO (rel. #1127, #1452) | P3 (mantener) | sí | sí | sí (CSS Syntax §4.3.4, ASCII case-insensitive) |
| w2-core-03 | SOLO NUEVO (rel. #1128, #1469) | P3 (mantener) | sí | sí | sí (tope interno 1e6 de `Length::parse`) |
| w2-core-04 | SOLO NUEVO (rel. #1153, #1445) | P2 (mantener) | sí | sí | sí (no

## verify-f1-A

# Mesa A — verificación scope F1 (`w2-t-*`)

Verificador independiente. Repros ejecutados desde `/tmp/opencode/megi-verify`
(HEAD `3c06db8`, limpio) con `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target`
y `TMPDIR=/home/jp/.cache/megi-tmp`; fuzz charset con
`CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-fuzz`.
Presencia re-comprobada en el HEAD actual del árbol principal (`2eaab73`):
los 8 hallazgos siguen presentes (único desplazamiento: el oráculo de
`charset.rs` está ahora en línea 47 por ediciones concurrentes, contenido idéntico).
No se editó código ni ledger. El worktree verify quedó como se encontró
(`git status` limpio salvo los 2 untracked preexistentes `f2-exp10/12.html`).

| id | veredicto | comando | salida literal (recorte) | nota |
|----|-----------|---------|--------------------------|------|
| w2-t-01 | VERIFIED | `printf '\xEF\xBB\xBF\xFF\xFE\x3C\x00\x00\x00\x00\x00\x38' > $TMPDIR/w2-t-01-input.bin` + `cd fuzz && cargo +nightly fuzz run charset $TMPDIR/w2-t-01-input.bin` | `thread '<unnamed>' (1252188) panicked at fuzz_targets/charset.rs:45:17:` / `a UTF-8 BOM must not decode to a replacement character` / `Error: Fuzz target exited with exit status: 77` | Replay de 1 archivo, no campaña completa. Input 12 B, base64 `77u///48AAAAAAA4` — idéntico al del informe. Tras pelar el BOM, `rest=FF FE 3C 00…` decodifica lossy con U+FFFD inicial (comprobado en Python: `starts with FFFD: True`), que es el resultado correcto per `crates/render/src/charset.rs:32-39` (`str

## verify-f1-B

# Verificación Mesa B — scope F1 (tests, fuzz targets, fixtures)

Fiscal de dedup y severidad sobre `docs/audit/findings/wave2/f1-tests-fuzz-fixtures.md` (8 hallazgos `w2-t-01..08`).
No se edita código ni ledger. Fecha: 2026-11-18.

## Fuentes de dedup consultadas

- `docs/audit/FINDINGS.md` (228 findings: 118 deferred, 107 fixed, 3 unclassified). Búsqueda por `charset`, `BOM`, `FFFD`, `websocket_send`, `cascade_core`, `dialog_top_layer`, `DEBUG`, `generate_exif`, `websocket_e2e`, `form_post`, `Content-Length`: único hit relevante es #1212 (prescan panic, fixed) y #1338 (svg entities→FFFD, deferred, otro crate). Ningún solape con los 8.
- `docs/GAPS.md`: brechas de producto (WebSocket ausente #1534, XHR ausente, etc.). Ninguna fila cubre oráculos de test/fuzz ni helpers e2e; sin solape.
- `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json` (1000 issues; abiertos reales: #1544, #1541, #1534, #1519, #1516): ninguno es sobre estos archivos de test. #1364 (EXIF oracle, CLOSED) es antecedente citado por el propio informe, distinto del conflicto entre generadores. #1212 CLOSED, distinto.
- `gh issue list --limit 20`: sólo abiertos de producto (iframe, shadow DOM, websocket ausente, IME, a11y). Sin solape.
- `docs/audit/findings/f1/` (7 archivos): el informe ya declara dedup contra ellos; verificación por grep no muestra estos 8 targets (F1 no cubre `fuzz_targets/charset.rs`; `let _` muertos citados son de otros archivos).

## Tabla de veredictos

| id | v

## verify-js-A

# Verificación Mesa A — `f2-js.md` (`w2-js-*`)

- Fecha: 2026-11-18. Verificador: Mesa A (reproductor independiente).
- Informe: `docs/audit/findings/wave2/f2-js.md` (9 hallazgos `w2-js-001…009`).
- Repros: `docs/audit/findings/wave2/repro-f2-js/` (`r_main.rs` = R1–R10, `r3b.rs`, `r4.rs` = R11);
  fuentes del harness idénticas a las del informe (`diff` limpio).
- Árboles probados: `/tmp/opencode/megi-verify` (HEAD `3c06db8`) y árbol principal
  (HEAD `e87f2068`, incluye #1544/#1541/#1534). Salidas **idénticas** en ambos.
- Env: `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target TMPDIR=/home/jp/.cache/megi-tmp`,
  harness con path-deps redirigidas al árbol correspondiente
  (`repro-f2-js-verify` → `megi-verify`, `repro-f2-js-head` → árbol principal).
- No se editó código ni ledger. Ningún hallazgo requiere aviso `security` privado
  (w2-js-001 es DoS local por script del mismo realm, no explotable remotamente).

## Tabla de veredictos

| id | veredicto | comando | salida literal (recortada) | nota |
|---|---|---|---|---|
| w2-js-001 | VERIFIED | `cargo run --release --bin repro-f2-js` (R3) + `--bin r3b` | `R3 fillRect(5000x5000) on 300x150 canvas: 25.49ms` · `fillRect(1000x1000): 1.84ms` / `(2000x2000): 4.78ms` / `(4000x4000): 16.10ms` | Coste ∝ área del rect, no de la superficie (25M iteraciones sobre 45k píxeles); escalado ≈×4 al duplicar lado (~cuadrático; 1.84→4.78→16.10). Código: `canvas.rs:197-201,209-212,224-233` sin clamp antes de `for py in y0..y1 / for px in x0..x1

## verify-js-B

# Verificación Mesa B — scope F2-js (crates/js/src)

Fiscal de dedup y severidad sobre `docs/audit/findings/wave2/f2-js.md` (9 hallazgos `w2-js-01..09`).
No se edita código ni ledger. Fecha: 2026-11-18. Informe restaurado desde contexto tras
borrado accidental (ver nota en el propio `f2-js.md`); los veredictos se emiten sobre el
contenido restaurado, idéntico al auditado.

## Fuentes de dedup consultadas

- `docs/audit/FINDINGS.md` (254 líneas). Búsquedas por `1465`, `worker_terminate`, `canvas`/`fillRect`, `CustomEvent`, `writeln`, `setTimeout`, `XMLHttpRequest`, `dynamic import`, `rewrite_dynamic`, `createElement.*cap`, `textContent.*cap`, `max_dom_nodes`, `657`: hits relevantes: fila #1138 (slots de workers, fixed — citada como verificada, no como hallazgo), fila #1145 (reescritura textual de imports en strings/comments, "parcialmente abordado" — solapa con w2-js-002). Sin filas sobre canvas-DoS, CustomEvent cancelable, writeln, Event() sin args, setTimeout string, XHR duplicado ni handle de canvas.
- `docs/GAPS.md`: brechas de producto (canvas ausente #1533/#1545, XHR ausente, etc. — todas pre-implementación y hoy stale donde el informe lo señala). Ninguna fila cubre los 9 defectos.
- `docs/audit/findings/f2/00-index.md`: cita #1465 (worker_terminate, índice forjado) y #644 (handles escribibles) como antecedentes conocidos; el informe los declara verificados-corregidos, no hallazgos — correcto.
- `gh issue list --state all` (búsquedas `1465`, `canvas fillRect DoS`, `write

## verify-lp-A

# Verificación Mesa A — layout/paint (w2-lp-01…w2-lp-20)

Worktree: `/tmp/opencode/megi-verify` (HEAD `3c06db8`).
`diff -r` confirma `crates/layout` y `crates/paint` **byte-idénticos** al árbol
principal, por lo que los veredictos valen para el HEAD actual en este scope.
Env: `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target`,
`TMPDIR=/home/jp/.cache/megi-tmp`. Suites: `cargo test -p megi-paint` ⇒
**253 passed, 0 failed**; `cargo test -p megi-layout` ⇒ **287 passed, 0 failed**.

| id | veredicto | comando | salida literal (recorte) | nota |
|----|-----------|---------|--------------------------|------|
| w2-lp-01 | VERIFIED | `cargo run --bin w2lp` (en `/tmp/opencode/w2lp`) | `R1 background-clip:padding-box red rects: [Rect { x: 0.0, y: 0.0, width: 120.0, height: 70.0 }]` / `R1 verdict: red covers border-box (120x70)? true` | El rojo cubre 120×70 = border box con `clip: padding-box`. Código: `paint_background_border` emite sobre `border_box` sin consultar `layer.clip`. |
| w2-lp-02 | VERIFIED | `cargo run --bin w2lp` | `R2 object-fit:none image rect: Some(Rect { x: -75.0, y: -75.0, width: 200.0, height: 200.0 }), any PushClip: false` / `overflows content box unclipped? true` | Sólo `Cover` recibe `PushClip`; la rama `else` (`None`/`Contain`/`ScaleDown`) emite sin clip. `none` sobredimensionado desborda. |
| w2-lp-03 | VERIFIED | lectura `rasterizer.rs:3784-3830` + `cargo test -p megi-paint` (253 passed, 0 failed) | `planes[0][index] = f32::from(pixel.r); …` (RGB sin premul

## verify-lp-B

# Mesa B — fiscal de dedup y severidad · F2 layout+paint (w2-lp-01…20)

Fuentes: `docs/audit/FINDINGS.md` (ledger), `docs/GAPS.md`, `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json` (1000 issues), `gh issue view` (etiquetas y estado real). No se editó código ni ledger.

Regla P0–P3 aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass; P2 = corrupción visible o contrato roto; P3 = cosmetic/docs/latente sin efecto visible.

## Veredictos

| id | veredicto | dedup | sev. informe → tracker → propuesta | título / accionabilidad |
|---|---|---|---|---|
| w2-lp-01 | YA REPORTADO #1183 (comentar con evidencia nueva) | FINDINGS deferred #1183, título idéntico | P2 → P2 → **P2** | Título concreto, fix accionable (clip_rect + PushClip, líneas citadas). Spec: CSS Backgrounds 3 existe como documento; nº de sección no verificado en esta mesa. |
| w2-lp-02 | YA REPORTADO #1185 (comentar con evidencia nueva) | FINDINGS deferred #1185, título idéntico | P2 → P3 → **P2** | Título concreto pero de alcance mayor que la evidencia: el informe admite que sólo `none` sobredimensionado está roto (`cover` ya recorta, `contain`/`scale-down` no desbordan). Fix accionable. Upgrade P3→P2 defendible (invasión visible del layout vecino), dejar a chair. |
| w2-lp-03 | YA REPORTADO #1184 (comentar con evidencia nueva) | FINDINGS deferred #1184, título idéntico | P2 → P2 → **P2 con reserva** | Título concreto, fix accionable, pero **sin repro de píxeles** (sólo lectura + sui

## verify-net-A

# Mesa A — Verificación independiente F2-net (`crates/net`)

- Verificador: Mesa A (reproductor independiente, sin fiarse del auditor).
- Commit verificado: `3c06db8` en worktree limpio `/tmp/opencode/megi-verify`
  (el árbol principal tiene WIP que rompe la compilación; no se usó).
- `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target`.
- Repro reutilizado del informe (sin modificar): copiado a
  `/tmp/opencode/w2net-verify-A` con el `path` de `net` re-apuntado al worktree
  limpio. `cargo run --offline` + `cargo test -p megi-net --offline` ejecutados ahí.
- `cargo test -p megi-net`: todo verde (157 unit + suites de integración, 0 fallos).

## Hallazgos nuevos

| id | veredicto | comando | salida literal recortada | nota |
|----|-----------|---------|--------------------------|------|
| w2-net-001 | VERIFIED | `cd /tmp/opencode/w2net-verify-A && cargo run --offline` (R7) + lectura `loader.rs:509-527` | `R7 blocked-push-cookie-in-jar: true` | `store_pushed` hace `store_from_response` del jar antes del gate `cors::validate_response`; el push bloqueado contamina el jar. El comentario del propio código exige el mismo gate CORS, pero el jar ya quedó escrito. |
| w2-net-002 | VERIFIED | mismo binario (R1, R1b) + lectura `websocket.rs:99-122` | `R1 bogus-status-accepted: true` / `R1b missing-upgrade-headers-accepted: true` | Status validado con `contains(" 101")` (acepta `200 OK ( 101 reasons)`); sólo se exige `Sec-WebSocket-Accept`, nunca `Upgrade`/`Connection`. |
| w2-net-003 | 

## verify-net-B

# Mesa B — Fiscalía de dedup y severidad F2-net (`crates/net`)

- Fiscal: Mesa B (criterio de juez; no reproduce, juzga).
- Fuentes de dedup consultadas: `docs/audit/FINDINGS.md` (líneas 79–99 confirman
  #1252, #1260, #1262, #1286, #1288, #1290, #1293, #1294, #1297, #1298 con
  títulos coincidentes), `grep` de `websocket|Sec-WebSocket|wss://` en
  `FINDINGS.md` (sin entradas WebSocket: sólo falsos positivos de `mask` en
  svg), informe `f2-net.md` (declaraciones de dedup del auditor).
- No se consultó `gh issue list` (sin red en esta sesión); los 10 dedups se
  aceptan contra el ledger local, que los lista como Deferred/Fixed con el
  mismo título.
- Escala aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass;
  P2 = corrupción visible o contrato roto; P3 = cosmetic/docs.

## Veredictos

| id | veredicto | sev. auditor → fiscal | motivo |
|----|-----------|----------------------|--------|
| w2-net-001 | YA REPORTADO #1252 (comentar con evidencia nueva) | P1 → P1 | El título de #1252 cubre exactamente esta ruta (`Set-Cookie` antes del gate CORS). Evidencia nueva real: el test existente no cubre cookie-en-bloqueado y el jar sí se contamina. Título concreto, fix accionable, spec Fetch/CORS existe. `security`: hereda advisory, no issue público. |
| w2-net-002 | SOLO NUEVO | P1 → P1 | Sin entrada WS en el ledger. `contains(" 101")` + ausencia de `Upgrade`/`Connection` es bypass del gate de handshake (confusión de protocolo). Título concreto, fix barato y accionable, spec R

## verify-render-A

# Verificación Mesa A — `f2-render.md` (`w2-render-01…13`)

- Fecha: 2026-11-18. Verificador: Mesa A (reproductor independiente).
- Informe: `docs/audit/findings/wave2/f2-render.md` (13 hallazgos).
- Commit verificado: `3c06db8` en worktree limpio `/tmp/opencode/megi-verify`
  (HEAD confirmado `git rev-parse`; árbol limpio salvo fixtures `f2-exp10/12.html`
  generados por las propias corridas).
- Env: `TMPDIR=/home/jp/.cache/megi-tmp CARGO_TARGET_DIR=/home/jp/.cache/f2-target`,
  ejecución desde `/tmp/opencode/megi-verify`. Binario `render` construido por Mesa A
  desde ese árbol (`cargo build -p megi-render --bin render` → `Finished dev profile`,
  binario en `/home/jp/.cache/f2-target/debug/render`).
- Repros: crate auxiliar del auditor en `/home/jp/.cache/f2-repro` reutilizado
  SIN modificar (sus path-deps apuntan a `/tmp/opencode/megi-verify`; el `.d`
  del binario precompilado lo confirma; fuentes idénticas al commit verificado).
  Sondas propias nuevas en `/home/jp/.cache/w2renderA/` (CLI `--json` + servidor
  WS en python con log de handshake) donde el repro del auditor resultó insuficiente.
- Excepción: `w2-render-11` no existe en `3c06db8` (grep de `contentDocument` /
  `frame_documents` vacío en `crates/`); se verificó con binario construido por
  Mesa A desde el árbol principal (HEAD `d82118c`, que sí contiene el código
  `e87f206`): `CARGO_TARGET_DIR=/home/jp/.cache/w2r11-target` → `Finished dev`.
- No se editó código ni ledger. No se borró ningún fichero.

## Ta

## verify-render-B

# Mesa B — fiscal de dedup y severidad: `f2-render.md`

Fecha: 2026-11-18 · HEAD auditado por el informe: `3c06db8` · HEAD actual al fiscalizar: `7e1d2d7`.
Fuentes: `docs/audit/FINDINGS.md` (ledger, 228), `docs/GAPS.md`, `gh issue list --state open`
(33 abiertas: #1516, #1519, #1534, #1541, #1544, #1618–#1646; ver Concern 2 sobre el índice),
`SECURITY.md` (criterio de reporte privado), verificación puntual en código HEAD
(`frames.rs`, `scripts.rs:1290-1330`, `ws_bridge.rs:45-105`, `lib.rs`, `images.rs`).

Nota de alcance: `verifier-template.md` (Mesa B) no existe en el repo y tampoco
`issues-index.json` (citado por otras mesas; `glob **/*issues*` solo devuelve
`tools/audit_recover_issues.py`). Se usa `verify-browser-B.md` como plantilla de
formato y `gh` como fuente de abiertos.

## Veredictos

| id | veredicto | severidad | título | accionable | spec | motivo |
|---|---|---|---|---|---|---|
| w2-render-01 | SOLO NUEVO — **vía advisory privado primero** (ver Decisión advisory) | P1 — mantener | concreto | sí (contador global de frames por página, análogo a `MAX_TOTAL_IMPORTS`; `frames.rs:44`, `lib.rs:1229`) | propiedad DoS-resistance (`SECURITY.md` diseño + línea 25: hang/CPU/memoria se reporta en privado) | Sin colisión en ledger, GAPS ni abiertas (iframe en GAPS es #1532 "blank", distinto). Código HEAD confirma: `out` es local por llamada (`frames.rs:43`), `MAX_FRAMES_PER_PAGE=16` se aplica por nivel, `MAX_FRAME_DEPTH=4`; la amplificación 1+16+256+4096+65536 = 69905 renders

## verify-st-A

# Verificación Mesa A — scope shell/bench/harness (`f2-shell-tools.md`)

- Mesa: A (reproductor independiente). Fecha: 2026-11-18.
- Informe verificado: `docs/audit/findings/wave2/f2-shell-tools.md` (F1–F19).
- Base de ejecución: worktree limpio `/tmp/opencode/megi-verify`, HEAD `3c06db8`
  (sólo `?? tests/fixtures/render/f2-exp10.html`, `f2-exp12.html` untracked;
  `git status` sin WIP en `crates/`).
  Env: `TMPDIR=/home/jp/.cache/megi-tmp`, `CARGO_TARGET_DIR=/home/jp/.cache/f2-target`.
  Binario: build fresco `cargo build -p megi-browser` (24.8 s, sin warnings relevantes).
- Regla aplicada: todo lo que depende de ventana real (`window.rs`, display) queda
  en `needs-repro` aunque la evidencia de código sea concluyente. Los marcados
  `security`+`advisory` (F2 favicon `file:`, F9 tmp predecibles) quedan como
  **advisory, no issue público**.

## Tabla de veredictos

| id | veredicto | comando | salida literal (recortada) | nota |
|----|-----------|---------|----------------------------|------|
| F1 w2-st-1169-session-tabs | NEEDS-REPRO | lectura `window.rs:371-402` | `Some(session) => { … restored … }` sin referencia a `view.tabs`; rama `None => TabsState::from_urls(view.tabs.clone())` | El merge sólo contempla `input` (posición/blank-replace/open_tab). Requiere display (sesión + `--tabs`). Código concluyente, evento no ejercitado. |
| F2 w2-st-1173-file-icon | VERIFIED + **ADVISORY** | `megi http://127.0.0.1:8932/present.html --chrome --screenshot h_present.png` vs `absent.

## verify-st-B

# Verificación Mesa B — scope shell/bench/harness (`f2-shell-tools.md`)

Fiscal de dedup y severidad sobre `docs/audit/findings/wave2/f2-shell-tools.md`
(19 hallazgos `w2-st-*`: F1–F19). No se edita código ni ledger.
Fecha: 2026-11-18. Nota: no existe ningún `verifier-template.md` en el árbol
(`glob **/*verif*.md` solo devuelve los `verify-*.md` de wave2); se usa como
plantilla el formato de `verify-js-B.md` / `verify-ci-B.md` (tabla de veredictos
+ concerns).

## Fuentes de dedup consultadas

- `docs/audit/FINDINGS.md`: filas #1163, #1165, #1166, #1167, #1168, #1169,
  #1171, #1172, #1173, #1174, #1175, #1176, #1177, #1178 (shell),
  #1197, #1198, #1199, #1201 (bench) — todas Deferred en ledger.
- `docs/GAPS.md`: brechas de producto ausente (form submission, live event loop,
  JS APIs, a11y tree…). Ninguna fila cubre los 19 defectos (son defectos de
  código existente, no features ausentes).
- `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json`
  (1000 issues; 5 OPEN: #1544, #1541, #1534, #1519, #1516 — ninguna de scope
  shell/bench/harness). Los 19 issues de referencia están CLOSED.
- `docs/audit/findings/wave2/issues/`: sin ficheros `w2-st-*` (sin colisión con
  borradores existentes).
- Verificación puntual contra el árbol: `favicon.rs:50-129` (sin chequeo de
  esquema, confirma F2), `download.rs:106-161` (tmp predecible + overflow sin
  `exists()`, confirma F9/F18).
- Escala aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass;
  P2 = c

