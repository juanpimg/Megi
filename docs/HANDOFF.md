# Handoff de sesión — ronda 27 (fix PNG, h2 push/priorities/h2c, gaps DOM/JS, perf efectos y chrome pulido)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `9d2298b`
**Estado**: **1695 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; F4 ≈99 %, F5 ≈99 %, **F6 ≈97 %**, **F7 ≈94 %**. Manifiesto `docs/status/progress.toml` actualizado (F4/F5/F6/F7 + `next_up`); `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `9d2298b`; fuzz PR build-only; suite determinista.
**Estado de fases**: fuente editable [`docs/status/progress.toml`](status/progress.toml), página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados.

## Qué se hizo en la ronda 27 (5 commits + status)

| Commit | Bloque | Resultado |
|---|---|---|
| `0ff6434` `perf(image)` | **Fix regresión PNG** | El segundo caller de `pixel_from_row`/`render_row` perdió el inlining; `#[inline(always)]` en decode_pass/unfilter/render_row/pixel_from_row. A/B r25 6,62 / r26 7,97 / fix **6,54 ms** (≤6,6); escalado preservado full 6,27–6,43, ×2 5,53–5,69, ×8 5,02–5,13 (−29/−36 %); image 76. |
| `d34d078` `feat(net)` | **HTTP/2 push/priorities/h2c** | PUSH_PROMISE con SETTINGS_ENABLE_PUSH=0 por defecto y error tipado; `Limits.enable_server_push` valida y cancela con RST_STREAM(CANCEL) sincronizando HPACK. `Priority{depends_on,weight 1..=256,exclusive}` + `H2Request::with_priority` y scheduler por peso; `Limits.allow_h2c` + `h2c_request` (101+preface+stream 1, fallback h1). 10 tests `h2_advanced.rs`; net **259**. Límites: push descartado (render no lo consume), h2c GET-only, prioridades MVP. |
| `a373d47` `feat(dom,js)` | **Gaps DOM/JS r26** | URL/URLSearchParams subset, dataset lectura, innerText aprox documentada, attributes NamedNodeMap, normalize recursivo, compareDocumentPosition bits+constantes, queueMicrotask, requestIdleCallback determinista (timeRemaining 50), MouseEvent/PointerEvent relatedTarget/pointerId, focus/blur + activeElement, scrollIntoView no-op, atob/btoa, structuredClone subset; js **128**/dom **36** (+20). Gaps restantes documentados (geometría sin layout provider, fetch/XHR, crypto, dataset escritura, innerText con layout, Range, NodeList live). |
| `ecd3092` `perf(css,paint,bench)` | **Perf de efectos** | `compute_effects` de un solo barrido (antes 5 búsquedas/elemento) y parseo unificado filter/-webkit/backdrop/mix-blend; E2E filter overhead 0,148→**0,036 ms (−75,7 %)**; backdrop 0,692→0,684 (1,2 %: el resto es blur raster + PNG más entrópico; byte-safe no toca el encoder de goldens). Paint consolida optimizaciones r26 y corrige doble filtrado; filas `pipeline/filter-fixture-300x260(±no-effect)` y backdrop. |
| `d07d70a` `feat(shell)` | **Chrome pulido** | Address bar Home/End/Shift-selección/Delete/Ctrl+A-C-X-V con portapapeles interno/Ctrl+Backspace, autocompletado historial-sesión (≥2 chars, sufijo gris, Tab), Ctrl+L, caret sólido determinista; tab strip con overflow real (lista scroll, activar/cerrar, Esc/fuera), rueda wrap, indicador de carga, ellipsis; `<select>` con optgroup no seleccionable, Home/End/PageUp/PageDown/primer-carácter y scrollbar minimal; shell **195** (+12); `--chrome` md5 `1a29d6b3…` y golden página `7324e851…` intactos. |
| `9d2298b` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; manifiesto editable actualizado en esta ronda (r27). |

## Evidencia verificada

- `cargo test --workspace` → **1695 tests, 0 fallos**: net **259**, js **128**, dom **36**, shell **195**, image **76**, paint **208**, css **230**, layout **215**, render **153**, svg **97**, html **60**, fonts 17 + 1 doctest, bench **9**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **Perf**: PNG decode **6,54 ms** (r25 6,62, r26 7,97) con escalado ×2 5,53–5,69 y ×8 5,02–5,13; filter E2E −75,7 %; backdrop 0,692→0,684 documentado.
- **Goldens**: `--chrome` md5 `1a29d6b3…`; página `7324e851…`; resto intactos.
- **Status**: `docs/status/progress.toml` es la fuente editable; `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*`; CI ejecuta `--check --no-gate --no-count`.
- **Fuzz**: matriz sin cambios (10 targets, corpora 11/9/21, CI 300 s + PR build-only).

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1695 tests, doc y gate
cargo test --workspace                              # 1695 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 27
cargo test -p image
cargo test -p net --test h2_advanced
cargo test -p js --test gap_apis && cargo test -p dom --test gap_api
cargo run --release -p bench -- --filter pipeline/filter-fixture-300x260
cargo run -p shell -- tests/fixtures/render/basic.html --chrome --screenshot /tmp/chrome.png --width 640 --height 400

# Progress status
python3 tools/update_progress.py --write
python3 tools/update_progress.py --check --no-gate --no-count
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS + gaps r27 (normalize/compareDocumentPosition) | **36** / 60 |
| `css` | F2+F4 + var/calc/pseudos + grid completo + filtros/blend + Color 4/5 + effects single-pass | **230** |
| `layout` | F4 completa + flex/grid + calc mixto/sticky/tipografía + interning + aspect-ratio | **215** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + EffectGroup + Color 4/5 + perf r27 | **208** |
| `image` | PNG (fix regresión) + JPEG baseline/progresivo color + GIF + EXIF 1–8 + WebP VP8L + decode escalado | **76** |
| `net` | F5: TLS/caché/cookies/PSL/Referrer/Range + HTTP/2 (HPACK/framing/flow/mux/SETTINGS/bodies/trailers/100-continue/**push/priorities/h2c**) + h1 POST | **259** |
| `js` | F6: DOM/eventos/timers/APIs + ES modules + gaps r27 | **128** |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + clipPath/mask/filtros + SMIL | **97** |
| `render` | Pipeline URL→PNG + imágenes + module scripts + scaled decode + providers JS | **153** |
| `shell` | F7 + chrome pulido/address bar/autocompletado/tab overflow/select + sesión/save-png | **195** |
| `bench` / `test_harness` | Tooling (filas E2E filter/backdrop r27) / gate 4 suites | 9 / 11 |
| `fuzz` (workspace aparte) | 10 targets + corpus curado + PR build-only | build 10/10 |

## Deuda priorizada

1. **JS/DOM**: geometry provider para `getBoundingClientRect`/`offset*`, fetch/XHR, crypto inyectable, dataset writes, innerText con layout, Range/NodeList live; dynamic import/TLA/import maps/live bindings y conformance.
2. **Red**: push-to-render, streaming uploads y respuestas en streaming, PSL completa/multi-range.
3. **Imágenes/SVG**: VP8 lossy, reduced IDCT/EXIF thumbnails, SVG textPath/keyTimes/eventos/CSS animations y rendimiento en páginas reales.
4. **Paint/bench**: backdrop blur/PNG entropy, layer-effect performance, PNG regions y presupuestos de rasterizer.
5. **Shell**: save-as/prompt visual, descarga de recursos, cookies/passwords, favicons y multi-select/native widgets.
6. **Fuzz/bench**: campañas más largas (imagen/HTTP2), bench E2E de las features r27 y mantener el status al cerrar cada ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
