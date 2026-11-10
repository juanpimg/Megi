# Handoff de sesión — ronda 9 (JS ejecutado en render, hit-testing y shell navegable)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `c86682c` (árbol limpio)
**Estado**: **793 tests, 0 fallos**; gate de 4 suites PASS; Fase 4 funcionalmente completa, Fase 5 ≈90 %, **Fase 6 ≈30 % (WP-6.1 + WP-6.2 host HECHOS)**, **Fase 7 ≈60 % (WP-7.5 navegación HECHA)**.
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md) · [`FUZZING.md`](FUZZING.md) · [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 9 (4 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `429a30f` `feat(paint)` | **Fase 4 — hit-testing aditivo** | Módulo `hit_test`: `HitKind`, `HitRegion { rect, node_id, kind }` en **orden de pintado** con clip aplicado, `hit_test`/`hit_test_all` (topmost gana) y `HitLink`/`find_link_at` (sube al `<a href>` HTML, href crudo); builders aditivos `build_display_list_with_hit_regions`/`build_display_list_and_hit_regions`. Las entradas clásicas quedan **byte-idénticas** (golden intacto) y el coste es opt-in. 11 tests nuevos → **108** paint. |
| `4ac56ba` `feat(shell)` | **Fase 7 WP-7.5 — navegación real** | Clic izquierdo en enlace ⇒ resolución con `Url::join` ⇒ render en worker (generación) + historial; `NavigationState` puro (back/forward/replace/reload, cap 256, truncado forward, no-op misma URL, fallos conservan entrada). Atajos: Alt+←/Backspace/b atrás, Alt+→/n adelante, `r` recarga, `q`/Esc cierra. Título dinámico `<title>` + URL corta + % scroll + `[recortado]`; errores «Megi — error: …»; hit regions y título del **mismo** render que los píxeles; redirects HTTP replace. 34 tests nuevos → **80** shell. |
| `c86682c` `feat(render)` | **Fase 6 WP-6.2 host — JS en el pipeline** | `render::ScriptingMode { Enabled, Disabled }` (default Enabled con feature `js`; Disabled con `--no-default-features`), `RenderOptions::{scripting, scripting_limits}`, `ScriptingLimits`/`ScriptBudget::SMALL` y CLI `--no-scripting`. Descubre `<script>` clásicos en orden (inline/`src`; módulos avisados y saltados, `nomodule` ignorado), carga externos con el ResourceLoader (`--no-network`, dedupe, caps 8 MiB/script, 32 MiB total, 64 scripts), ejecuta con `js::JsRuntime::with_document` con budget por script + total, drena microtasks y hasta 10.000 timers, captura console (`scripts_executed/failed/bytes/console` + `timings.scripting`) y **recalcula estilos→layout→paint sobre el DOM mutado** (mutaciones visibles en el PNG). 7 fixtures `js_*` y 12 tests → **67** render. |
| `40817d9` `docs` | Cierre ronda 8 | Sincronización de README/ROADMAP/ARCHITECTURE/HANDOFF/PHASE4-SUPPORT/TOOLING/SECURITY/CONTRIBUTING/CHANGELOG con la ronda 8 (734 tests, backgrounds E2E, golden `c95c7eff…`). |

## Evidencia verificada

- `cargo test --workspace` → **793 tests, 0 fallos**; fmt, clippy `-D warnings`, `cargo doc` y MSRV 1.88 OK.
- Gate de conformidad: tokenizer **7.050/7.050**, árbol **3.898/3.904** (allowlist `scripted_*` hasta WP-6.4), serializador **230/230**, CSS Syntax **30/30**; `--summary` exit 0.
- **JS en render**: `js_text.html` con scripting 15.876 B `md5 e1da51d432804fd540792d40398b72eb` vs `--no-scripting` 15.499 B `md5 53f67e77a335a8b024e0896497e63999`. Showcase golden intacto: 1024×768, 80.220 B, `md5 c95c7eff83e7bd08f815631aa42afdb0`. Basic fixture: 15.498 B, `md5 7324e8512e831de132b1172965332a73`. Phase-4 `basic.rgba` byte-idéntico (`md5 60f8984e…`).
- **Hit-testing**: `paint` 108 tests; `basic.rgba` y PNG del showcase sin cambios byte a byte.
- **Shell**: 80 tests (incluye `NavigationState`, título, worker; ventana real no testeada en CI por falta de servidor gráfico).

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO: arena generacional, API de navegador | 28 |
| `html` | COMPLETO: parser, serializador §13.3, `innerHTML`/`document.write` | 53 |
| `css` | COMPLETO: selectores/cascada/@media/query API, background-image parsing | 57 |
| `layout` | COMPLETO Fase 4: tablas §17, árbol de celdas fijado | 133 |
| `fonts` | COMPLETO (base LTR) | 13 + 1 doctest |
| `paint` | COMPLETO Fase 4 + backgrounds + **hit-testing** | 108 |
| `net` | Fase 5: TLS/caché/data:/cookies/CORS/gzip | 180 |
| `image` | Decoder PNG + inflate propios | 45 + 1 doctest |
| `js` | **Fase 6 WP-6.1**: Boa 0.21.1 + bindings DOM + budgets; consumido por `render` | 12 |
| `render` | Pipeline URL→PNG + imágenes/backgrounds + **scripts E2E** | 67 |
| `shell` | Fase 7 usable: scroll/entrada/HiDPI/worker + **navegación/historial/título** | 80 |
| `bench` | Benchmarks reproducibles (parseo/estilos/layout/paint/PNG) | 8 |
| `test_harness` | Gate de conformidad 4 suites + allowlist | 7 |

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 793 tests, doc y gate
cargo test --workspace                              # 793 tests (con JS por defecto)
cargo test --workspace --no-default-features        # stub sin Boa (build mínimo)
cargo test -p js && cargo test -p js --no-default-features
cargo test -p paint && cargo test -p render && cargo test -p shell

# JS en el pipeline (mutaciones visibles en el PNG)
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_on.png
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_off.png --no-scripting
md5sum /tmp/js_on.png /tmp/js_off.png               # e1da51d4… vs 53f67e77…

# Hit-testing (API aditiva; PNG/golden no cambian)
cargo test -p paint --test hit_testing

# Shell navegable (controles: clic izq enlace, Alt+←/→, b/n, r, q/Esc)
cargo run -p shell -- tests/fixtures/render/showcase/index.html
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/shot.png --width 640 --height 400

# Showcase / fase 4
cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/phase4.png
python3 tools/png_check.py /tmp/phase4.png
./tools/bench.sh --scale 0.1
```

## Deuda priorizada

1. **WP-A (js)**: `EventTarget`/eventos y timers de reloj real (hoy sólo scheduler base y microtasks); **WP-B (js)**: CSSOM/`getComputedStyle` y utilidades; caché de identidad de wrappers.
2. **WP-6.4 (render+js)**: ejecución **durante el parsing** (`parser-inserted` + `document.write` con re-tokenización) para sacar los `scripted_*` de la allowlist (criterio árbol **3.904/3.904**); luego `async`/`defer` y módulos ES avisados.
3. **Cablear `paint::find_link_at` en `render`** (`RenderOutput.hit_regions`) para que el shell use la API de paint (follow-up de una función; PNG/golden no cambian).
4. **Rendimiento**: parseo+layout (baseline en `docs/PERFORMANCE.md`), fast path de tablas ya en ~59 ms.
5. **Fase 4**: `background-repeat/size/position`, colores de conflictos `border-collapse`, block-in-inline.
6. **Fase 5**: HTTP/2 y streaming, CORS completo (SameSite/PSL), `br`.
7. **Fase 7**: formularios, pestañas, zoom, JS en el bucle del host, tests de ventana con Xvfb.
8. **Fuzzing**: targets `render`/`js` además de los 4 actuales (`html_parse`, `css_parse`, `selectors`, `net_url`).

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` es la excepción tipográfica autorizada (§3.1); `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image` std puro; `js` usa **Boa 0.21.1** (MIT, feature `js` default ON, stub sin feature); `render` no añade deps externas; `shell` usa `winit`/`softbuffer`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/` y `.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo; el ADR JS ya está aceptado (2026-11-10).
