# Handoff de sesión — ronda 22 (JPEG/GIF, SVG effects, HTTP/2 MVP, chrome, grid completo y filtros CSS)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `00d8388`
**Estado**: **1449 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; F4 ≈99 %, F5 ≈99 %, F6 ≈90 % y F7 ≈90 %; el manifiesto `docs/status/progress.toml` se actualizó en esta ronda (F4/F5/F7 + `next_up`) y `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con el paso **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `00d8388`; fuzz PR build-only desde la ronda 18; suite determinista.
**Estado de fases**: fuente de verdad en [`docs/status/progress.toml`](status/progress.toml) y página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados (`PROGRESS.md`, `docs/status/*.svg`).

## Qué se hizo en la ronda 22 (5 commits + refresco de status)

| Commit | Bloque | Resultado |
|---|---|---|
| `77095c7` `feat(image,render)` | **JPEG y GIF desde cero** | JPEG baseline (SOI/DQT/SOF0/DHT/SOS/DRI/RST, IDCT separable, 4:4:4/4:2:2/4:2:0 nearest, grayscale/YCbCr/Adobe RGB, errores tipados para progressive/CMYK/truncados y caps antes de asignar) y GIF87a/89a (LZW 12-bit, paletas, transparencia, interlace, primer frame). `render` decodifica PNG\|JPEG\|GIF por sniffing (WebP = aviso no fatal). PNG decode 800×600: **6,65 ms p50 (~290 MB/s)** con `inflate_zlib_with_capacity`. image **54**, render **137** tests. |
| `82101f1` `feat(svg)` | **clipPath/mask/filtros** | `<clipPath>` (shapes/paths/`use`, `clipPathUnits`, anidados, AA, ciclos sin panic), `<mask>` (`maskUnits`/`maskContentUnits`, región, luminancia, gradientes) y filtros `feGaussianBlur`/`feOffset`/subset `feFlood`+`feComposite` con `filterUnits`/región; caps 4.096 shapes/16 primitivas/blur 64/64M muestras. svg **85** tests; fixtures `clip_path`/`mask`/`filter_blur`. |
| `93f2b40` `feat(net)` | **HTTP/2 MVP** | HPACK completo (static 61/dynamic eviction/literals/Huffman Apéndice B/encoder mínimo), framing HEADERS/CONTINUATION/DATA/SETTINGS/WINDOW_UPDATE/PING/RST/GOAWAY con estados y caps 16 KiB frame/64 KiB headers/2 streams, cliente ALPN `h2` sobre rustls con fallback HTTP/1.1, GET END_STREAM, ACKs, RST/GOAWAY, timeout/body caps y `FetchError::Http2`; redirects/cache/cookies/CORS/compresión intactos. net **225** tests. |
| `cc567ec` `feat(shell)` | **Chrome del navegador** | `chrome.rs`: `AddressBar` (Ctrl+L, caret, Enter con `Url::join`, Esc), `TabStripLayout` (títulos/activa/cerrar/+/overflow + hit-testing), `DropdownList` para `<select>` (cap 8 filas, scroll, teclado, selected patch + re-render), overlay 58 px (device·zoom, fuente bitmap) y `--screenshot --chrome`. shell **160** tests; golden basic sin chrome `7324e851…` intacto; chrome `1a29d6b3…`. |
| `b5b7e3c` `feat(css,layout,paint)` | **Grid completo + filtros CSS** | Grid: `grid-auto-flow row/column+dense`, `grid-template-areas`/`grid-area`, líneas nombradas y placement por nombre, `justify/align-content` con stretch y `repeat(auto-fit)`; layout **206**/css **195**; fixtures `grid_areas` `8f9a9eba…`/`grid_dense` `1789daf8…`/`grid_content` `123d1a2b…`. CSS: `filter` (blur/brightness/contrast/grayscale/invert/opacity/sepia/saturate/hue-rotate/drop-shadow + alias `-webkit`), `mix-blend-mode` (6 modos) e `image-set()` 1x; paint `DisplayItem::EffectGroup` offscreen contra backdrop, caps 16,7 Mpx/blur 64/funciones 16; paint **190**; fixtures `filter_effects` `ffe06406…`/`blend_modes` `32788e6d…`/`image_set` `dc638211…`; goldens intactos. |
| `00d8388` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; fuente editable `docs/status/progress.toml` actualizada en esta ronda (F5 99/F7 90, items done y `next_up`). |

## Evidencia verificada

- `cargo test --workspace` → **1449 tests, 0 fallos**: css **195**, layout **206**, paint **190**, image **54**, render **137**, net **225**, svg **85**, shell **160**, html **60**, js **71**, dom **28**, fonts 17 + 1 doctest, bench **9**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **Rendimiento**: PNG decode 800×600 6,65 ms p50 (~290 MB/s); presupuestos de la ronda 21 siguen cumplidos (pipeline ≤140, layout ≤43; detalle en `docs/PERFORMANCE.md`, otro carril).
- **Fixtures/md5** citadas arriba; goldens antiguos (showcase `c95c7eff…`, basic `7324e851…`, PNG `9e957177…`/`60f8984e…`) intactos; sin md5 obsoletos.
- **Status**: `docs/status/progress.toml` es la fuente editable (fases F1–F7, `items`/`pending`/`next_up`); `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*` (no editar a mano). CI ejecuta `--check --no-gate --no-count` (Status freshness).
- **Fuzz**: sin targets nuevos en la ronda 22; JPEG/GIF/HPACK cubiertos por tests unitarios/robustez, campañas dedicadas pendientes (ver `docs/FUZZING.md`).

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1449 tests, doc y gate
cargo test --workspace                              # 1449 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 22
cargo test -p image --test decode                   # PNG/JPEG/GIF
cargo run -p render -- tests/fixtures/render/jpeg_gif.html /tmp/jpeg_gif.png --json
cargo test -p svg --test effects                    # clipPath/mask/filtros
cargo test -p net --test http2_tests                # HPACK/framing/ALPN
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/chrome.png --chrome --width 640 --height 400
cargo run -p render -- tests/fixtures/render/grid_areas.html /tmp/grid_areas.png --json   # 8f9a9eba…
cargo run -p render -- tests/fixtures/render/filter_effects.html /tmp/filter.png --json   # ffe06406…

# Progress status (regenerar tras cerrar la ronda)
python3 tools/update_progress.py --write
python3 tools/update_progress.py --check --no-gate --no-count   # CI freshness
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS (arena + parser 3.904/3.904) | 28 / 60 |
| `css` | F2+F4 + var/calc/pseudos + gradientes + grid completo + **filtros/blend/image-set** | **195** |
| `layout` | F4 completa + flex/grid **completo** + calc mixto/sticky/tipografía + interning | **206** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + **EffectGroup** | **190** |
| `image` | **PNG + JPEG baseline + GIF87a/89a** (inflate propio) | **54** |
| `net` | F5: TLS/caché/cookies/PSL/Referrer/Range + **HTTP/2 MVP (HPACK/framing/ALPN)** | **225** |
| `js` | F6: DOM/eventos/timers + APIs de navegador (sin cambios r22) | 71 |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + **clipPath/mask/filtros** | **85** |
| `render` | Pipeline URL→PNG + imágenes (PNG/JPEG/GIF/SVG) + event loop + **chrome stats** | **137** |
| `shell` | F7 + tabs/find/forms/zoom + **chrome (address bar/tab strip/select)** | **160** |
| `bench` / `test_harness` | Tooling (filas E2E) / gate 4 suites | 9 / 11 |
| `fuzz` (workspace aparte) | 7 targets + corpus + job PR build-only (sin cambios r22) | build 7/7 |

## Deuda priorizada

1. **HTTP/2**: multiplexado, flow-control completo, push/prioridades, h2c, trailers, request bodies y streaming de respuestas.
2. **Imágenes**: WebP, EXIF orientation, progressive JPEG, GIF animado y presupuestos de decode/rasterizer.
3. **SVG**: `<textPath>`, `feColorMatrix`/`feBlend`/`feMerge`, animación SVG y color/display-p3.
4. **CSS/paint**: `backdrop-filter`, espacios de color avanzados y efectos por capa en lugar de por grupo.
5. **Grid/layout**: overlaps fuera de orden de documento, subgrid/masonry, `grid-auto-columns`, block-in-inline y colores de `border-collapse`.
6. **Shell**: autocompletar/historial en address bar, favicons/iconos, menú de overflow, select múltiple/nativo, descargas/bookmarks/sesión e aislamiento por pestaña.
7. **JS/fuzz**: módulos ES, memoria real, eventos resize/scroll/change; campañas dedicadas para JPEG/GIF/HPACK y fuzz más largas.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
