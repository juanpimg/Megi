# WPT Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ejecutar subtests reales de Web Platform Tests con un runner propio sobre Boa, con pin por SHA, allowlist monotónica y métrica `P/T` verificable en CI.

**Architecture:** Los fixtures WPT se vendorizan por directorios completos con un manifiesto que es la única fuente del denominador. Un shim JS (`megi-testharness.js`) implementa el subconjunto de la API de testharness que usan los directorios adoptados; el orquestador en Rust (`crates/test_harness/src/wpt/`) parsea el HTML, inyecta el shim, ejecuta los scripts con `JsRuntime::eval`, drena microtasks con el reloj virtual y emite un wptreport JSON. Un gate compara el reporte contra un baseline congelado.

**Tech Stack:** Rust (Boa 0.22, serde_json), Python 3.11 stdlib para herramientas, SHA-256 de WPT fijado, licencia BSD-3-Clause.

> **v2 (sin calendarización)**: las referencias a Q1–Q4 de este plan son hitos lógicos (M0–M9 del master plan v2), no fechas. Las cifras WPT son pisos, no techos: las épocas continúan hasta cubrir el árbol completo. Las carreras antes llamadas 'anti-objetivos' (JIT, WebGL, vídeo, SW completo) están ahora en `js-evolution`, `graphics-media` y `web-platform-extension`.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (§4.1 y épocas del §4.5 añadidas por builder check).

## Global Constraints

- MSRV Rust 1.91; toolchain CI fijado a 1.99.1.
- `#![forbid(unsafe_code)]` en todos los crates `megi-*`; **ninguna dependencia nueva** en `crates/test_harness/Cargo.toml` (serde_json ya está en el workspace).
- Toda identificación de subtest usa `sha256(test_path + "\0" + subtest_name)` como clave canónica.
- El denominador `T` incluye FAIL, TIMEOUT, NOTRUN y `harness_error`. Nunca se excluye un fallo del denominador.
- WPT se fija por SHA de commit de `web-platform-tests/wpt`; **nunca `master`**.
- Los archivos vendored son byte-idénticos y no se editan; una adaptación se registra como `skip` con `skip_reason`.
- `lib.rs` solo re-exporta; cada módulo nuevo ≤400 líneas (regla `AGENTS.md` / `check_arch.py`).
- CI sin red y con `persist-credentials: false` en el job WPT.

## Review Focus

1. **El shim reporta PASS donde upstream reportaría FAIL** (falso positivo) → golden de ~30 archivos con resultados upstream conocidos, incluidos FAIL; tests unitarios del shim en `shim.rs`.
2. **Un directorio nuevo entra sin pasar por el manifiesto** → `.gitignore` con excepciones explícitas por directorio y `vendor_wpt.py --check` falla si hay archivos no declarados.
3. **Un test WPT hace DoS o usa red** → `Budget::SMALL`, 250 ms wall / 2 s virtuales por test, `src` HTTP/HTTPS ⇒ `harness_error`, sin `testdriver.js`.
4. **Regresión silenciosa**: un subtest pasa de PASS a FAIL mientras el total sube → `wpt_gate.py` compara estados clave por clave contra el baseline, no solo el total.
5. **Comparación engañosa con Ladybird** → `wpt_compare.py` exige el mismo `wpt_revision` en ambos JSON y reporta solo la intersección; sin artefacto ajeno imprime "sin comparación disponible".

---

### Task 1: Vendorizado y pin de WPT

**Files:**
- Create: `tools/vendor_wpt.py`
- Create: `tests/wpt/WPT-REVISION.json`, `tests/wpt/MANIFEST.json`, `tests/wpt/NOTICE.md`
- Modify: `.gitignore` (excepciones explícitas por directorio)
- Modify: `tests/wpt/css/css-syntax/README.md` (sustituir "master" por el SHA)
- Test: `tools/test_vendor_wpt.py`

**Interfaces:**
- Consumes: repositorio WPT clonado en `--source`, SHA fijado en `WPT-REVISION.json`.
- Produces: `MANIFEST.json` con entries `{path, sha256, bytes, license, kind, skip_reason, dir, subtest_count_expected}`; exit 0 de `vendor_wpt.py --check`.

- [ ] **Step 1: Escribir el test que falla**

`tools/test_vendor_wpt.py` con tres tests: (a) `test_check_fails_on_undeclared_file`: crear un archivo temporal bajo `tests/wpt/css/css-syntax/` no presente en el manifiesto y assert que `check_manifest(root)` devuelve error que contiene el path; (b) `test_sha_mismatch_is_hard_error`: corromper un byte de un fixture temporal y assert error; (c) `test_license_allowlist_rejects_unknown`: entry con `license="UNKNOWN"` ⇒ error.

- [ ] **Step 2: Verificar que falla**

Run: `python3 -m unittest tools.test_vendor_wpt -v`
Expected: FAIL — `ModuleNotFoundError` o `ImportError` de `tools.vendor_wpt`.

- [ ] **Step 3: Implementar el importador**

`tools/vendor_wpt.py` con constantes `LICENSE_ALLOWLIST = {"BSD-3-Clause","MIT","Apache-2.0","OFL-1.1"}`, `MAX_BYTES_BY_PHASE = {1: 4_000_000, 2: 12_000_000, 3: 40_000_000}`, y funciones:
  - `load_revision(root: Path) -> dict`
  - `load_manifest(root: Path) -> list[dict]`
  - `check_manifest(root: Path) -> Result<(), String>` (valida que todo archivo bajo `tests/wpt/` esté declarado, que `sha256` coincida, que licencia esté permitida, que `-manual.html`/`*-ref.html`/tests con `testdriver.js` tengan `kind="skip"` y `skip_reason` no vacío, y que el total de bytes no exceda el presupuesto de la fase).
  - `vendor(source: Path, commit: str, dirs: list[str], root: Path) -> None` (copia byte-idéntica, escribe manifiesto y revisión, falla si `git rev-parse` en `source` no devuelve `commit`).
CLI: `--source`, `--commit`, `--dirs`, `--check`.

- [ ] **Step 4: Verificar que pasa**

Run: `python3 -m unittest tools.test_vendor_wpt -v && python3 tools/vendor_wpt.py --check`
Expected: 3 tests OK y `WPT manifest OK: N files` con exit 0.

- [ ] **Step 5: Importar la época 1**

```bash
git clone --filter=blob:none --no-checkout https://github.com/web-platform-tests/wpt.git /tmp/wpt-pin
cd /tmp/wpt-pin && git fetch --depth=1 origin <PIN_SHA> && git sparse-checkout set --cone css/css-syntax dom/events html/webappapis/timers html/webappapis/microtask-queuing dom/nodes css/css-selectors && git checkout FETCH_HEAD
cd /home/jp/Escritorio/Proyectos/Megi
python3 tools/vendor_wpt.py --source /tmp/wpt-pin --commit <PIN_SHA> --dirs css/css-syntax,dom/events,html/webappapis/timers,html/webappapis/microtask-queuing,dom/nodes,css/css-selectors
```
Expected: `WPT-REVISION.json` contiene el SHA de 40 hex; `MANIFEST.json` declara todos los archivos; `python3 tools/vendor_wpt.py --check` exit 0.

- [ ] **Step 6: Commit**

```bash
git add tools/vendor_wpt.py tools/test_vendor_wpt.py tests/wpt .gitignore
git commit -m "feat(wpt): pin and vendor WPT epoch 1 with manifest and license checks"
```

### Task 2: Shim de testharness en JS

**Files:**
- Create: `crates/test_harness/src/wpt/testharness_shim.js`
- Create: `crates/test_harness/src/wpt/shim.rs`
- Modify: `crates/test_harness/src/wpt/mod.rs` (re-export)
- Test: `#[cfg(test)] mod tests` dentro de `shim.rs`

**Interfaces:**
- Consumes: Boa `JsRuntime::eval(source, filename) -> Result<String, ...>` (`crates/js/src/runtime/api/runtime_impl.rs:53`).
- Produces: `pub const SHIM_SOURCE: &str`, `pub const REPORT_EXPR: &str = "__megi_testharness_report()"`, y un JSON con `{results: [{name, status, message}]}`.

- [ ] **Step 1: Escribir tests del shim que fallan**

En `shim.rs`, tests `shim_registers_and_passes_a_sync_test`, `shim_marks_unreached_as_fail`, `shim_reports_timeout_when_done_never_called`, `shim_includes_failed_assertion_message`. Cada uno crea un `JsRuntime` con un `Document` mínimo, evalúa `SHIM_SOURCE`, evalúa un snippet de test y llama a `REPORT_EXPR`, assertions sobre el JSON parseado.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-test-harness wpt_shim -- --nocapture`
Expected: FAIL — `SHIM_SOURCE` no existe.

- [ ] **Step 3: Implementar el shim**

`testharness_shim.js` (~300–500 líneas) con: registro `test()`, `async_test()`, `promise_test()`, `generate_tests()`, `setup()`; control `test.done()`, `step_func`, `step_func_done`, `step_timeout`, `add_completion_callback`, `on_event`; aserciones `assert_true/false`, `assert_equals/not_equals`, `assert_array_equals`, `assert_object_equals`, `assert_throws_js/dom`, `assert_unreached`, `assert_class_string`, `assert_approx_equals`, `assert_regexp_match`, `assert_in_array`, `assert_own_property`, `assert_node`; estados `PASS/FAIL/TIMEOUT/NOTRUN`; función global `__megi_testharness_report()` que devuelve `JSON.stringify({results})`. `shim.rs` expone `include_str!` y las constantes.

- [ ] **Step 4: Verificar que pasan**

Run: `cargo test -p megi-test-harness wpt_shim -- --nocapture`
Expected: 4 tests OK; ningún test del shim llama a red ni a APIs del sistema.

- [ ] **Step 5: Golden anti-falsos positivos**

Añadir `tests/wpt/results/shim_golden.json` con los resultados de ~30 archivos WPT conocidos (incluye al menos 3 FAIL upstream). Test `shim_golden_matches_upstream` compara las claves `sha256(path+name)` y los status; un FAIL upstream convertido en PASS rompe el test.

- [ ] **Step 6: Commit**

```bash
git add crates/test_harness/src/wpt tests/wpt/results/shim_golden.json
git commit -m "feat(wpt): add testharness shim with upstream-result golden"
```

### Task 3: Orquestador Rust y estado de reporte

**Files:**
- Create: `crates/test_harness/src/wpt/manifest.rs`
- Create: `crates/test_harness/src/wpt/runner.rs`
- Create: `crates/test_harness/src/wpt/report.rs`
- Modify: `crates/test_harness/src/lib.rs` (pub mod wpt + constantes)
- Modify: `crates/test_harness/src/run.rs` (flag `--wpt`)
- Test: `crates/test_harness/tests/wpt_runner.rs`

**Interfaces:**
- Consumes: `vendor_wpt.py` (Task 1), `SHIM_SOURCE` y `REPORT_EXPR` (Task 2), `megi_html::parse_document`, `JsRuntime::with_document`, `run_microtasks`, `advance`, `dispatch_simple_event`.
- Produces: `pub struct WptReport { pub run_info: RunInfo, pub results: Vec<TestResult> }`, `pub fn run_wpt_subset(root: &Path) -> Result<WptReport, String>`, y `megi-test-harness --wpt --report <path>` con exit 0.

- [ ] **Step 1: Escribir tests del runner que fallan**

`crates/test_harness/tests/wpt_runner.rs`: (a) `test_run_single_inline_script`: HTML con `<script>test("t", () => assert_equals(1,1));</script>` ⇒ 1 subtest PASS; (b) `test_remote_script_is_harness_error`: `<script src="https://example.com/x.js">` ⇒ status `ERROR` y `subtests` vacío; (c) `test_key_is_sha256_of_path_and_name`: assertion sobre la clave canónica de un subtest conocido.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-test-harness --test wpt_runner`
Expected: FAIL — `run_wpt_subset` no existe.

- [ ] **Step 3: Implementar `manifest.rs` y `report.rs`**

  - `manifest.rs`: `pub struct WptEntry { path, sha256, bytes, license, kind, skip_reason, dir, subtest_count_expected }`, `pub fn load(root: &Path) -> Result<Vec<WptEntry>, String>`, `pub fn subtest_key(path: &str, name: &str) -> String` (SHA-256 hex de `path + "\0" + name`).
  - `report.rs`: `pub struct WptReport` con `run_info` (`wpt_revision`, `megi_commit`, `harness: "megi-shim/0.1"`) y `results`; `#[derive(Serialize, Deserialize)]`; `pub fn counts(&self) -> (usize, usize, usize)` → (passed, failed, total) donde total incluye TIMEOUT/NOTRUN/ERROR.

- [ ] **Step 4: Implementar `runner.rs`**

`pub fn run_wpt_subset(root: &Path) -> Result<WptReport, String>`: por cada entry `kind="testharness"`, parsear HTML con `megi_html::parse_document(html, false)`, crear `JsRuntime::with_document(doc, file_url)`, `set_budget(Budget::SMALL)`, evaluar `SHIM_SOURCE`, sustituir `<script src="/resources/testharness.js|testharnessreport.js">` por el shim, evaluar cada script inline o `src` local con cap de 250 ms wall / 2 s virtuales, `run_microtasks()`, `advance(100ms)`, `dispatch_simple_event(None,"load",false)`, `run_microtasks()`, evaluar `REPORT_EXPR` y mapear a `TestResult`. `src` remoto ⇒ `TestResult{status:"ERROR", message:"harness_error: remote src"}`.

- [ ] **Step 5: Cablear el flag**

En `run.rs`, añadir `--wpt` y `--report <path>`; imprimir `WPT <sha8>: <P>/<T> passed (F fail, TO timeout, NR notrun, E harness_error)`; escribir JSON en `--report`.

- [ ] **Step 6: Verificar**

Run: `cargo test -p megi-test-harness --test wpt_runner && cargo run --release --locked -p megi-test-harness -- --wpt --summary --report /tmp/opencode/wpt.json`
Expected: 3 tests OK; salida con `WPT <sha>: N/M passed`; JSON válido con `run_info.wpt_revision` igual al SHA del archivo.

- [ ] **Step 7: Commit**

```bash
git add crates/test_harness/src/wpt crates/test_harness/src/lib.rs crates/test_harness/src/run.rs crates/test_harness/tests/wpt_runner.rs
git commit -m "feat(wpt): add runner over Boa and --wpt report"
```

### Task 4: Gate monotónico y allowlist

**Files:**
- Create: `tools/wpt_gate.py`
- Create: `tests/wpt/allowlist.toml`, `tests/wpt/baseline.json`
- Create: `tools/test_wpt_gate.py`
- Modify: `.github/workflows/ci.yml` (job `wpt` + umbrella `needs`)

**Interfaces:**
- Consumes: `WptReport` JSON de Task 3.
- Produces: exit 0 de `python3 tools/wpt_gate.py --check`, y baseline/allowlist congelados.

- [ ] **Step 1: Tests del gate que fallan**

`tools/test_wpt_gate.py`: (a) `test_new_failure_fails_gate`: baseline con un PASS, reporte con ese mismo subtest FAIL ⇒ `check_gate` devuelve error "status changed"; (b) `test_fix_removing_allowlist_passes`: allowlist con una entrada, reporte con ese subtest PASS y allowlist vacía ⇒ OK; (c) `test_allowlist_cannot_grow`: allowlist con más entradas que el baseline ⇒ error; (d) `test_missing_file_fails`: manifest declara un archivo ausente ⇒ error.

- [ ] **Step 2: Verificar que fallan**

Run: `python3 -m unittest tools.test_wpt_gate -v`
Expected: FAIL — módulo inexistente.

- [ ] **Step 3: Implementar el gate**

`tools/wpt_gate.py` con:
  - `Epoch = (revision_sha, sha256(manifest_bytes))` — ambos leídos de los archivos; cualquier cambio crea época nueva y exige `--rebaseline`.
  - `check_gate(report: dict, baseline: dict, allowlist: dict) -> Result<(), str>`: exige que toda clave del baseline siga en el reporte con el mismo status o mejor (FAIL→PASS permitido, PASS→FAIL no); `len(allowlist) <= len(baseline["allowlist"])`; ninguna clave nueva en allowlist; `passed >= baseline["passed"]`.
  - `rebaseline(report, baseline_path, allowlist_path)`: escribe baseline nuevo con `passed` y la lista de fallos actuales como allowlist, con `reason=""` e `issue=""` para completar a mano.
  - CLI `--check|--rebaseline --report --baseline --allowlist`.
  - Cada entrada de allowlist lleva `key`, `test`, `subtest`, `reason`, `issue`, `spec` y **prohíbe comodines**.

- [ ] **Step 4: Verificar que pasan y generar la época 1**

```bash
python3 -m unittest tools.test_wpt_gate -v
python3 tools/wpt_gate.py --rebaseline --report /tmp/opencode/wpt.json --baseline tests/wpt/baseline.json --allowlist tests/wpt/allowlist.toml
python3 tools/wpt_gate.py --check --report /tmp/opencode/wpt.json --baseline tests/wpt/baseline.json --allowlist tests/wpt/allowlist.toml
```
Expected: 4 tests OK y `--check` exit 0.

- [ ] **Step 5: Añadir job en CI**

En `.github/workflows/ci.yml`, job `wpt` con `timeout-minutes: 10`, checkout sin credenciales, toolchain 1.99.1, pasos `vendor_wpt.py --check` → `cargo run --release --locked -p megi-test-harness -- --wpt --report ...` → `wpt_gate.py --check` → `upload-artifact`. Añadir `wpt` a la lista `needs` del job umbrella `check`.

- [ ] **Step 6: Verificar CI local**

Run: `actionlint .github/workflows/ci.yml` (o `python3 -c "import yaml; yaml.safe_load(...)"` si no está instalado) && `python3 tools/check_docs_links.py`
Expected: sin errores de sintaxis; links OK.

- [ ] **Step 7: Commit**

```bash
git add tools/wpt_gate.py tools/test_wpt_gate.py tests/wpt/baseline.json tests/wpt/allowlist.toml .github/workflows/ci.yml
git commit -m "feat(wpt): monotonic gate, allowlist and CI job"
```

### Task 5: Métrica honesta separada de capability coverage

**Files:**
- Modify: `tools/update_progress.py`
- Modify: `docs/status/progress.toml` (bloque `[wpt]` generado)
- Modify: `docs/status/COVERAGE.md`
- Modify: `PROGRESS.md` (regenerado)
- Test: `tools/test_update_progress.py` o test existente de progress

**Interfaces:**
- Consumes: `WptReport` y `WPT-REVISION.json`.
- Produces: badge `docs/status/badge-wpt.svg`, línea en `PROGRESS.md`: `WPT <sha8> — P/T subtests pasados (dirs: N; F FAIL, TO, NR, E)`.

- [ ] **Step 1: Test que falla**

Test: dado un reporte sintético con `{pass:3, fail:2, timeout:1, notrun:1, error:1}`, `render_wpt_headline(report, epoch)` devuelve exactamente `WPT abc1234 — 3/8 subtests pasados (dirs: 6; 2 FAIL, 1 TIMEOUT, 1 NOTRUN, 1 harness_error)`.

- [ ] **Step 2: Verificar que falla**

Run: `python3 -m unittest tools.test_update_progress -v`
Expected: FAIL en el nuevo test.

- [ ] **Step 3: Implementar**

`render_wpt_headline(report: dict, epoch: str) -> str` en `tools/update_progress.py`; bloque `[wpt]` generado en `progress.toml` con `revision`, `passed`, `total`, `dirs`, `distribution`; badge `badge-wpt.svg` con `WPT 3/8`. Añadir a `docs/status/COVERAGE.md`: "capability coverage y WPT conformance son métricas distintas; nunca se agregan en un solo porcentaje".

- [ ] **Step 4: Verificar**

Run: `python3 tools/update_progress.py --write --no-gate && python3 tools/update_progress.py --check --no-gate && grep -n 'WPT' PROGRESS.md`
Expected: `--check` exit 0; línea WPT presente; el 75/75 sigue existiendo pero sin la palabra "conformance".

- [ ] **Step 5: Commit**

```bash
git add tools/update_progress.py docs/status/progress.toml docs/status/badge-wpt.svg docs/status/COVERAGE.md PROGRESS.md
git commit -m "docs(wpt): publish honest P/T headline separate from capability coverage"
```

### Task 6: Comparación por intersección y generación de issues

**Files:**
- Create: `tools/wpt_compare.py`, `tools/wpt_failures.py`
- Create: `tools/test_wpt_compare.py`
- Test: tests unitarios de intersección y de generación de borradores.

**Interfaces:**
- Consumes: dos `WptReport` con `run_info.wpt_revision`.
- Produces: tabla Markdown de comparación; borradores de issue por clúster de fallo.

- [ ] **Step 1: Tests que fallan**

(a) `test_different_revision_refuses`: reportes con SHA distinto ⇒ exit 2 y mensaje "same wpt_revision required"; (b) `test_intersection_only`: A tiene claves {x,y}, B {y,z} ⇒ solo se reporta `y`; (c) `test_failure_draft_has_spec_and_repro`: borrador contiene `spec`, `expected/actual` y ruta de repro.

- [ ] **Step 2: Verificar que fallan**

Run: `python3 -m unittest tools.test_wpt_compare -v`
Expected: FAIL.

- [ ] **Step 3: Implementar**

`wpt_compare.py --ours A.json --ladybird B.json --intersection-only --format md`: valida igualdad de `wpt_revision`, calcula intersección de `subtest_key`, imprime `N subtests comunes; Megi P, Ladybird L, Δ=L−P; acuerdos PASS/FAIL`. Sin artefacto ⇒ `sin comparación disponible`, exit 0. `wpt_failures.py --report A.json --epoch <sha>`: agrupa fallos por aserción compartida, genera `tests/wpt/repros/<key>.html` y borrador Markdown con SHA, ruta, nombre, expected/actual, sección de spec (vía `tools/whatwg_spec.py`) y etiquetas `wpt, conformance, good-first-issue`.

- [ ] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_wpt_compare -v && python3 tools/wpt_compare.py --ours /tmp/opencode/wpt.json --ladybird /tmp/opencode/ladybird-wpt.json --intersection-only --format md`
Expected: 3 tests OK; tabla Markdown o "sin comparación disponible".

- [ ] **Step 5: Añadir tests al job docs y commit**

Añadir `tools.test_wpt_compare` a la lista de `python3 -m unittest` en `ci.yml`.

```bash
git add tools/wpt_compare.py tools/wpt_failures.py tools/test_wpt_compare.py .github/workflows/ci.yml
git commit -m "feat(wpt): same-revision comparison and failure-to-issue loop"
```

### Task 7: Época 2 (puente getComputedStyle + nuevos directorios)

**Files:**
- Modify: `tests/wpt/WPT-REVISION.json`, `tests/wpt/MANIFEST.json` (rebaseline)
- Create: `crates/js/src/runtime/api/computed_style.rs` o el módulo existente equivalente
- Modify: `crates/test_harness/src/wpt/testharness_shim.js` (API nueva solo si un directorio la exige)

**Interfaces:**
- Consumes: Task 1–6 funcionando; `ComputedStyle` del pipeline de render.
- Produces: `getComputedStyle(el).getPropertyValue(name)` devuelve valores declarados para propiedades soportadas; ~6.000/9.000 en época 2.

- [ ] **Step 1: Test que falla**

Test WPT de `css/css-cascade` que consulta `getComputedStyle(div).color` ⇒ actualmente `TypeError`. Añadir fixture `tests/wpt/fixtures/computed_style_bridge.html` con assertions sobre `color`, `display`, `width`.

- [ ] **Step 2: Verificar que falla**

Run: `cargo run --release -p megi-test-harness -- --wpt --summary` y `python3 tools/wpt_gate.py --check ...`
Expected: el subtest aparece como FAIL/harness_error; el gate no se toca todavía.

- [ ] **Step 3: Implementar el puente**

Exponer `getComputedStyle` en `JsRuntime` con `ComputedStyleProvider` inyectable; devolver solo propiedades con soporte declarado (`color`, `display`, `width`, `font-size`); las demás devuelven `""` y registran warning. Sin calcular estilos nuevos fuera del render.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-js computed_style && cargo run --release -p megi-test-harness -- --wpt --summary`
Expected: tests del puente OK; subtests de computed style pasan.

- [ ] **Step 5: Rebaseline explícito (época 2)**

```bash
python3 tools/vendor_wpt.py --source /tmp/wpt-pin --commit <PIN_SHA> --dirs <época1>,html/dom/documents,html/semantics/document-metadata,html/webappapis/atob,html/webappapis/scripting,css/css-cascade,css/css-values,dom/ranges
python3 tools/wpt_gate.py --rebaseline --report /tmp/opencode/wpt.json --baseline tests/wpt/baseline.json --allowlist tests/wpt/allowlist.toml
```
Expected: `T` crece a ~9.000; `P` ≥6.000; allowlist nueva con `issue` pendiente por entrada, completada en el mismo PR.

- [ ] **Step 6: Commit**

```bash
git add crates/js/src tests/wpt .github
git commit -m "feat(wpt): computed-style bridge and epoch 2 rebaseline"
```
