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

## Tabla de veredictos

| id | veredicto | comando | salida literal (recortada) | nota |
|---|---|---|---|---|
| w2-render-01 | VERIFIED | `f2-repro exp01 2` + `exp01 3` + `exp01-flat 2` + lectura `frames.rs:32-48,113` | `EXP01 width=2 elapsed_ms=413 warnings=0` · `width=3 elapsed_ms=1473` · `EXP01-FLAT width=2 elapsed_ms=35` | Escalado lineal en nº de renders (31→121 renders ≈ 413→1473 ms, ×3.6) y exponencial en W; flat 35 ms (×12 / ×42). El cap `out.len() >= MAX_FRAMES_PER_PAGE` es local a cada llamada `collect_frames` (vector `out` por nivel) y la recursión vía `render_url_inner(..., depth+1)` sólo topa en `MAX_FRAME_DEPTH` (termina, pero amplifica). Proyección W=16 = 69905 renders: aritmética confirmada, sin re-ejecutar los 31.5 s. P1 proporcionado. |
| w2-render-02 | VERIFIED | `cargo check -p megi-render --no-default-features` (mismo env) | `error[E0433]: cannot find module or crate 'js' in this scope --> crates/render/src/ws_bridge.rs:8:5` (ídem `:9:5`) · `error: could not compile 'render' (lib) due to 2 previous errors` | `pub mod ws_bridge` incondicional (`lib.rs:215`) frente a `#[cfg(feature = "js")] mod browser_host`. Literal idéntica al informe. |
| w2-render-03 | VERIFIED | `f2-repro exp03` + lectura `scripts.rs:1284-1300`, `:100`, `:132` | `EXP03 executed=10 failed=0 limited=true` · `EXP03 warn: total JavaScript loop-iteration budget exhausted; skipping remaining scripts` | Matemática del informe confirmada en código: total `1_000_000` / grant `100_000` = 10 scripts; el 11º recibe `grant == 0` → `break`. `max_scripts` (64) inalcanzable por defecto. |
| w2-render-04 | VERIFIED | `f2-repro exp04` + lectura `lib.rs:1488` vs `webfonts.rs:143` | `EXP04 bundled_faces=2 with_system_faces=1106 bundled_digest=14069339365844523798 with_system_digest=8606334732936271880 equal=false` | Byte-idéntica al informe (incluidos ambos digests). Ruta SVG directa usa `new_with_bundled_and_system_fonts()`. |
| w2-render-05 | CONVERGENTE | `f2-repro exp05` / `exp05 await` + CLI `--json` (módulo con error sintáctico plain vs `//await`) + lectura `scripts.rs:1310` | `EXP05 await_comment=false executed=1 tasks=1 virt_ms=50 limited=false r="done"` · idéntica con `await_comment=true` · `PLAIN … failed: SyntaxError: unexpected token ';' … at line 4` vs `AWAIT … at line 2` (mismo mensaje, solo nº de línea desplazado) | La rama `if script.text.contains("await")` existe y se toma, pero converge en comportamiento en las 3 sondas (timer 50 ms + sintaxis). Riesgo latente + maquinaria async innecesaria: el diagnóstico a nivel de código sigue válido. Sin cambio de severidad propuesto (P2). |
| w2-render-06 | PARCIAL | `f2-repro exp06` (×2: máquina cargada y en reposo) + 3 sondas propias instrumentadas (servidor WS python con log + `console.warn` como oráculo) + lectura `scripts.rs:1704-1714` | `EXP06 idle_open_socket_elapsed_ms=424` · `inclusion_hits=0/10` (idéntico cargada/reposo) · propia 95 ms: `EV:open` + `EV:msg:hello`, `real 0m0,512s` · propia 2000 ms: sin `EV:msg`, `real 0m0,492s` · control sin WS: `real 0m0,097s` | Mecanismo confirmado en código (`sleep 5 ms × 20` dentro del dreno determinista, acoplado al wall-clock). Magnitudes del informe NO reproducidas: overhead medido ~0.33–0.43 s, no 2 s (2.56 s del informe plausiblemente bajo su máquina degradada por tmpfs lleno). **Oráculo del repro del auditor refutado**: `document.title = e.data` NO sincroniza el elemento `<title>` (`DOC-TITLE:hello` pero `TITLE-EL:init`), luego sus series 10/10–8/8–0/6 medidas vía `get_elements_by_tag_name("title")` son artefacto del oráculo, no de la entrega (la entrega a 95 ms FUNCIONA). El corte existe (a 2000 ms el mensaje se pierde: dreno ya terminado) pero su posición la decide el reloj real. Ver Concern C1. |
| w2-render-07 | VERIFIED | `render exp07.html exp07.png --json` (`<link>` a `missing-xyz.css`) | `"external_stylesheets":1` · `warnings:["skipping stylesheet file:///home/jp/.cache/w2renderA/missing-xyz.css: file not found: …"]` | `summary.external_stylesheets += 1` (`lib.rs:1981`) antes del `load_stylesheet_text` que puede fallar. Literal coincide. |
| w2-render-08 | VERIFIED (estático) | `grep -n LayoutFailed lib.rs` + lectura `lib.rs:1304-1308`, `:1520-1526`, `error.rs:36-37,85` | `1308: return Err(RenderError::LayoutFailed);` tras `if png.is_empty()` (ruta HTML) · mismo patrón en `:1526` (ruta SVG directa) · `LayoutFailed => write!(f, "layout produced no root box")` | Sin variante `PngEncode`; la más cercana (`Output(io::Error)`) es solo escritura de fichero CLI. Sin repro dinámico (forzar `to_png()` vacío no es observable vía CLI); el mapeo erróneo es literal en código. (Líneas ±14 vs informe por deriva del árbol; semántica idéntica.) |
| w2-render-09 | VERIFIED | `f2-repro exp09` + lectura `lib.rs:1121` | `EXP09 executed=0 failed=1 inline_svgs=0 dom_mutated=1 svg_nodes_left=1` | Guarda `scripts_executed == 0 && !source_mentions_svg` no exige `failed == 0`: el script mutó el DOM (`dom_mutated=1`, un `<svg>` vivo) y lanzó después. Números idénticos al informe. |
| w2-render-10 | VERIFIED | `f2-repro exp10` + lectura `images.rs:694-701` | `EXP10 file_bytes=6016 loaded=2 failed=0 downloaded=12032` · `EXP10 warn: image byte budget exhausted at … (18048 bytes total, max 15040)` | `break` sin `failed++` y sin contar omitidas (contrasta con `continue`+`failed++` de backgrounds). Cifras idénticas al informe. |
| w2-render-11 | VERIFIED | Binario Mesa A del árbol principal (`/home/jp/.cache/w2r11-target/debug/render`): `f2-parent-src.html` (iframe `src` same-origin) vs `f2-parent-srcdoc.html` (control) | `SRC executed= 1 failed= 0` · `WARN: console.warn: RESULT:NULL` ——— `SRCDOC executed= 1 failed= 0` · `WARN: console.warn: RESULT:from-srcdoc` | Asimetría `src`-no / `srcdoc`-sí confirmada por ejecución en código HEAD. En `3c06db8` la feature no existe (grep vacío): el hallazgo describe código posterior a ese commit; ver Concern C3. |
| w2-render-12 | VERIFIED | `f2-repro exp12` + lectura `images.rs:519-521` | `EXP12 loaded=1024 failed=0 warnings=0` | `if sources.len() >= 1024 { break; }` sin warning. Cero avisos con 1100 `<img>`. Idéntico al informe. |
| w2-render-13 | NEEDS INFO | — (solo lectura: `ws_bridge.rs:53-57`, `:63-70`, `:97-99`) | `let text = String::from_utf8_lossy(&frame.payload).into_owned();` · rama `Opcode::Close` envía `Close(code)` y `break` sin `stop.store(true)` · `is_open()` solo lee `stop` | Sin sonda ejecutable vía CLI (`is_open` es diagnóstico interno). Se confirma por lectura el doble sub-claim (lossy + cierre remoto no reflejado), pero queda pendiente de test unitario según el propio informe. No se marca verificado. |

## Controles adicionales

- `cargo build -p megi-render --bin render` (mismo env): `Finished dev profile … in 26.19s`.
- `grep -rn "contentDocument\|frame_documents" crates/` en `3c06db8`: vacío
  (la feature `set_frame_document`/`AccessorKind::ContentDocument` solo existe en
  el árbol principal, `runtime.rs:1497,6506+,7946` + `scripts.rs:288,836-837`).
- Sonda título-oráculo (C1): `DOC-TITLE:hello` / `TITLE-EL:init` en la misma
  ejecución — el setter `document.title` no sincroniza el elemento.

## Resumen

- 11/12 verificados con salida real o lectura literal (incl. w2-render-08 estático):
  `VERIFIED ×11` (01, 02, 03, 04, 06-parcial→verificado-mecanismo, 07, 08, 09, 10, 11, 12).
  En términos estrictos: 10 `VERIFIED` + 1 `VERIFIED (estático)` + 1 `PARCIAL`
  (w2-render-06, mecanismo sí / cuantificación y oráculo no) + 1 `CONVERGENTE`
  (w2-render-05) + 1 `NEEDS INFO` (w2-render-13).
- Severidades del informe consideradas proporcionadas; sin propuesta de cambio,
  salvo C1 (nuevo posible hallazgo).

## Concerns de Mesa A

1. **Oráculo roto en el repro de w2-render-06 + posible hallazgo nuevo**: el
   `exp06` del auditor mide inclusión vía el texto del elemento `<title>`, pero
   `document.title = x` no lo actualiza (`TITLE-EL` queda `init` aunque el mensaje
   llegó y `document.title` lo refleja). Sus series 10/10–8/8–0/6 no son
   evidencia de entrega WS. La desincronización `document.title`↔elemento
   `<title>` es en sí un bug de contrato observable (ámbito js/host, fuera de
   esta mesa) y merece hallazgo propio en la mesa correspondiente.
2. **Cuantificación de w2-render-06 no reproducida**: overhead de socket abierto
   ~0.33–0.43 s aquí frente a 2.56 s del informe; el acoplamiento al wall-clock
   (dreno con `sleep`) es real y verificado en código, pero la cifra de 2 s
   parece ambiental (máquina del auditor con tmpfs lleno). El corte de inclusión
   existe (mensaje a 2000 ms perdido) aunque su posición es ambiental.
3. **w2-render-11 verificado fuera del commit base**: en `3c06db8` no hay
   `contentDocument` en absoluto; la asimetría se confirmó con binario del árbol
   principal (HEAD `d82118c`). El hallazgo es válido para HEAD, no para el
   commit de verificación.
4. **w2-render-01 (P1) se sostiene con escalado a menor escala**: ×12 con W=2,
   ×42 con W=3 frente a plano; la proyección a W=16 (~70k renders) es
   aritmética directa del código (cap por nivel, sin contador global).
