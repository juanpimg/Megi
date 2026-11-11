# Handoff de sesión — ronda 29 (streaming/keep-alive, textPath/SMIL, import maps, WebP lossy header, shell pages y fuzz 14)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `8f9cfa9`
**Estado**: **1796 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; F4 ≈99 %, F5 ≈99 %, **F6 ≈99 %**, **F7 ≈96 %**. Manifiesto `docs/status/progress.toml` actualizado (F4/F5/F6/F7 + `next_up`); `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `8f9cfa9`; fuzz PR build-only (14 targets); suite determinista.
**Estado de fases**: fuente editable [`docs/status/progress.toml`](status/progress.toml), página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados.

## Qué se hizo en la ronda 29 (7 commits + status)

| Commit | Bloque | Resultado |
|---|---|---|
| `20b662b` `feat(net)` | **Streaming + keep-alive** | `get_streaming`/`request_streaming` con DataCallback (h2 DATA por frame; error ⇒ RST_STREAM(CANCEL)); h1 `fetch_streaming` con fallback buffered 16 KiB; loader `load_streaming` (file/data troceado, bypass cachés documentado); `H1Pool` keep-alive por origin (cap 4, idle timeout, `FetchOptions.reuse_connections/max_connections_per_origin/connection_idle_timeout`, `idle_connections()`) ⇒ 2 loads = 1 TCP; h2 reutiliza `Session`; `RenderOptions.stream_responses` byte-idéntico; net **287**/render **162**. |
| `5bddd19` `feat(svg)` | **textPath + SMIL** | `<textPath>` (path flatten, glyph advance/baseline, startOffset px/%, text-anchor; sin auto-rotate/side right/textLength, hijo directo); SMIL `values`+`keyTimes`+`calcMode` linear/discrete/**spline** con `keySplines` cúbicos, `keyTimes` con set y animación opacity/fill/stroke/color/flood-color/stop-color en sRGB+alpha; inválidos ignorados ⇒ byte-idéntico; svg **109** (+12). |
| `6b5ee45` `feat(js,render)` | **Import maps + dynamic import** | `ImportMap` (`set_import_map_json`: exact, prefijo `/` más largo, null ⇒ ImportMapBlocked antes del host resolver, JSON inválido), `import()` dinámico solo literales (eager, Promise resuelto; no-literal ⇒ DynamicImport) y top-level await como gap tipado `TopLevelAwait` (sin falso soporte); render parsea el primer `<script type=importmap>` con warnings no fatales; js **137** (+7). |
| `4673582` `feat(image,render)` | **WebP lossy header experimental** | `image::vp8` parsea RIFF/WEBP `VP8 ` (frame tag 0–3, start code, dims 14-bit+scaling, partición, caps) y `lossy_info`; decode devuelve `UnsupportedFeature` claro (no mis-decode como VP8L); render warning no fatal "WebP VP8 lossy is experimental" + placeholder (img/background); pixel decoder pendiente documentado; image **90** (+4). |
| `af20b06` `feat(shell)` | **Pages + favicons** | file:// directorios deterministas (dirs-first, nombre/tipo/tamaño, breadcrumb, hrefs absolutos escapados) en headless+worker; páginas de error DNS/HTTP/TLS/red/file con URL/código/mensaje/Reintentar R y URL inválida typed; favicon `<link rel=icon>`/favicon.ico (fetch cap 64 KiB, decode 16×16 nearest, dot FNV determinista, FaviconCache, dibujo por pestaña y `--chrome`); shell **224** (+20); md5 dir `d0b66f74…`, error `58a65985…`, chrome+dot `4b9033c7…`; goldens sin chrome intactos. |
| `376fbf3` `test(fuzz)` | **Fuzz 14 targets** | `h2_push`/`body_stream`/`webp_vp8`/`js_module` (matriz 10→14, corpus 37 seeds, 300 s c/u, `js_module` `-detect_leaks=0`, PR build-only 14); build 14/14 y smoke 300 runs sin crashes; `docs/FUZZING.md` actualizado. |
| `d9633b3` `docs(perf)` | **Bench r28** | Baseline load 0,67–0,96: pipeline **115,67/140 (82,6 %)** ✅, css 27,51 ≤35, layout 37,99 ≤45, showcase 26,74 ≤35, display 8,61 ≤25, shell 29,25 (83,6 %), render-to-png 9,31; **todos los presupuestos cumplidos sin subirlos**; A/B prístino r27→r28 paridad (−1,4 %) con caveat +5–9 % por perfil/frecuencia; costs js_geometry +2,38 ms, push −0,46 ms, image_scaled −1,85 ms (−8,7 %), backdrop raster/png 1,289/0,812. |
| `8f9cfa9` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; manifiesto editable actualizado en esta ronda (r29). |

## Evidencia verificada

- `cargo test --workspace` → **1796 tests, 0 fallos**: net **287**, render **162**, svg **109**, js **137**, image **90**, shell **224**, paint **208**, css **230**, layout **215**, dom **36**, html **60**, fonts 17 + 1 doctest, bench **9**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **Fuzz**: matriz 14 targets, corpus 37, build 14/14 y smoke 300 runs.
- **Bench r28**: pipeline 115,67/140 (82,6 %) y resto de presupuestos ✅; paridad A/B −1,4 %.
- **Goldens**: dir `d0b66f74…`, error `58a65985…`, chrome+dot `4b9033c7…`; goldens sin chrome y página intactos.
- **Status**: `docs/status/progress.toml` es la fuente editable; `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*`; CI ejecuta `--check --no-gate --no-count`.

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1796 tests, doc y gate
cargo test --workspace                              # 1796 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 29
cargo test -p net --test streaming_responses && cargo test -p net --test connection_reuse
cargo test -p svg --test textpath_smil
cargo test -p js --test es_modules
cargo test -p image --test webp_lossy
cargo run -p shell -- tests/fixtures/render/basic.html --chrome --screenshot /tmp/chrome.png --width 640 --height 400
cd fuzz && cargo +nightly fuzz build

# Progress status
python3 tools/update_progress.py --write
python3 tools/update_progress.py --check --no-gate --no-count
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS + gaps r27 | **36** / 60 |
| `css` | F2+F4 + var/calc/pseudos + grid + filtros/blend + Color 4/5 + effects single-pass | **230** |
| `layout` | F4 completa + flex/grid + calc mixto/sticky/tipografía + interning + aspect-ratio | **215** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + EffectGroup + Color 4/5 + blur/PNG byte-safe | **208** |
| `image` | PNG + JPEG baseline/progresivo/IDCT reducida/EXIF thumb + GIF + EXIF 1–8 + WebP VP8L + **VP8 lossy header (experimental)** | **90** |
| `net` | F5: TLS/caché/cookies/PSL/Referrer/Range + HTTP/2 (HPACK/framing/flow/mux/SETTINGS/bodies/trailers/100-continue/push/priorities/h2c/**streaming respuestas**) + h1 POST + **keep-alive pool/session reuse** | **287** |
| `js` | F6: DOM/eventos/timers/APIs + ES modules + import maps/dynamic import + geometría | **137** |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + clipPath/mask/filtros + **textPath/SMIL avanzado** | **109** |
| `render` | Pipeline URL→PNG + imágenes + module scripts/importmap + scaled decode + providers JS + streaming/keep-alive + WebP lossy warning | **162** |
| `shell` | F7 + chrome contextual + **directorios/error pages/favicons** + sesión/save-png | **224** |
| `bench` / `test_harness` | Tooling (baseline r28) / gate 4 suites | 9 / 11 |
| `fuzz` (workspace aparte) | **14 targets** + corpus 37 + PR build-only | build 14/14 |

## Deuda priorizada

1. **Red**: push prioritization/trailers, PSL completa/proxies, h1 streaming buffered fallback y h2 cross-load pooling.
2. **JS**: top-level await/live bindings, workers/fetch, import-map scopes/integrity, conformance y memoria.
3. **Imágenes/SVG**: VP8 lossy pixel decoder, EXIF/IDCT edge cases, SVG auto-rotate/side right/textLength y SMIL restante.
4. **Paint/bench**: backdrop blur floor, layer-effect performance, PNG regions y rasterizer budgets.
5. **Shell**: save-as/prompt visual, descarga de recursos, cookies/passwords, multi-select/native widgets y pulido.
6. **Fuzz/bench**: campañas más largas (imagen/HTTP2/JS module), bench E2E de las features r29 y mantener el status al cerrar cada ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
