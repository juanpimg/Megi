# Handoff de sesión — ronda 7 (query APIs, shell usable, tablas avanzadas, @import+showcase, ADR JS)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `c002ec0` (árbol limpio)
**Estado**: **602 tests, 0 fallos**; gate de 4 suites PASS; Fase 4 ≈100 % funcional, Fase 5 ≈80 %, **Fase 6 con decisión ACEPTADA (2026-11-10) e implementación en curso (WP-6.1)**, Fase 7 ≈40 %.
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md) · [`FUZZING.md`](FUZZING.md) · [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 7 (6 commits + ADR)

| Commit | Bloque | Resultado |
|---|---|---|
| `e5b0b2a` `feat(css)` | Query APIs + background-image | `css::query::{query_selector, query_selector_all, matches}` (+`try_*` con `SelectorParseError`), árbol en preorden, `:scope` no soportado; `ComputedStyle` gana `background_image`/`list_style_image` (`url(...)`, `none`, shorthand color+imagen; gradientes → `None`). **57 tests** css. El pintado de background aún no existe. |
| `ae95d07` `feat(shell)` | Shell usable | **Scroll vertical** (rueda/touchpad, ↑↓, PageUp/Down, Home/End; clamp y cap 8192 CSS px con marca `[recortado]`), **entrada** (clic imprime URL; r re-render; q/Esc/close salir), **HiDPI** (CSS viewport = physical/scale, nearest-neighbour; re-render al cambiar de monitor) y **render en worker** con generación/cancelación/coalescing (debounce 120 ms, `EventLoopProxy`, sin polling). `--screenshot` headless intacto (640×400 `md5 7324e851…`). **46 tests** shell. |
| `5978984` `feat(layout)` | Tablas avanzadas | `rowspan` (u16, matriz de ocupación, reparto bottom-up), `table-layout: fixed`, `inline-table` atómico (baseline de la primera fila) y `border-collapse` con resolución de conflictos (`hidden > width > estilo > origen`; colores no resueltos; borde ganador en ambas celdas), más **fast path** para `separate + auto + sin rowspan` (`layout/realistic-1mb` ~59 ms). **129 tests** layout; golden paint byte-idéntico. |
| `4d64917` `fix(paint)` | Underflow de cobertura | Corrige el underflow de cobertura parcial en `fill_rect` para rects <1 px/coordenadas fraccionales; 4 tests de regresión, golden sin cambios; bench paint 0,88→0,83 ms. **88 tests** paint. |
| `c002ec0` `feat(render)` | `@import` + media + showcase | `@import` **recursivo** (resolución relativa a la hoja, dedup/ciclos, profundidad 8, caps), **media real** en `<link media>`/`<style media>`/`@import media` (viewport screen) y stats de imports/media. Nuevo **showcase** `tests/fixtures/render/showcase/` (CSS externo + `@import` + media + 2 PNG + tabla + stage absoluto): **1024×768, 65.151 B, `md5 570aa554c88c2fdf75481e8cd076bf61`** determinista (file:// y HTTP). **40 tests** render. Limitación visible: el texto de celdas de tabla aún no se pinta; `background-image` tampoco. |
| `ada4ffd` `docs` | ADR JavaScript (Fase 6) **ACEPTADO** | [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) (ADR-001 **ACEPTADO 2026-11-10**): motor **Boa 0.21.x** (pure Rust, MSRV 1.88, Unlicense OR MIT), **feature `js` ON por defecto** (job CI `--no-default-features` para el build mínimo) y **alcance completo** (`document.write`+bindings+eventos+timers+CSSOM+`querySelector`, WP-6.1–6.7, ≈25–30 días). Bindings por tabla lateral `NodeId↔JsValue`/realm. **Fase 6 en implementación (WP-6.1 en curso)**; `crates/js` aún no existe en este HEAD. |

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO: arena generacional, API de navegador | 28 |
| `html` | COMPLETO: parser, serializador §13.3, `innerHTML`/`document.write` | 53 |
| `css` | COMPLETO + query API + `@import`/media + `background-image` parsing | 57 |
| `layout` | COMPLETO Fase 4: tablas avanzadas (rowspan/fixed/inline-table/collapse) + fast path | 129 |
| `fonts` | COMPLETO (base LTR) | 13 + 1 doctest |
| `paint` | COMPLETO Fase 4 + imágenes; fix underflow | 88 |
| `net` | Fase 5: URL/HTTP1.1/TLS/caché/`data:`/file/MIME/loader | 85 |
| `image` | Decoder PNG + inflate propios | 45 + 1 doctest |
| `render` | Pipeline URL→PNG + imágenes + `@import`/media + showcase | 40 |
| `shell` | Fase 7 usable: scroll/entrada/HiDPI/worker + headless | 46 |
| `bench` | Benchmarks deterministas + baseline | 8 |
| `test_harness` | Gate de 4 suites + allowlists | 7 |

**Total workspace: 602 tests, 0 fallos.** MSRV Rust 1.88; `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (4 targets, CI nocturna).

## Comandos de verificación exactos

```bash
export CARGO_HOME=/home/jp/Escritorio/Proyectos/.cargo-home   # sólo en el sandbox del equipo
cd /home/jp/Escritorio/Proyectos/Megi

./tools/dev.sh all                                  # fmt, clippy, 602 tests, doc y gate
cargo +1.88 check --workspace --all-targets         # MSRV (incluye shell/winit)
cargo test --workspace                              # 602 tests (con JS por defecto)
cargo test --workspace --no-default-features        # build mínimo sin JS (CI)
cargo run -p test_harness -- --summary              # gate 4 suites: PASS, exit 0
cargo run -p test_harness -- --summary --strict     # exit 1 por fallos conocidos

# Showcase de navegador real (1024×768, golden determinista)
cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768
python3 tools/png_check.py docs/images/showcase.png  # 65.151 B; md5 570aa554…
cargo test -p render

# Shell: ventana usable o screenshot headless
tools/megi.sh tests/fixtures/render/showcase/index.html      # ventana (X11/Wayland)
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/megi-shot.png --width 640 --height 400
md5sum /tmp/megi-shot.png                                     # 7324e851… (idéntico a render)

# Pipeline Fase 4/imágenes, red, rendimiento y fuzz
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
cargo test -p net -p image
./tools/bench.sh
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
```

Evidencia: showcase **65.151 B `md5 570aa554c88c2fdf75481e8cd076bf61`**; basic shell/render **15.498 B `md5 7324e851…`**; Phase-4 golden `basic.rgba` `md5 60f8984e…` (sin cambios); gate PASS.

## Deuda técnica priorizada (siguiente ronda)

1. **`background-image` de extremo a extremo**: el parseo y `crates/image` ya existen; falta fetch (net, caps), decodificación y pintado con `background-repeat/position/size` básicos.
2. **Texto de celdas de tabla en paint**: el showcase evidencia que la tabla sale como grid vacío; revisar el recorrido de cajas de tabla en `paint` (no hay celdas con contenido pintadas) y añadir golden que lo cubra.
3. **Fase 7 restante**: **hit-testing** (`NodeId`/`LayoutBox` ↔ `DisplayItem`), navegación/historial, eventos DOM/foco, zoom y pulido de HiDPI; pruebas de ventana reales en CI con Xvfb.
4. **Fase 6 (JS) — EN IMPLEMENTACIÓN (decisión ACEPTADA 2026-11-10)**: crear `crates/js` y ejecutar WP-6.1–6.7 con **Boa 0.21.x**, feature `js` **ON por defecto** (mantener job CI `--no-default-features` para el build mínimo) y alcance completo; hito clave WP-6.3: `scripted_*` fuera de la allowlist y árbol 3.904/3.904.
5. **Red avanzada (Fase 5)**: CORS/`Referrer-Policy`, cookies, `Content-Encoding` gzip/br, HTTP/2, streaming y proxies.
6. **Rendimiento**: optimizar parseo+layout (presupuesto del pipeline), tablas grandes, compresión PNG en páginas grandes y glyph cache global.
7. **Fuzz/seguridad**: targets `image` (PNG/inflate) y `render`; mantener caps de imágenes/red; revisar la superficie del shell (winit/softbuffer, worker).
8. **Fixtures/goldens**: `real_page.html`/`torture.html` con tablas con texto, imágenes y CSS externo; goldens con tolerancia para AA.
9. **Robustez layout**: construcción profunda sigue recursiva (sólo el `Drop` es iterativo); `ParseOptions::max_nodes` (1M) vs `MAX_SLOTS` (1.048.575).
10. **Documentación**: mantener README/ROADMAP/ARCHITECTURE/HANDOFF/PHASE4-SUPPORT/TOOLING/SECURITY/CONTRIBUTING/CHANGELOG y el ADR sincronizados por ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` es la excepción tipográfica autorizada (§3.1); `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image` es std puro; `render` sin deps externas nuevas; `shell` usa `winit`/`softbuffer` autorizados.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/` y `.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3; el ADR JS sigue pendiente de aprobación del dueño.
