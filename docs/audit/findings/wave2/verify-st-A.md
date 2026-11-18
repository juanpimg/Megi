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
| F2 w2-st-1173-file-icon | VERIFIED + **ADVISORY** | `megi http://127.0.0.1:8932/present.html --chrome --screenshot h_present.png` vs `absent.html` → `h_absent.png`; lectura `favicon.rs:50-129` | `wrote h_present.png (800x600, 19024 bytes…)`; `wrote h_absent.png (800x600, 19149 bytes…)`; md5 `8d912306d429de1165b63370e9cf8723` vs `c33a6f75d2c9ecf7cd54a69c8638b47c` (difieren) | `icon_url` acepta cualquier `href` resuelto (línea 70); `fetch` rama `File` lee disco sin mirar la página (83–99); `for_page` no filtra (116–129). Oráculo confirmado headless con `--chrome`. **Canal privado según `SECURITY.md`; no publicar como issue.** |
| F3 w2-st-1166-optgroup-rows | NEEDS-REPRO | `grep -n dropdown_layout window.rs chrome.rs` + lectura | `window.rs:946` click (`dropdown.options.len()`), `window.rs:2638` hover (`hit_row`, mismo ctor), `window.rs:2609` rueda (mismo ctor) vs pintado `chrome.rs:1875-1876` (`dropdown.items()` + `dropdown_layout_items`) | 3 call sites usan modelo sin headers; el pintado incluye `DropdownItem{option_index:None}` por grupo. Divergencia exacta confirmada; click real requiere display. |
| F4 w2-st-1167-find-offset | NEEDS-REPRO | lectura `window.rs:2465-2490` + `find.rs:284-349` | `overlay_highlights_dest(…, css_width, css_height, …)` con `scale_y = dest_height / css_viewport_height` sin descontar `chrome_px`; `current_key = rects.get(current)` sobre `rects` deduplicados por `rect_key` | Doble defecto confirmado: mapeo sin offset de chrome + `current` índice DOM vs fragmentos. Overlay real requiere ventana. |
| F5 w2-st-1165-reader-url | NEEDS-REPRO | lectura `window.rs:1282-1301` + `save_active_tab 2142-2152` | `self.input = "about:reader".to_string()` (`open_reader`); `tab.url.clone_from(&self.input)` (`save_active_tab`); tras reinicio `reader_html=None` → render de página vacía | Cadena persistencia→restore completa en código. Requiere display (Ctrl+Shift+R + reinicio con sesión). |
| F6 w2-st-1163-history-push | NEEDS-REPRO | lectura `window.rs:1337-1356`, `start_navigation 556-590`, `navigate_to` | `handle_address_key(Enter)` → `start_navigation` (sin `self.nav.…push`); `navigate_selected_history` → `start_navigation` (ídem); sólo `navigate_to` hace push (`NavigationOutcome::Pushed`) | Llamantes tipados confirmados sin push. Back real requiere display. |
| F7 w2-st-1175-headless-flags | VERIFIED | `awk '/match args.mode/,0' main.rs \| grep …` | `save_png/save_html/find/chrome/reader` sólo en rama `LaunchMode::Screenshot`; rama `LaunchMode::Window => megi_browser::run_window_with_view_options(` sin referencias | Flags muertos en ventana confirmados estáticamente (la rama exige display, el hecho "no se referencia" no). |
| F8 w2-st-1171-no-fsync | VERIFIED (estático) + `needs-repro` (evento) | `grep -rn "sync_all\|fsync\|fdatasync" crates/browser/src/ \| wc -l` | `0` (+ `(no matches)`); `session.rs:422-446` `write_temporary` + `fs::rename` sin `sync_all` | Ausencia total de fsync probada. La pérdida concreta (crash entre write y flush) es `needs-repro` por naturaleza. |
| F9 w2-st-1176-tmp-writers | CONFIRMED (código) + **ADVISORY** + `needs-repro` (explotación) | lectura `download.rs:138-161`, `headless.rs:318-348,388-409` vs `session.rs:477-504` | `write_atomic`: `path.with_file_name(format!(".{file_name}.tmp"))` + `fs::write` (trunca/sigue symlinks); `save_screenshot`/`write_bytes`: mismo patrón `".{}.tmp"` + `fs::write`; `session.rs:write_temporary`: `create_new` + único + guard | 3 call sites vulnerables confirmados; `session.rs` es el contraejemplo correcto. Explotación (symlink + descarga) requiere display salvo `--save-png` → `needs-repro`. **Canal privado; no issue público.** |
| F10 w2-st-1174-narrow | NEEDS-REPRO | lectura `chrome.rs:545-569` | `if per_tab * count as f32 > available && available >= TAB_MIN_WIDTH` → con `width<100` (`available<72`) nunca hay `+N`; `visible=count`, tabs de 72 px pintados fuera del strip | Guard confirmado; estrechar ventana <100 px requiere display. |
| F11 w2-st-1197-strip-noop | VERIFIED | réplica python del filtro sobre `filter_effects.html` | `total: 29 stripped: 29 identical: True`; `grep -c 'filter:'` → `12` (todos `filter:` dentro de regla, nunca a inicio de línea: e.g. `.gray { filter: grayscale(1); }`) | Strip estructuralmente vacío confirmado. Nota: 29 líneas aquí vs "30" del informe (deriva menor de fixture); conclusión idéntica. |
| F12 w2-st-1198-units | VERIFIED | lectura `benches.rs:39-40,1415-1427` + aritmética | `VIEWPORT_WIDTH 800`, `VIEWPORT_HEIGHT 600` → 480000 px = **0.48 MPix**; código `.with_units(0.096, "MPix")` (0.096×5 = 0.48 exacto) | Factor 5× confirmado. |
| F13 w2-st-1199-warmup | VERIFIED | lectura `runner.rs:100-120` | `let iterations = if entry.heavy { opts.iterations.min(3) … }; let warmup = if entry.heavy { opts.warmup.min(1) … }` sin aviso | Flags de usuario capados en `heavy` confirmados estáticamente. |
| F14 w2-st-1177-stale-docs | VERIFIED | lectura `lib.rs:90-150` + `grep` | `There is no visual tab bar yet` (l.109) vs tab strip real; `still no tabs` (l.118) vs `tabs.rs`; `Enter never submits` (l.134) vs `submit_focused_form`; `scripts disabled while editing` (l.136) vs `begin_ui_event_request`; `right/middle click ignored` (l.138) vs `context_menu` + middle-click-new-tab (`window.rs:703+`) | 5 contradicciones localizadas. Inspección pura. |
| F15 w2-st-1178-find-count | VERIFIED | lectura `headless.rs:265-310` + `main.rs:100-112` + struct | `render_target_with_find` retorna `(output, hits)`; `screenshot_with_find`: `let _ = hits;` + `ScreenshotInfo{path,width,height,png_bytes}` (sin campo `hits`); `main.rs`: `let (output, _hits) = …` | Descarte en dos niveles + struct sin `hits` confirmados. |
| F16 w2-st-headless-tabs | VERIFIED | `megi "data:…AAA" --tabs B --screenshot t1.png` y con `--tabs CCC-different` → `t2.png`; `md5sum`; lectura `main.rs:60-73` | `wrote … (800x600, 14344 bytes…)` ×2; md5 `29c7df93e9775d51d46db8c170d1ad32` **idénticos**; `args.tabs`/`view.tabs` sin referencia en rama `Screenshot` | Flags aceptados sin efecto ni aviso. Repro del auditor replicado byte a byte (mismo md5 `29c7df93…`). |
| F17 w2-st-reader-quadratic | VERIFIED | lectura `reader.rs:44-110` + sonda ligera (71 KB, 3000 divs anidados): `--screenshot` vs `--reader --screenshot` | Código: `or_insert_with(\|\| visible_text(…).chars().count())` ejecuta el walk completo antes de cachear; por cada contenedor anidado → ~D²/2 visitas. Sonda: base `0.545 s` (wrote 25684 B) vs reader `3.495 s` (`reader mode found no article`) → Δ ≈ 6× aun sin artículo | Mecanismo + delta direccional confirmados. Números del auditor (1.54 s vs 4.58 s, fixture 198 KB/15k) no replicados al milímetro (otra fixture y build), pero el coste es del `extract`, no del render. |
| F18 w2-st-overflow-overwrite | NEEDS-REPRO | lectura `download.rs:106-119` + doc l.6 | Tras `for number in 1..=1024` sin libre → `dir.join(…-overflow.{ext})` **sin `exists()`**; `write_atomic` trunca destino; doc promete `never overwrite` | Sobrescritura silenciosa a partir de la descarga 1025 confirmada en código. Repro (precrear 1024 + 2 `save_image`) requiere ventana → no ejecutado. |
| F19 w2-st-session-doc-drift | VERIFIED | lectura `session.rs:1-6` vs `default_session_path 522-553` | Doc: `falling back to ~/.local/share/… and then ~/.megi/…`; código: con `HOME` definido retorna siempre `~/.local/share/…`; `None` sólo sin `XDG` ni `HOME`; **ninguna rama `~/.megi`** | Contrato falso confirmado por inspección. |

## Comandos ejecutados (salida literal recortada)

Todos desde `/tmp/opencode/megi-verify` (HEAD `3c06db8`), `TMPDIR=/home/jp/.cache/megi-tmp`,
`CARGO_TARGET_DIR=/home/jp/.cache/f2-target`, salvo `grep`/`sed` (sólo lectura).

1. `cargo build -p megi-browser` → `Finished dev profile … in 24.84s`; binario `/home/jp/.cache/f2-target/debug/megi`.
2. F16: dos renders `--tabs` distintos → ambos `wrote … 14344 bytes`, md5 `29c7df93e9775d51d46db8c170d1ad32` idénticos.
3. F2: servidor `http.server 8932` + `present.html` (`<link rel=icon href="file:…/marker.png">`) vs `absent.html`;
   `--chrome --screenshot` → `h_present.png` md5 `8d912306…` (19024 B) vs `h_absent.png` `c33a6f75…` (19149 B): difieren.
   (Nota: difieren del md5 del auditor —otra fixture/marker— pero la dirección es la misma: oráculo presente/ausente.)
4. F11: réplica python → `total: 29 stripped: 29 identical: True`.
5. F8: `grep -rn "sync_all|fsync|fdatasync" crates/browser/src/ | wc -l` → `0`.
6. F17-ligero: fixture 71626 B / 3000 divs → `--screenshot` `0.545 s` vs `--reader --screenshot` `3.495 s`
   (`reader mode found no article on this page`).
7. F3/F4/F5/F6/F10/F18: lecturas dirigidas (`sed`/`grep`) citadas en la tabla; sin ejecución con display.
8. `git status --short` en worktree: sólo `?? tests/fixtures/render/f2-exp10.html`, `f2-exp12.html`; `crates/` limpio.

## Concerns (para Mesa C)

1. **Advisories privados (F2, F9)**: reportar por canal privado según `SECURITY.md`; no crear issues públicos.
   Este fichero de verificación contiene md5 y rutas pero no PoC explotable más allá del oráculo; aun así no hacerlo público.
2. **Ventana real no disponible**: F1, F3–F6, F10, F18 quedan en `needs-repro`. La evidencia de código es en todos
   concluyente (líneas exactas citadas); sólo falta el ejercicio evento→acción con display o inyección.
3. **F2/F16 usan `--chrome`/headless sin display**: reproducibles en CI; no confundir con los `needs-repro` de ventana.
4. **F11 conteo 29 vs 30**: deriva menor de fixture respecto al informe; el hecho (strip vacío) es robusto.
5. **F8 veredicto mixto**: hecho estático VERIFIED (cero fsync); el evento de pérdida es intrínsecamente `needs-repro`.
   Si Mesa C exige un solo estado, usar `needs-repro`.
6. **F17 números no idénticos al auditor** (otra fixture/build): el mecanismo cuadrático está confirmado en código
   (`or_insert_with` camina antes de cachear) y la sonda ligera lo respalda direccionalmente (6×).
7. **Servidores http de prueba** (puerto 8932) apagados al final (`no server` confirmado). No se borró ningún archivo
   del worktree ni del árbol principal.
