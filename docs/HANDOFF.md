# Handoff de sesión — ronda 16 (SVG con CSS/gradientes, backgrounds multicapa, APIs de navegador cableadas)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `653626f` (árbol limpio; el sitio del dueño vive en commits propios y queda fuera del scope de esta documentación)
**Estado**: **1046 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; Fase 4 prácticamente completa (SVG y backgrounds sólidos), **Fase 5 ≈95 %**, **Fase 6 ≈90 %** (APIs de navegador cableadas), **Fase 7 ≈70 %**.
**CI/Repo**: GitHub Actions **en verde** (quality, conformance y MSRV; deps Wayland/X11/xcb en el runner); `main` y `sprint/robustez-fase4` sincronizadas en `653626f`. Suite **determinista** (reloj virtual, epoch fijo de rAF, warnings en vez de fallos por errores JS).
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md) · [`FUZZING.md`](FUZZING.md) · [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 16 (4 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `1477050` `feat(svg)` | **CSS interno + gradientes** | `<style>`/class/id/descendientes con cascada (inline > id > clase > tipo > presentación), herencia y CDATA; gradientes `linearGradient`/`radialGradient` completos (stops/offset/color/opacity, `gradientUnits` objectBoundingBox/userSpaceOnUse, `spreadMethod` pad/reflect/repeat, `gradientTransform`, geometría y herencia `href`), usables por `fill`/`stroke` con evaluación determinista por píxel; ciclos ⇒ sin pintura; límites 1 MiB/4096 reglas/256 stops/32 saltos. **`<pattern>` pendiente (documentado)**. svg 41→**60 tests**; fixture+ejemplo `gradient.svg` (120×60, md5 `ff59b80a…`). |
| `5fd4ea2` `feat(js)` | **APIs de navegador** | `matchMedia` con provider, `set_viewport_provider`/`ViewportInfo` (innerWidth/Height/DPR/scroll, scrollTo/scrollBy), `requestAnimationFrame`/`cancelAnimationFrame` a 16 ms virtuales (`pending_animation_frames`), `document.onerror`/`window.onerror` (5 args) + consola (`debug/assert/table/dir`), `document.readyState`+`set_ready_state`, provider de `getComputedStyle` verificado para 24 props. js 58→**71 tests**. Límites: sin `change` automático, lineno/colno=0, rAF fijo 16 ms, scroll lógico del host, sin eventos `resize`/`scroll`. |
| `ac1c330` `feat(css,paint)` | **Backgrounds multicapa + origin/clip/attachment** | `background-origin` border/padding/content, `background-clip` (text parseado, pinta border-box), `background-attachment` scroll/fixed, **`ComputedStyle.background_layers` top-first** (listas cíclicas, shorthand multicapa, color solo en la última capa, espejo legacy para render/goldens), `space`/`round` reales; css 69→**76**, paint 122→**130** tests. Goldens intactos (`background_geometry_explicit`). Límites: `clip:text` no pintado, attachment local no modelado. |
| `653626f` `feat(render)` | **Cableado real de APIs en el pipeline** | Pasada de estilos **pre-script** con provider real de `getComputedStyle` (24 props, fallback inline; snapshot pre-script, `RenderTimings::styles_pre_script`), `matchMedia`/viewport/DPR con MediaContext reales, `readyState` loading→interactive→complete, rAF bombeado por el event loop virtual con **epoch virtual fijo ⇒ 48 ms exactos** (corrige el flake 49 vs 48, PNG idéntico), `document.onerror`/`console.warn|error`/`last_event_error` ⇒ warnings no fatales + `RenderStats.console_errors`, `collect_background_references` recorre `background_layers` (capas 2..N E2E) y caps herméticos. render 114→**123 tests**; multi-capa `background_layers.html` 240×260 md5 `c357cc2c43039ce03545be3b4221a814`. |

## Evidencia verificada

- `cargo test --workspace` → **1046 tests, 0 fallos**: bench 8, **css 76**, dom 28, **fonts 17 + 1 doctest**, html 60, image 45+1, **js 71**, layout 135, net 180, **paint 130**, **render 123**, shell 100, **svg 60**, test_harness 11.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, **árbol 3.904/3.904 (100 %)**, serializer **230/230**, CSS Syntax **30/30**; allowlists vacías. fmt/clippy `-D warnings`/doc/MSRV 1.88 OK.
- **Píxeles/evidencias**: showcase 1024×768 80.220 B `md5 c95c7eff…` intacto; `basic.rgba` `60f8984e…`; PNG phase4 `9e957177…`; **background layers** 240×260 `md5 c357cc2c…`; **gradiente SVG** `ff59b80a…`; rAF `script_virtual_time_ms=48`; `styles_pre_script` ~687 µs con scripts y 0 sin; errores JS ⇒ `console_errors` sin abortar.
- **Rendimiento**: layout ~36,3 ms y pipeline ~120,5 ms (ronda 15, vigente); sin regresión por los cambios de la ronda 16.

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO: arena generacional, API de navegador | 28 |
| `html` | COMPLETO: parser 3.904/3.904, serializador §13.3, hook `ScriptExecutor` | 60 |
| `css` | COMPLETO + `@font-face`/media/query API + backgrounds multicapa/origin/clip/attachment | 76 |
| `layout` | COMPLETO Fase 4: tablas §17, `Arc<ComputedStyle>`, IFC optimizado | 135 |
| `fonts` | COMPLETO LTR + fuentes web @font-face/WOFF2 | 17 + 1 doctest |
| `paint` | COMPLETO Fase 4 + hit-testing + PNG invariante + backgrounds multicapa | 130 |
| `net` | Fase 5: TLS/caché/data:/cookies/CORS/gzip | 180 |
| `image` | Decoder PNG + inflate propios | 45 + 1 doctest |
| `js` | Fase 6: DOM/eventos/timers + **APIs de navegador** (matchMedia/viewport/rAF/onerror/readyState/getComputedStyle) | 71 |
| `svg` | Parser+modelo+rasterizador AA + **CSS interno y gradientes**; `<pattern>` pendiente | 60 |
| `render` | Pipeline URL→PNG + imágenes/backgrounds multicapa/SVG + event loop determinista + webfonts + **APIs de navegador cableadas** | 123 |
| `shell` | Fase 7: scroll/entrada/HiDPI/worker + navegación + zoom/auto-refresh/estado | 100 |
| `bench` | Benchmarks reproducibles | 8 |
| `test_harness` | Gate 4 suites (árbol 3.904/3.904 sin allowlist) + `JsScriptExecutor` | 11 |

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1046 tests, doc y gate
cargo test --workspace                              # 1046 tests (con JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0

# Ronda 16
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/gradient.svg /tmp/gradient.png
cargo run -p render -- tests/fixtures/render/background_layers.html /tmp/layers.png --width 240 --height 260  # c357cc2c…
cargo run -p render -- tests/fixtures/render/browser_api.html /tmp/api.png --json
cargo run -p render -- tests/fixtures/render/raf_animation.html /tmp/raf.png --json    # script_virtual_time_ms=48
cargo test -p svg -p js -p css -p paint -p render

# Rendimiento y goldens
cargo run --release -p bench -- --filter layout --iterations 5     # ~36 ms
cargo run --release -p bench -- --filter pipeline --iterations 3   # ~120 ms
cargo run -p render -- tests/fixtures/render/showcase/index.html /tmp/showcase.png --width 1024 --height 768  # c95c7eff…

# Rustdoc / MSRV
cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets
```

## Deuda priorizada

1. **SVG**: `<pattern>`, `<text>`/fuentes dentro del SVG, filtros y CSS interno que alcance al `<img>` sintético del render.
2. **Fase 6**: módulos ES/`async`/`defer`, contabilidad de memoria real, eventos `resize`/`scroll`, `matchMedia` con `change` automático y `lineno/colno` reales en `onerror`.
3. **Backgrounds**: `background-clip: text` real, `background-attachment` local, `border-image` y pulido de conflictos `border-collapse`.
4. **Layout/perf**: IFC otra vuelta (<35 ms), owned-styles/provider de `getComputedStyle` en el pipeline (ya real en render), PNG por regiones.
5. **Fase 5**: HTTP/2 y streaming, SameSite/PSL, `br`.
6. **Fase 7**: formularios, pestañas, event loop real en el host y verificación de ventana con Xvfb.
7. **Fuzzing/bench**: targets `render`/`js`/`svg`, bench E2E de SVG/backgrounds/APIs y cachear Boa en CI (+25 s).

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust) para WOFF2**; `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image`/`svg` std (+`image` interno en `svg`); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js` para el corpus.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo (ADR JS aceptado 2026-11-10).
