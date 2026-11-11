# Handoff de sesión — ronda 28 (IDCT reducida/EXIF, push, uploads streaming, geometría JS, blur byte-safe y menú contextual)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `cb7af43`
**Estado**: **1738 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; F4 ≈99 %, F5 ≈99 %, **F6 ≈98 %**, **F7 ≈95 %**. Manifiesto `docs/status/progress.toml` actualizado (F4/F5/F6/F7 + `next_up`); `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `cb7af43`; fuzz PR build-only; suite determinista.
**Estado de fases**: fuente editable [`docs/status/progress.toml`](status/progress.toml), página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados.

## Qué se hizo en la ronda 28 (6 commits + status)

| Commit | Bloque | Resultado |
|---|---|---|
| `972bd7d` `feat(image)` | **Reduced IDCT + EXIF thumbnails** | `decode_scaled` usa IDCT 4×4/2×2/1×1 para escalas 2/4/8 (basis separable centrada, AC descartados, planos reducidos; full intacto, progresivo fallback documentado); `exif_thumbnail` APP1/IFD1 con caps y política resize. A/B 1024×768 full 19,28 → ×2 8,24 (−57 %) / ×4 3,30 (−82 %) / ×8 1,97 ms (−90 %); image **84** (+8). |
| `f5423e8` `feat(net)` | **Server push + streaming uploads** | Push consumido opt-in (`Limits.enable_server_push`): PUSH_PROMISE HPACK→URL, HEADERS/DATA del stream prometido con flow-control propio, `H2PushedResponse`, caps/RST(CANCEL) y frames tardíos ignorados. `RequestBody::Stream` pull-based con backpressure, `H2Request`/`request_many_owned`, DATA incremental, cap 16 MiB y `FetchError::RequestBody`; h1 chunked/100-continue/redirects; loader `accept_push` + preload caché por carga (`pushed_used/discarded`). net **279** (+20). |
| `ffa49e0` `feat(js)` | **Geometría JS** | `LayoutProvider` (NodeId→`LayoutRect{border_box,content_box}`), `getBoundingClientRect`, `offsetWidth/Height`, `clientWidth/Height` y `ViewportProvider` (innerWidth/Height/DPR); sin provider 0 documentado; js **130** (+2), gaps actualizados. |
| `e6be506` `feat(render)` | **Push preload + geometría** | `RenderOptions.accept_server_push` preload documento+subrecursos y stats `pushed_used/discarded` (CLI `--json`); E2E TLS h2 que empuja imagen (pinta rojo+azul, `images_loaded=2`, `pushed_used=1`, sin petición de la empujada). Wiring de geometría: layout pre-script → NodeId→LayoutRect antes de scripts; primer script sin snapshot ⇒ 0, `display:none` ⇒ 0; fixtures `js_geometry_*`; render **157** (+4). |
| `2cc1127` `perf(paint)` | **Blur/PNG byte-safe** | Blur separable por zonas + plano thread-local alternado (misma aritmética f32) y encode PNG con filter_row de 1 barrido y tablas LZ77 thread-local u32 (misma salida greedy). A/B encode backdrop 0,243→0,186 (−23,5 %), blur 0,298→0,284 (−4,7 %), E2E 0,569→0,494 (−13,2 %), full 2,107→2,058; objetivo 1,65 ms no alcanzable (floor no-effect ~1,54) documentado; goldens byte-idénticos; paint 208. |
| `9b5ea6b` `feat(shell)` | **Hover + menú contextual** | Hover de enlaces con barra de estado 20 px (hit-test scroll/zoom, cursor mano); Ctrl+clic/clic medio nueva pestaña (MAX_TABS); menú contextual con Open link in new tab/Copy link address (portapapeles interno)/Save image (ResourceLoader+magic+naming+atómico)/Copy image no-op/Reload/Save page PNG/HTML, teclado ↑↓/Enter/Esc, hover, clic-fuera y flip en bordes; `images.rs` extrae `<img>`; shell **204** (+9); `--chrome --chrome-menu` md5 `b59202c1…`. |
| `cb7af43` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; manifiesto editable actualizado en esta ronda (r28). |

## Evidencia verificada

- `cargo test --workspace` → **1738 tests, 0 fallos**: net **279**, render **157**, js **130**, image **84**, shell **204**, paint **208**, css **230**, layout **215**, dom **36**, svg **97**, html **60**, fonts 17 + 1 doctest, bench **9**, test_harness **11**. (Una primera pasada de workspace tuvo fallos transitorios en `h2_flow_multiplex_tests`/`html` por concurrencia; rerun individuales verdes.)
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **Perf**: A/B JPEG 1024×768 full 19,28 → ×2 8,24 / ×4 3,30 / ×8 1,97 ms; backdrop encode −23,5 %, E2E −13,2 %, blur −4,7 %, floor ~1,54 ms documentado.
- **Goldens**: `--chrome --chrome-menu` md5 `b59202c1…`; página `7324e851…` y goldens de paint byte-idénticos.
- **Status**: `docs/status/progress.toml` es la fuente editable; `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*`; CI ejecuta `--check --no-gate --no-count`.
- **Fuzz**: matriz sin cambios (10 targets, corpora 11/9/21, CI 300 s + PR build-only).

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1738 tests, doc y gate
cargo test --workspace                              # 1738 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 28
cargo test -p image
cargo test -p net --test h2_push_tests && cargo test -p net --test upload_streaming
cargo test -p js --test gap_apis && cargo test -p render --test js_geometry_tests
cargo run -p render -- tests/fixtures/render/js_geometry_fixed.html /tmp/geo.png --json
cargo run -p shell -- tests/fixtures/render/basic.html --chrome --chrome-menu --screenshot /tmp/menu.png --width 640 --height 400

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
| `image` | PNG + JPEG baseline/progresivo color + GIF + EXIF 1–8 + WebP VP8L + decode escalado con **IDCT reducida** y **EXIF thumbnails** | **84** |
| `net` | F5: TLS/caché/cookies/PSL/Referrer/Range + HTTP/2 (HPACK/framing/flow/mux/SETTINGS/bodies/trailers/100-continue/push/priorities/h2c/**push consumido/streaming uploads**) + h1 POST | **279** |
| `js` | F6: DOM/eventos/timers/APIs + ES modules + gaps DOM/JS + **geometría** | **130** |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + clipPath/mask/filtros + SMIL | **97** |
| `render` | Pipeline URL→PNG + imágenes + module scripts + scaled decode + providers JS + **push preload/geometría** | **157** |
| `shell` | F7 + chrome pulido + **hover/menú contextual/Ctrl+clic** + sesión/save-png | **204** |
| `bench` / `test_harness` | Tooling (filas blur/PNG r28) / gate 4 suites | 9 / 11 |
| `fuzz` (workspace aparte) | 10 targets + corpus curado + PR build-only | build 10/10 |

## Deuda priorizada

1. **Red**: push prioritization/trailers, streaming responses, PSL completa/multi-range.
2. **JS/DOM**: fetch/XHR/workers, import maps/TLA/live bindings, dataset writes, innerText con layout, Range/NodeList live, conformance y memoria.
3. **Imágenes/SVG**: VP8 lossy, SVG textPath/keyTimes/event SMIL/CSS animations y rendimiento en páginas reales.
4. **Paint/bench**: backdrop blur floor, layer-effect performance, PNG regions y presupuestos de rasterizer.
5. **Shell**: save-as/prompt visual, descarga de recursos, cookies/passwords, favicons y multi-select/native widgets.
6. **Fuzz/bench**: campañas más largas (imagen/HTTP2), bench E2E de las features r28 y mantener el status al cerrar cada ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
