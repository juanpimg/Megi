# Handoff de sesión — ronda 32 (perf CSS/layout/PNG, TLA/live bindings, h1 streaming y push priority)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `3e69968`
**Estado**: **1929 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; **F4/F5/F6 ≈100 %**, **F7 ≈98 %**. Manifiesto `docs/status/progress.toml` actualizado (F5/F6 + `next_up`); `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `3e69968`; fuzz PR build-only (14 targets); suite determinista.
**Estado de fases**: fuente editable [`docs/status/progress.toml`](status/progress.toml), página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados.

## Qué se hizo en la ronda 32 (6 commits + status)

| Commit | Bloque | Resultado |
|---|---|---|
| `8f3956c` `perf(css,layout)` | **Bloom pruning + layout** | Ancestor-class Bloom pruning (solo descendiente/hijo; candidatos 33→1-2/span), fast paths @container vacío/`has_revert_layer`, Vec de hijos de capacidad exacta y un solo barrido block/inline: css resolve **28,58→14,14 ms (−50,5 %)**, layout **40,18→37,35 ms (−7,0 %)**, resolve-2k −47 %, vars-calc −8,5 %; goldens intactos. |
| `a90ab4a` `perf(image)` | **PNG decode −62,4 %** | adler32 por bloques 1024 B con módulo diferido, render_row fast path no entrelazado y unfiltro especializado: **6,739→2,536 ms** (inflate 4,06→1,24; render 1,25→0,31), raw md5 idéntico a PIL; sin SIMD/unsafe; fila `image/decode-png-scaled-x2` 2,644 ms. |
| `343f75c` `perf(render)` | **Fast paths render** | backgrounds 7,0→0,8 ms si ningún estilo referencia imágenes; inline_svg 2,8→0,6 ms sin scripts ni `<svg`; **hallazgo**: la fila pipeline del bench no usa el crate render (compone html/paint/layout/raster a mano) ⇒ no puede moverla; perfil CLI fetch 0,7/parse 20,8/estilos 77,5/layout 104,6/display 10,6/raster 1,1/PNG ~8 (total ~234, glue ~5), A/B 235,0→219,8 ms (−6,5 %) con PNG `b37c60c3…`. |
| `b4eb190` `feat(js,render)` | **TLA real + live bindings** | `eval_module_async` sobre módulos nativos de Boa 0.21 con ModuleLoader sobre resolver/loader del host (file/http/data, `import.meta.url`): await Promise/fetch/timer, orden topológico, rechazos tipados, let/var vivas sin re-eval, const inmutable, ciclos nativos; render usa async para módulos con await y mantiene el preprocesado para import maps; js **168**/render **172**; fixtures js_tla_fetch/timer. |
| `5296cab` `feat(net)` | **h1 streaming + push priority** | h1 streaming real (status/headers incremental, redirects antes del body, CL/chunked/read-to-EOF, caps/timeouts, abort por callback; gzip/deflate buffered+16 KiB slices documentado) y push prioritization (PADDED/PRIORITY, `Limits.pushed_priority: ServerWeights|Fifo`, orden por peso con desalojo RST del menor; peso 16 default ⇒ FIFO); net **325** (+10). |
| `a477061`/`76e1678` `perf(paint)`/`test(bench)` | **PNG encode byte-safe + filas bench** | encode **−18 % flat / −22 % alpha** (filter −36/−29 %, deflate −17,4/−18,8 %), render-to-png **8,653→6,259 ms (−27,7 %)**; filas png-filter/deflate/encode-800/1024 y decode-png-scaled-x2; goldens byte-idénticos; paint 208/bench 10. |
| `3e69968` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; manifiesto editable actualizado en esta ronda (r32). |

## Evidencia verificada

- `cargo test --workspace` → **1929 tests, 0 fallos**: net **325**, render **172**, js **168**, css **260**, layout **224**, image **90**, paint **208**, shell **238**, dom **36**, html **60**, svg **109**, fonts 17 + 1 doctest, bench **10**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **Perf**: css resolve −50,5 %, layout −7,0 %, PNG decode −62,4 %, encode −18/−22 %, render-to-png −27,7 %, CLI render −6,5 % con PNG byte-idéntico.
- **Goldens**: básicos/showcase/phase4 intactos; PNG raw md5 idéntico a PIL; TLA E2E 2/2.
- **Status**: `docs/status/progress.toml` es la fuente editable; `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*`; CI ejecuta `--check --no-gate --no-count`.
- **Fuzz**: 14 targets, corpus 37, CI 300 s + PR build-only.

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1929 tests, doc y gate
cargo test --workspace                              # 1929 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 32
cargo run --release -p bench -- --filter css/resolve-styles
cargo run --release -p bench -- --filter image/decode-png
cargo run --release -p bench -- --filter paint/encode-png-1024
cargo test -p js --test tla_live_bindings && cargo test -p render --test tla_tests
cargo test -p net --test h1_streaming && cargo test -p net --test push_priority

# Progress status
python3 tools/update_progress.py --write
python3 tools/update_progress.py --check --no-gate --no-count
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS + gaps r27 | **36** / 60 |
| `css` | F2+F4 + grid + filtros/blend + Color 4/5 + container queries + cascade layers + **Bloom pruning** | **260** |
| `layout` | F4 completa + flex/grid + calc/sticky/tipografía + interning + aspect-ratio + container queries + **perf r32** | **224** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + EffectGroup + Color 4/5 + blur/PNG byte-safe + **encode r32** | **208** |
| `image` | PNG (+decode r32) + JPEG baseline/progresivo/IDCT/EXIF + GIF + EXIF 1–8 + WebP VP8L + VP8 lossy keyframe | **90** |
| `net` | F5: TLS/caché/cookies/PSL/Referrer/Range + HTTP/2 (HPACK/framing/flow/mux/SETTINGS/bodies/trailers/100-continue/push/priorities/h2c/streaming/keep-alive) + h1 POST + proxies + **h1 streaming/push priority** | **325** |
| `js` | F6: DOM/eventos/timers/APIs + ES modules/import maps + **TLA/live bindings nativos** + geometría + Web Workers + fetch | **168** |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + clipPath/mask/filtros + textPath/SMIL avanzado | **109** |
| `render` | Pipeline URL→PNG + imágenes + module scripts/importmap + scaled decode + providers JS + streaming/keep-alive + workers + encode_png/fetch + **TLA + fast paths** | **172** |
| `shell` | F7 + chrome contextual + pages/favicons + historial + chrome fast path + PNG single-encode + sesión/save-png | **238** |
| `bench` / `test_harness` | Tooling (PNG/decode/VP8/chrome rows) / gate 4 suites | 10 / 11 |
| `fuzz` (workspace aparte) | 14 targets + corpus 37 + PR build-only | build 14/14 |

## Deuda priorizada

1. **Red**: HTTP/1.0, gzip streaming, proxy PAC/SOCKS/pooling, h2 cross-load pooling y árbol de prioridades completo.
2. **JS**: fetch streaming/abort/signal, workers con DOM, import-map scopes/integrity, conformance/memoria.
3. **Imágenes/SVG**: VP8 interframe/VP8X/ALPH/ANIM, SVG auto-rotate/side right/textLength y SMIL restante.
4. **Paint/bench**: fila bench propia del crate render (la pipeline actual no lo usa), backdrop blur floor, PNG regions y rasterizer budgets.
5. **Shell**: save-as/prompt visual, descarga de recursos, cookies/passwords, favicons/multi-select y pulido de overlay.
6. **Fuzz/bench**: campañas más largas (imagen/HTTP2/JS module), fingerprint cache en layout y mantener el status al cerrar cada ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests) y PSL MPL-2.0 vendorizada; `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
