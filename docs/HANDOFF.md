# Handoff de sesión — ronda 19 (gradientes CSS, grid avanzado, render referrer/state y find-in-page)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `d9e07e0`
**Estado**: **1260 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; Fase 4 prácticamente completa (gradientes, grid avanzado, decoraciones, clipping, clip:text, var/calc/pseudos), **Fase 5 ≈97 %**, **Fase 6 ≈90 %**, **Fase 7 ≈78 %** (find-in-page).
**CI/Repo**: GitHub Actions en verde; `main` y `sprint/robustez-fase4` sincronizadas en `d9e07e0`; fuzz PR build-only desde la ronda 18; suite determinista.
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`ROADMAP.md`](ROADMAP.md) · [`ARCHITECTURE.md`](ARCHITECTURE.md) · [`TOOLING.md`](TOOLING.md) · [`FUZZING.md`](FUZZING.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md).

## Qué se hizo en la ronda 19 (6 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `5c2115d` `docs` | **Docs ronda 18** | Sincronización de la documentación de la ronda 18 (fuzz, clipping, grid MVP, var/calc, red). |
| `3c82cc5` `feat(css,paint)` | **Gradientes CSS E2E** | `linear-gradient` (ángulos deg/grad/rad/turn, `to side/corner`), `radial-gradient` (circle/ellipse, closest/farthest, radios, `at position`) y `repeating-*` en capas de `background-image` mezclables con `url()`; stops con alpha/`currentColor`/0-2 posiciones/hard stops, caps **64 stops / 256 gradientes** por estilo, shorthand `background`; paint `DisplayItem::Gradient` por tile con AA/sRGB determinista respetando size/position/repeat(space/round)/origin/clip/radius/clip:text/opacidad. css 128→**142**, paint 150→**162**; `gradient_linear` 240×220 `md5 6a04fdc6d2e699d82523ea23a9d7f561`; `gradient_radial`; goldens intactos. |
| `3d052b3` `feat(layout)` | **Grid avanzado** | Placement explícito `grid-column/row` + longhands con líneas 1-based/negativas, `span N`, `auto`; auto-placer salta ocupadas y crece tracks implícitos; spans = tracks+gaps; `minmax()` (px/%/auto min; px/%/fr/auto max) y `repeat(auto-fill|auto-fit)` (auto-fit = auto-fill documentado, cap **1.024**); `order` estable en grid/flex. layout 158→**173**; `grid_span` `md5 0c3764063f85443939eb3b608a779187` y `grid_autofill` `d2255ca3ad99ad4a244abd4e5ef8727b` con bboxes verificados; solapes explícitos no soportados. |
| `aef6df1` `feat(render)` | **Referrer-Policy/SameSite + estado CSS real** | `ReferrerConfig` (header > `meta` > default `strict-origin-when-cross-origin`) instalado en el loader **antes de subrecursos** ⇒ `Referer` correcto y SameSite automático en cookies; nueva `RenderStats.referrer_policy` en `--json`; 6 tests loopback. Resolución CSS bajo `css::state::with_state` pre/post-script con `:checked/:disabled/:enabled/:read-only/:required/:placeholder-shown` y `data-megi-*` (`data-megi-focused` desde el shell; render sin foco; hover/active sólo por atributo); fixture `css_state.html` y 5 tests con bordes/fondos visibles en PNG; 2 tests de integración de gradientes. render 123→**136**. |
| `abcd8fa` `feat(shell)` | **Find-in-page** | `Ctrl+F` case-insensitive sobre nodos Text (`find.rs` puro: `FindMatch`/`search`/`highlight_rects`/`FindState`/`scroll_offset_for`/`overlays`), Enter siguiente/Shift+Enter anterior con wrap, Esc cierra, scroll centra el hit; highlight amarillo `rgba(255,235,59,0.45)` y actual `0.75` pintado en framebuffer **sin re-render** al cambiar de hit y en headless con `--find QUERY --screenshot`; navegación F5/g reload, Alt+Home, Ctrl+D; worker `RenderRequest.find_query`/`find_matches`/`find_rects` generacional. shell 108→**125** (+17); `find.html`; basic golden `7324e851…` intacto. |
| `d9e07e0` `test(render)` | **Fixture css_state + docs de fixtures** | Añade `tests/fixtures/render/css_state.html` y documenta las fixtures nuevas (gradientes, grid avanzado, estado CSS) en el README de fixtures. |

## Evidencia verificada

- `cargo test --workspace` → **1260 tests, 0 fallos** (salida real: bench 8, **css 142**, dom 28, **fonts 17 + 1 doctest**, html 60, image 45 + 1 doctest, js 71, **layout 173**, **net 207**, **paint 162**, **render 136**, **shell 125**, svg 73, test_harness 11).
- Gate verificado con `cargo run -p test_harness -- --summary` (exit 0): tokenizer **7.050/7.050**, **árbol 3.904/3.904**, serializer **230/230**, CSS Syntax **30/30**; allowlists tokenizer/tree **(none)**.
- **Evidencia de píxeles comprobada en esta ronda**: `gradient_linear` 240×220 `md5 6a04fdc6d2e699d82523ea23a9d7f561`; `grid_span` `md5 0c3764063f85443939eb3b608a779187`; `grid_autofill` `md5 d2255ca3ad99ad4a244abd4e5ef8727b`; `css_state` renderizado y PNG válido; find headless `--find "link" --screenshot` renderiza y valida (`png_check` OK). Goldens antiguos (showcase `c95c7eff…`, basic `7324e851…`, phase4 `9e957177…`/`60f8984e…`) intactos según la suite.
- **Fuzz**: sin cambios en la ronda 19; la matriz de 7 targets, corpus y job PR build-only de la ronda 18 sigue vigente.
- **Rendimiento**: layout ~36,3 ms y pipeline ~120,5 ms (vigente; la ronda 19 no altera fixtures sin gradientes/grid avanzado).

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1260 tests, doc y gate
cargo test --workspace                              # 1260 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 19
cargo test -p css --test gradients && cargo test -p paint --test gradients
cargo run -p render -- tests/fixtures/render/gradient_linear.html /tmp/grad_linear.png --width 240 --height 220  # 6a04fdc6…
cargo run -p render -- tests/fixtures/render/gradient_radial.html /tmp/grad_radial.png --json
cargo test -p layout --test grid                    # 26 tests (spans/minmax/auto-fill/order)
cargo run -p render -- tests/fixtures/render/grid_span.html /tmp/grid_span.png --json       # 0c376406…
cargo run -p render -- tests/fixtures/render/grid_autofill.html /tmp/grid_auto.png --json   # d2255ca3…
cargo test -p render --test referrer_policy_tests && cargo test -p render --test css_state_tests
cargo run -p render -- tests/fixtures/render/css_state.html /tmp/css_state.png --json
cargo test -p shell --test shell_tests              # find-in-page (+17)
cargo run -p shell -- tests/fixtures/render/find.html --find "link" --screenshot /tmp/find.png --width 640 --height 400
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS (arena + parser 3.904/3.904) | 28 / 60 |
| `css` | COMPLETO F2+F4 + var/calc/pseudos + **gradientes** | **142** |
| `layout` | F4 completa + Flexbox + **grid avanzado** + IFC | **173** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds + decoraciones + clipping + **gradientes** | **162** |
| `net` / `image` | F5 (TLS/caché/SameSite/PSL/Referrer/Range) / decoder PNG | **207** / 45 + 1 doctest |
| `js` | F6: DOM/eventos/timers + APIs de navegador | 71 |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text | 73 |
| `render` | Pipeline URL→PNG + imágenes/SVG/webfonts + **referrer/state/event loop** | **136** |
| `shell` | F7 + zoom/refresh/forms/navegación + **find-in-page** | **125** |
| `bench` / `test_harness` | Tooling / gate 4 suites | 8 / 11 |
| `fuzz` (workspace aparte) | 7 targets + corpus + job PR build-only (sin cambios r19) | build 7/7 |

## Deuda priorizada

1. **CSS/paint**: `conic-gradient`/color hints/espacios de color avanzados, `image-set()`, `border-image`, filtros; `calc()` mixto `%+px`.
2. **Grid**: `dense`, zonas/`grid-template-areas`, auto-fit real, solapes explícitos; block-in-inline y colores de `border-collapse`.
3. **Render**: snapshot de estado CSS por render (sin hover/active), SameSite top-level aproximado; PSL completa.
4. **Shell**: pestañas, barra visual de búsqueda, validación/submit, widgets nativos/select, event loop vivo multi-frame.
5. **Red**: HTTP/2, streaming, multi-range/If-Range, `br`.
6. **Fuzz/bench**: campañas más largas en CI, bench E2E de gradientes/grid/find, cache de Boa en CI.
7. **IFC/PNG**: regiones y más optimización de pipeline (coordinado con el carril de rendimiento).

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo (ADR JS aceptado 2026-11-10). `docs/PERFORMANCE.md` lo mantiene otro agente.
