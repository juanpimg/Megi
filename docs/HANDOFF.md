# Handoff de sesión — ronda 24 (JS browser APIs, aspect-ratio/object-fit/backdrop, progresivo/WebP, sesión/descargas y fuzz 10 targets)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `0b3adee`
**Estado**: **1547 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; F4 ≈99 %, F5 ≈99 %, **F6 ≈92 %**, **F7 ≈92 %**. Manifiesto `docs/status/progress.toml` actualizado en esta ronda (F4/F6/F7 + `next_up`); `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `0b3adee`; fuzz PR build-only; suite determinista.
**Estado de fases**: fuente editable [`docs/status/progress.toml`](status/progress.toml), página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados.

## Qué se hizo en la ronda 24 (6 commits + refresco de status)

| Commit | Bloque | Resultado |
|---|---|---|
| `05e2b77` `feat(js)` | **History/location/Web Storage/MutationObserver** | History API con providers (`pushState/replaceState/back/forward/go`; sin provider no-op), `location` getters + `assign/replace/reload`, `localStorage`/`sessionStorage` (cap 1 MiB/realm, `QuotaExceededError`, `StorageProvider` load/save) y `MutationObserver` subset (childList/attributes/characterData+subtree, `takeRecords`/`disconnect`; records al drenar microtasks). `requestIdleCallback` no implementado. js **83** tests (+12). |
| `13c3f6a` `feat(css,paint)` | **aspect-ratio/object-fit/backdrop-filter** | `aspect-ratio` (`auto`/ratio), `object-fit` fill/contain/cover/none/scale-down, `object-position` keywords/%/px y `backdrop-filter` + alias `-webkit`; paint `DisplayItem::BackdropFilter` (backdrop ya pintado + funciones + blur determinista; caps 64/16/16,7 Mpx) y object-fit con clip al content box y bilineal (`fill` byte-idéntico, goldens intactos). Fixtures aspect_ratio `a1e4d950…`, object_fit `b54ddebe…`, backdrop_filter `ff04fac9…`; css **210**/paint **205**. |
| `9ad3391` `feat(layout)` | **Consumo de aspect-ratio** | w/h derivados en block/replaced/flex/grid, ambos definidos gana definido, replaced `auto` usa intrínseco, fallback 300×150. Fixture `layout_aspect_ratio` 400×400 `md5 6f021d43…` (6.016 B); goldens intactos; layout **215** tests. |
| `c444e96` `feat(image,render)` | **Progresivo color + WebP VP8L subset** | JPEG progresivo de color **completo** (fix EOBRUN en AC-first; color Δmáx ≤2, gris Δ0) y **WebP VP8L subset** (RIFF/VP8L, header, Huffman simple, literales ARGB; transforms/meta-Huffman/LZ77/VP8 lossy ⇒ `UnsupportedFeature` documentado). `render` decodifica el subset y avisa del resto; `webp.html` E2E; image **67**/render **139**. |
| `0f3f080` `feat(shell)` | **Sesión persistente + descargas** | Sesión JSON v1 (tabs/historial/zoom/scroll, caps 32/64, escritura atómica + lock best-effort, XDG, `--no-session`/`--session-file`, headless no escribe) y descargas (`Ctrl+S` PNG, `Ctrl+Shift+S` HTML serializado con doctype, `--save-png`/`--save-html`, naming sanitizado); shell **180** tests (+20); golden basic intacto. |
| `1f58102` `test(fuzz)` | **Fuzz 10 targets** | Nuevos `image_decode`, `exif_parse`, `hpack_decode` (matriz 7→10, corpus curado 11/9/21, CI 300 s matrix + PR build-only); build 10/10 y smoke 300 runs sin crashes. |
| `0b3adee` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; manifiesto editable actualizado en esta ronda (r24). |

## Evidencia verificada

- `cargo test --workspace` → **1547 tests, 0 fallos**: css **210**, layout **215**, paint **205**, image **67**, render **139**, shell **180**, js **83**, net **237**, svg **85**, html **60**, dom **28**, fonts 17 + 1 doctest, bench **9**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **Fixtures**: aspect_ratio `a1e4d950…`, object_fit `b54ddebe…`, backdrop_filter `ff04fac9…`; layout_aspect_ratio 400×400 `6f021d43…` (6.016 B); v8l_*/`webp.html`; goldens anteriores intactos (showcase `c95c7eff…`, basic `7324e851…`, PNG `9e957177…`/`60f8984e…`).
- **Fuzz**: 10 targets, `cargo +nightly fuzz build` 10/10 y smoke 300 runs (`-rss_limit_mb=2048 -timeout=25`) sin panics/ASan/timeouts.
- **Status**: `docs/status/progress.toml` es la fuente editable; `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*`; CI ejecuta `--check --no-gate --no-count`.

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1547 tests, doc y gate
cargo test --workspace                              # 1547 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 24
cargo test -p js --test browser_features_r24
cargo test -p css && cargo test -p layout --test aspect_ratio
cargo run -p render -- tests/fixtures/render/aspect_ratio.html /tmp/aspect.png --json    # a1e4d950…
cargo run -p render -- tests/fixtures/render/object_fit.html /tmp/object_fit.png --json  # b54ddebe…
cargo run -p render -- tests/fixtures/render/backdrop_filter.html /tmp/backdrop.png --json # ff04fac9…
cargo test -p image --test decode && cargo run -p render -- tests/fixtures/render/webp.html /tmp/webp.png --json
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/shot.png --width 640 --height 400
cd fuzz && cargo +nightly fuzz build

# Progress status
python3 tools/update_progress.py --write
python3 tools/update_progress.py --check --no-gate --no-count
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS (arena + parser 3.904/3.904) | 28 / 60 |
| `css` | F2+F4 + var/calc/pseudos + grid completo + filtros/blend/image-set + **aspect-ratio/object-fit/backdrop** | **210** |
| `layout` | F4 completa + flex/grid + calc mixto/sticky/tipografía + interning + **aspect-ratio** | **215** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + EffectGroup + **BackdropFilter/object-fit** | **205** |
| `image` | PNG + JPEG baseline/**progresivo color completo** + GIF + EXIF 1–8 + **WebP VP8L subset** | **67** |
| `net` | F5: TLS/caché/cookies/PSL/Referrer/Range + HTTP/2 (HPACK/framing/flow/mux/SETTINGS) | **237** |
| `js` | F6: DOM/eventos/timers/APIs + **History/location/Web Storage/MutationObserver** | **83** |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + clipPath/mask/filtros | **85** |
| `render` | Pipeline URL→PNG + imágenes PNG/JPEG/GIF/SVG/WebP-subset + **webp E2E** | **139** |
| `shell` | F7 + chrome/tabs/find/forms/zoom + **sesión persistente/descargas** | **180** |
| `bench` / `test_harness` | Tooling / gate 4 suites | 9 / 11 |
| `fuzz` (workspace aparte) | **10 targets** + corpus curado + PR build-only | build 10/10 |

## Deuda priorizada

1. **Imágenes/WebP**: VP8L transforms/meta-Huffman/LZ77 completos, VP8 lossy, display-p3, EXIF thumbnails y rendimiento decode/rasterizer.
2. **JS**: módulos ES/dynamic import; cableado de providers history/location/storage en render/shell (canal render→shell para location); StorageEvent/proxy/requestIdleCallback; fetch/XHR/workers; memoria real.
3. **HTTP/2**: push/prioridades, h2c, trailers, request bodies/100-continue y streaming.
4. **CSS/paint**: efectos por capa, color avanzado, `object-view-box`/`image-set` múltiple, rendimiento de group effects.
5. **Shell**: save-as/prompt visual, descarga de recursos (no solo PNG/HTML), cookies/passwords, autocompletar/favicons, multi-select/native widgets, aislamiento.
6. **Fuzz/bench**: campañas más largas (imagen/HTTP2), bench E2E de las features r24 y mantener status generado al cerrar cada ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
