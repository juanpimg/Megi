# Handoff de sesión — ronda 30 (VP8 lossy, container queries, PSL/trailers, Web Workers, historial y chrome fast path)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `f7dad8d`
**Estado**: **1853 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; **F4 ≈100 %**, F5 ≈99 %, F6 ≈99 %, **F7 ≈97 %**. Manifiesto `docs/status/progress.toml` actualizado (F4/F5/F6/F7 + `next_up`); `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `f7dad8d`; fuzz PR build-only (14 targets); suite determinista.
**Estado de fases**: fuente editable [`docs/status/progress.toml`](status/progress.toml), página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados.

## Qué se hizo en la ronda 30 (5 commits + status)

| Commit | Bloque | Resultado |
|---|---|---|
| `7ed6be3` `feat(image,render)` | **Decoder VP8 keyframe completo** | WebP lossy desde cero (~1.8k líneas + tablas): boolean decoder, segmentación, loop filters normal+simple, particiones 1/2/4/8, intra Y16/UV DC/V/H/TM y B_PRED 10 modos, dequant/IDCT 4×4/Y2 IWHT, YUV420→RGB fancy upsampling libwebp y BT.601 fixed-point. Tolerancias vs PIL q75 max 6/0,12, q90 max 2/0,01, 1024×768 max 7/0,01; decode **53,6 ms** release; image **90**/render **165**. Límites: keyframe only, simple filter luma-only, fixtures 1 partición (multi implementada). |
| `a2ee4a0` `feat(css,layout)` | **Container Queries** | `container-type: normal|inline-size|size`, `container-name`, `@container [name] (min/max-width|height|inline-size|block-size, and)` con px/%/em/rem, ancestro compatible más cercano, anidamiento y fast path sin reglas; predicción de tamaño de una pasada documentada; css **241**/layout **224**; fixtures container_queries `c358c11f…`/named `57b09283…`. Límites sin style/scroll-state ni cqi/cqw completas. |
| `87b493d` `feat(net)` | **PSL real + request trailers** | PSL vendorizada `crates/net/data/public_suffix_list.dat` (snapshot publicsuffix.org, 16.610 líneas/~339 KB, MPL-2.0, include_str!, parser OnceLock, reglas normal/wildcard/exception, algoritmo completo, fallback último label, IDN ASCII/punycode documentado, `tools/update_psl.sh`) y trailers h2 (`H2Request::with_trailers`, HEADERS final END_STREAM tras DATA, body requerido) y h1 (`fetch_with_trailers`, chunked `0\r\n`+trailers); framing sin trailers byte-idéntico; net **299**. |
| `89c56ba` `feat(js,render)` | **Web Workers deterministas** | `Worker(url)` con `set_worker_loader`, realm aislado sin DOM, `postMessage`/`onmessage`/`addEventListener('message')`/`terminate`, colas FIFO JSON-safe, entrega `run_worker_tasks`/microtasks, `onerror` tipado, caps 8/256; fix de 2 bugs de HostScope (handler worker y callbacks principal) + drenaje en `execute_scripts`; `RenderOptions.enable_workers` y loader `file://`; js **147**/render **164**. Límites: cooperativos, sin SharedWorker/importScripts. |
| `67365af` `feat(shell,bench)` | **Historial + chrome fast path** | `HistoryLog` por pestaña (cap 64, dedupe), `RecentHistory` cap 10, `about:history` determinista, Ctrl+H, botones ←/→/⟳ con hit-testing/disabled, recent en session.json; `draw_chrome_over_pixmap` dibuja solo banda superior sobre el Pixmap RGBA (sin round-trip 0RGB), píxeles idénticos, componente no-PNG **10,06→1,06 ms (−89,5 %)**; resto wall dominado por PNG final (floor 22,9 ms a 1024), follow-up omitir PNG redundante en `--chrome`/`--find`; shell **236**; goldens basic `7324e851…`, showcase `c95c7eff…`, phase4 `9e957177…`, basic.rgba `60f8984e…`, about:history `5ad5a2be…`. |
| `f7dad8d` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; manifiesto editable actualizado en esta ronda (r30). |

## Evidencia verificada

- `cargo test --workspace` → **1853 tests, 0 fallos**: css **241**, layout **224**, net **299**, js **147**, shell **236**, render **165**, image **90**, paint **208**, dom **36**, html **60**, svg **109**, fonts 17 + 1 doctest, bench **9**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **Perf**: VP8 1024×768 decode 53,6 ms; chrome fast path no-PNG −89,5 % (10,06→1,06 ms) con píxeles idénticos.
- **Goldens**: página basic/showcase/phase4/basic.rgba intactos; about:history `5ad5a2be…`; container fixtures `c358c11f…`/`57b09283…`.
- **Status**: `docs/status/progress.toml` es la fuente editable; `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*`; CI ejecuta `--check --no-gate --no-count`.
- **Fuzz**: 14 targets, corpus 37, CI 300 s + PR build-only.

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1853 tests, doc y gate
cargo test --workspace                              # 1853 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 30
cargo test -p image --test vp8
cargo test -p css && cargo test -p layout
cargo test -p net --test psl && cargo test -p net --test trailers
cargo test -p js --test workers && cargo test -p render --test worker_tests
cargo test -p shell --test history

# Progress status
python3 tools/update_progress.py --write
python3 tools/update_progress.py --check --no-gate --no-count
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS + gaps r27 | **36** / 60 |
| `css` | F2+F4 + var/calc/pseudos + grid + filtros/blend + Color 4/5 + **container queries** | **241** |
| `layout` | F4 completa + flex/grid + calc mixto/sticky/tipografía + interning + aspect-ratio + **container queries** | **224** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + EffectGroup + Color 4/5 + blur/PNG byte-safe | **208** |
| `image` | PNG + JPEG baseline/progresivo/IDCT/EXIF + GIF + EXIF 1–8 + WebP VP8L + **VP8 lossy keyframe** | **90** |
| `net` | F5: TLS/caché/cookies/**PSL real**/Referrer/Range + HTTP/2 (HPACK/framing/flow/mux/SETTINGS/bodies/trailers/100-continue/push/priorities/h2c/streaming/keep-alive) + h1 POST | **299** |
| `js` | F6: DOM/eventos/timers/APIs + ES modules/import maps + geometría + **Web Workers** | **147** |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + clipPath/mask/filtros + textPath/SMIL avanzado | **109** |
| `render` | Pipeline URL→PNG + imágenes (VP8 lossy) + module scripts/importmap + scaled decode + providers JS + streaming/keep-alive + workers | **165** |
| `shell` | F7 + chrome contextual + pages/favicons + **historial** + **chrome fast path** + sesión/save-png | **236** |
| `bench` / `test_harness` | Tooling (chrome-png rows) / gate 4 suites | 9 / 11 |
| `fuzz` (workspace aparte) | 14 targets + corpus 37 + PR build-only | build 14/14 |

## Deuda priorizada

1. **Red**: push prioritization, proxies, HTTP/1.0, streaming responses; IDN PSL y trailers sin body.
2. **JS**: TLA real/live bindings/fetch, conformance/memoria; workers follow-ups (loader no-file, SharedWorker fuera de alcance).
3. **Imágenes/SVG**: VP8 interframe/VP8X/ALPH/ANIM y multi-partition fixtures; SVG auto-rotate/side right/textLength y SMIL restante.
4. **Paint/bench**: backdrop blur floor, PNG regions, rasterizer budgets y opción render para omitir PNG redundante en `--chrome`/`--find`.
5. **Shell**: save-as/prompt visual, descarga de recursos, cookies/passwords, favicons/multi-select y pulido de historial.
6. **Fuzz/bench**: campañas más largas (imagen/HTTP2/JS module), bench E2E de las features r30 y mantener el status al cerrar cada ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests) y PSL MPL-2.0 vendorizada; `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
