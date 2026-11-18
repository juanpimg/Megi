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
| w2-st-1173-file-icon (F2) | VERIFIED + ADVISORY (A) / YA REPORTADO #1173 (B) | P2 | shell favicon | security, advisory-privado | exclude (advisory) | `file:` desde `http(s)` sin chequeo (`favicon.rs:50-129`); oráculo 4 renders + diff 128 px + control. Canal privado, nunca issue público. Borrador en `advisories/w2-st-1173-file-icon.md`. |
| w2-st-1166-optgroup-rows (F3) | NEEDS-REPRO (A) / YA REPORTADO #1166 (B) | P2 | shell dropdown | public, contract | exclude (comment-existing) | 3 call sites usan modelo sin headers vs pintado con `items()` (`window.rs:945-963,2605-2623` vs `chrome.rs:1869-1945,687-727`). Click real requiere display. |
| w2-st-1167-find-offset (F4) | NEEDS-REPRO (A) / YA REPORTADO #1167 (B) | P2 (P3 admisible) | shell find overlay | public, contract | exclude (comment-existing) | Mapeo sin offset chrome + `current` índice DOM vs fragmentos (`window.rs:2471-2484`, `find.rs:284-349`). Overlay real requiere ventana. |
| w2-st-1165-reader-url (F5) | NEEDS-REPRO (A) / YA REPORTADO #1165 (B) | P2 | shell reader persist | public, contract | exclude (comment-existing) | `about:reader` persiste como URL y restaura vacía (`window.rs:1282-1301,2142-2152` + `session.rs:83-132`). Requiere display. |
| w2-st-1163-history-push (F6) | NEEDS-REPRO (A) / YA REPORTADO #1163 (B) | P2 | shell navigation | public, contract | exclude (comment-existing) | `start_navigation` sin push; solo `navigate_to` hace push (`window.rs:1337-1356,1304-1314,556-590`). Back real requiere display. |
| w2-st-1175-headless-flags (F7) | VERIFIED (A) / YA REPORTADO #1175 (B) | P3 | shell cli | public, contract | exclude (comment-existing) | Flags muertos en rama Window (`main.rs:60-73,175-182`); verificable con `grep`. |
| w2-st-1171-no-fsync (F8) | VERIFIED (estático) + needs-repro evento (A) / YA REPORTADO #1171 (B) | P2 (P3 admisible) | shell session | public, contract | exclude (comment-existing) | `grep sync_all` → 0 (`session.rs:422-446` rename sin fsync). El evento de pérdida es intrínsecamente needs-repro. |
| w2-st-1176-tmp-writers (F9) | CONFIRMED + ADVISORY + needs-repro explotación (A) / YA REPORTADO #1176 (B) | P2 | shell download/headless | security, advisory-privado | exclude (advisory) | Tmp predecible + `fs::write` en 3 call sites (`download.rs:138-161`, `headless.rs:318-348,388-409`); solo `session.rs` migrado. Canal privado. Borrador en `advisories/w2-st-1176-tmp-writers.md`. |
| w2-st-1174-narrow (F10) | NEEDS-REPRO (A) / YA REPORTADO #1174 (B) | P3 | shell tab strip | public, bug | exclude (comment-existing) | Guard `available >= TAB_MIN_WIDTH` desactiva menú bajo ~100 px (`chrome.rs:545-569`). Requiere display. |
| w2-st-1197-strip-noop (F11) | VERIFIED (A) / YA REPORTADO #1197 (B) | P2 (P3 admisible) | bench | public, bench | exclude (comment-existing) | Strip vacío ejecutado (29 vs 29 idénticas en sonda A; 30 vs 30 en informe; deriva menor de fixture). Solo bench. |
| w2-st-1198-units (F12) | VERIFIED (A) / YA REPORTADO #1198 (B) | P3 | bench | public, bench | exclude (comment-existing) | One-liner `0.096` → `0.48` MPix (800×600, `benches.rs:1421-1427`). |
| w2-st-1199-warmup (F13) | VERIFIED (A) / YA REPORTADO #1199 (B) | P3 | bench | public, bench | exclude (comment-existing) | `warmup.min(1)` / `iterations.min(3)` en heavy sin aviso (`runner.rs:108-117`). |
| w2-st-1177-stale-docs (F14) | VERIFIED (A) / YA REPORTADO #1177 (B) | P3 | shell docs | public, docs | exclude (comment-existing) | 5 contradicciones localizadas (`lib.rs:93-149`); reescribir sección de una vez. |
| w2-st-1178-find-count (F15) | VERIFIED (A) / YA REPORTADO #1178 (B) | P3 | shell headless | public, contract | exclude (comment-existing) | Descarte en dos niveles (`headless.rs:269-310` + `main.rs:105-108`; struct sin `hits`). |
| w2-st-headless-tabs (F16) | VERIFIED (A) / NUEVO (B) | P2 | shell headless | public | include | md5 idénticos `29c7df93…` (14344 B ambos); rama Screenshot sin referencia a `args.tabs`. Cuerpo en `issues/w2-st-headless-tabs.md`. |
| w2-st-reader-quadratic (F17) | VERIFIED (A) / NUEVO (B) | P2 (P3 admisible) | shell reader | public, perf | include | `or_insert_with` camina antes de cachear (~D²/2); sonda A 0.545 s vs 3.495 s; auditor 1.54 s vs 4.58 s. Cuerpo en `issues/w2-st-reader-quadratic.md`. |
| w2-st-overflow-overwrite (F18) | NEEDS-REPRO (A) / NUEVO (B) | P3 | shell download | public, bug | exclude (needs-repro) | `overflow` sin `exists()` (`download.rs:106-119`); a partir de 1025 se sobrescribe. Repro (1024 + 2 saves) requiere ventana; no ejecutado. No se publica. |
| w2-st-session-doc-drift (F19) | VERIFIED (A) / NUEVO (B) | P3 | shell docs | public, docs | include | Fallback `~/.megi` documentado inalcanzable (`session.rs:1-6` vs `522-553`). Cuerpo en `issues/w2-st-session-doc-drift.md`. |

## Avisos privados (2, sin issue público)

- w2-st-1173-file-icon (F2, P2): página remota provoca lectura de ficheros locales para el icono (oráculo + píxeles 16×16). Borrador sin PoC en `docs/audit/findings/wave2/advisories/w2-st-1173-file-icon.md`.
- w2-st-1176-tmp-writers (F9, P2): tmp predecible + truncate/symlink-follow en 3 call sites (descargas, screenshots). Borrador sin pasos explotables en `docs/audit/findings/wave2/advisories/w2-st-1176-tmp-writers.md`.

## Comment-existing (texto listo, NO issue nuevo)

- **F1 → comentar en #1169**:
  > Evidencia nueva en HEAD `3c06db8`: `WindowApp::new` rama `Some(session)` (`crates/shell/src/window.rs:371-394`) restaura y fusiona solo `input`, nunca consulta `view.tabs`; la rama `None` sí usa `TabsState::from_urls(view.tabs)` (línea 396). Apoyo: en headless los `--tabs` también se ignoran (F16, md5 idénticos). Mesa A: NEEDS-REPRO (requiere display). Mesa B: YA REPORTADO (#1169), P2 mantener.
- **F3 → comentar en #1166**:
  > Evidencia nueva en HEAD `3c06db8`: `click_at_cursor` (`window.rs:946`) y `CursorMoved`/`rueda` (`window.rs:2609,2638`) construyen `dropdown_layout(&rect, dropdown.options.len(), …)` (1:1 con opciones), mientras `paint_dropdown` usa `dropdown.items()` con `DropdownItem{option_index:None}` por cada `<optgroup>` (`chrome.rs:1875-1876` + `dropdown_layout_items`). A partir del primer header, fila pintada N ≠ opción N (3 call sites afectados). Mesa A: NEEDS-REPRO (click real requiere display). Mesa B: YA REPORTADO (#1166), P2 mantener.
- **F4 → comentar en #1167**:
  > Evidencia nueva en HEAD `3c06db8`: doble defecto — (a) `overlay_highlights_dest` recibe `css_width/css_height` completos y `scale_y = dest_height / css_viewport_height` sin descontar `chrome_px` (vs `present()` que pinta con `page_viewport_height` + offset `present_region_at`), todo el overlay queda desplazado una banda; (b) `current` es índice de `FindMatch` (DOM) usado contra `rects` deduplicados por `rect_key` (`current_key = rects.get(current)`). Ficheros: `window.rs:2465-2490`, `find.rs:284-349`. Mesa A: NEEDS-REPRO (overlay real requiere ventana). Mesa B: YA REPORTADO (#1167), P2 mantener (P3 admisible).
- **F5 → comentar en #1165**:
  > Evidencia nueva en HEAD `3c06db8`: cadena completa — `open_reader` fija `input="about:reader"` + `reader_html=Some` (`window.rs:1282-1301`); `save_active_tab` persiste `url="about:reader"` (`window.rs:2142-2152`); tras reinicio `reader_html=None` → render de página vacía; el historial global guarda `about:reader`. Fix: persistir URL fuente (+flag reader) o no persistir el tab lector. Mesa A: NEEDS-REPRO (requiere display + reinicio con sesión). Mesa B: YA REPORTADO (#1165), P2 mantener.
- **F6 → comentar en #1163**:
  > Evidencia nueva en HEAD `3c06db8`: `handle_address_key(Enter)` (`window.rs:1337-1356`) y `navigate_selected_history` llaman a `start_navigation` (`556-590`), que nunca toca `self.nav` (sin `navigate_to`/`push`); solo los links (`navigate_to`, línea 593, `NavigationOutcome::Pushed`) hacen push. Back no alcanza la URL tipada. Mesa A: NEEDS-REPRO (Back real requiere display). Mesa B: YA REPORTADO (#1163), P2 mantener.
- **F7 → comentar en #1175**:
  > Evidencia nueva en HEAD `3c06db8`: `finish_args` acepta `--save-png/--save-html/--find/--chrome/--reader` (`main.rs:60-73`) pero la rama `Window` no los referencia (`main.rs:175-182`); solo la rama `Screenshot` los consume. Verificable con `grep` (la rama window exige display; el hecho "no se referencia" es estático). Fix: aviso `megi: --save-png solo aplica con --screenshot` o `Usage`. Mesa A: VERIFIED. Mesa B: YA REPORTADO (#1175), P3 ok.
- **F8 → comentar en #1171**:
  > Evidencia nueva en HEAD `3c06db8`: `save()` (`session.rs:422-446`) hace `write_temporary` + `fs::rename` sin `sync_all`; `grep -rn "sync_all|fsync|fdatasync" crates/shell/src/ | wc -l` → `0` (ejecutado en worktree limpio). La pérdida concreta (crash entre write y flush) es `needs-repro` por naturaleza; el hecho estático está probado. Mesa A: VERIFIED (estático) + needs-repro (evento). Mesa B: YA REPORTADO (#1171), P2 mantener (P3 admisible).
- **F10 → comentar en #1174**:
  > Evidencia nueva en HEAD `3c06db8`: `if per_tab * count > available && available >= TAB_MIN_WIDTH` (`chrome.rs:545-569`) — con `width<100` (`available<72`) nunca hay marker `+N`; `visible=count`, tabs de 72 px pintados fuera del strip. Mesa A: NEEDS-REPRO (requiere display). Mesa B: YA REPORTADO (#1174), P3 ok.
- **F11 → comentar en #1197**:
  > Evidencia nueva en HEAD `3c06db8`: el strip filtra líneas cuyo `trim_start` empiece por `filter:`/`backdrop-filter:`, pero el fixture declara `filter:` dentro de reglas (`.gray { filter: … }`), nunca a inicio de línea. Réplica en python → 29 vs 29 idénticas (informe: 30 vs 30; deriva menor de fixture, conclusión idéntica) + `grep -c 'filter:'` → 12. Deltas A/B estructuralmente cero. Mesa A: VERIFIED. Mesa B: YA REPORTADO (#1197), P2 mantener (P3 admisible, es solo bench).
- **F12 → comentar en #1198**:
  > Evidencia nueva en HEAD `3c06db8`: viewport 800×600 = 480000 px = 0.48 MPix (`benches.rs:39-40`), pero el código declara `.with_units(0.096, "MPix")` (`benches.rs:1421-1427`; 0.096×5 = 0.48 exacto). Fix una línea (`0.48`). Mesa A: VERIFIED. Mesa B: YA REPORTADO (#1198), P3 ok.
- **F13 → comentar en #1199**:
  > Evidencia nueva en HEAD `3c06db8`: `opts.warmup.min(1)` / `opts.iterations.min(3)` en `heavy` sin aviso (`runner.rs:100-120`). Flags documentados sin efecto en filas heavy. Fix: documentar en `--help`/tabla o avisar. Mesa A: VERIFIED. Mesa B: YA REPORTADO (#1199), P3 ok.
- **F14 → comentar en #1177**:
  > Evidencia nueva en HEAD `3c06db8`: 5 contradicciones en `lib.rs:90-150` — "There is no visual tab bar yet" (l.109) vs tab strip real; "still no tabs" (l.118) vs `tabs.rs`; "`Enter` never submits" (l.134) vs `submit_focused_form`; "scripts disabled while editing" (l.136) vs `begin_ui_event_request`; "right/middle click ignored" (l.138) vs context menu + middle-click-new-tab (`window.rs:703+`). Fix: reescribir la sección "What it does not do yet" de una vez. Mesa A: VERIFIED. Mesa B: YA REPORTADO (#1177), P3 ok.
- **F15 → comentar en #1178**:
  > Evidencia nueva en HEAD `3c06db8`: `render_target_with_find` retorna `(output, hits)`; `screenshot_with_find` lo descarta (`let _ = hits;`, struct `ScreenshotInfo{path,width,height,png_bytes}` sin campo `hits`, `headless.rs:265-310`); `main.rs:100-112` usa `let (output, _hits)`. `--find` headless no informa nº de coincidencias. Fix 2 líneas + struct. Mesa A: VERIFIED. Mesa B: YA REPORTADO (#1178), P3 ok.

## Incluidos como issue público (3)

w2-st-headless-tabs (F16), w2-st-reader-quadratic (F17), w2-st-session-doc-drift (F19). Cuerpos en `docs/audit/findings/wave2/issues/<id>.md` (primera línea `[shell] …`, resumen, impacto, pasos, esperado/observado, evidencia literal, `Commit: 3c06db8`, `Dedup:`).

## Descartes (1 + reglas procesales)

- w2-st-overflow-overwrite (F18, P3, NUEVO): código concluyente (`download.rs:106-119` sin `exists()` tras `1..=1024`, contradice "never overwrite" de línea 6; `write_atomic` trunca) pero repro (precrear 1024 + 2 `save_image`) requiere ventana y no fue ejecutado → `exclude (needs-repro)`. No se publica borrador en `issues/`; queda documentado en el informe fuente y en esta acta.
- F1–F15 (salvo los dos advisories F2/F9): no se descartan por mérito; van a `comment-existing` por regla procesal (YA REPORTADO con evidencia nueva → reapertura/comentario, no issue nuevo; veredicto B).
- F2/F9: excluidos del circuito de issues por `security`+`advisory`; solo borradores privados en `advisories/`, sin pasos explotables detallados.
- Verificados-corregidos del informe (#1172, #1468, #1200, #1202–#1210, #1168-scripts por diseño): correctamente no publicados.
- #1201 (crate `layout`, fuera de scope) y falso positivo descartado (`site_origin` mayúsculas, `net` fuera de scope): correctamente no publicados.
