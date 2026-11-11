# Handoff de sesión — ronda 31 (cascade layers, PNG single-encode, fetch, proxies y perf VP8)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `aba05f8`
**Estado**: **1907 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; **F4/F5/F6 ≈100 %**, **F7 ≈98 %**. Manifiesto `docs/status/progress.toml` actualizado (F5/F6/F7 + `next_up`); `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `aba05f8`; fuzz PR build-only (14 targets); suite determinista.
**Estado de fases**: fuente editable [`docs/status/progress.toml`](status/progress.toml), página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados.

## Qué se hizo en la ronda 31 (4 commits + status)

| Commit | Bloque | Resultado |
|---|---|---|
| `b37457e` `feat(css)` | **Cascade Layers** | `@layer name;`/bloques/anónimas/anidadas/compuestas, orden por primera aparición, origin→capa→especificidad→orden, unlayered normal gana/implicit final, `!important` invierte capas, `revert-layer`, @media/@container dentro de capa y fast path byte-idéntico sin capas; fix AtKeyword; css **260** (+19); fixtures `ddeafab9…`/`5944fb8a…`. Límites: sin `layer()` en imports, sublayers aproximadas. |
| `6e97eae` `feat(render,shell)` | **PNG single-encode en overlays** | `RenderOptions.encode_png` (default true) + `ensure_png()`/`png_bytes()` perezosos idempotentes byte-idénticos; shell `--chrome/--find/--chrome-menu` clona con encode_png=false, pinta overlay y codifica **una vez**; A/B release CLI `--chrome` **+8,57→+1,13 ms** 640, **+21,67→+1,75 ms** 1024, showcase **+91,20→+2,32 ms** (≤8,5); goldens de overlay byte-idénticos; shell **238**/render **170**. |
| `8dde3dc` `feat(js,render)` | **fetch() subset** | Headers/Request/Response (ok/status/statusText/url/text/json/arrayBuffer/clone, body single-use), `set_fetch_provider` (FetchRequest/FetchResult), GET/POST, caps 64 headers/8 MiB, sin provider ⇒ rejection tipada; **fix HostScope en `run_microtasks`** para Promises con DOM/console; render con `net::http::fetch_with_method`+ResourceLoader file/data, URL contra `location.href`, CORS same-origin, `--no-network` rejection y 404 sintético; js **158**/render **170**. |
| `822c8a2` `feat(net)` + `perf(image)` | **Proxies HTTP/HTTPS + perf VP8** | `ProxyConfig{http,https,no_proxy}`/env, matching host/sufijo/IP/host:port/`*`, absolute-form+Basic auth, CONNECT validado+TLS h1/h2, errores ProxyAuthRequired/ProxyConnect/ProxyTunnel y no_proxy por hop; net **315** (+16). VP8: allocs 25.305→9, YUV sin Vec por fila, predict stack y loop filter early-exit; A/B 1024×768 **13,96→10,91 ms (−22,2 %)**; bench `image/decode-webp-lossy` **10,815 ms** (−20,2 %, 72,7 MPix/s); image **90**. |
| `aba05f8` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; manifiesto editable actualizado en esta ronda (r31). |

## Evidencia verificada

- `cargo test --workspace` → **1907 tests, 0 fallos**: css **260**, net **315**, js **158**, shell **238**, render **170**, image **90**, layout **224**, paint **208**, dom **36**, html **60**, svg **109**, fonts 17 + 1 doctest, bench **10**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **Perf**: chrome overlay overhead +1,13/+1,75/+2,32 ms (antes +8,57/+21,67/+91,20); VP8 10,91 ms A/B y bench 10,815 ms.
- **Goldens**: basic plain `7324e851…`, showcase `c95c7eff…`, phase4 `9e957177…`, basic.rgba `60f8984e…`; overlay basic chrome `93e53942`/`00b7d375`, menu `e9a8d7ad`/`b8422fc3`, showcase chrome `2d85b856`/`931e45ce`, menu `6e63a7a9`/`c06b0268`.
- **Status**: `docs/status/progress.toml` es la fuente editable; `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*`; CI ejecuta `--check --no-gate --no-count`.
- **Fuzz**: 14 targets, corpus 37, CI 300 s + PR build-only.

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1907 tests, doc y gate
cargo test --workspace                              # 1907 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 31
cargo test -p css --test cascade_layers
cargo test -p render --test encode_png_tests
cargo test -p js --test fetch_tests && cargo test -p render --test fetch_tests
cargo test -p net --test proxy
cargo run --release -p bench -- --filter image/decode-webp-lossy

# Progress status
python3 tools/update_progress.py --write
python3 tools/update_progress.py --check --no-gate --no-count
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS + gaps r27 | **36** / 60 |
| `css` | F2+F4 + grid + filtros/blend + Color 4/5 + container queries + **cascade layers** | **260** |
| `layout` | F4 completa + flex/grid + calc/sticky/tipografía + interning + aspect-ratio + container queries | **224** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + EffectGroup + Color 4/5 + blur/PNG byte-safe | **208** |
| `image` | PNG + JPEG baseline/progresivo/IDCT/EXIF + GIF + EXIF 1–8 + WebP VP8L + VP8 lossy keyframe (+perf) | **90** |
| `net` | F5: TLS/caché/cookies/PSL/Referrer/Range + HTTP/2 (HPACK/framing/flow/mux/SETTINGS/bodies/trailers/100-continue/push/priorities/h2c/streaming/keep-alive) + h1 POST + **proxies HTTP/HTTPS** | **315** |
| `js` | F6: DOM/eventos/timers/APIs + ES modules/import maps + geometría + Web Workers + **fetch()** | **158** |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + clipPath/mask/filtros + textPath/SMIL avanzado | **109** |
| `render` | Pipeline URL→PNG + imágenes + module scripts/importmap + scaled decode + providers JS + streaming/keep-alive + workers + **encode_png/fetch** | **170** |
| `shell` | F7 + chrome contextual + pages/favicons + historial + chrome fast path + **PNG single-encode** + sesión/save-png | **238** |
| `bench` / `test_harness` | Tooling (VP8/chrome-png rows) / gate 4 suites | 10 / 11 |
| `fuzz` (workspace aparte) | 14 targets + corpus 37 + PR build-only | build 14/14 |

## Deuda priorizada

1. **Red**: push prioritization, HTTP/1.0, streaming responses; proxy PAC/SOCKS/pooling.
2. **JS**: TLA real/live bindings, fetch streaming/abort/timeout, workers con DOM, conformance/memoria.
3. **Imágenes/SVG**: VP8 interframe/VP8X/ALPH/ANIM, SVG auto-rotate/side right/textLength y SMIL restante.
4. **Paint/bench**: backdrop blur floor, PNG regions y rasterizer budgets.
5. **Shell**: save-as/prompt visual, descarga de recursos, cookies/passwords, favicons/multi-select y pulido de overlay.
6. **Fuzz/bench**: campañas más largas (imagen/HTTP2/JS module), bench E2E de las features r31 y mantener el status al cerrar cada ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests) y PSL MPL-2.0 vendorizada; `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
