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
| `crates/js/src/events.rs` | 1815 | leído — sin hallazgo |
| `crates/js/src/runtime.rs` | 9733 | leído — 7 hallazgos (w2-js-002…w2-js-007, w2-js-009) |

## Cobertura (`leído — sin hallazgo` por archivo)

- `error.rs` (1–69): enum de errores tipados; `Display` coherente. Sin hallazgo.
- `websocket.rs` (1–238): prelude + `ws_native` + `poll`. `send:/close:<handle>` con
  `parse::<u32>().unwrap_or(0)` cae a no-op si el handle no existe; `close` elimina del
  mapa; `__megi_ws_instances[handle]` se borra en `close`. Sin hallazgo
  (el `handle < 0` de JS nunca ocurre: el nativo devuelve `-1.0` y el prelude lo trata).
- `import_map.rs` (1–415): exact+longest-prefix, `null`→bloqueo, tope 1024 entradas,
  `MAX_JSON_DEPTH=128`, `\u` con sustitutos bien tratados, `literal()` con
  `self.bytes[self.position..]` — `position <= len` siempre (solo avanza) → sin pánico
  (`starts_with` con slice vacío devuelve false). Sin hallazgo.
- `host.rs` (1–1033): tabla lateral `nodes`+`node_index`, `handle_for` con
  `max_handles=1_000_000`, `node_for_handle` revalida con `try_node`, consola con cap
  4096 O(1), `record_mutation` con bitset acotado por `MAX_OBSERVERS` y terminación en
  cadena de padres cíclica (test `record_mutation_terminates_on_cyclic_parent_chain`).
  `resolve_url` sin validación de esquema pero es resolución local, no fetch. Sin hallazgo.
- `events.rs` (1–1815): `DispatchStateGuard` restaura estado apilado (#648/#1472),
  `once`/`passive`/`capture` implementados, `removeEventListener` con identidad +
  flag capture, excepciones de listeners aisladas (budget sigue propagando),
  `MAX_EVENT_LISTENERS=16384`, `MAX_PENDING_TIMERS=4096`, timers `(due,id)`.
  `deliver()` ordena AT_TARGET con scan O(L) por comparación (ver Concern C2).
  Sin hallazgo publicable.
- `xhr.rs` (1–683): XHR completo sobre `__megi_fetch_native`, validación RFC 9110,
  forbidden headers, sync-reject, timeout medido, abort determinista. Sin hallazgo
  (la duplicidad con el prelude va en w2-js-009).
- `lib.rs`: ver w2-js-008. `canvas.rs`: ver w2-js-001. `runtime.rs`: ver w2-js-002…007, 009.

## Hallazgos

### w2-js-001 — `canvas.fillRect/clearRect/strokeRect` iteran el rectángulo completo sin recortar: hang por script (DoS)
- `archivo:línea`: `crates/js/src/canvas.rs:196-235` (`fill_rect`, `clear_rect`, `stroke_rect`)
- `lente`: límites y DoS. `severidad`: P1. `categoría`: security (DoS local por script).
  `dedup`: new (issues de canvas: #108 ausente→cerrado, #1545 feature; sin issue de DoS en js).
- `title`: los métodos de relleno iteran `(x1-x0)×(y1-y0)` píxeles en Rust sin recortar a
  la superficie y sin ningún presupuesto; el bucle nativo no está sujeto a
  `Budget::loop_iterations` de Boa, así que `ctx.fillRect(0,0,1e12,1e12)` cuelga el hilo.
- `evidence`:
  - `R3 fillRect(5000x5000) on 300x150 canvas: 37.9ms` (árbol limpio; 76.7ms en la
    primera ejecución en árbol principal) para 25M iteraciones sobre una superficie de
    45k píxeles (el coste no depende de la superficie).
  - Escalado cuadrático (2.ª ejecución, árbol limpio): `1000²→5.0ms, 2000²→9.9ms,
    4000²→53.6ms` (≈×4 al duplicar el lado). Extrapolación: lados ≥1e9 son cuelgues efectivos.
  - Código: `let (x1, y1) = ((x + w).round() as i64, ...)` sin clamp; `blend()` filtra
    por píxel pero el `for py in y0..y1 / for px in x0..x1` recorre todo el rect.
    `clear_rect` igual (chequeo por píxel dentro del mismo doble bucle).
- `impact`: cualquier página con `<canvas>` + script puede colgar el render
  indefinidamente (negación de servicio; en `render` el pump es cooperativo).
- `expected`: recortar `(x0,y0,x1,y1)` a `[0,width]×[0,height]` antes de iterar
  (canvas 2D: los rectángulos fuera de la superficie no pintan nada) y/o cargar los
  píxeles al presupuesto por `eval` (p. ej. rechazar área > k×superficie).
- `fix`: `let (x0,_,x1,_) = clamped vs 0..width` en los tres métodos; test con rect
  gigante que termine en ms.
- `repro`: `docs/audit/findings/wave2/repro-f2-js/r_main.rs` (R3),
  `r3b.rs` (serie de escalado + R8b). Comando:
  `cd /tmp/opencode/repro-f2-js-verify && CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target cargo run --release --bin r3b`.

### w2-js-002 — `rewrite_dynamic_imports` reescribe `import('…')` dentro de strings/comentarios y `myimport('…')`
- `archivo:línea`: `crates/js/src/runtime.rs:5454-5506`
- `lente`: contrato vs especificación. `severidad`: P2. `categoría`: bug.
  `dedup`: #1145 (residuo vivo: #1145 se cerró como "parcialmente abordado" con
  `mask_js_literals`, pero solo se usa en `module_has_top_level_await`, no aquí).
- `title`: el rewriter busca la subcadena `import` sobre el cuerpo sin enmascarar:
  rompe módulos válidos cuyo código contiene `import('x')` en un literal/comentario o
  llamadas a funciones como `myimport('x')`.
- `evidence`:
  - `R1 string-import rewrite: Err(Syntax("SyntaxError: expected token ';', got 'now' …"))`
    para el módulo `var s = "call import('ghost') now"; export const v = s;`
    (el literal queda sustituido por `__MEGI_DYNAMIC_0__`). Confirmado en árbol limpio.
  - `R2 myimport call: Err(DynamicImport("only string-literal import() specifiers are supported"))`
    para `function myimport(x){return x;} export const v = myimport('lit');`
    (nunca debió tratarse como import dinámico). Confirmado en árbol limpio.
- `impact`: módulos válidos fallan con `SyntaxError`/`DynamicImport` espurios; peor:
  un string inocente (`"ghost"`) se convierte en dependencia y puede producir
  `ModuleNotFound` o cargar un módulo no deseado vía el loader del host.
- `expected`: el preprocesador documenta "imports must start their line" para estáticos;
  el rewriter de dinámicos debe operar sobre el cuerpo enmascarado (`mask_js_literals`)
  y exigir frontera de palabra antes de `import`.
- `fix`: aplicar `mask_js_literals` (o escaneo con estado string/comentario) en
  `rewrite_dynamic_imports` + exigir que el carácter previo no sea identificador.
- `repro`: `r_main.rs` (R1, R2).

### w2-js-003 — `CustomEvent` nace siempre con `cancelable: true` (el spec dice `false`)
- `archivo:línea`: `crates/js/src/events.rs:482-502` (`custom_event_constructor`, línea 501)
- `lente`: contrato vs especificación. `severidad`: P2. `categoría`: contract.
  `dedup`: new (sin issue: `CustomEvent` no aparece en FINDINGS/GAPS/f2-index).
- `title`: el constructor ignora la init-dict y fija `cancelable=true`, al contrario que
  DOM UI Events (`new CustomEvent(type)` / `new CustomEvent(type, {})` debe ser no cancelable).
- `evidence`: `R5 CustomEvent cancelable default: true` (árbol limpio también).
- `impact`: `preventDefault()` surte efecto donde el spec lo prohíbe;
  `dispatchEvent` devuelve `false` y `defaultPrevented` se fija en eventos que ninguna
  página podría cancelar → comportamiento observable distinto al de navegadores.
- `expected`: UI Events § CustomEvent: `cancelable` por defecto `false`; leer
  `init.cancelable` como hace `event_constructor` con `bubbles`.
- `fix`: `let cancelable = …unwrap_or(false)` simétrico a `bubbles`.
- `repro`: `r_main.rs` (R5).

### w2-js-004 — `document.writeln` no añade el salto de línea
- `archivo:línea`: `crates/js/src/runtime.rs:1345-1349` (`("writeln", 1, document_write)`)
- `lente`: contrato vs especificación. `severidad`: P3. `categoría`: contract. `dedup`: new.
- `title`: `writeln` comparte la función de `write` sin anexar `\n` (HTML § `writeln`
  debe anexar U+000A tras cada argumento).
- `evidence`: `R4 writeln body text: "ab"` para `document.writeln('a'); document.write('b');`
  (esperado `"a\nb"`). Confirmado en árbol limpio.
- `impact`: diferencias de layout/texto frente a navegadores en páginas que usan `writeln`.
- `expected`: HTML Standard, `document.writeln(...)`: "append U+000A to the string".
- `fix`: envoltorio que concatene `"\n"` o pase flag a `document_write`.
- `repro`: `r_main.rs` (R4).

### w2-js-005 — `new Event()` no lanza `TypeError` y deja `type === "undefined"`
- `archivo:línea`: `crates/js/src/events.rs:450-480` (`event_constructor` vía `arg_string`)
- `lente`: contrato vs especificación. `severidad`: P3. `categoría`: contract. `dedup`: new.
- `title`: sin argumentos, `arg_string` convierte `undefined` al string `"undefined"` en
  vez del `TypeError` que exige WebIDL/DOM ("2 arguments required" / conversión fallida).
- `evidence`: `R6 Event() no-arg: Ok("no-throw:undefined")`. Confirmado en árbol limpio.
- `impact`: páginas que detectan constructores rotos (`try{new Event()}catch`) toman la
  rama equivocada; eventos con tipo `"undefined"` circulan por el dispatch.
- `expected`: DOM `Event(type, dict)`: `type` es requerido; lanzar `TypeError`.
- `fix`: exigir `args.first()` presente (como ya hace `open()` en xhr.rs:429-431).
- `repro`: `r_main.rs` (R6).

### w2-js-006 — `setTimeout(fn, "100")` no coerciona el retardo (dispara de inmediato)
- `archivo:línea`: `crates/js/src/events.rs:1587-1607` (`schedule_timer`)
- `lente`: contrato vs especificación. `severidad`: P3. `categoría`: contract. `dedup`: new.
- `title`: el retardo usa `as_number()` (solo `Number`, sin ToNumber), así que un string
  numérico cae a `unwrap_or(0.0)` y el timer vence de inmediato.
- `evidence`: `R9 string-delay fired at t=50 (spec: false): true` tras
  `setTimeout(fn,'100')` + `run_until(50)`. Confirmado en árbol limpio.
- `impact`: temporización distinta a navegadores para el patrón (raro pero legal)
  `setTimeout(fn, "100")`.
- `expected`: HTML § timers: retardo vía WebIDL `long` (ToNumber + clamp ≥ 0).
- `fix`: `to_number(context)` como en `clear_timer`/`history_go`, con clamp a ≥0.
- `repro`: `r_main.rs` (R9).

### w2-js-007 — `__megi_canvas_node` escribible desde script permite pintar en otro canvas
- `archivo:línea`: `crates/js/src/runtime.rs:6206-6211` (`Attribute::all()`),
  lectura en `canvas_context_node` (6155–6170)
- `lente`: estado global/contrato. `severidad`: P3. `categoría`: bug.
  `dedup`: relacionado con #644 (wrappers `__megi_node` protegidos con
  `Attribute::default()` = READONLY/NON_ENUMERABLE/PERMANENT, verificado en fuentes
  de Boa 0.21; aquí son los objetos secundarios los que quedaron escribibles).
- `title`: el objeto de contexto 2D expone `__megi_canvas_node` escribible y el objeto
  persiste entre llamadas, así que reasignarlo redirige `fillStyle`/`fillRect`/… a la
  superficie de otro canvas del mismo documento.
- `evidence`: `R8b canvas handle write: Ok("c1=#000000|c2=#ff0000")` — tras
  `ctx.__megi_canvas_node = ctx2.__megi_canvas_node`, pintar vía `ctx` modificó `c2`.
  Confirmado en árbol limpio.
  (Contrapunto: `el.style.__megi_node` es igualmente `Attribute::all()`
  —`style_object` 4154–4158, `class_list_object` 4371–4375— pero `style`/`classList`
  se reconstruyen en cada acceso, R8: `a=color: red;|b=null`, así que ahí no persiste.)
- `impact`: confusión intra-documento (mismo realm, sin frontera de seguridad):
  un script puede pintar donde no debe; rompe la invariante "los handles no se
  falsifican" (#644) en objetos secundarios.
- `expected`: `Attribute::default()` como en `node_wrapper`/`worker_constructor`.
- `fix`: `Attribute::default()` en `canvas_get_context` (+ `style_object`,
  `class_list_object`, wrappers de frame por defensa en profundidad).
- `repro`: `r3b.rs` (R8b), `r_main.rs` (R8).

### w2-js-008 — `lib.rs` documenta `fetch`/XHR (y `getBoundingClientRect`) como gaps ausentes, pero están implementados
- `archivo:línea`: `crates/js/src/lib.rs:177-192` ("Remaining gaps (documented): …
  `getBoundingClientRect`/`offsetWidth`… (not implemented to fake values), `fetch`/XHR …")
- `lente`: contrato vs documentación. `severidad`: P3. `categoría`: docs. `dedup`: new.
- `title`: la doc de `fetch`/XHR como gap es stale: `install_xhr` (xhr.rs, #1532),
  el prelude `fetch`/`Request`/`Response`/`Headers`/`AbortController`/`ReadableStream`
  (runtime.rs:7041-7436, `__megi_fetch_native` 8462+) y `canvas`/layout-provider existen;
  hay suites `xhr_tests.rs`, `fetch_tests.rs`, `fetch_abort_tests.rs`.
- `evidence`: `R11 XHR impl: Ok("upload=yes|hasGetAll=function|asyncReject=threw")`
  (gana la implementación xhr.rs: `upload`, `getAllResponseHeaders`, rechazo de sync).
  Confirmado en árbol limpio. Código: `install_globals` llama `install_gap_apis` +
  `crate::xhr::install_xhr` (runtime.rs:717-720).
- `impact`: lectores del contrato (y otros auditores) asumen APIs ausentes; riesgo de
  doble trabajo o de apoyarse en gaps que ya no existen.
- `expected`: actualizar la lista de gaps a lo realmente ausente (`dataset` writes,
  `innerText` layout, `Range`, live `NodeList`).
- `fix`: editar el bloque de doc de lib.rs:177-192.
- `repro`: `r4.rs` (R11).

### w2-js-009 — XHR implementado dos veces: el prelude de `install_gap_apis` es código muerto que se evalúa en cada realm
- `archivo:línea`: `crates/js/src/runtime.rs:7438-7550` (segundo `XMLHttpRequest`)
  vs `crates/js/src/xhr.rs:21-674`
- `lente`: errores tragados/mantenimiento. `severidad`: P3. `categoría`: perf.
  `dedup`: new.
- `title`: `install_globals` evalúa el prelude gigante de gap-APIs (que define su propio
  `XMLHttpRequest` sin `upload`/`getAllResponseHeaders`/validación) y después
  `install_xhr` lo sobrescribe; el primero solo cuesta parse+eval por cada `JsRuntime`
  (incluidos workers y `eval_on_document`) y es trampa para futuros cambios
  (editar el XHR equivocado).
- `evidence`: R11 (gana xhr.rs) + orden de instalación runtime.rs:717-720.
  Confirmado en árbol limpio.
- `impact`: coste de arranque por realm; riesgo de divergencia silenciosa.
- `expected`: una sola implementación (mover `FormData` —que xhr.rs sí usa— a su
  propio prelude o documentar la dependencia).
- `fix`: eliminar el bloque XHR del prelude de gap-APIs (mantener `FormData`).
- `repro`: `r4.rs` (R11).

## Verificado-corregido (dedup, sin hallazgo nuevo)

- **#1465 `worker_terminate` con índice manipulable — CORREGIDO en el árbol auditado**
  (también en HEAD limpio 3c06db8 de `/tmp/opencode/megi-verify`).
  `__megi_worker` es `Attribute::default()` (no escribible, runtime.rs:8173-8177),
  `worker_handle` sigue sin cota superior pero **todos** los consumidores usan
  `host.workers.get_mut(id as usize)` / `.get(...)` (`postMessage` 8213,
  `terminate` 8235, `addEventListener` 8255, `onmessage` 8268/8290, `onerror` 8315,
  `deliver_*` 7983/8021). Evidencia nueva: `R7 forged worker terminate: Ok("no-panic")`
  (`w.__megi_worker = 1000; w.terminate()` no patea; la asignación es no-op).
  Confirmado en árbol limpio.
- **#1138 slots de workers** — `terminate` libera el slot (`*slot = None`, 8235-8237).
- **#644 `__megi_node` falsificable** — wrappers con `Attribute::default()` (1524-1528);
  `target_key`/`argument_node`/`this_handle` validan (377, 1702-1722, 3275-3291).

## Verificación contra árbol limpio (`/tmp/opencode/megi-verify`, HEAD 3c06db8)

Diff `megi-verify` ↔ árbol principal para `crates/js/src/`: los únicos ficheros distintos
son `host.rs` (campos `frame_*`) y `runtime.rs` (añadidos `contentDocument`,
`frame_handle`/`frame_node`/`frame_element_wrapper`/`frame_get_attribute`/…,
`iframe_content_document`; contador de accessors 42→43). `canvas.rs`, `error.rs`,
`events.rs`, `lib.rs`, `import_map.rs`, `websocket.rs`, `xhr.rs` son **idénticos**.
Ninguna región citada por los hallazgos w2-js-001…009 cae dentro de un hunk WIP;
**ningún hallazgo depende solo de cambios sin commitear** (no hay marcas
`excluir: WIP del usuario`). La rotura de compilación E0308 observada a mitad de
auditoría fue un estado intermedio de edición concurrente, no defecto de HEAD
(ver Concern C1). Repros re-ejecutados contra el árbol limpio con harness en
`/tmp/opencode/repro-f2-js-verify` (path-deps a `megi-verify`): salidas idénticas,
ver `## Comandos ejecutados (árbol limpio)`.

## Concern

- **C1 — El árbol principal mutó bajo la auditoría.** A mitad de lectura aparecieron
  ediciones sin commitear (`runtime.rs` pasó por un estado `44 vs 43` que rompía
  `cargo test` con E0308; al cerrar declara 43/43 y compila). Ningún hallazgo depende
  de esos cambios (diff verificado hunk a hunk), pero las citas `archivo:línea` de
  `runtime.rs`/`host.rs` pueden haberse desplazado ±líneas respecto al informe.
- **C2 — `deliver()` AT_TARGET ordena con scan O(L) por comparación** (events.rs:997-1005):
  con 16384 listeners en el mismo target+tipo, cada `dispatchEvent` cuesta
  ~m·log·m barridos del host. Sin repro de tiempo → no se publica; candidato a
  micro-benchmark futuro.
- **C3 — `put_image_data` materializa `bytes` con gets JS por elemento antes de validar**
  (runtime.rs:6407-6425): un `data` con `length` esparso gigante itera miles de
  millones de veces. Sin repro (peligroso/lento) → no publicado, solo concern.

## Comandos ejecutados (árbol limpio `/tmp/opencode/megi-verify`, HEAD 3c06db8)

- `cargo test -p js`: 32 suites, todas `ok`, 0 fallos (totales por suite: 27, 13, 19,
  3, 2, 3, 3, 12, 1, 14, 1, 20, 11, 16, 5, 20, 9, 7, 12, 10, 3, 3, 10, 2, 12, 18,
  10, 23, 19, 15, 6, 0 doc-tests). `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target`
  (el `/tmp` tmpfs de 7.7G se llenó con los target/ de los harness; se borraron y se
  redirigió el build a `/home`).
- `cargo check -p js --no-default-features`: `Finished`, sin errores.
- Repros contra el árbol limpio (`/tmp/opencode/repro-f2-js-verify`, path-deps a
  `megi-verify`; salidas idénticas a las del árbol principal):
  `R1 Err(Syntax(...got 'now'...))`, `R2 Err(DynamicImport(...))`,
  `R3 fillRect(5000x5000): 37.9ms`, `R3b 1000²→5.0ms / 2000²→9.9ms / 4000²→53.6ms`
  (escalado cuadrático), `R8b c1=#000000|c2=#ff0000`, `R11 upload=yes|hasGetAll=function|asyncReject=threw`,
  `R4 "ab"`, `R5 true`, `R6 no-throw:undefined`, `R7 no-panic`,
  `R8 a=color:red|b=null`, `R9 true`, `R10 2`.
