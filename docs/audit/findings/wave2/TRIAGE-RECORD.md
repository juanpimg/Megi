# Registro de triaje — Ola 2 (2026-11-24)

Consolidación de las actas de mesa. Detalle completo en `git log -- docs/audit/findings/wave2/meeting-*.md`.

## meeting-assets

# Mesa C — reunión de verificación (scope fonts/image/svg, `w2-assets-*`)

- Fecha: 2026-11-18. Chair: Mesa C (scope fonts/image/svg).
- Commit verificado: `3c06db8` (worktree limpio `/tmp/opencode/megi-verify` según informe F2 y Mesa A; HEAD actual `d82118c` sin cambios en los archivos citados — `git log 3c06db8..HEAD` vacío en `document.rs`, `raster.rs`, `color.rs`, `gradient.rs`, `webp.rs`, `jpeg.rs`).
- Fuentes: informe `docs/audit/findings/wave2/f2-assets.md` (11 hallazgos `w2-assets-01…11` + concerns C1–C6), `verify-assets-A.md` (Mesa A, 11/11 VERIFIED + C1 ejecutado), `verify-assets-B.md` (Mesa B, fiscal dedup/severidad), spot-check propio del chair (C1 en HEAD actual).
- Nota de procedimiento: `verifier-template.md` (Mesa C) no existe en el repo (Mesa B ya lo constató por glob sin resultados); formato tomado de `meeting-net.md` (tabla final + avisos/incluidos/descartes).
- Regla de inclusión: `include` exige A=VERIFIED **y** B≠DUPLICADO (`YA REPORTADO` → `comment-existing`, no issue nuevo; `SOLO NUEVO` → publicar). No se edita código ni ledger. No se borra ningún archivo.

## Tabla final

| id | veredicto | sev | área | etiquetas | incl/excl | motivo |
|----|-----------|-----|------|-----------|-----------|--------|
| w2-assets-01 | VERIFIED (A) / YA REPORTADO #1329 (B; #1273 duplicado previo) | P2 | svg switch | public, spec-s5.8 | exclude (comment-existing) | Rama `"g" \| "a" \| "switch"` como grupo (`document.rs:1067`); ambos hijos pintados (`painted= 5000`, rojo 2500 + azul 2500). Evidencia nueva en HEAD, pero match exacto con #1329 (P2 coincidente). |
| w2-assets-02 | VERIFIED (A) / YA REPORTADO #1324 (B) | P1 | svg miter | public, spec-s11.4 | exclude (comment-existing) | Intersección `v+t·n0` vs `v+u·n1` con `t ≡ -1` implícito (`raster.rs:1883-1920`); tip (100,1.97) vacío. Evidencia nueva fuerte, pero #1324 es el mismo defecto P1. |
| w2-assets-03 | VERIFIED (A) / SOLO NUEVO (B; rel #1129 otro crate) | P2 | svg color | public, css-color-3 | include | 

## meeting-browser

# Mesa C — reunión de verificación, scope browser (`w2-browser-*`)

Fecha: 2026-11-18 · Chair: Mesa C.
HEAD auditado por el informe: `3c06db8` · HEAD actual: `e87f206`.

## Fuentes

- Plantilla: `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/verifier-template.md`.
- Mesa B: `docs/audit/findings/wave2/verify-browser-B.md` (veredictos y dedup citados abajo).
- Mesa A / informe original: `docs/audit/findings/wave2/verify-browser-A.md` y `docs/audit/findings/wave2/f5-break-browser.md` **no localizados** — búsqueda en todo el árbol (`w2-browser` solo aparece en `verify-browser-B.md`). Decisión: se acepta el `A=VERIFIED` del brief del chair para 02/03, reforzado con verificación propia a nivel de código sobre `3c06db8` (citas literales en cada cuerpo). 01 se trata como regresión ya reportada (comentario, no issue). 04 se excluye por WIP declarado en el propio informe.
- No se editó código ni ledger. Ningún hallazgo requiere aviso `security` privado (03 es publicable según Mesa B).

## Tabla final

| id | veredicto consensuado | sev | área | etiquetas | include/exclude | motivo |
|---|---|---|---|---|---|---|
| w2-browser-01 | YA REPORTADO #1470 (regresión, evidencia nueva) | P2 | render | bug, P2, render | exclude (comment-existing) | #1470 CLOSED/fixed describe el mismo síntoma; la evidencia nueva muestra otra vía (snapshot pre-script del pipeline, no `OnceLock`). Comentar/reabrir #1470, no abrir issue. |
| w2-browser-02 | SOLO NUEVO · A=VERIFIED (brief; mecanismo confirmado en código `3c06db8`) | P2 | render | bug, P2, render | **include** | Setter `document.title` no-op (solo `host.title`, sin lectores); título de ventana sale del `<title>` del DOM. Sin colisiones. |
| w2-browser-03 | SOLO NUEVO · A=VERIFIED (brief; mecanismo confirmado en código `3c06db8`) | P3 | render | P3, render, security (menor, publicable) | **include** | Errores del worker loader con ruta `file://` absoluta hasta el script. Solo local, sin remoto. Sin colisiones. |
| w2-browser-04 

## meeting-ci

# Mesa C — reunión de verificación (scope tools/CI/perf, `w2-ci-*` + `w2-perf-*`)

- Fecha: 2026-11-18. Chair: Mesa C (scope tools/CI/perf).
- Commit verificado: `3c06db8` (Mesa A: worktree limpio `/tmp/opencode/megi-verify`;
  `git diff 3c06db8 HEAD --` sobre todos los paths del scope → vacío, vale en HEAD actual).
- Fuentes: `f3-tooling-ci-docs.md` (w2-ci-01…03), `f4-perf.md` (w2-perf-01…05),
  `verify-ci-A.md` (8/8 VERIFIED con salida literal), `verify-ci-B.md`
  (7 REQUIERE MÁS EVIDENCIA + w2-ci-02 mantenimiento).
- Nota de procedimiento: Mesa B fiscalizó sin materia (informes ausentes en ese
  momento, ya restaurados). Dado que A verificó los 8 con salida real y las fuentes
  ya existen, esta mesa incluye los 7 con A=VERIFIED y no-duplicados como issues
  públicos; w2-ci-02 queda como tarea de mantenimiento (no issue). NO se borra nada,
  no se edita código ni ledger, no se publica.

## Tabla final

| id | veredicto | sev | área | etiquetas | incl/excl | motivo |
|---|---|---|---|---|---|---|
| w2-ci-01 | VERIFIED (A) | P2 | tools/gate | public, bug | include | Centinela `"PÁGINA EN BLANK"` vs `"PÁGINA EN BLANCO"` de `classify()`; repro `blank_gate.py` con salida literal. Dedup: new. |
| w2-ci-02 | VERIFIED (A) pero NO ISSUE | P3 máx | ledger/docs | maintenance | exclude (mantenimiento) | Fixes #1308/#1311 existen en HEAD; sólo la fila de FINDINGS está stale por boilerplate de cierre. Arreglo procedural (regenerar/anotar), no defecto de producto. Ver `maintenance` abajo. |
| w2-ci-03 | VERIFIED (A) | P3 | docs/fuzz | public, docs | include | Deriva 26→28 bins, matriz 25+9, corpus 170/258/158 — deriva bajo cualquier número. Rel. #1507 (re-deriva posterior al cierre). |
| w2-perf-01 | VERIFIED (A) | P2 | bench | public, perf | include | Strip 0/29; bench 0,816 vs 0,818 (A: 0,307 vs 0,307). Rel. #1197. |
| w2-perf-02 | VERIFIED (A) | P2 | bench | public, perf | include | 800×600=0,48 MPix vs 0,096 declarado (5×). Rel. #1198. |
| w2-perf-03 | VERIFIED (A) | P3 | be

## meeting-core

# Mesa C — reunión de verificación (scope core, `w2-core-*`)

- Fecha: 2026-11-18. Chair: Mesa C (scope core).
- Commit verificado: `3c06db8` (worktree limpio `/tmp/opencode/megi-verify`).
- Fuentes: informe `f2-core.md` (15 hallazgos 01–09, 10a, 10b, 11–14 + 1 latent;
  sonda `/tmp/probe` T1–T10, L1–L7, O1/O2, H1–H3, W1, B1/B2, Q1/Q2, V1, M1/M2,
  F1, C1/C2, S1 + `mem`, 2 ejecuciones idénticas; `cargo test -p megi-core -p megi-html -p megi-css`
  → 529 passed, 0 failed; `cargo build -p megi-css` → 0 warnings; `/tmp/dup.rs`),
  `verify-core-A.md` (reproductor independiente), `verify-core-B.md` (fiscal
  inicial 01–10, truncado), `verify-core-B2.md` (fiscal completo, sustituye a B),
  y dedup contra índice local (1000 issues; abiertas: #1544, #1541, #1534, #1519,
  #1516 — ninguna del scope) + `docs/audit/FINDINGS.md` (228) + `docs/GAPS.md`.
- Regla aplicada: include exige A=VERIFIED/PARTIAL y B≠DUPLICADO. B2 deja
  14 SOLO NUEVO (01–09, 10a, 10b, 11, 12, 13) y 2 REQUIERE MÁS EVIDENCIA
  (14 y latent) → exclude.
- Notas de corrección aplicadas al publicar: 10a y 10b como DOS ISSUES SEPARADOS
  (10b sólo el tope 1e6; `VerticalAlign` SÍ filtra `is_finite`, claim retirado;
  negativos válidos por CSS 2.1 §10.8.1); w2-core-06 con el discriminador B1b
  (`no-repeat, bogus`); w2-core-09 como cleanup, no bug de producto.
- `verifier-template.md` no existe en `docs/audit/findings/wave2/` (verificado
  por el chair antes de escribir). No se borró ningún archivo. No se editó
  código ni ledger ni se publicó nada.

## Tabla final

| id | veredicto | sev | área | etiquetas | incl/excl | motivo |
|---|---|---|---|---|---|---|
| w2-core-01 | VERIFIED | P2 | css-transform | public | include | `%`/`rad`/`qpx` aceptados, aridad ignorada (`transform.rs:216-236`); rel. #1539 cerrada |
| w2-core-02 | VERIFIED | P3 | css-transform | public | include | nombres de función case-sensitive (`transform.rs:51`); rel. #1127, #1452 |
| w2-core-03 | VERIFIED | P3 | css-calc | public | include | `c

## meeting-f1

# Mesa C — reunión de verificación (scope F1, `w2-t-*`)

- Fecha: 2026-11-18. Chair: Mesa C.
- Commit auditado: `3c06db8` (árbol limpio `/tmp/opencode/megi-verify`; presencia re-comprobada por Mesa A en HEAD principal `2eaab73`: los 8 persisten, único desplazamiento: oráculo de `charset.rs` ahora en línea 47, contenido idéntico).
- Fuentes: informe `docs/audit/findings/wave2/f1-tests-fuzz-fixtures.md` (8 hallazgos `w2-t-01..08`), Mesa A `verify-f1-A.md` (8/8 VERIFIED), Mesa B `verify-f1-B.md` (8/8 SOLO NUEVO). (Nota: `verifier-template.md` (Mesa C) no existe en el árbol; formato reconstruido de `meeting-net.md` / `meeting-js.md`.)
- Regla include: A=VERIFIED y B=SOLO NUEVO → los 8 se incluyen. No se editó código ni ledger. Ningún `security`/producto: todo el scope es `test`/`fuzz` (w2-t-01 expresamente `fuzz`/`test`, nunca `security`).

## Tabla final

| id | A | B | sev | área | etiquetas | include/exclude | motivo / cuerpo |
|---|---|---|---|---|---|---|---|
| w2-t-01 | VERIFIED | SOLO NUEVO | P2 (techo; P3 admisible) | fuzz target charset | fuzz, test | include | Oráculo "BOM ⇒ sin U+FFFD" falso positivo; aborta la campaña con entradas válidas (lossy correcto). Cuerpo en `issues/w2-t-01.md`. |
| w2-t-02 | VERIFIED | SOLO NUEVO | P3 | js websocket_api_tests | test | include | `Rc` de enviados descartado con `let _ = seen`; `send` sin assert. Cuerpo en `issues/w2-t-02.md`. |
| w2-t-03 | VERIFIED | SOLO NUEVO | P3 | css cascade_core | test | include | Test "records rules for each group" no toca `collection` (`let _`). Cuerpo en `issues/w2-t-03.md`. |
| w2-t-04 | VERIFIED | SOLO NUEVO | P3 | paint dialog_top_layer | test | include | `filter(SolidColor).count()` descartado con `let _`; sin assert de orden en display list. Cuerpo en `issues/w2-t-04.md`. |
| w2-t-05 | VERIFIED | SOLO NUEVO | P3 | net auth_tests | test | include | `eprintln!("DEBUG …")` commiteado (línea 69). Cuerpo en `issues/w2-t-05.md`. |
| w2-t-06 | VERIFIED | SOLO NUEVO | P3 | image EXIF fixtures | 

## meeting-js

# Meeting Mesa C — scope js (`w2-js-*`)

- Informe: `docs/audit/findings/wave2/f2-js.md` (9 hallazgos, no presente en árbol; reconstruido desde Mesa A + brief).
- Mesa A: `docs/audit/findings/wave2/verify-js-A.md` — 9/9 VERIFIED (HEAD `3c06db8` y `e87f2068` idénticas).
- Mesa B: veredictos según brief — 7× SOLO NUEVO, 2× YA REPORTADO (comentar, no abrir).
- Regla include: A=VERIFIED y B≠DUPLICADO. B marcó 2 como YA REPORTADO con recomendación de comentar.
- Commit auditado: `3c06db8`. No se editó código ni ledger. Ningún `security` privado.

## Tabla final

| id | A | B | sev | área | etiquetas | include/exclude | motivo / cuerpo |
|---|---|---|---|---|---|---|---|
| w2-js-001 | VERIFIED | SOLO NUEVO | P1 | canvas 2d fillRect | js, canvas, perf, DoS | include | bypass Budget, escalado cuadrático. Cuerpo en `issues/w2-js-001.md`. |
| w2-js-002 | VERIFIED | YA REPORTADO #1145 | P2 | import rewrite | js, modules | exclude (comment-existing) | ver comentario abajo; no abrir issue. |
| w2-js-003 | VERIFIED | SOLO NUEVO | P2 | CustomEvent | js, dom-events | include | `cancelable` default true vs false. Cuerpo en `issues/w2-js-003.md`. |
| w2-js-004 | VERIFIED | SOLO NUEVO | P3 | document.write | js, dom | include | `writeln` sin `\n`. Cuerpo en `issues/w2-js-004.md`. |
| w2-js-005 | VERIFIED | SOLO NUEVO | P3 | Event ctor | js, dom-events | include | no lanza sin tipo. Cuerpo en `issues/w2-js-005.md`. |
| w2-js-006 | VERIFIED | SOLO NUEVO | P3 | timers | js, timers | include | string-delay → 0. Cuerpo en `issues/w2-js-006.md`. |
| w2-js-007 | VERIFIED | YA REPORTADO #644 | P3 | canvas handle | js, canvas | exclude (comment-existing) | ver comentario abajo; no abrir issue. |
| w2-js-008 | VERIFIED | SOLO NUEVO | P3 docs | docs XHR/fetch | js, docs | include | doc stale. Cuerpo en `issues/w2-js-008.md`. |
| w2-js-009 | VERIFIED | SOLO NUEVO | P3 | XHR doble impl | js, perf, maintenance | include | gap prelude vs xhr.rs. Cuerpo en `issues/w2-js-009.md`. |

Resultado: 7 inclu

## meeting-lp

# Mesa C — chair · F2 layout+paint (w2-lp-01…20)

- Fecha: 2026-11-18. Chair: Mesa C (scope layout/paint).
- Entradas: `docs/audit/findings/wave2/verify-lp-B.md` (fiscal dedup/severidad).
- Faltantes: informe original `docs/audit/findings/wave2/f2-layout-paint.md` **no existe**
  en el árbol y `docs/audit/findings/wave2/verify-lp-A.md` **no existe**
  (sólo existe `verify-js-A.md` de otro scope). No hay veredictos de Mesa A
  disponibles para este scope.
- HEAD de referencia para la evidencia: `3c06db8` (baseline wave 2).
  HEAD actual del árbol: `e87f2068`. No se editó código ni ledger.
- Regla aplicada (verifier-template): `include` exige A = VERIFIED/PARTIAL y
  B ≠ DUPLICADO; empate o duda → `exclude`. Sin mesa A, **cero `include`
  como issue nuevo**. Los 12 vivos van a `comment-existing` (comentar
  evidencia fresca en issues existentes, nunca abrir nuevos — coincide con
  la concern #1 de Mesa B y con R5 del plan: un deferred no se rearchiva).

## Tabla final

| id | veredicto consensuado | sev. | área | etiquetas | include/exclude | motivo |
|---|---|---|---|---|---|---|
| w2-lp-01 | YA REPORTADO #1183 | P2 | paint | bug, P2 | comment-existing | B: título idéntico, fix accionable. Sin A → no nuevo issue; comentar evidencia HEAD `3c06db8`. |
| w2-lp-02 | YA REPORTADO #1185 | P2 (upgrade P3→P2 propuesto) | paint | bug | comment-existing | B: sólo `none` sobredimensionado roto; upgrade defendible, decide chair → se acepta P2 en el comentario, sin mover etiqueta sin consenso. |
| w2-lp-03 | YA REPORTADO #1184 | P2 con reserva | paint | bug | comment-existing | B: sin repro de píxeles (sólo lectura); P2 sólo si hay efecto visible. El comentario lo marca como reserva probatoria. |
| w2-lp-04 | YA REPORTADO #1192 | P2 (upgrade P3→P2 propuesto) | paint | bug | comment-existing | B: repro de píxeles en HEAD, fix accionable. Upgrade defendible; se propone en el comentario. |
| w2-lp-05 | YA REPORTADO #1191 | P3 | paint | bug | comment-existing | Título concreto, P3 co

## meeting-net

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
| w2-net-002 | VERIFIE

## meeting-render

# Mesa C — reunión de verificación (scope render, `w2-render-*`)

- Fecha: 2026-11-18. Chair: Mesa C (scope render).
- Commit verificado: `3c06db8` (worktree limpio `/tmp/opencode/megi-verify`; HEAD confirmado `git rev-parse` por Mesa A).
- Fuentes: informe `docs/audit/findings/wave2/f2-render.md` (13 hallazgos), `docs/audit/findings/wave2/verify-render-A.md` (reproductor independiente, binario `render` construido en `3c06db8` salvo w2-render-11), `docs/audit/findings/wave2/verify-render-B.md` (fiscal dedup/severidad contra FINDINGS/GAPS/`gh issue list`, HEAD código).
- Env Mesa A: `TMPDIR=/home/jp/.cache/megi-tmp CARGO_TARGET_DIR=/home/jp/.cache/f2-target`, repros en crate auxiliar `/home/jp/.cache/f2-repro` (sin modificar) + sondas propias `/home/jp/.cache/w2renderA/`.
- Regla include de esta ola: A=VERIFIED (+estático) y B=SOLO NUEVO, excluyendo (a) P1 DoS vía advisory privado (SECURITY.md), (b) B=YA REPORTADO → comment-existing, (c) A≠VERIFIED pleno (CONVERGENTE/NEEDS INFO/PARCIAL con oráculo roto), (d) código que no existe en `3c06db8`.
- No se editó código ni ledger. No se publicó nada.
- Formato según `meeting-net.md`.

## Tabla final

| id | A | B | sev | área | etiquetas | incl/excl | motivo / cuerpo |
|---|---|---|---|---|---|---|---|
| w2-render-01 | VERIFIED | SOLO NUEVO (vía advisory privado) | P1 | frames / DoS | security, advisory-privado | exclude (advisory) | Cap por nivel, no global; 341 renders desde ~0.3 KiB, proyección W=16 ≈70k. Borrador en `docs/audit/findings/wave2/advisories/w2-render-01.md`. Nunca issue público (SECURITY.md:25 hang/CPU/memoria en privado). |
| w2-render-02 | VERIFIED | SOLO NUEVO | P2 | build | public, build | include | `--no-default-features` roto (`ws_bridge.rs:8-9` + `lib.rs:215`). Cuerpo en `issues/w2-render-02.md`. |
| w2-render-03 | VERIFIED | YA REPORTADO #1299 | P2 | scripts budget | public, budget | exclude (comment-existing) | Presupuesto cobrado por adelantado, ≤10 scripts por defecto. Ver comentario #1299 abajo;

## meeting-st

# Mesa C — reunión de verificación (scope shell/bench/harness, `w2-st-*`)

- Fecha: 2026-11-18. Chair: Mesa C (scope shell/bench/harness).
- Commit verificado: `3c06db8` (worktree limpio `/tmp/opencode/megi-verify` según informe F2 y Mesa A; árbol principal y worktree coinciden byte a byte en el scope, `cmp` limpio).
- Fuentes: informe `docs/audit/findings/wave2/f2-shell-tools.md` (F1–F19), `verify-st-A.md` (Mesa A, reproductor independiente), `verify-st-B.md` (Mesa B, fiscal dedup/severidad), lectura directa de `favicon.rs:50-129` y `download.rs:106-161` citada por B.
- Nota de procedimiento: formato tomado de `meeting-net.md` (tabla final + avisos/incluidos/descartes) y `meeting-assets.md` (sección `comment-existing` con texto listo). No se edita código ni ledger. No se borra ningún archivo. No se publica ningún issue desde esta mesa; solo se escriben borradores bajo `issues/` y `advisories/`.
- Regla de inclusión: `include` exige A=VERIFIED **y** B=NUEVO **y** no `security`/`advisory` **y** no `needs-repro` para el evento (rama window sin display queda en `exclude (comment-existing)` o `exclude (needs-repro)` aunque el código sea concluyente). `YA REPORTADO` → `comment-existing`, no issue nuevo. `security`+`advisory` → `exclude (advisory)`, borrador en `advisories/` sin pasos explotables detallados.

## Tabla final

| id | veredicto | sev | área | etiquetas | incl/excl | motivo |
|----|-----------|-----|------|-----------|-----------|--------|
| w2-st-1169-session-tabs (F1) | NEEDS-REPRO (A) / YA REPORTADO #1169 (B) | P2 | shell session | public, contract | exclude (comment-existing) | Merge solo contempla `input`, `view.tabs` muerta en rama `Some` (`window.rs:371-402`). Requiere display. Evidencia nueva real. |
| w2-st-1173-file-icon (F2) | VERIFIED + ADVISORY (A) / YA REPORTADO #1173 (B) | P2 | shell favicon | security, advisory-privado | exclude (advisory) | `file:` desde `http(s)` sin chequeo (`favicon.rs:50-129`); oráculo 4 renders + diff 128 px + control. C

