# Verificación Mesa B — scope F2-js (crates/js/src)

Fiscal de dedup y severidad sobre `docs/audit/findings/wave2/f2-js.md` (9 hallazgos `w2-js-01..09`).
No se edita código ni ledger. Fecha: 2026-11-18. Informe restaurado desde contexto tras
borrado accidental (ver nota en el propio `f2-js.md`); los veredictos se emiten sobre el
contenido restaurado, idéntico al auditado.

## Fuentes de dedup consultadas

- `docs/audit/FINDINGS.md` (254 líneas). Búsquedas por `1465`, `worker_terminate`, `canvas`/`fillRect`, `CustomEvent`, `writeln`, `setTimeout`, `XMLHttpRequest`, `dynamic import`, `rewrite_dynamic`, `createElement.*cap`, `textContent.*cap`, `max_dom_nodes`, `657`: hits relevantes: fila #1138 (slots de workers, fixed — citada como verificada, no como hallazgo), fila #1145 (reescritura textual de imports en strings/comments, "parcialmente abordado" — solapa con w2-js-002). Sin filas sobre canvas-DoS, CustomEvent cancelable, writeln, Event() sin args, setTimeout string, XHR duplicado ni handle de canvas.
- `docs/GAPS.md`: brechas de producto (canvas ausente #1533/#1545, XHR ausente, etc. — todas pre-implementación y hoy stale donde el informe lo señala). Ninguna fila cubre los 9 defectos.
- `docs/audit/findings/f2/00-index.md`: cita #1465 (worker_terminate, índice forjado) y #644 (handles escribibles) como antecedentes conocidos; el informe los declara verificados-corregidos, no hallazgos — correcto.
- `gh issue list --state all` (búsquedas `1465`, `canvas fillRect DoS`, `writeln`, `CustomEvent cancelable`, `setTimeout string delay`, `CustomEvent`, `writeln/newline`, `fillRect clamp/canvas DoS/canvas hang`): #1465 CLOSED (fix verificado con repro R7), #1145 CLOSED-parcial (residuo vivo verificado con R1/R2), #225 (dispatch global slot, CLOSED, otro defecto), #108/#1545 (canvas ausente/feature, cerrados). Sin issues abiertos que solapen con los 7 nuevos.

## Tabla de veredictos

| id | veredicto | sev. propuesta | título concreto | accionable | spec citada | nota fiscal |
|----|-----------|----------------|-----------------|------------|-------------|-------------|
| w2-js-001 | SOLO NUEVO | P1 (mantener) | sí | sí | sí (canvas 2D: pintado fuera de superficie es no-op; presupuesto anti-DoS del crate) | DoS real por lectura + 2 ejecuciones con escalado cuadrático (R3 5000², R3b 1000²/2000²/4000² ≈×4). El bucle Rust nativo elude `Budget::loop_iterations`. Sin issue previo en js. Fix concreto (clamp a superficie). P1 correcta: cuelgue de hilo por script, no solo lentitud. |
| w2-js-002 | YA REPORTADO (#1145) | P2 (mantener) | sí | sí | sí (subset documentado del preprocesador) | Residuo vivo de #1145: `mask_js_literals` solo se usa en TLA, no en `rewrite_dynamic_imports`. Evidencia nueva y reproducida en árbol limpio (R1 SyntaxError por string, R2 DynamicImport por `myimport`). No es duplicado exacto sino reapertura parcial: filear como comentario/reapertura de #1145, no como issue nuevo. |
| w2-js-003 | SOLO NUEVO | P2 (mantener; P3 admisible) | sí | sí | sí (UI Events § CustomEvent: `cancelable` default `false`) | `custom_event_constructor` fija `cancelable=true` ignorando la init-dict (línea 501). Cambia `dispatchEvent`/`defaultPrevented` observables. Sin issue previo. P2 defendible por semántica de eventos rota; P3 aceptable si la política reserva P2 a crash/DoS. |
| w2-js-004 | SOLO NUEVO | P3 (ok) | sí | sí | sí (HTML `document.writeln`: anexar U+000A) | `writeln` alias de `document_write` sin `\n` (R4 `"ab"` vs `"a\nb"`). Desviación menor de layout/texto. Fix one-liner. P3 correcta. |
| w2-js-005 | SOLO NUEVO | P3 (ok) | sí | sí | sí (DOM `Event(type)` requiere arg; WebIDL TypeError) | `arg_string` convierte `undefined` en `"undefined"` (R6). Rama de feature-detection equivocada. Fix = exigir `args.first()` (precedente xhr.rs:429). P3 correcta. |
| w2-js-006 | SOLO NUEVO | P3 (ok) | sí | sí | sí (HTML timers: retardo WebIDL `long`) | `as_number()` sin ToNumber: string `"100"` → delay 0 (R9). Patrón raro pero legal. Fix = `to_number(context)` + clamp (precedente `history_go`). P3 correcta. |
| w2-js-007 | YA REPORTADO (#644, parcial) | P3 (ok) | sí | sí | n/a (invariante interna #644) | Los wrappers quedaron protegidos (verificado), pero `__megi_canvas_node` sigue `Attribute::all()` y el objeto ctx persiste → cross-canvas write intra-realm (R8b). Es el mismo defecto #644 en objetos secundarios: filear como seguimiento de #644, no issue nuevo. `style`/`classList` no explotables (objeto fresco por acceso, R8). P3 correcta (mismo realm, sin frontera de seguridad). |
| w2-js-008 | SOLO NUEVO | P3 (ok) | sí | sí | n/a (docs) | Doc stale verificada por lectura + R11 + suites existentes (`xhr_tests`, `fetch_tests`, `fetch_abort_tests`). Fix = editar lib.rs:177-192. P3 correcta. |
| w2-js-009 | SOLO NUEVO | P3 (ok) | sí | sí | n/a (perf/mantenimiento) | Segundo XHR muerto probado por orden de instalación + R11 (gana xhr.rs). Coste por realm + trampa de edición. Fix = eliminar bloque manteniendo `FormData`. P3 correcta. |

## Concerns

1. **Restauración desde contexto**: el informe y los repros se perdieron por la limpieza de `/tmp` de otro agente y se re-escribieron desde la sesión. El contenido es el auditado (9 hallazgos, mismas evidencias R1–R11 confirmadas en árbol limpio), pero el Chair debería cotejar el diff de una línea si existe copia externa.
2. **w2-js-003, severidad**: único P2 no-DoS del scope. Mantengo P2 (semántica de eventos rota observable), pero P3 es admisible bajo política estricta "P2 = crash/DoS/pérdida de datos".
3. **C2/C3 del informe** (sort O(L) en `deliver()`, `put_image_data` pre-validación): correctamente no publicados (sin repro). No requieren acción de la Mesa.
4. **Dedup residual**: las búsquedas `gh` son por título/archivo; al crear issues (especialmente la reapertura de #1145 y el seguimiento de #644) re-chequear que ningún otro auditor wave2 reportó el mismo defecto con otro título.
