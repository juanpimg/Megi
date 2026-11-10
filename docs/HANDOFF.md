# Handoff de sesión — ronda 18 (fuzz ampliado, clipping/clip:text, grid, CSS moderno y red avanzada)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `718d075`
**Estado**: **1189 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; Fase 4 prácticamente completa (grid, clipping, `background-clip:text`, var/calc/pseudo-clases), **Fase 5 ≈97 %**, **Fase 6 ≈90 %**, **Fase 7 ≈75 %**.
**CI/Repo**: GitHub Actions en verde (quality, conformance y MSRV) + nuevo job **fuzz PR build-only** (no bloqueante); `main` y `sprint/robustez-fase4` sincronizadas en `718d075`; suite determinista (reloj virtual, sin flake).
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`ROADMAP.md`](ROADMAP.md) · [`ARCHITECTURE.md`](ARCHITECTURE.md) · [`FUZZING.md`](FUZZING.md) · [`TOOLING.md`](TOOLING.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md).

## Qué se hizo en la ronda 18 (5 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `b565ee0` `test(fuzz)` | **Fuzz render/JS/SVG + CI** | 3 targets nuevos (`render_pipeline` con data: URL/caps, `js_eval` con `Budget` + DOM APIs + dispatch, `svg_parse` con `Limits` estrictos), seeds 4/6/5, matrix de **7 targets** en `fuzz-nightly.yml` y job PR build-only (`cargo +nightly fuzz build`). Smoke local 7/7 sin panics/ASan/UBSan; Boa 0.21 tiene una allocation one-time constante (~56 B) ⇒ `-detect_leaks=0` sólo en `js_eval`. [`FUZZING.md`](FUZZING.md) actualizado. |
| `dc0a056` `feat(css)` | **CSS moderno + grid parsing + clip alias** | **Custom properties/`var()`** (`custom.rs`: cascada/herencia/`initial|unset|inherit`, fallback anidado, ciclos, límite 32; shorthands e inline tras sustituir), **`calc()`** (`calc.rs`: + − * /, paréntesis/anidado, px/em/rem/%/vh/vw; mezcla %+px ⇒ invalid documentado), **pseudo-clases de estado** (`state.rs`: `:focus`, `:focus-visible`, `:hover`, `:active`, `:checked`, `:disabled`, `:enabled`, `:read-only`, `:read-write`, `:required`, `:placeholder-shown`) con `StateContext`/`data-megi-*` y `matches_with_state`/`with_state`/`resolve_document_styles_with_media_and_state`, parseo de **grid** (tracks fixed/%/fr/auto/repeat, cap 1024) y alias **`-webkit-background-clip`**. css 94→**128** tests (+26 `state_vars_calc`, +3 grid, +5 clip prefixed). |
| `01339a3` `feat(paint)` | **Clipping real + `background-clip:text`** | `overflow:hidden`+`border-radius` recorta el subárbol (padding box, radios internos `radius − bordes`, AA 4×4 offscreen, cap **16,7 Mpx**) y `background-clip:text` compone fondo color+imágenes por máscara de glifos (multlínea, cap **4096 runs**); sin clipping/clip:text el display list es byte-idéntico. 9 tests `paint --test clipping` (paint 141→**150**); goldens intactos. |
| `a872a86` `feat(layout)` | **Grid MVP** | `layout/grid.rs` + `css/grid.rs`: `display:grid/inline-grid` (flag sobre `InlineBlock`), template columns/rows fixed/%/fr/auto/repeat, `row-gap`/`column-gap`, auto-placement row-major, `justify-items/self`, `align-items/self`, `grid-auto-rows` e items BFC/IFC. Fuera de alcance documentado: spans, named lines, `minmax()`, `auto-fill/auto-fit`, subgrid y colocación explícita. Layout 146→**158** tests y 3 fixtures E2E (`grid_columns` `2d6714bc…`, `grid_rows` `c411ff6d…`, `grid_alignment` `93d1126d…`). |
| `718d075` `feat(net)` | **SameSite/PSL, Referrer-Policy, Range/206, cache** | SameSite Strict/Lax/None+Secure (top-level aproximado, ausente ⇒ Lax), **PSL reducida** (`psl.rs`, ~150 sufijos + fallback, `registrable_domain`/`site`/`same_site`), **Referrer-Policy** (`referrer.rs`, 8 políticas, header/meta, downgrade, per-hop `Referer`), **Range/206** (`range.rs`: `ByteRange`/`ContentRange`/`load_range`/`InvalidRange`; sin multi-range ni If-Range) y cache refinada (`no-cache` vs `must-revalidate`, `s-maxage`, `Age`, `Expires`, `requires_revalidation`; 304 con ETag). Net 180→**207** tests (+27). **Integración pendiente**: instalar `ReferrerConfig` en el loader de `render` antes de subrecursos (1 línea, documentada). |

## Evidencia verificada

- `cargo test --workspace` → **1189 tests, 0 fallos**: bench 8, **css 128**, dom 28, **fonts 17 + 1 doctest**, html 60, image 45 + 1 doctest, js 71, **layout 158**, **net 207**, **paint 150**, render 123, shell 108, svg 73, test_harness 11.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, **árbol 3.904/3.904 (100 %)**, serializer **230/230**, CSS Syntax **30/30**; allowlists de tokenizer/tree **vacías**. fmt/clippy `-D warnings`/doc/MSRV 1.88 OK.
- **Píxeles/evidencias**: goldens intactos — showcase `c95c7eff…` (80.220 B), `basic.rgba` `60f8984e…`, PNG `9e957177…`; grid E2E `2d6714bc…`/`c411ff6d…`/`93d1126d…`; sin clipping/clip:text el display list sigue byte-idéntico.
- **Fuzz**: `cargo +nightly fuzz build` compila 7/7 targets; smoke local por target (400 runs parser, 40 pipeline, 300 JS) sin panics/ASan/UBSan/timeouts (JS con `-detect_leaks=0` por la allocation constante de Boa).
- **Rendimiento**: layout ~36,3 ms y pipeline ~120,5 ms (ronda 15, vigente); `html/massive-end-p` ~19 ms y `tag-40k-attrs` ~12,4 ms (ronda 13); encoder PNG −30,5 % invariante. La ronda 18 no toca el pipeline sin grid/clipping.

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1189 tests, doc y gate
cargo test --workspace                              # 1189 tests (con JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 18
cd fuzz && cargo +nightly fuzz build                # 7 targets
cargo test -p css --test state_vars_calc            # var()/calc()/pseudo-clases (26)
cargo test -p css --test background_clip_prefixed   # -webkit-background-clip (5)
cargo test -p paint --test clipping                 # overflow+radius y clip:text (9)
cargo test -p layout --test grid                    # grid MVP (12)
cargo run -p render -- tests/fixtures/render/grid_columns.html /tmp/grid.png --json
cargo run -p render -- tests/fixtures/render/grid_rows.html /tmp/grid_rows.png --json
cargo run -p render -- tests/fixtures/render/grid_alignment.html /tmp/grid_align.png --json
cargo test -p net referrer && cargo test -p net range && cargo test -p net cache

# Goldens / rendimiento
cargo run -p render -- tests/fixtures/render/showcase/index.html /tmp/showcase.png --width 1024 --height 768  # c95c7eff…
cargo run --release -p bench -- --filter layout --iterations 5     # ~36 ms
cargo run --release -p bench -- --filter pipeline --iterations 3   # ~120 ms
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS (arena + parser 3.904/3.904) | 28 / 60 |
| `css` | COMPLETO F2 + F4 + **var/calc/pseudo-clases/grid parsing** | **128** |
| `layout` | F4 completa + Flexbox + **Grid MVP** + IFC optimizado | **158** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds + decoraciones + **clipping/clip:text** | **150** |
| `net` / `image` | F5 (TLS/caché/**SameSite/PSL/Referrer/Range**) / decoder PNG propio | **207** / 45 + 1 doctest |
| `js` | F6: DOM/eventos/timers + APIs de navegador | 71 |
| `svg` | Parser/modelo/rasterizador AA + CSS/gradientes + pattern/text | 73 |
| `render` | Pipeline URL→PNG + imágenes/backgrounds/SVG + event loop + webfonts | 123 |
| `shell` | F7 + zoom/refresh + formularios editables | 108 |
| `bench` / `test_harness` | Tooling / gate 4 suites | 8 / 11 |
| `fuzz` (workspace aparte) | **7 targets** + corpus + job PR build-only | build 7/7 |

## Deuda priorizada

1. **Integración render**: instalar `ReferrerConfig` en el loader (1 línea) y resolver estilos con `StateContext`/`data-megi-*` para `:focus`/`:checked` (la API CSS ya existe).
2. **CSS**: `calc()` mixto `% + px` (requiere un `Length` con componente porcentual+absoluta); gradientes CSS/conic/`image-set`; `background-clip:text` en más displays.
3. **Grid/layout**: spans, named lines, `minmax()`, `auto-fill/auto-fit`, subgrid, `order`/`flex-flow`/`align-content`; block-in-inline y colores de `border-collapse`; IFC <35 ms.
4. **Paint**: `border-image`, filtros, inset shadows exactas, text mask en subárboles anidados.
5. **Shell**: pestañas, find-in-page, widgets nativos/select dropdown, validación/submit y event loop vivo multi-frame.
6. **Fase 5**: HTTP/2, streaming, PSL completa, multi-range/If-Range, encodings apilados (`br`).
7. **Fuzz/bench**: corpus más largo en CI, bench E2E de grid/clipping/SVG y cachear Boa en CI (+25 s).

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo (ADR JS aceptado 2026-11-10). `docs/PERFORMANCE.md` lo mantiene otro agente.
