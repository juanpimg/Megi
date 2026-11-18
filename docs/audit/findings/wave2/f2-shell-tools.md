# Auditoría ola 2 — F2 shell-tools (prefijo `w2-st`)

- Scope: todos los `.rs` bajo `crates/shell/src/`, `crates/bench/src/`, `crates/test_harness/src/` (41 archivos, ~25k líneas), leídos línea por línea.
- HEAD auditado: `3c06db8`. Árbol principal y worktree coinciden en el scope (`cmp` limpio archivo por archivo).
- **Proveniencia de ejecución**: el árbol principal puede tener cambios WIP que afectan a la compilación; **todos** los comandos `cargo`/binario se ejecutaron desde el worktree limpio `/tmp/opencode/megi-verify` (HEAD `3c06db8`, `git status` limpio). Primeros sondeos en el árbol principal se descartaron por sospecha de binario stale (ver concerns). Ningún hallazgo depende de cambios sin commitear → **ninguno lleva `excluir: WIP del usuario`**.
- Issues de referencia verificados en `gh` (todos CLOSED) y en `docs/audit/FINDINGS.md` (marcados Deferred): #1163 #1165 #1166 #1167 #1168 #1169 #1171 #1172 #1173 #1174 #1175 #1176 #1197 #1198 #1199 #1201, más #1177 #1178 y #1468.

## Alcance

Leídos de arriba abajo (tests incluidos):

- `crates/shell/src/`: `a11y.rs` (527), `chrome.rs` (3186), `cli.rs` (771), `convert.rs` (92), `download.rs` (606), `error.rs` (42), `favicon.rs` (347), `find.rs` (692), `forms.rs` (1579), `headless.rs` (409), `history.rs` (391), `i18n.rs` (179), `images.rs` (119), `lib.rs` (274), `links.rs` (491), `main.rs` (203), `navigation.rs` (391), `pages.rs` (625), `present.rs` (255), `reader.rs` (233), `refresh.rs` (206), `scale.rs` (173), `scroll.rs` (249), `session.rs` (1445), `tabs.rs` (473), `title.rs` (370), `window.rs` (3200), `worker.rs` (1121), `zoom.rs` (268).
- `crates/bench/src/`: `benches.rs` (1720), `cli.rs` (183), `generators.rs` (584), `main.rs` (82), `report.rs` (243), `runner.rs` (337).
- `crates/test_harness/src/`: `css_harness.rs` (447), `js_executor.rs` (189), `lib.rs` (181), `main.rs` (513), `serializer_harness.rs` (856), `tokenizer_harness.rs` (469), `tree_harness.rs` (267).

## Cobertura

- `a11y.rs` — leído — sin hallazgo (roles/nombres/orden de foco sanos; `snapshot` O(n²) sobre nodos expuestos, irrelevante en la práctica).
- `chrome.rs` — leído — hallazgos F3 (click vs pintado), F10 (guard <100px), F14-vía-lib (docs).
- `cli.rs` — leído — sin hallazgo (límite 32 tabs coherente con `tabs::MAX_TABS`; `--refresh-ms` acotado; `normalize_input` escapa `#?%`).
- `convert.rs` — leído — sin hallazgo.
- `download.rs` — leído — hallazgos F9 (tmp predecible), F18 (overflow sin comprobar).
- `error.rs` — leído — sin hallazgo.
- `favicon.rs` — leído — hallazgo F2 (file: desde http).
- `find.rs` — leído — hallazgo F4 (mapeo + current). Búsqueda y `map_range` UTF-8 correctos (tests `İ`, emoji).
- `forms.rs` — leído — sin hallazgo nuevo (IDs estables, patch/multi-select sanos; `form_get_target`/`urlencode` correctos).
- `headless.rs` — leído — hallazgos F9 (tmp), F15 (hits descartados).
- `history.rs` — leído — sin hallazgo.
- `i18n.rs` — leído — sin hallazgo (tabla completa ES/EN; fallback ES documentado).
- `images.rs` — leído — sin hallazgo.
- `lib.rs` — leído — hallazgo F14 (docs contradicen implementación).
- `links.rs` — leído — sin hallazgo.
- `main.rs` — leído — hallazgos F7 (flags ignorados), F16 (tabs ignorados en headless).
- `navigation.rs` — leído — sin hallazgo (máquina de estados sana).
- `pages.rs` — leído — sin hallazgo (escapes, caps de listado).
- `present.rs` — leído — sin hallazgo (mapeo con offset chrome correcto aquí; el bug está en el llamante del overlay, F4).
- `reader.rs` — leído — hallazgo F17 (coste cuadrático).
- `refresh.rs` — leído — sin hallazgo.
- `scale.rs` — leído — sin hallazgo. Zoom — leído — sin hallazgo (`site_origin` preserva mayúsculas pero `normalize_site_zooms` minusculiza: posible miss tras restore; no verificable sin `net` en scope → concern, no hallazgo).
- `scroll.rs` — leído — sin hallazgo.
- `session.rs` — leído — hallazgos F8 (sin fsync), F9-parte (ya migrado a tmp único), F19 (doc `~/.megi` inalcanzable).
- `tabs.rs` — leído — sin hallazgo.
- `title.rs` — leído — sin hallazgo.
- `window.rs` — leído — hallazgos F1 (#1169), F3, F4, F5, F6.
- `worker.rs` — leído — #1468 verificado corregido (`catch_unwind` + test); #1168-scripts verificado (probe sin scripts).
- `bench/*` — leído — hallazgos F11, F12, F13; resto de regresiones #1200–#1210 verificadas corregidas.
- `test_harness/*` — leído — sin hallazgo (gates coherentes con constantes; harness no ejercita shell).

## Hallazgos

### F1 — w2-st-1169-session-tabs · `window.rs:371-402` · lente contrato · P2 · contract · `needs-repro` · dedup #1169

- title: `--tabs` se ignora cuando existe session file.
- evidence: `WindowApp::new`, rama `Some(session)` (líneas 371–394) restaura y fusiona solo `input`, nunca consulta `view.tabs`; la rama `None` sí usa `TabsState::from_urls(view.tabs)` (línea 396). Apoyo ejecución: en headless los `--tabs` también se ignoran (F16, md5 idénticos).
- impact: el usuario pierde pestañas pedidas sin aviso al reabrir con sesión.
- expected: fusionar `--tabs` (abrir las que falten / activar) o avisar.
- fix: tras `restore()`, para cada URL de `view.tabs` no presente hacer `open_tab`, o documentar prelación.
- repro (requiere display): `megi URL --session-file /tmp/s.json` (crea sesión) → `megi URL --session-file /tmp/s.json --tabs B` → B no se abre.
- dedup: #1169 (evidencia nueva: el merge solo contempla `input`, `view.tabs` queda muerto en esa rama).

### F2 — w2-st-1173-file-icon · `favicon.rs:50-76,82-112,116-129` · lente contrato · P2 · **security + advisory** · dedup #1173

- title: `favicon::fetch` lee `file:` indicado por páginas `http(s)` sin chequeo de esquema.
- evidence: `icon_url` acepta cualquier `href` que `page.join` resuelva (línea 70); `fetch` rama `File` lee disco sin comprobar la página (líneas 83–99); `for_page` no filtra (líneas 116–129). Ejecuciones (worktree limpio, build fresco, página servida por `http.server`, carga confirmada vía `--save-html` → contiene `hi-http`):
  - misma URL, marker presente vs ausente → md5 `f06e0a…` vs `699f90…` (difieren).
  - diff RGBA: 128 px en banda chrome (tab icon + address), bbox x 3..796 y 4..52.
  - control `file:` misma prueba → también difiere (`1fd055…` vs `c172f5…`).
- impact: página remota provoca lectura de ficheros locales (oráculo de existencia/validez-de-imagen + exfiltración de píxeles 16×16 de cualquier imagen local referenciada).
- expected: icono `file:` solo para páginas `file:`; `http(s)` solo `http(s)` (o allowlist explícita); resto → fallback.
- fix: en `for_page`/`icon_url`, exigir mismo esquema (o rechazar `file:` cuando `page` es `http(s)`/`data:`).
- repro: ver `## Comandos ejecutados` (E-icon).
- dedup: #1173 (evidencia nueva: 4 renders + diff de píxeles + prueba de carga).
- **advisory: no publicar como issue; reportar por canal privado según `SECURITY.md`** (ver concerns).

### F3 — w2-st-1166-optgroup-rows · `window.rs:945-963,2605-2623` vs `chrome.rs:1869-1945,687-727` · lente contrato · P2 · bug · `needs-repro` · dedup #1166

- title: click/hover del `<select>` usa modelo de filas sin headers; el pintado incluye headers de `<optgroup>`.
- evidence: `click_at_cursor` y `CursorMoved` construyen `chrome::dropdown_layout(&rect, dropdown.options.len(), …)` (items 1:1 con opciones, sin headers), mientras `paint_dropdown` usa `dropdown.items()` (con `DropdownItem{option_index:None}` por cada grupo). A partir del primer header, fila pintada N ≠ opción N.
- impact: con `<optgroup>`, click/Enter/hover elige o resalta la opción equivocada (desplazada por nº de headers previos).
- expected: usar `dropdown_layout_items` con `dropdown.items()` en los tres call sites (click, hover, rueda).
- fix: idem; test con select agrupado comparando `hit_option` pintado vs lógico.
- repro (requiere display): página con `<select><optgroup label=G><option>A…` → abrir dropdown → click en 2ª fila bajo el header.
- dedup: #1166 (evidencia nueva: divergencia exacta de los dos constructores + 3 call sites afectados).

### F4 — w2-st-1167-find-offset · `window.rs:2471-2484` + `find.rs:284-349` + `present.rs:119-138` · lente aritmética · P2 · bug · `needs-repro` · dedup #1167

- title: overlay de find mapea con altura CSS completa (ignora banda chrome) y el current mezcla índices DOM vs fragmento.
- evidence: `present()` pinta la página con `page_viewport_height` + offset `chrome_px` (`present_region_at`), pero `overlay_highlights_dest` recibe `css_width/css_height` completos y `dest_height` físico completo → `scale_y = dest_height/css_viewport_height` no descuenta chrome → todo el overlay queda desplazado hacia arriba una banda. Además `current` es índice de `FindMatch` (DOM) y se usa contra `rects` (fragmentos layout, 1 por fragmento aunque contenga N hits) → resaltado fuerte arbitrario.
- impact: highlights en posición errónea; el "current" no corresponde al hit.
- expected: replicar el mapeo de `present_region_at` (offset + alto de página) y resolver current por fragmento con el mismo módulo que `current_rect`.
- fix: idem.
- repro (requiere display): `--find` en página con scroll + chrome visible.
- dedup: #1167 (evidencia nueva: divergencia de mapeos + doble índice).

### F5 — w2-st-1165-reader-url · `window.rs:1282-1301,2142-2152` + `session.rs:83-132` · lente contrato · P2 · contract · `needs-repro` · dedup #1165

- title: `about:reader` persiste como URL del tab y restaura página vacía.
- evidence: `open_reader` fija `input="about:reader"` + `reader_html=Some`; `save_active_tab` persiste `url="about:reader"`; tras reinicio `reader_html=None` → `request_render` usa `reader_page("Reader", &[], …)` vacía.
- impact: tabs de lector se pierden al reiniciar; el historial global guarda `about:reader`.
- expected: persistir URL fuente (+flag reader) y re-extraer al restaurar, o no persistir el tab lector.
- fix: idem.
- repro (requiere display): abrir artículo → Ctrl+Shift+R → cerrar → reabrir con sesión.
- dedup: #1165 (evidencia nueva: cadena completa persistencia→restore).

### F6 — w2-st-1163-history-push · `window.rs:1337-1356,1304-1314` → `556-590` · lente contrato · P2 · contract · `needs-repro` · dedup #1163

- title: la navegación tipada (Enter en address bar, historial Ctrl+H) no entra en el historial de navegación.
- evidence: `handle_address_key(Enter)` y `navigate_selected_history` llaman a `start_navigation`, que nunca toca `self.nav` (no `navigate_to`/`push`); solo links (`navigate_to`, línea 593) hacen push.
- impact: Back no alcanza la página tipada; la URL tipada queda fuera de back/forward.
- expected: push salvo `SameUrl` (reutilizar `navigate_to`).
- fix: en `start_navigation` o sus llamantes tipados, hacer push del `Url` parseado.
- repro (requiere display): A → Ctrl+L → B → Back (no vuelve a B como entrada propia / B se pierde).
- dedup: #1163 (evidencia nueva: `start_navigation` sin push; variante del título original).

### F7 — w2-st-1175-headless-flags · `main.rs:60-73,175-182` · lente contrato · P3 · contract · dedup #1175

- title: modo window ignora silenciosamente `--save-png/--save-html/--find/--chrome/--reader`.
- evidence: `finish_args` los acepta y `main` solo los consume en la rama `Screenshot`; la rama `Window` no los referencia (compila sin warnings porque viajan en `args`/`view` parcial).
- impact: el usuario cree exportar/resaltar/leer en ventana; nada ocurre y nada avisa.
- expected: aviso `megi: --save-png solo aplica con --screenshot` (o `Usage`).
- fix: validar combinaciones tras `finish_args`.
- repro: estático (rama window exige display; el hecho "no se referencia" es verificable con `grep`).
- dedup: #1175 (evidencia nueva: enumeración exacta de flags muertos en rama Window).

### F8 — w2-st-1171-no-fsync · `session.rs:422-446` · lente contrato · P2 · contract · `needs-repro` (solo para el evento de pérdida) · dedup #1171

- title: `save()` renombra sin `fsync` del fichero ni del directorio.
- evidence: `write_temporary` + `fs::rename` sin `sync_all`; `grep -rn "sync_all|fsync|fdatasync" crates/shell/src/` → 0 resultados (comando ejecutado en worktree limpio).
- impact: caída de energía/crash tras rename → `session.json` truncado o vacío pese a tmp+rename.
- expected: `sync_all` del tmp + `sync_all` del dir antes de `rename` (best-effort, como hacen otros navegadores).
- fix: idem; test con `LD_PRELOAD`/crash-injection o inspección de trazas.
- repro: matar el proceso entre write y flush del FS (difícil determinista → `needs-repro` para el evento; el hecho estático está probado).
- dedup: #1171 (evidencia nueva: ausencia total de fsync en el crate).

### F9 — w2-st-1176-tmp-writers · `download.rs:138-161` + `headless.rs:318-348,388-409` · lente concurrencia/contrato · P2 · **security + advisory** · `needs-repro` · dedup #1176

- title: los writers secundarios usan `.<name>.tmp` predecible + `fs::write` (trunca y sigue symlinks); solo `session.rs` se migró a tmp único + `create_new`.
- evidence: `write_atomic` (`download.rs:147-155`) y `save_screenshot`/`write_bytes` (`headless.rs:323-335,389-401`) usan nombre fijo y `fs::write`; `session.rs:477-504` es el contraejemplo correcto.
- impact: atacante local planta symlink en el nombre predecible (`~/Downloads/.megi-<host>-1.png.tmp` o el dir destino) → el shell escribe bytes controlados fuera del directorio.
- expected: reutilizar el patrón `write_temporary` único + `create_new` en los tres.
- fix: extraer helper común o replicar guard+`create_new`.
- repro: plantar symlink + provocar descarga (requiere display salvo `--save-png`) → `needs-repro`.
- dedup: #1176 (evidencia nueva: la corrección solo cubrió `session.rs`; 3 call sites siguen vulnerables).
- **advisory: canal privado según `SECURITY.md`**.

### F10 — w2-st-1174-narrow · `chrome.rs:545-569` · lente límites · P3 · bug · `needs-repro` · dedup #1174

- title: sin overflow por debajo de ~100 px CSS (`available < TAB_MIN_WIDTH` desactiva el menú).
- evidence: `if per_tab * count > available && available >= TAB_MIN_WIDTH` — con `width<100` (`available<72`) nunca hay marker `+N`; `visible=count`, tabs de 72px pintados fuera del strip.
- impact: tabs inaccesibles/inclickables en ventanas estrechas.
- expected: calcular overflow siempre que `hidden>0` (eliminar el guard o bajarlo).
- fix: idem.
- repro (requiere display): estrechar ventana <100px con ≥2 tabs.
- dedup: #1174 (evidencia nueva: el menú existe pero el guard lo desactiva justo en el caso del título).

### F11 — w2-st-1197-strip-noop · `benches.rs:880-903` + fixture · lente contrato · P2 · bench · dedup #1197

- title: las filas `-no-effect` miden el stylesheet idéntico (el strip no elimina nada).
- evidence: el strip filtra líneas cuyo `trim_start` empiece por `filter:`/`backdrop-filter:`, pero el fixture declara `filter:` dentro de reglas (`.gray { filter: … }`), nunca a inicio de línea. Ejecución: réplica del strip en python → 30 líneas vs 30, idénticas (`identical: True`).
- impact: deltas A/B estructuralmente cero; las filas no miden lo que dicen.
- expected: strip a nivel de declaración (regex) o fixtures generados con/sin efecto.
- fix: idem.
- repro: comando python en `## Comandos`.
- dedup: #1197 (evidencia nueva: diff vacío ejecutado).

### F12 — w2-st-1198-units · `benches.rs:1421-1427` · lente aritmética · P3 · bench · dedup #1198

- title: throughput de `render/html-to-png` 5× erróneo (`0.096` vs `0.48` MPix).
- evidence: viewport 800×600 = 480000 px = 0.48 MPix; el código declara `.with_units(0.096, "MPix")` (0.096×5 = 0.48 exacto, coincide con el título del issue).
- impact: comparativas entre filas incomparables.
- expected: `0.48`.
- fix: una línea.
- repro: aritmética + `bench --json` (campo `throughput.count`).
- dedup: #1198 (evidencia nueva: factor exacto).

### F13 — w2-st-1199-warmup · `runner.rs:108-117` · lente contrato · P3 · bench · dedup #1199

- title: `heavy` limita warmup a 1 e ignora `--warmup`/`--iterations` del usuario.
- evidence: `opts.warmup.min(1)` / `opts.iterations.min(3)` sin aviso.
- impact: flags documentados sin efecto en filas heavy.
- expected: documentar en `--help`/tabla o avisar.
- fix: idem.
- repro: estático.
- dedup: #1199 (evidencia nueva: líneas exactas).

### F14 — w2-st-1177-stale-docs · `lib.rs:93-149` · lente contrato/docs · P3 · docs · dedup #1177

- title: docs del módulo contradicen la implementación (tabs, menú contextual, submit, scripting en edición).
- evidence: "There is no visual tab bar yet" (línea ~109) vs tab strip real; "no tabs" (línea ~118) vs `tabs.rs`; "right/middle click ignored" (línea ~138) vs context menu + middle-click-new-tab; "`Enter` never submits" (líneas 68,131) vs `submit_focused_form` (#1538); "scripts disabled while editing" (línea 16) vs `begin_ui_event_request` (#1504).
- impact: contrato falso para lectores del crate.
- expected: reescribir el bloque "What it does not do yet".
- fix: idem.
- repro: inspección.
- dedup: #1177 (evidencia nueva: 5 contradicciones localizadas).

### F15 — w2-st-1178-find-count · `headless.rs:269-284,290-308` + `main.rs:105-108` · lente contrato · P3 · contract · dedup #1178

- title: `ScreenshotInfo` pierde el hit count (`let _ = hits`).
- evidence: `render_target_with_find` devuelve `(output, hits)`; `screenshot_with_find` lo descarta; `main.rs` usa `_hits`.
- impact: `--find` headless no informa nº de coincidencias (CI no puede aseverar).
- expected: campo `hits` en `ScreenshotInfo` + print.
- fix: idem (2 líneas + struct).
- repro: inspección.
- dedup: #1178 (evidencia nueva: descarte en dos niveles).

### F16 — w2-st-headless-tabs · `main.rs:75-113` · lente contrato · P2 · bug · dedup new

- title: `--tabs` se ignora en headless `--screenshot` (solo se renderiza el input posicional).
- evidence: la rama `Screenshot` no referencia `args.tabs`/`view.tabs` (construido en líneas 61-66 y abandonado). Ejecuciones (worktree limpio): dos renders con `--tabs` distintos → md5 idénticos `29c7df93…` (14344 B ambos).
- impact: flags aceptados sin efecto ni aviso; `lib.rs:108` ("headless renders the active (first) tab") sugiere lo contrario.
- expected: renderizar el primer tab / avisar `--tabs` sin efecto en headless.
- fix: o usar `args.tabs[0]` como input + aviso, o `Usage`.
- repro: comandos E-tabs en `## Comandos`.
- dedup: new (espejo headless de #1169).

### F17 — w2-st-reader-quadratic · `reader.rs:82-104,44-73` · lente límites/DoS · P2 · perf · dedup new

- title: `extract()` re-escanea subárboles por cada contenedor (O(n²)); la caché no evita los walks.
- evidence: por cada nodo contenedor, `visible_text` recorre todo su subárbol (`or_insert_with` ejecuta el walk antes de cachear); nodos anidados de profundidad D → ~D²/2 visitas. Ejecuciones (worktree limpio, debug): página 198 KB con 15k divs anidados → `--screenshot` 1.54 s (falla por límites, exit 2) vs `--reader` 4.58 s (éxito). El delta ≈ coste de `extract` (~3 s en debug).
- impact: página hostil + Ctrl+Shift+R/`--reader` → freeze de segundos (en ventana corre en el event loop); DoS local de bajo coste para el atacante.
- expected: texto bottom-up una sola pasada (o cap de contenedores evaluados).
- fix: computar longitudes una vez por nodo reutilizando hijos; mantener determinismo.
- repro: generador + 2 comandos en `## Comandos` (E-reader).
- dedup: new.

### F18 — w2-st-overflow-overwrite · `download.rs:106-119` · lente contrato · P3 · bug · `needs-repro` · dedup new

- title: el nombre `overflow` no se comprueba: a partir de 1025 descargas se sobrescribe silenciosamente.
- evidence: tras el bucle `1..=1024`, retorna `megi-<host>-overflow.<ext>` sin `exists()`; `write_atomic` trunca destino. Contradice "never overwrite" del módulo (línea 6).
- impact: pérdida silenciosa de la descarga overflow anterior.
- expected: sufijo incremental también tras overflow, o error.
- fix: bucle abierto con sufijo / timestamp + colisión-check.
- repro: precrear 1024 ficheros + 2 `save_image` (requiere ventana; no ejecutado) → `needs-repro`.
- dedup: new.

### F19 — w2-st-session-doc-drift · `session.rs:522-553` vs doc `session.rs:3-5` · lente contrato/docs · P3 · docs · dedup new

- title: el fallback `~/.megi/session.json` documentado es inalcanzable.
- evidence: con `HOME` definido (caso normal) retorna `~/.local/share/…` siempre; `None` solo sin `XDG` ni `HOME`. La rama `~/.megi` no existe en el código.
- impact: contrato falso menor.
- expected: implementar fallback o corregir docs.
- fix: una de las dos.
- repro: inspección.
- dedup: new.

## Verificados corregidos en HEAD (sin hallazgo; constan para dedup)

- #1172: tests de `session.rs` hoy son `#[test]` independientes de primer nivel (ej. `failed_saves_clean_up_their_temporary`, `temporaries_are_unique_and_exclusive`, `saves_use_a_unique_temporary_and_leave_no_files`). Corregido.
- #1468: `spawn_worker` envuelve el render en `catch_unwind` → `WorkerMessage::Failed` (worker.rs:463-479) + test `panicking_render_does_not_kill_the_worker`. Corregido.
- #1168 (scripts): el probe desactiva scripting (`view_options(..., probe=true)`, worker.rs:627-650; comentario #1471) → los scripts corren una vez; el doble render persiste por diseño (documentado). Parcialmente vivo por diseño, no se reporta.
- #1200: `bench/cli.rs:34-36,83-115` acota scale/iterations/warmup y rechaza `1e300`/`inf`/`u64::MAX`. Corregido.
- #1202: `main.rs:71-81` retorna FAILURE con 0 resultados; `runner.rs:140-146` filtra skipped por el mismo predicado. Corregido.
- #1203: closures usan `viewport.width/height` y `expect("fixture must lay out")` (benches.rs:452-460, 588-601). Corregido.
- #1204: filas VP8L sin `.heavy()` (benches.rs:1301-1336). Corregido.
- #1205: `push_json_number` (null ante no-finito) + `push_json_string(unit)` (report.rs:212-218, 173-182). Corregido.
- #1206: anchos con `chars().count()` + separador de `header` (report.rs:38-58). Corregido.
- #1207: sin literales de 26 espacios (generators/report revisados). Corregido.
- #1208: `heavy` por profundidad escalada (benches.rs:131). Corregido.
- #1209: `args_os()` + error UTF-8 (bench/main.rs:27-40). Corregido.
- #1210: `env!("BENCH_RUSTC_VERSION")` (runner.rs:293-296). Corregido.
- #1199/#1197/#1198: vivos (F11–F13). #1201: fuera de scope (crate `layout`, no leído) → concern.
- #1468–#1476 (índice f2): solo #1468 cae en mi scope; resto (estado JS/render) fuera de scope.

## Comandos ejecutados

Todos desde `/tmp/opencode/megi-verify` (HEAD 3c06db8 limpio), salvo `gh` (solo lectura).

1. `cargo test -p shell -p bench -p test_harness` → shell lib 269 passed; shell bins 12+1; bench 58; harness lib 3; closed_loop 8 (100.6 s). Todo `ok`, 0 fallos.
2. `grep -rn "sync_all|fsync|fdatasync" crates/shell/src/ | wc -l` → `0`.
3. `gh issue view` ×17 → los 17 issues CLOSED (lista en el encabezado).
4. E-tabs (F16): `megi "data:text/html,<h1>AAA</h1>" --tabs B --screenshot t1.png` y con `--tabs CCC-different` → ambos `wrote … 14344 bytes`, md5 `29c7df93e9775d51d46db8c170d1ad32` idénticos.
5. E-icon (F2): servidor `http.server 8932` + página con `<link rel=icon href="file:///tmp/opencode/v2/secret/marker.png">` (fixture `rgba8.png`); `--save-html` confirma carga (`hi-http`).
   `h_present.png` md5 `f06e0a…` vs `h_absent.png` `699f90…`; diff RGBA 128 px, bbox x 3..796 y 4..52. Control `file:`: `1fd055…` vs `c172f5…` (también difiere).
6. E-reader (F17): fixture 198467 B / 15k divs anidados. `--screenshot` → 1.54 s + `I/O error … 10908 limit violation(s)` (exit 2); `--reader --screenshot` → 4.58 s, `wrote nr.png 37515 bytes`.
7. E-strip (F11): réplica del filtro (`trim_start` + `startswith("filter:"/"-webkit-filter:")`) sobre `filter_effects.html` → 30 vs 30 líneas, idénticas.
8. `grep -n "0.096" crates/bench/src/benches.rs` → línea 1425 (F12); `heavy`+`warmup.min(1)` en `runner.rs:108-117` (F13).
9. `cmp` de los 41 ficheros del scope entre árbol principal y worktree → sin diferencias.

## Concern

1. **Advisories privados pendientes**: F2 (`file:`→`http` icon fetch) y F9 (tmp predecibles en `download.rs`/`headless.rs`) deben reportarse por canal privado según `SECURITY.md` (no como issues públicos). Ambos reproducidos o con evidencia sólida en HEAD.
2. **WIP del usuario**: los ficheros del scope son byte-idénticos entre el árbol principal y el worktree limpio; ningún hallazgo depende de los cambios sin commitear (`crates/js`, `crates/render`). No se usa la etiqueta `excluir: WIP del usuario` en ningún hallazgo.
3. **Proveniencia**: sondeos iniciales en el árbol principal dieron resultados inconsistentes (posible binario stale); se descartaron y todo lo reportado se re-ejecutó en el worktree limpio con build fresco. Los servidores http de prueba (puertos 8931/8932) deben apagarse.
4. **`window.rs` no es ejecutable en CI** (sin display): F1, F3–F6, F10, F18 quedan en `needs-repro` pese a evidencia de código concluyente; el cableado evento→acción solo se puede verificar con tests de integración con display o refactors de inyección.
5. **#1201 fuera de scope** (medidor de `layout`, crate no asignada): no se verifica; si el runner usa el measurer de producción o el fallback determina si sigue vivo.
6. **Posible falso positivo descartado**: `site_origin` vs `normalize_site_zooms` (mayúsculas) no se reporta porque `net::Url` probablemente normaliza el host y `net` está fuera del scope.
7. **Deuda de docs (F14)**: `lib.rs` acumula contradicciones por features aterrizadas (#1503/#1504/#1538, tabs, chrome); conviene reescribir la sección "What it does not do yet" de una vez en vez de parches.
