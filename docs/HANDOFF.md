# Handoff de sesión — ronda 20 (fix custom properties, conic/hints/OKLab, calc mixto, tipografía/sticky, pestañas y bench E2E)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `c6b66b1`
**Estado**: **1344 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; Fase 4 prácticamente completa, **Fase 5 ≈97 %**, **Fase 6 ≈90 %**, **Fase 7 ≈80 %** (pestañas + find-in-page).
**CI/Repo**: GitHub Actions verde; `main` y `sprint/robustez-fase4` sincronizadas en `c6b66b1`; fuzz PR build-only desde la ronda 18; suite determinista.
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`ROADMAP.md`](ROADMAP.md) · [`ARCHITECTURE.md`](ARCHITECTURE.md) · [`TOOLING.md`](TOOLING.md) · [`FUZZING.md`](FUZZING.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md).

## Qué se hizo en la ronda 20 (4 commits + docs perf previos)

| Commit | Bloque | Resultado |
|---|---|---|
| `a7e32d5` `feat(css,paint)` | **Fix perf custom props + conic/hints/OKLab + calc + tipografía** | **Fix bloqueante**: `ComputedStyle.custom_properties` pasa a `Option<Arc<HashMap>>` lazy/COW (None si no hay `--*` ni ancestros; clon solo al añadir propios) y `compute()` usa `custom::needs_resolution` (escaneo de bytes sin allocar de `--`/`var(`) + `contains_var` sin lowercase por declaración. A/B intercalado: `css/resolve-styles-10k-nodes` 46,8→39,8 ms (**−14,9 %** en calma; −9,4 % con carga) y `pipeline/html-to-png-800x600` 159,0→153,6 ms (**−3,4 %**; −6,1 % con carga); UA-cache probada y revertida (+16 %); presupuestos intactos. **Conic**: `conic-gradient`/`repeating-conic` (`from`, `at`, stops angulares, hard stops, alpha, wrap), color hints y espacios `in srgb|srgb-linear|oklab` (default sRGB byte-idéntico); caps 64 stops/256; raster en `DisplayItem::Gradient` con size/position/repeat/origin/clip/radius/clip:text. css **175**, paint **175** tests. |
| `feade5c` `feat(layout)` | **calc mixto + sticky + tipografía** | Tabla lateral `CalcLengths` (Length intacto) resuelta por eje: width/height/min/max, margins/paddings (shorthands), insets, gaps, background-position/size y border-radius; base indefinida ⇒ porcentaje 0; calc anidado em/rem/vh/vw; `font-size` solo absoluto documentado. `position: sticky` headless (slot de flujo + offsets top>bottom/left>right, sin afectar hermanos, hook de scroll documentado); `text-transform` uppercase/lowercase/capitalize durante shaping sin mutar el DOM; `word-spacing`/`letter-spacing` heredados; `white-space: pre-wrap/pre-line`. layout 173→**196** tests. |
| `43d62e3` `feat(shell)` | **Pestañas** | `tabs.rs` puro: `Tab { navigation, title, url, scroll, zoom, find }`; `TabsState { new, from_urls, open_tab, activate, next, previous, close }`, MAX 32, cerrar la última = New Tab blank (`CloseOutcome`); `Ctrl+T/W/Tab/Shift+Tab`, `Alt+1..9`, título `[N/M]`, `--tabs URL` repetible y frames por generación sin mezclar pestañas. shell 125→**140** tests; límites: sin barra visual/reordenar/sesión/aislamiento/reopen. |
| `c6b66b1` `test(bench)` | **Filas E2E del bench** | `pipeline/gradients-800x600` (54,1 ms), `pipeline/grid-800x600` (7,2 ms), `css/vars-calc-10k` (107,5 ms), `paint/clipping-800x600` (53,1 ms) y `layout/sticky-text-1mb` como **SKIP visible** (el motor ya implementa sticky/text-transform; activación pendiente). Filtros en el rustdoc del bench y en `tools/bench.sh`. |

## Evidencia verificada

- `cargo test --workspace` → **1344 tests, 0 fallos**: css **175**, layout **196**, paint **175**, render **136**, shell **140**, net **207**, svg **73**, html **60**, js **71**, dom **28**, image 45 + 1 doctest, fonts 17 + 1 doctest, bench **8**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, **árbol 3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **A/B del fix** (binarios pre/post del mismo árbol, intercalado): css resolve-10k 46,8→39,8 ms (**−14,9 %** mediana en calma; 53,9→48,8 = −9,4 % con carga) y pipeline 159,0→153,6 ms (**−3,4 %**; 200,8→188,5 = −6,1 % con carga); microbench 60,7→46,1 ms (−24 %); prueba de suelo: desactivar la maquinaria de customs da ≈ el lazy actual ⇒ ya no está en el hot path. Layout sin cambio (51,46→51,18 ms, ruido). Objetivos absolutos (~34 ms css, pipeline <140) no reproducibles en la máquina cargada de la sesión; re-baseline limpia asignada al carril de performance. **No se subieron presupuestos.**
- **Fixtures md5 verificados renderizando**: `conic_gradients` `fff4b8df…`, `conic_repeating` `547c9958…`, `calc_width` `8c1351e3…`, `calc_margin` `e02a96d8…`, `calc_gap` `58a9c51a…`, `sticky_offsets` `08d9a63b…`, `text_transform` `5935cf43…`, `word_spacing` `a9fcad22…`, `white_space` `c4134d67…`. Goldens antiguos intactos (showcase `c95c7eff…`, basic `7324e851…`, PNG `9e957177…`/`60f8984e…`).
- **Pendiente de commit (test-only, verificado)**: `crates/js/tests/wp_a_events_timers.rs` — el test `timers_run_in_due_then_insertion_order` usaba `run_until(10)` absoluto tras un `advance(ZERO)` cuyo retorno puede ser >0 ms bajo carga; ahora usa `base + 10` (5/5 ejecuciones verdes). No entra en el commit de docs; el lead puede incluirlo al integrar.

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1344 tests, doc y gate
cargo test --workspace                              # 1344 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 20
cargo test -p css --test gradients --test calc_lengths --test typography_props
cargo run -p render -- tests/fixtures/render/conic_gradients.html /tmp/conic.png --json        # fff4b8df…
cargo run -p render -- tests/fixtures/render/conic_repeating.html /tmp/conic_rep.png --json    # 547c9958…
cargo run -p render -- tests/fixtures/render/calc_width.html /tmp/calc_width.png --json        # 8c1351e3…
cargo run -p render -- tests/fixtures/render/sticky_offsets.html /tmp/sticky.png --json        # 08d9a63b…
cargo run -p shell -- tests/fixtures/render/basic.html --tabs https://example.org --screenshot /tmp/tabs.png --width 640 --height 400
cargo run --release -p bench -- --filter pipeline/gradients
cargo run --release -p bench -- --filter pipeline/grid
cargo run --release -p bench -- --filter css/vars-calc
cargo run --release -p bench -- --filter paint/clipping
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS (arena + parser 3.904/3.904) | 28 / 60 |
| `css` | F2+F4 + var/calc/pseudos + gradientes **conic/hints/OKLab** + **custom props lazy/COW** | **175** |
| `layout` | F4 completa + flex/grid + **calc mixto, sticky, tipografía** + IFC | **196** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping + **conic/OKLab** | **175** |
| `net` / `image` | F5 (TLS/caché/SameSite/PSL/Referrer/Range) / decoder PNG | **207** / 45 + 1 doctest |
| `js` | F6: DOM/eventos/timers + APIs de navegador (sin cambios r20) | 71 |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text | 73 |
| `render` | Pipeline URL→PNG + imágenes/SVG/webfonts + event loop | **136** |
| `shell` | F7 + zoom/refresh/forms/find-in-page + **pestañas** | **140** |
| `bench` / `test_harness` | Tooling (filas E2E) / gate 4 suites | 8 / 11 |
| `fuzz` (workspace aparte) | 7 targets + corpus + job PR build-only (sin cambios r20) | build 7/7 |

## Deuda priorizada

1. **Perf**: re-baseline en calma y verificar pipeline ≤140 ms; si sigue alto, atacar base de estilos/layout (el fix de customs ya no está en el hot path). Activar `layout/sticky-text-1mb` cuando se quiera medirlo E2E.
2. **CSS/paint**: `display-p3`/espacios de color avanzados, `image-set()`, `border-image`, filtros; `calc()` mixto en más propiedades.
3. **Grid/layout**: `dense`, zonas/`grid-template-areas`, auto-fit real, solapes explícitos; block-in-inline y colores de `border-collapse`; sticky con scroll real.
4. **Shell**: barra visual de pestañas, reordenar/sesión/reopen, aislamiento entre pestañas, widgets nativos/select, validación/submit, event loop vivo multi-frame.
5. **Red/JS**: HTTP/2, streaming, PSL completa, multi-range; módulos ES, memoria real, eventos resize/scroll/change.
6. **Fuzz/bench**: campañas más largas en CI, cache de Boa en CI y bench E2E de find/sticky.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo (ADR JS aceptado 2026-11-10). `docs/PERFORMANCE.md` y `docs/TOOLING.md`/`ci.yml` (progress status, task-129) los mantienen otros agentes.
