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
| w2-js-001 | VERIFIED | `cargo run --release --bin repro-f2-js` (R3) + `--bin r3b` | `R3 fillRect(5000x5000) on 300x150 canvas: 25.49ms` · `fillRect(1000x1000): 1.84ms` / `(2000x2000): 4.78ms` / `(4000x4000): 16.10ms` | Coste ∝ área del rect, no de la superficie (25M iteraciones sobre 45k píxeles); escalado ≈×4 al duplicar lado (~cuadrático; 1.84→4.78→16.10). Código: `canvas.rs:197-201,209-212,224-233` sin clamp antes de `for py in y0..y1 / for px in x0..x1`. Extrapolación a lados ≥1e9 = cuelgue efectivo; el bucle nativo no pasa por `Budget`. Tiempos absolutos menores que en el informe (76.7ms) por máquina, pero el mecanismo y el escalado son idénticos. P1 proporcionado (bypass del presupuesto = DoS por script). |
| w2-js-002 | VERIFIED | `cargo run --release --bin repro-f2-js` (R1, R2) | `R1 string-import rewrite: Err(Syntax("SyntaxError: expected token ';', got 'now' in expression statement at line 4, col 41 (native)"))` · `R2 myimport call: Err(DynamicImport("only string-literal import() specifiers are supported"))` | Código: `runtime.rs:5452-5504` busca `rest.find("import")` sobre el cuerpo sin enmascarar y sin frontera de palabra; `mask_js_literals` (definida en 5511, documentada para #1145) no se usa aquí. Ambas roturas de módulos válidos reproducen literalmente. P2 proporcionado. |
| w2-js-003 | VERIFIED | `cargo run --release --bin repro-f2-js` (R5) | `R5 CustomEvent cancelable default: true` | Código: `events.rs:501` fija `cancelable=true` literal (`build_event(..., bubbles, true, detail)`), mientras `event_constructor` (465-472) sí lee la init-dict. Spec: por defecto `false`. P2 proporcionado (contrato observable). |
| w2-js-004 | VERIFIED | `cargo run --release --bin repro-f2-js` (R4) | `R4 writeln body text: "ab"` (esperado `"a\nb"`) | Código: `runtime.rs:1349` registra `("writeln", 1, document_write)` — misma función que `write`, sin anexar `\n`. P3 proporcionado. |
| w2-js-005 | VERIFIED | `cargo run --release --bin repro-f2-js` (R6) | `R6 Event() no-arg: Ok("no-throw:undefined")` | Código: `events.rs:455` vía `arg_string` (102) convierte `undefined` en `"undefined"` en vez de `TypeError`. P3 proporcionado. |
| w2-js-006 | VERIFIED | `cargo run --release --bin repro-f2-js` (R9) | `R9 string-delay fired at t=50 (spec: false): true` | Código: `events.rs:1603-1606` usa `JsValue::as_number` (sin ToNumber) + `unwrap_or(0.0)`; el string `"100"` cae a retardo 0. P3 proporcionado. |
| w2-js-007 | VERIFIED | `cargo run --release --bin r3b` (R8b) + `repro-f2-js` (R8) | `R8b canvas handle write: Ok("c1=#000000\|c2=#ff0000")` · `R8 style handle write: Ok("a=color: red;\|b=null")` | Código: `runtime.rs:6205-6209` expone `__megi_canvas_node` con `Attribute::all()` (escribible) sobre un objeto de contexto persistente; reasignarlo redirige el pintado a otro canvas (R8b). El contrapunto del informe se confirma: `style`/`classList` también usan `Attribute::all()` pero se reconstruyen por acceso y R8 muestra que ahí no persiste. Impacto intra-documento mismo realm → P3 proporcionado. |
| w2-js-008 | VERIFIED | `cargo run --release --bin r4` (R11) + lectura `lib.rs:188-189` | `R11 XHR impl: Ok("upload=yes\|hasGetAll=function\|asyncReject=threw")` | La doc (`lib.rs:188-189`) lista `fetch`/XHR como gaps ausentes, pero `install_xhr` (xhr.rs) + prelude fetch (runtime.rs) existen y R11 prueba que la implementación real tiene `upload`, `getAllResponseHeaders` y rechazo de sync. Doc stale. P3 docs proporcionado. |
| w2-js-009 | VERIFIED | `cargo run --release --bin r4` (R11) + lectura `runtime.rs:718-719,7545-7657` | `R11 XHR impl: Ok("upload=yes\|hasGetAll=function\|asyncReject=threw")` | `install_globals` evalúa primero `install_gap_apis` (718) —cuyo prelude define su propio `XMLHttpRequest` sin `upload` (7548…)— y después `install_xhr` (719) lo sobrescribe; R11 demuestra que gana xhr.rs. Doble implementación / coste por realm confirmado en código + comportamiento. P3 perf/mantenimiento proporcionado. |

## Controles adicionales (secciones del informe fuera de hallazgos)

- R7 (`#1465` verificado-corregido): `R7 forged worker terminate: Ok("no-panic")` — idéntico en ambos árboles; la asignación forjada es no-op. Confirmado.
- R10 (cap de listeners): `R10 listener cap probe: Ok("2")` — el cap lanza como documenta el informe.
- WIP concurrente (Concern C1 del informe): en el HEAD actual `e87f2068` el árbol principal compila y todas las salidas son idénticas a las del árbol limpio `3c06db8`; ningún hallazgo depende de hunks WIP.

## Resumen

- 9/9 `VERIFIED`. 0 `PARTIAL`, 0 `NOT VERIFIED`, 0 `NEEDS INFO`.
- Severidades del informe (P1×1, P2×2, P3×6) consideradas proporcionadas; sin propuesta de cambio.
- Concerns propios: ninguno nuevo; C1–C3 del informe siguen vigentes tal cual (C1 mitigado en HEAD actual: salidas idénticas).
