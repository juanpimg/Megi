# Handoff de sesión — ronda 11 (HITO: árbol HTML5 3.904/3.904, WP-6.4, WP-B y event loop)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `13f3f21` (árbol limpio)
**Estado**: **862 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %) y sin allowlist de tree**; Fase 4 funcionalmente completa, **Fase 5 ≈95 %**, **Fase 6 ≈75-80 % (WP-6.1/6.2/6.4 + WP-A/WP-B HECHOS)**, **Fase 7 ≈65-70 %**.
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md) · [`FUZZING.md`](FUZZING.md) · [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 11 (5 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `3a8c0ac` `feat(js)` | **WP-B + APIs Rust** | `CSSStyleDeclaration` completa (`cssText`/`length`/`item`/`getPropertyValue`/`setProperty`/`removeProperty`), `getComputedStyle` con **provider inyectable**, `NodeList` snapshot, `matches`/`closest`, `insertAdjacentHTML`/`Text` e **identidad de wrappers** (cache wrapper↔nodo). APIs Rust para hosts: **`eval_on_document`**, **`dispatch_simple_event`**, **`take_document`**. **43→46 tests js** (incluye `document` como `EventTarget` en el follow-up). |
| `3e20b94` `feat(html)` | **WP-6.4 + fix de rendimiento** | `html::scripting::{ScriptExecutor, ScriptExecution}`; `TreeBuilder::with_script_executor` y `parse_document_with_options_and_executor`. Los scripts **parser-inserted** se ejecutan en el "text insertion mode" al `</script>` (o EOF) con el DOM parcial visible; `document.write` se **re-tokeniza en la posición actual** con `Tokenizer::insert_input` (splice de code points, reconsume-aware, `line_starts`/UTF-16 ajustados, scripts anidados incluidos). Externos `src` los ejecuta el host; errores no fatales (`ParseOutcome::script_errors`); camino sin scripting **zero-cost**. Incluye el **fix de la regresión `tag-40k-attrs`** (~12,4 ms; dedupe con `std::HashSet`, `FastHasher` word-at-a-time solo en bookkeeping, guard `state == Data` en el batching). **60 tests html**. |
| `32b421d` `test(conformance)` | **Harness con JS** | `js_executor::JsScriptExecutor` sobre `JsRuntime::eval_on_document`: preludio JS que intercepta `document.write`/`writeln` y polyfillea `documentElement.remove()`/`document.appendChild()` (sin tocar `crates/js`), epílogo con marcador extraído y eliminado con `remove_subtree`. `tree_harness` lo activa solo en modo `script-on`. **Gate: árbol 3.904/3.904, las 4 suites `scripted_*` verdes, `KNOWN_TREE_FAILURES=[]`, `TREE_EXPECTED_PASSED=3904`**; test explícito en `closed_loop`. **11 tests test_harness**. |
| `786fbcc` `feat(render)` | **Event loop presupuestado** | Reloj virtual, `DOMContentLoaded`/`load`, timers con delay (`max_tasks` 10.000, `max_virtual_time` 1000 ms), **un solo re-render** tras drenar la cola, stats `script_*`/`timings.event_loop`; **6 fixtures `js_event_*` y 11 tests** → **88 tests render**. |
| `13f3f21` `test(render)` | Follow-up | `DOMContentLoaded` despachado a través del nuevo `EventTarget` del documento (`js` 46 tests). |

## Evidencia verificada

- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, **árbol 3.904/3.904 (100 %)**, serializer **230/230**, CSS Syntax **30/30**; allowlists de tokenizer y de árbol **vacías**. `--strict` exit 1 solo por las diferencias de parse-error documentadas (37 order-only + 1 `xmlViolation`).
- `cargo test --workspace` → **862 tests, 0 fallos**: bench 8, css 57, dom 28, fonts 13+1, html 60, image 45+1, **js 46**, layout 133, net 180, paint 108, **render 88**, shell 83, test_harness 11. fmt, clippy `-D warnings` (workspace), `cargo doc` y MSRV 1.88 OK.
- **Píxeles sin cambios**: showcase 1024×768 **80.220 B `md5 c95c7eff83e7bd08f815631aa42afdb0`**; render basic 15.498 B **`md5 7324e851…`**; Phase-4 `basic.rgba` byte-idéntico (`md5 60f8984e…`).
- **Rendimiento**: `html/tag-40k-attrs` **~12,4 ms** (A/B siempre por debajo del baseline pre-r10) manteniendo las ganancias de la ronda 10 (`html/realistic-1mb` ~19-22 ms según carga); pipeline intercalado **~154-158 ms** (medido 155,19 ms mediana / 148,80 min con load 5-8).
- JS: `js_text.html` 15.876 B `md5 e1da51d4…` con scripting vs 15.499 B `md5 53f67e77…` con `--no-scripting`.

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO: arena generacional, API de navegador | 28 |
| `html` | **COMPLETO**: parser 3.904/3.904, serializador §13.3, hook `ScriptExecutor` (WP-6.4) | 60 |
| `css` | COMPLETO: selectores/cascada/@media/query API, background-image parsing | 57 |
| `layout` | COMPLETO Fase 4: tablas §17, árbol de celdas fijado | 133 |
| `fonts` | COMPLETO (base LTR) | 13 + 1 doctest |
| `paint` | COMPLETO Fase 4 + backgrounds + hit-testing | 108 |
| `net` | Fase 5: TLS/caché/data:/cookies/CORS/gzip | 180 |
| `image` | Decoder PNG + inflate propios | 45 + 1 doctest |
| `js` | **Fase 6 WP-A/B + APIs host**: Boa 0.21.1, eventos/timers, CSSOM/DOM WP-B, `document` EventTarget | 46 |
| `render` | Pipeline URL→PNG + imágenes/backgrounds + scripts + **event loop presupuestado** | 88 |
| `shell` | Fase 7 usable: scroll/entrada/HiDPI/worker + navegación/historial/título + `find_link_at` | 83 |
| `bench` | Benchmarks reproducibles (parseo/estilos/layout/paint/PNG) | 8 |
| `test_harness` | Gate 4 suites (árbol 3.904/3.904 **sin allowlist**) + `JsScriptExecutor` (WP-6.4) | 11 |

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 862 tests, doc y gate
cargo test --workspace                              # 862 tests (con JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo run -p test_harness -- --strict               # exit 1 solo por diffs de parse-error documentadas
cargo test -p html -p test_harness -p js -p render

# WP-6.4: scripts parser-inserted + document.write re-tokenizado (gate)
cargo test -p html --lib scripting                 # 7 tests del hook
cargo test -p test_harness --test closed_loop      # incluye el test de las 4 suites scripted_*

# Event loop presupuestado + fixtures js_event_*
cargo run -p render -- tests/fixtures/render/js_event_timeout.html /tmp/timer.png --json
cargo run -p render -- tests/fixtures/render/js_event_domcontentloaded.html /tmp/dcl.png
cargo test -p render --test event_loop_tests

# Rendimiento del parser (A/B) y pipeline
cargo run --release -p bench -- --filter html
cargo run --release -p bench -- --filter pipeline

# Showcase / fase 4
cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/phase4.png
python3 tools/png_check.py /tmp/phase4.png
```

## Deuda priorizada

1. **`js`**: `onclick`/`once`/`passive`, `currentTarget`/identidad en `removeEventListener`; módulos ES y `async`/`defer` externos; presupuesto de memoria y aislamiento de realms; `MutationObserver`.
2. **`render`**: cablear el **provider de `getComputedStyle`** al pipeline (cascada real) y usar el event loop en páginas reales con `requestAnimationFrame`; optimizar el **PNG encoder** (~59 % del tiempo de render).
3. **Rendimiento**: techo de layout → `LayoutBox.style` como `Arc<ComputedStyle>` (toca layout+paint); bench E2E con JS.
4. **Harness**: feature-gate para que el `--no-default-features` no arrastre Boa vía `test_harness`; mantener 3.904/3.904 como invariante de CI.
5. **Fase 5**: HTTP/2 y streaming, SameSite/PSL, `br`.
6. **Fase 7**: formularios, pestañas, zoom, event loop vivo en el shell y pruebas de ventana con Xvfb.
7. **Fase 4**: `background-repeat/size/position`, colores de `border-collapse`, block-in-inline.
8. **Fuzzing**: targets `render`/`js` además de los 4 actuales.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` es la excepción tipográfica autorizada (§3.1); `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image` std puro; `js` usa **Boa 0.21.1** (MIT, feature `js` default ON, stub sin feature); `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js` para ejecutar los scripts del corpus (follow-up de feature-gate).
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/` y `.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo; el ADR JS ya está aceptado (2026-11-10).
