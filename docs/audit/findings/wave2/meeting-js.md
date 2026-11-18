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

Resultado: 7 include (cuerpos listos en `docs/audit/findings/wave2/issues/w2-js-{001,003,004,005,006,008,009}.md` con primera línea `[js] ...`, evidencia literal, `Commit: 3c06db8`, workaround y `Dedup:`), 2 comment-existing, 0 descartes DUPLICADO, 0 security.

## comment-existing (pegar en issues existentes, no abrir nuevos)

### #1145 ← w2-js-002
```
Evidencia nueva wave2 (HEAD 3c06db8, repro-f2-js R1/R2):
R1 string-import rewrite: Err(Syntax("SyntaxError: expected token ';', got 'now' in expression statement at line 4, col 41 (native)"))
R2 myimport call: Err(DynamicImport("only string-literal import() specifiers are supported"))
Causa: runtime.rs:5452-5504 busca rest.find("import") sin enmascarar ni frontera de palabra; mask_js_literals (definida en 5511 para este issue) no se usa aquí. Ambas roturas de módulos válidos reproducen literalmente. Mesa A: VERIFIED.
```

### #644 ← w2-js-007
```
Evidencia nueva wave2 (HEAD 3c06db8, r3b R8b + repro R8):
R8b canvas handle write: Ok("c1=#000000|c2=#ff0000")
R8 style handle write: Ok("a=color: red;|b=null")
Causa: runtime.rs:6205-6209 expone __megi_canvas_node con Attribute::all() escribible sobre objeto persistente; reasignarlo redirige el pintado. style/classList usan Attribute::all() pero se reconstruyen por acceso (R8: no persiste). Impacto intra-documento mismo realm. Mesa A: VERIFIED, P3.
```

## Descartes
- Ningún DUPLICADO seco; los 2 YA REPORTADO van a comment-existing por recomendación de Mesa B.
- Controles R7 (#1465 no-panic), R10 (cap listeners=2), C1 (WIP: salidas idénticas en e87f2068) confirmados por Mesa A; sin acción.
