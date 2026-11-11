# Handoff de sesión — ronda 23 (perf CSS/paint, EXIF/progresivo, WebP detectado y HTTP/2 flow control)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `d1f3121`
**Estado**: **1468 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; presupuestos recuperados con margen (css resolve **26,54 ms**, pipeline **~105,5 ms** en A/B; build-display **7,34–7,72 ms ≤8**); F4 ≈99 %, F5 ≈99 %, F6 ≈90 %, F7 ≈90 %. Manifiesto `docs/status/progress.toml` actualizado en esta ronda (F4/F5 + `next_up`); `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con el paso **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `d1f3121`; fuzz PR build-only desde la ronda 18; suite determinista.
**Estado de fases**: fuente editable [`docs/status/progress.toml`](status/progress.toml), página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados.

## Qué se hizo en la ronda 23 (4 commits + refresco de status)

| Commit | Bloque | Resultado |
|---|---|---|
| `cfad971` `perf(css)` | **CSS perf** | Clave de caché de estilos por **identidad de estilo del padre** (no NodeId), **move-on-miss** (sin clon en la ruta de fallo) y **AST UA cacheado en `OnceLock`**. A/B intercalado: `css/resolve-styles-10k-nodes` 35,81→**26,54 ms** (**−25,9 %**) y `pipeline/html-to-png-800x600` 142,64→**105,47 ms** (**−26,1 %**); clave 128-bit, umbral 2, cap 1024; inline-styled parents con clave única conservadora. |
| `3f5ce3e` `perf(paint)` | **Paint effects analysis** | Análisis agregado `effects_present` por pase + fast path r21-equivalente (sin filter/blend/image-set no se clonan estilos ni `EffectGroup`), capacidad `DisplayItem` 20480 e `is_list_item` antes de phase. A/B build-display 7,72→**7,34 ms** (−4,8 %) y 9,20→8,31 (−9,6 %); absoluto **7,34–7,72 ms ≤8**. |
| `1e8e078` `feat(image,render)` | **EXIF + JPEG progresivo + WebP detectado** | **EXIF orientation completa**: APP1/TIFF little+big, IFD0 tag 0x0112, orientaciones 1–8 aplicadas en `image::decode`, `exif_orientation()`, fixtures PIL `exif_o1..o8` con aserciones exactas. **JPEG progresivo subset**: SOF2 multiscan DC/AC first+refine, spectral selection, successive approximation, EOB runs y restart; **gris exacto (delta 0)**; refinamiento multi-componente ⇒ `UnsupportedFeature` claro/documentado. **WebP se detecta** (RIFF/VP8L) pero **no se decodifica** (aviso no fatal `VP8L/VP8 decoder pending`). `render` añade `image_formats.html` E2E (2 imágenes pintadas; aviso WebP sin romper). image **60**/render **138** tests. |
| `a2fb055` `feat(net)` | **HTTP/2 flow control + multiplexado + SETTINGS** | Flow control de conexión/stream (ventanas, WINDOW_UPDATE replenish, `FLOW_CONTROL_ERROR`, delta 0 inválido), **multiplexado ≥2 streams** (`client::get_many`, Session/scheduler, `next_stream_id` impar, RST aislado, `Limits.max_concurrent_streams` default 8 respetando MAX_CONCURRENT_STREAMS) y **SETTINGS** avanzados (HEADER_TABLE_SIZE/MAX_CONCURRENT_STREAMS/INITIAL_WINDOW_SIZE/MAX_FRAME_SIZE/ENABLE_PUSH validados+ACK, HEADERS+CONTINUATION, HPACK compartido). 12 tests loopback nuevos; net **237**. `load`/fallback h1 intactos. |
| `d1f3121` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; manifiesto editable actualizado en esta ronda (r23). |

## Evidencia verificada

- `cargo test --workspace` → **1468 tests, 0 fallos**: css **195**, layout **206**, paint **190**, image **60**, render **138**, net **237**, svg **85**, shell **160**, html **60**, js **71**, dom **28**, fonts 17 + 1 doctest, bench **9**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **A/B de rendimiento** (binarios pre/post del mismo árbol, intercalado, `taskset`): css resolve 35,81→26,54 ms (−25,9 %); pipeline 142,64→105,47 ms (−26,1 %); build-display 7,34–7,72 ms ≤8. Presupuestos ≤140 pipeline/≤43 layout sin subir.
- **Fixtures**: EXIF o1–o8 y `baseline_ref.jpg`/`gray_prog.jpg`/`progressive.jpg` en `crates/image/tests/fixtures/jpeg/`; `tests/fixtures/render/image_formats.html` + assets `exif.jpg`, `progressive.jpg`, `photo.webp`. Goldens existentes intactos (showcase `c95c7eff…`, basic `7324e851…`, PNG `9e957177…`/`60f8984e…`).
- **Status**: `docs/status/progress.toml` es la fuente editable; `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*`; CI ejecuta `--check --no-gate --no-count` (Status freshness).
- **Fuzz**: sin targets nuevos en la ronda 23; progressive JPEG/EXIF/WebP sniffing/HPACK-flow cubiertos por tests unitarios/loopback, campañas dedicadas pendientes (ver `docs/FUZZING.md`).

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1468 tests, doc y gate
cargo test --workspace                              # 1468 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 23
cargo run --release -p bench -- --filter resolve-styles-10k-nodes   # 26,54 ms A/B
cargo run --release -p bench -- --filter build-display              # 7,34–7,72 ms
cargo run --release -p bench -- --filter pipeline/html-to-png      # 105,47 ms A/B
cargo test -p image --test exif                                     # EXIF 1–8
cargo test -p image --test decode                                   # progresivo (gris exacto)
cargo test -p net --test h2_flow_multiplex_tests                    # flow control/mux/SETTINGS
cargo run -p render -- tests/fixtures/render/image_formats.html /tmp/image_formats.png --json

# Progress status (regenerar al cerrar la ronda)
python3 tools/update_progress.py --write
python3 tools/update_progress.py --check --no-gate --no-count
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS (arena + parser 3.904/3.904) | 28 / 60 |
| `css` | F2+F4 + var/calc/pseudos + gradientes + grid completo + filtros/blend/image-set + **caché por estilo de padre** | **195** |
| `layout` | F4 completa + flex/grid completo + calc mixto/sticky/tipografía + interning | **206** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + EffectGroup + **effects fast path** | **190** |
| `image` | PNG + JPEG baseline/**progresivo subset** + GIF + **EXIF orientation 1–8**; **WebP solo detectado** | **60** |
| `net` | F5: TLS/caché/cookies/PSL/Referrer/Range + HTTP/2 (HPACK/framing/ALPN/**flow control/mux/SETTINGS**) | **237** |
| `js` | F6: DOM/eventos/timers + APIs de navegador (sin cambios r23) | 71 |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + clipPath/mask/filtros | **85** |
| `render` | Pipeline URL→PNG + imágenes PNG/JPEG/GIF/SVG + event loop + **image_formats E2E** | **138** |
| `shell` | F7 + tabs/find/forms/zoom + chrome (address bar/tab strip/select) | **160** |
| `bench` / `test_harness` | Tooling (filas E2E) / gate 4 suites | 9 / 11 |
| `fuzz` (workspace aparte) | 7 targets + corpus + job PR build-only (sin cambios r23) | build 7/7 |

## Deuda priorizada

1. **Imágenes**: WebP VP8L/VP8 decoder, refinamiento de color en JPEG progresivo, CMYK/12-bit, EXIF thumbnails, GIF animado y presupuestos de decode/rasterizer.
2. **HTTP/2**: push/prioridades, h2c, trailers, 100-continue y request bodies; streaming de respuestas.
3. **CSS/paint**: display-p3/espacios de color avanzados, `backdrop-filter`, efectos por capa y `object-fit`/`aspect-ratio`.
4. **SVG/JS**: `<textPath>`, `feColorMatrix`/`feBlend`/`feMerge`, animación SVG; módulos ES y memoria real en Boa.
5. **Grid/layout**: overlaps fuera de orden de documento, subgrid/masonry, block-in-inline y colores de `border-collapse`.
6. **Shell**: autocompletar/historial, favicons, overflow menu, select múltiple/nativo, descargas/bookmarks/sesión e aislamiento.
7. **Fuzz/bench**: campañas dedicadas para JPEG progresivo/EXIF/HPACK-flow y fuzz más largas; mantener status generado al cerrar cada ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
