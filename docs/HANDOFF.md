# Handoff de sesión — ronda 26 (module scripts, decode escalado, HTTP/2 bodies, bug-bash DOM/JS, SVG filters/SMIL y bench aislado)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `f42b9b9`
**Estado**: **1653 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; F4 ≈99 %, F5 ≈99 %, **F6 ≈96 %**, F7 ≈93 %. Manifiesto `docs/status/progress.toml` actualizado (F4/F5/F6 + `next_up`); `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `f42b9b9`; fuzz PR build-only; suite determinista.
**Estado de fases**: fuente editable [`docs/status/progress.toml`](status/progress.toml), página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados.

## Qué se hizo en la ronda 26 (7 commits + status)

| Commit | Bloque | Resultado |
|---|---|---|
| `0167bf8` `feat(render)` | **`<script type=module>`** | Inline/externos con resolver `Url` contra el módulo importador (file/http(s)/data; bare ⇒ `ModuleNotFound`) y loader file/http con `FetchOptions`/`--no-network`, caps por módulo y total del grafo; fallo no fatal; microtasks/timers/re-render. Fixtures `modules/*.js` + `module_*.html`; render **153** (+6). |
| `cf258fa` `feat(image,render)` | **Decode escalado** | `decode_scaled`/`decode_scaled_with_limits`: factor `ceil(longest/max_dim)`; PNG submuestrea durante unfilter (incl. Adam7) sin buffer completo, JPEG/GIF/WebP post-subsample (documentado); `RenderOptions.max_image_dim` + `--max-image-dim`. A/B PNG full ×2 7,98→5,74 ms (−28 %) y ×8 5,08→3,25 ms (−36 %); stage images −30 %, total 23,3–24,7→18,1–18,7 ms; image **76**/render **153**. |
| `7a5893e` `feat(net)` | **HTTP/2 bodies/trailers/100-continue + h1 POST** | `H2Request`/`request_many`, DATA con MAX_FRAME_SIZE/ventanas, END_STREAM único, cap 16 MiB (`RequestBodyTooLarge`), RST durante envío, trailers h2/h1, `Expect: 100-continue` h2/h1 y h1 POST CL/chunked; loader `fetch_with_method`; net **249** (+12). Límites: sin push/priorities/h2c, cuerpos buffered, 100-continue sin timer. |
| `025a292` `fix(dom,js)` | **Bug-bash DOM/JS** | 6 bugs reales: fragmentos en append/insert/replace y replace en posición, atributos case-insensitive HTML, childNodes NodeList snapshot, propiedades reflejadas value/checked/disabled/selected/options, `createDocumentFragment` y `cloneNode` deep; +20 tests WPT-like; dom **33**/js **111**. Gaps documentados (fetch/dataset/innerText/attributes/normalize/compareDocumentPosition/getBoundingClientRect/Range/MouseEvent/relatedTarget/URLSearchParams/queueMicrotask/NodeList snapshot). |
| `2fb05a1` `feat(svg)` | **Filtros completos + SMIL básica** | `feColorMatrix` (matrix/saturate/hueRotate/luminanceToAlpha), `feBlend` (normal/multiply/screen/darken/lighten), `feMerge`, `feComposite` arithmetic k1–k4 y over/in/out/atop, con cadenas `in`/`in2`/`result`/`SourceGraphic`/`SourceAlpha` y región; SMIL `<animate from/to/by>`, `<set>`, `<animateTransform translate/rotate/scale>` determinista con `render_svg_with_time` (t=0/mitad); sin animación byte-idéntico; svg **97** (+12). Límites: sin keyTimes/keySplines/sync/CSS animations/textPath. |
| `7ab354c` `perf(paint,bench)` | **Bench aislado + optimizaciones seguras** | Filas `paint/layer-effects-800x600`, `object-fit-800x600`, filter/backdrop fixtures (paint tail; el sobrecoste de backdrop/aspect no está en paint), `shell/save-png-encode-800x600` 3,76 ms (write 0,003; total 3,78); `tools/bench.sh --save-png-isolated` (screenshot solo vs +save-png = 30 vs 29 ms; +38,8 ms era escribir 2 PNG). Optimizaciones: backdrop color-only in-place, buffers thread-local, blur salta alpha uniforme, object-fit clip solo cover, fast path SolidColor filtrado. |
| `a39c4ab` `docs(fixtures)` | **Fixtures r26** | Documenta fixtures de module scripts y scaled decode. |
| `f42b9b9` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; manifiesto editable actualizado en esta ronda (r26). |

## Evidencia verificada

- `cargo test --workspace` → **1653 tests, 0 fallos**: render **153**, net **249**, image **76**, svg **97**, dom **33**, js **111**, paint **208**, css **230**, layout **215**, shell **183**, html **60**, fonts 17 + 1 doctest, bench **9**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **Fixtures**: `modules/*.js` + `module_inline/external/chain/error/no_network/observers_timers.html`; foto 1024×768 para scaled decode; goldens showcase/basic intactos.
- **Bench**: `shell/save-png-encode-800x600` 3,76 ms; `--save-png-isolated` 30 vs 29 ms; filas layer-effects/object-fit/filter/backdrop documentadas.
- **Status**: `docs/status/progress.toml` es la fuente editable; `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*`; CI ejecuta `--check --no-gate --no-count`.
- **Fuzz**: matriz sin cambios (10 targets, corpora 11/9/21, CI 300 s + PR build-only).

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1653 tests, doc y gate
cargo test --workspace                              # 1653 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 26
cargo test -p render --test module_scripts_tests
cargo test -p image --test scaled
cargo run -p render -- tests/fixtures/render/module_chain.html /tmp/modules.png --json
cargo test -p net && cargo test -p dom && cargo test -p js
cargo test -p svg --test filters_smil
cargo run -p render -- tests/fixtures/render/showcase.html /tmp/showcase.png --json

# Progress status
python3 tools/update_progress.py --write
python3 tools/update_progress.py --check --no-gate --no-count
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS + bug-bash DOM (fragmentos/atributos/reflejadas/clone) | **33** / 60 |
| `css` | F2+F4 + var/calc/pseudos + grid completo + filtros/blend + Color 4/5 | **230** |
| `layout` | F4 completa + flex/grid + calc mixto/sticky/tipografía + interning + aspect-ratio | **215** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + EffectGroup + Color 4/5 | **208** |
| `image` | PNG + JPEG baseline/progresivo color + GIF + EXIF 1–8 + WebP VP8L completo + **decode escalado** | **76** |
| `net` | F5: TLS/caché/cookies/PSL/Referrer/Range + HTTP/2 (HPACK/framing/flow/mux/SETTINGS/**bodies/trailers/100-continue**) + h1 POST | **249** |
| `js` | F6: DOM/eventos/timers/APIs + ES modules + **bug-bash DOM/JS** | **111** |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + clipPath/mask/filtros + **feColorMatrix/Blend/Merge/Composite + SMIL** | **97** |
| `render` | Pipeline URL→PNG + imágenes + **module scripts** + **scaled decode** + providers JS | **153** |
| `shell` | F7 + chrome/tabs/find/forms/zoom + sesión persistente/save-png directo | **183** |
| `bench` / `test_harness` | Tooling (filas aisladas r26) / gate 4 suites | 9 / 11 |
| `fuzz` (workspace aparte) | 10 targets + corpus curado + PR build-only | build 10/10 |

## Deuda priorizada

1. **JS/DOM**: dynamic import/TLA/import maps y live bindings, loader real, StorageEvent/proxy/requestIdleCallback, fetch/XHR/workers, gaps DOM documentados, conformance y memoria.
2. **Red**: h2 push/priorities/h2c, streaming uploads y cuerpos/respuestas en streaming, PSL completa/multi-range.
3. **Imágenes/SVG**: VP8 lossy, reduced IDCT/EXIF thumbnails, SVG textPath y keyTimes/eventos/CSS animations, rendimiento de decode en páginas reales.
4. **Paint/bench**: rendimiento de layer-effects/backdrop, PNG regions y presupuestos de rasterizer.
5. **Shell**: save-as/prompt visual, descarga de recursos, cookies/passwords, autocompletar/favicons y multi-select.
6. **Fuzz/bench**: campañas más largas (imagen/HTTP2), bench E2E de las features r26 y mantener el status al cerrar cada ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
