# Engine Depth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pasar de "renderiza" a "se usa": event loop persistente, corpus JS real, formularios completos, profundidad CSS/tipografía, Canvas 2D, Shadow DOM, Service Workers fase 1 y WASM mínimo, manteniendo Boa y el determinismo.

**Architecture:** Se ataca la profundidad, no la anchura (GAPS.md es histórico). Cada capacidad se introduce con un fixture E2E determinista y un subconjunto WPT asociado; el north star es `real-sites.html` 4 PASS / 0 DEGRADED y 0 errores JS. Boa se conserva con una única cláusula de salida (migración a rquickjs si >10 % de fallos del corpus son semántica del motor).

**Tech Stack:** Rust 1.91, Boa 0.22, `unicode-bidi`/`unicode-linebreak`, `wasmi` (intérprete WASM puro Rust), `symphonia`+`rodio` (audio tardío), AccessKit ya presente.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (§4.2, secuencia Q1–Q4).

## Global Constraints

- `#![forbid(unsafe_code)]` en los 17 crates; ninguna dependencia con toolchain C en el motor sin enmienda ADR previa.
- `wasmi`, `unicode-bidi`, `unicode-linebreak` y `symphonia` requieren enmienda explícita de `ARCHITECTURE.md` §3.1 antes de añadirse (licencia, MSRV, `cargo deny`).
- WebGL, JIT, motor JS propio, vídeo con FFmpeg y Service Workers completos son anti-objetivos (spec §2.3).
- Todo cambio de render debe conservar goldens byte-idénticos salvo declaración explícita en el PR.
- Reloj virtual y `Budget` para todo script; sin reloj de pared en el event loop.

## Review Focus

1. **Script real que revienta**: `example.com/s.js` (`TypeError: not a callable function`) y Wikipedia (`cannot convert 'null'`) → corpus JS con esos dos casos como regresión.
2. **Formulario POST completo**: `multipart/form-data` y `FormData` → fixture con servidor loopback que devuelve 200 y el body esperado.
3. **Evento submit ausente**: submitting un form no dispara el handler → test E2E con handlers inline.
4. **CJK/árabe**: bidi y line-break incorrectos → fixture `bidi_cjk` con md5 estable y comparación visual.
5. **Determinismo roto por animaciones reales**: el reloj virtual debe seguir produciendo el mismo PNG → replay 20/20 (spec G4).

---

### Task 1: Corpus JS real y gate de decisión Boa/rquickjs

**Files:**
- Create: `tests/fixtures/js_corpus/{example_com_s, wikipedia_null, jquery_like, module_cycle}.html`
- Create: `crates/render/tests/js_corpus_tests.rs`
- Modify: `docs/status/COVERAGE.md` (métrica de corpus separada)

**Interfaces:**
- Consumes: `render_document` con scripting activado y `RenderOptions`.
- Produces: `pub fn run_js_corpus() -> CorpusReport` con `{total, passed, engine_semantic_failures, binding_failures}`.

- [ ] **Step 1: Tests que fallan**

`js_corpus_tests.rs`: (a) `example_com_script_has_no_console_error`, (b) `wikipedia_null_conversion_has_no_console_error`, (c) `corpus_report_classifies_failures` — un fallo de binding vs uno de semántica del motor. Los dos primeros FAIL con el código actual.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-render --test js_corpus_tests`
Expected: 2 FAIL con `console_errors` no vacío.

- [ ] **Step 3: Clasificar y corregir bindings**

Por cada fallo: reproducir con `--json`, identificar si es binding (API ausente/incorrecta) o semántica (Boa). Corregir bindings en `crates/js/src/runtime/gap_apis/` con test unitario de regresión. Registrar cada caso en `CorpusReport`.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-render --test js_corpus_tests`
Expected: 3 tests OK; `engine_semantic_failures / total <= 0.10`.

- [ ] **Step 5: Gate de decisión**

Si el ratio >0.10, abrir ADR de migración a `rquickjs` (una sola vez, ~20–30 días, asumiendo C). Si ≤0.10, documentar la decisión de seguir con Boa en `docs/JAVASCRIPT-STRATEGY.md` con la cifra medida.

- [ ] **Step 6: Commit**

```bash
git add tests/fixtures/js_corpus crates/render/tests/js_corpus_tests.rs docs/status/COVERAGE.md
git commit -m "test(js): real-site script corpus with engine-vs-binding classification"
```

### Task 2: Event loop persistente por pestaña

**Files:**
- Modify: `crates/render/src/scripts/event_loop.rs`
- Modify: `crates/browser/src/worker/pool.rs`
- Modify: `crates/browser/src/window/frontend.rs`
- Test: `crates/render/tests/event_loop_persistent_tests.rs`

**Interfaces:**
- Consumes: `ScriptRuntime` con reloj virtual, `max_tasks`, `max_virtual_time`.
- Produces: `pub struct PersistentEventLoop { ... }`, `pub fn pump(&mut self) -> Result<LoopOutcome, LoopError>` que sobrevive entre renders y conserva timers pendientes.

- [ ] **Step 1: Tests que fallan**

(a) `timer_fires_across_two_renders`: un `setTimeout(…, 500)` registrado en el render 1 dispara en el render 2 sin `--refresh-ms`; (b) `virtual_clock_monotonic`: `now_ms()` no retrocede entre pumps; (c) `budget_still_enforced`: bucle infinito se corta con `LoopError::Budget`.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-render --test event_loop_persistent_tests`
Expected: FAIL.

- [ ] **Step 3: Implementar**

Extraer el event loop del renderspecific a un estado por pestaña en `render`, con cola de tareas y reloj virtual persistente; el worker del browser llama a `pump()` al recibir cada generación nueva; eliminar la dependencia de `--refresh-ms` como único disparador (la flag queda como compatibilidad no-op documentada).

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-render --test event_loop_persistent_tests && cargo test -p megi-browser worker`
Expected: 3 tests OK; tests de worker sin regresión.

- [ ] **Step 5: Commit**

```bash
git add crates/render/src/scripts/event_loop.rs crates/browser/src crates/render/tests/event_loop_persistent_tests.rs
git commit -m "feat(render): persistent per-tab event loop with deterministic clock"
```

### Task 3: Formularios completos y submit real

**Files:**
- Modify: `crates/browser/src/forms/submit.rs`
- Modify: `crates/js/src/runtime/gap_apis/forms.rs`
- Test: `crates/browser/tests/form_submit_e2e_tests.rs`

**Interfaces:**
- Consumes: `Request` con cuerpo de `crates/net`, `LiveFormPatcher`, `FormData` prelude actual.
- Produces: `pub fn encode_form(form: &HtmlFormElement) -> Result<FormBody, FormError>` con variantes `UrlEncoded` y `Multipart{boundary, parts}`, y evento `submit` despachado.

- [ ] **Step 1: Tests que fallan**

Con servidor `TcpListener` local: (a) `urlencoded_post_reaches_server_with_correct_body`, (b) `multipart_post_has_boundary_and_parts`, (c) `submit_event_handler_runs`, (d) `formdata_append_and_get`.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-browser --test form_submit_e2e_tests`
Expected: FAIL en multipart y submit.

- [ ] **Step 3: Implementar**

`FormBody::UrlEncoded` y `Multipart` con boundary determinista (derivado de un contador, sin reloj); `FormData` real en el prelude (no solo stub); despachar `submit` con `Event` cancelable (`preventDefault` bloquea el envío).

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-browser --test form_submit_e2e_tests`
Expected: 4 tests OK.

- [ ] **Step 5: Commit**

```bash
git add crates/browser/src/forms crates/js/src/runtime/gap_apis/forms.rs crates/browser/tests/form_submit_e2e_tests.rs
git commit -m "feat(forms): multipart FormData and cancelable submit event"
```

### Task 4: EventSource / SSE y streaming

**Files:**
- Create: `crates/net/src/eventsource.rs`
- Modify: `crates/js/src/runtime/gap_apis/eventsource.rs`
- Test: `crates/net/tests/eventsource_tests.rs`, fixture `tests/fixtures/render/sse.html`

**Interfaces:**
- Consumes: `get_streaming` de `net` (DataCallback), event loop persistente (Task 2).
- Produces: `pub struct EventSource` con `onmessage`/`onopen`/`onerror` y reconexión determinista.

- [ ] **Step 1: Tests que fallan**

(a) `text_event_stream_delivers_three_messages`, (b) `last_event_id_sent_on_reconnect`, (c) `non_200_is_error_event`.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-net eventsource`
Expected: FAIL.

- [ ] **Step 3: Implementar**

Parser SSE sobre `get_streaming`, límites de línea/tamaño, mensajes `data:`/`event:`/`id:`/`retry:`, reconexión con `Last-Event-ID`, sin hilos nuevos (usa el event loop persistente).

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-net eventsource && cargo run -q -p megi-browser -- --screenshot tests/fixtures/render/sse.html /tmp/opencode/sse.png --json | grep -c console_errors`
Expected: 3 tests OK; fixture sin errores JS.

- [ ] **Step 5: Commit**

```bash
git add crates/net/src/eventsource.rs crates/js/src/runtime/gap_apis/eventsource.rs crates/net/tests/eventsource_tests.rs tests/fixtures/render/sse.html
git commit -m "feat(net): EventSource with deterministic reconnection"
```

### Task 5: Bidi, CJK y line-break

**Files:**
- Modify: `Cargo.toml` (enmienda §3.1: `unicode-bidi`, `unicode-linebreak`)
- Modify: `crates/layout/src/bidi.rs`, `crates/fonts/src/lib.rs`
- Test: `tests/fixtures/render/bidi_cjk.html`, `crates/layout/tests/bidi_cjk_tests.rs`

**Interfaces:**
- Consumes: rustybuzz ya integrado, `fontdb` fallback.
- Produces: reordenamiento UAX #9 completo para los niveles soportados y line-break CJK conforme.

- [ ] **Step 1: Fixture que falla**

`bidi_cjk.html` con texto árabe + hebreo + CJK; assertion de md5 estable y comparación contra una referencia visualdocumentada. Actualmente el md5 no coincide con la referencia esperada.

- [ ] **Step 2: Verificar que falla**

Run: `cargo test -p megi-layout bidi_cjk`
Expected: FAIL.

- [ ] **Step 3: Implementar**

Añadir `unicode-bidi`/`unicode-linebreak` (Rust puro), completar resolución de niveles y reordenamiento; fallback por glifo en fuentes; `lang`/`:lang()` y `dir`→`direction`.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-layout bidi_cjk && cargo run -q -p megi-render -- tests/fixtures/render/bidi_cjk.html /tmp/opencode/bidi.png && md5sum /tmp/opencode/bidi.png`
Expected: tests OK; md5 estable en dos ejecuciones.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/layout crates/fonts tests/fixtures/render/bidi_cjk.html
git commit -m "feat(i18n): full UAX#9 bidi and CJK line breaking"
```

### Task 6: Canvas 2D útil

**Files:**
- Modify: `crates/js/src/canvas.rs`, `crates/js/src/runtime/gap_apis/canvas.rs`
- Create: `crates/raster/src/canvas_primitives.rs` ( compartir primitivas)
- Test: `tests/fixtures/render/canvas_paths.html`, `crates/js/tests/canvas_paths_tests.rs`

**Interfaces:**
- Consumes: `crates/raster` (actualmente `js` no depende de él; añadir dependencia interna permitida).
- Produces: `lineTo`, `bezierCurveTo`, `rect`, transform, gradientes, `drawImage`, `globalAlpha`; `arc` respeta ángulos.

- [ ] **Step 1: Tests que fallan**

`canvas_paths_tests.rs`: (a) `arc_respects_start_and_end_angle`, (b) `bezier_draws_expected_pixels`, (c) `drawimage_scales_with_globalalpha`.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-js canvas_paths`
Expected: FAIL (arc ignora ángulos hoy).

- [ ] **Step 3: Implementar**

Primitivas compartidas en `raster`, exponer API a `js`, convertir comandos de canvas a `DisplayList` del pipeline.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-js canvas_paths && cargo run -q -p megi-render -- tests/fixtures/render/canvas_paths.html /tmp/opencode/canvas.png`
Expected: 3 tests OK; PNG con píxeles esperados.

- [ ] **Step 5: Commit**

```bash
git add crates/js/src/canvas.rs crates/js/src/runtime/gap_apis/canvas.rs crates/raster/src/canvas_primitives.rs tests/fixtures/render/canvas_paths.html
git commit -m "feat(canvas): path primitives, transforms and respecting arc angles"
```

### Task 7: Shadow DOM y custom elements

**Files:**
- Modify: `crates/render/src/shadow_dom.rs`
- Modify: `crates/js/src/runtime/gap_apis/{frames,slots}.rs`
- Modify: `crates/css/src/selector/chunk*.rs` (`:host`/`::part`)
- Test: `tests/fixtures/render/shadow_host.html`, WPT subtests de shadow-dom

**Interfaces:**
- Consumes: expansión declarativa actual, slots existentes.
- Produces: encapsulamiento real, `:host`, `::part`, ciclo de vida `customElements`.

- [ ] **Step 1: Tests que fallan**

WPT: ≥100 subtests de `custom-elements`/`shadow-dom`; fixture local con `:host` y `::part` → hoy reglas cruzan el boundary.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo run --release -p megi-test-harness -- --wpt --summary && cargo run -q -p megi-render -- tests/fixtures/render/shadow_host.html /tmp/opencode/shadow.png`
Expected: subtests FAIL; el PNG muestra estilo filtrado incorrecto.

- [ ] **Step 3: Implementar**

WP atómicos de 1–2 días por crate (core→html→css→layout→js→render), con test en cada capa antes de cablear; sin reescribir el núcleo.

- [ ] **Step 4: Verificar**

Run: `cargo test --workspace && python3 tools/wpt_gate.py --check ...`
Expected: workspace verde; allowlist de la época 2 reducida; subtests de shadow-dom pasan.

- [ ] **Step 5: Commit**

```bash
git add crates/render/src/shadow_dom.rs crates/js/src/runtime/gap_apis crates/css/src/selector tests/fixtures/render/shadow_host.html
git commit -m "feat(shadow-dom): encapsulation, host/part and custom element lifecycle"
```

### Task 8: Service Workers fase 1 y WASM mínimo

**Files:**
- Create: `crates/browser/src/sw/{cache,interceptor}.rs`
- Modify: `Cargo.toml` (enmienda §3.1: `wasmi`)
- Create: `crates/js/src/runtime/gap_apis/wasm.rs`
- Test: `tests/fixtures/render/sw_offline.html`, `tests/fixtures/render/wasm_min.html`

**Interfaces:**
- Consumes: persistente event loop (Task 2), `net` para fetch, `wasmi` intérprete.
- Produces: `CacheStorage`, intercepción de fetch dentro del scope, install/activate; `WebAssembly.instantiate` con exports numéricos.

- [ ] **Step 1: Tests que fallan**

(a) `sw_offline_renders_after_network_cut`: cargar con SW, cortar red, recargar ⇒ PNG válido; (b) `wasm_export_returns_42`.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-browser sw_offline && cargo test -p megi-js wasm_min`
Expected: FAIL (0 coincidencias de ServiceWorker/WebAssembly hoy).

- [ ] **Step 3: Implementar**

SW fase 1: runtime por origen, `Cache` con `put/match`, intercepción de `fetch` en el scope, ciclo install→activate. Sin push/background sync/notificaciones (anti-objetivo). WASM: `wasmi` sin WASI/SIMD/threads; enmienda ADR antes de añadir la dep.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-browser sw_offline && cargo test -p megi-js wasm_min && cargo deny check`
Expected: 2 tests OK; dependencias permitidas.

- [ ] **Step 5: Commit**

```bash
git add crates/browser/src/sw crates/js/src/runtime/gap_apis/wasm.rs Cargo.toml tests/fixtures/render
git commit -m "feat(web): service worker phase 1 and minimal WASM interpreter"
```

### Task 9: Hardening de ventaja y aceptación final

**Files:**
- Create: `tools/replay_check.py`
- Create: `docs/status/determinism.html`, `docs/status/cold-render.html` (generados)
- Modify: `crates/bench/` (filas de arranque)
- Test: `tools/test_replay_check.py`

**Interfaces:**
- Consumes: 20 fixtures JS/SVG animados, `--screenshot` y `RenderOutput.stats`.
- Produces: `replay 20/20`, binario ≤40 MB, RSS ≤100 MB, p95 ≤250 ms en render frío; `real-sites.html` 4 PASS.

- [ ] **Step 1: Test que falla**

`test_replay_same_md5_across_virtual_envs`: para cada fixture, dos ejecuciones con distinta `TZ`/locale ⇒ mismo md5 y mismos stats. Hoy no existe la herramienta.

- [ ] **Step 2: Verificar que falla**

Run: `python3 -m unittest tools.test_replay_check -v`
Expected: FAIL.

- [ ] **Step 3: Implementar**

`replay_check.py` con corpus declarado, hashes publicados, medición de p50/p95 con `/usr/bin/time -v`, y generación de las dos páginas de estado. Añadir job de CI y umbral.

- [ ] **Step 4: Verificar**

```bash
python3 tools/replay_check.py --check
python3 tools/real_site_check.py --check
cargo test --workspace --all-targets --locked
```
Expected: replay 20/20; real-sites 4 PASS / 0 DEGRADED, 0 errores JS, <2,5 s; workspace verde.

- [ ] **Step 5: Commit**

```bash
git add tools/replay_check.py tools/test_replay_check.py docs/status crates/bench
git commit -m "feat(quality): deterministic replay, cold-render baseline and final acceptance"
```
