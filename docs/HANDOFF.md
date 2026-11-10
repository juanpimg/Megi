# Handoff de sesión — ronda 13 (fuentes web @font-face/WOFF2 E2E, crate `svg`, shell zoom/refresh)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `b7ef1c1`
**Estado**: **959 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; Fase 4 prácticamente completa en motor (**fuentes web y SVG E2E HECHOS; pendientes `background-repeat/size/position`**), **Fase 5 ≈95 %**, **Fase 6 ≈80-85 %**, **Fase 7 ≈70 %**.
**CI/Repo**: GitHub Actions **en verde** (quality + conformance + MSRV) con las deps de sistema Wayland/X11/xcb; `main` y `sprint/robustez-fase4` sincronizadas en `5234630`. La nueva dep de `fonts` provocará **rebuild del cache de CI** la próxima vez.
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md) · [`FUZZING.md`](FUZZING.md) · [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 13 y su cierre (5 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `86be30e` `feat(shell)` | **Shell: zoom, auto-refresh y estado** | **Zoom** (`Ctrl+=/-/0`, `--zoom` 0.5–3.0, escalones 50–300 %, viewport CSS = físico/(scale·zoom)), **`Ctrl+L`** imprime la URL, **auto-refresh opt-in** `--refresh-ms [MS]` (250 ms por defecto; solo si `scripts_executed>0 && script_timers_pending>0`, render en worker generacional, scroll conservado, sin bucles) y **título enriquecido** (zoom/scroll/warnings/errores). **83→100 tests**; screenshot basic `md5 7324e851…` intacto y `--zoom 2` (640×400→320×200) `md5 c11903cfe0c4f7095289052d25750bd3`. Límites: sin barra de direcciones/tabs/formularios; el refresh repite el primer frame de animaciones (reloj virtual). |
| `2b97187` `feat(webfonts)` | **@font-face + WOFF2 de punta a punta** | `crates/css` parsea `@font-face` (`Stylesheet.font_faces`, `FontFaceRule { family, sources, weight, style }`); `crates/fonts` añade `decode_web_font`/`is_woff2` (WOFF2→sfnt con la dep autorizada **`woff2-patched` 0.4**, Apache-2.0, pure Rust sin C/FFI) y `FontDatabase::load_font_data_family(alias, sfnt)`; `crates/render` recoge las reglas de todas las hojas (incl. `@import`), deduplica, descarga con `ResourceLoader` (`--no-network`, caps **8 fuentes/8 MiB/32 MiB**), construye una **DB por render** compartida por layout y paint y añade stats `fonts_loaded/fonts_failed/font_bytes` + `timings.fonts` (fallos ⇒ warning no fatal y fallback). Fixtures OFL Open Sans WOFF2 (18.640 B) + licencia; evidencia: **webfont `md5 f38aed032b8d160075097f1e4ad83b49` vs fallback `e2d76dee98f7c4ef2896c5d003be74f2`** (`fonts_loaded=1`, `font_bytes=18640`, `timings.fonts` medido). CSS 57→61, fonts 13→17 (+1 doctest), render 88→93 tests. Límites: sin WOFF1, primer `src` soportado, weight/style no eligen cara, sin unicode-range/font-display (bloqueante), sin subsetting/color fonts, DB no cacheada. |
| `6971e50` `feat(render)` | **SVG cableado en render** | `<img src=*.svg>` y `background-image` SVG (detección por `Content-Type`/sniffing), tamaño intrínseco width/height/viewBox con 300×150 por defecto, cap **16 Mpx/64 MiB RGBA**, corrupto ⇒ warning no fatal, navegación directa envuelta en página sintética; nueva stat `svgs_rendered`; **5 fixtures `svg_*` + 6 assets**, `crates/render/tests/svg_tests.rs` con **11 tests** ⇒ **render 93→104 tests** (workspace 948→959). |
| `b7ef1c1` `perf(html)` | **Fix de regresión `massive-end-p`** | Restaurado el bucle de hashing por chunks de bytes para claves cortas en `crates/html/src/fast_hash.rs` (único fichero): **`html/massive-end-p` ~19 ms** (18,7-19,3; antes 21,4; r10 18,2) y **pipeline ~127-134 ms**, gate intacto (3904/3904). |
| `5234630` `feat(svg)` | **Crate `svg` desde cero** | Parser XML subset propio + modelo SVG (`svg/g/defs/use/rect` con rx/ry, circle/ellipse/line/polyline/polygon/path, nested svg, transforms, viewBox/preserveAspectRatio, estilos inline, colores, fill nonzero/evenodd, stroke width/linecap/linejoin/miterlimit, opacidad) + path `M/L/H/V/C/S/Q/T/A/Z` con arcos→cúbicas + **rasterizador scanline AA determinista** (4 subfilas + cobertura analítica; strokes con 4×4), RGBA straight-alpha y límites anti-DoS (8 MiB input / 256 profundidad / 100k elementos / 1M segmentos / 16k lado / 64 Mpx / `<use>` 16). `#![forbid(unsafe_code)]`, API `render_svg`/`intrinsic_size`/`SvgImage`/`SvgOptions`/`Limits`/`SvgError`, **41 tests** y `examples/svg_to_png` (PNG propio). **Cableado E2E en `render` en el commit posterior `6971e50`.** |

## Evidencia verificada

- `cargo test --workspace` → **959 tests, 0 fallos**: bench 8, **css 61**, dom 28, **fonts 17 + 1 doctest**, html 60, image 45+1, js 58, layout 135, net 180, paint 109, **render 104**, **shell 100**, **svg 41**, test_harness 11. fmt, clippy `-D warnings`, `cargo doc` y MSRV 1.88 OK.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, **árbol 3.904/3.904 (100 %)**, serializer **230/230**, CSS Syntax **30/30**; allowlists vacías.
- **SVG E2E**: `cargo run -p render -- tests/fixtures/render/svg_img.html /tmp/s.png --json` ⇒ `svgs_rendered=1`; fixtures `svg_img/svg_viewbox/svg_intrinsic/svg_corrupt/svg_background` + assets; corrupto/oversize ⇒ warning no fatal.
- **Rendimiento**: `html/massive-end-p` ~19 ms y pipeline ~127-134 ms (fix `b7ef1c1`).
- **Píxeles/evidencias**: showcase 1024×768 80.220 B `md5 c95c7eff…`; render basic 15.498 B `md5 7324e851…`; `basic.rgba` byte-idéntico `md5 60f8984e…`; **webfont** `f38aed03…` vs **fallback** `e2d76dee…`; shell `--zoom 2` 320×200 `c11903cf…`; PNG del crate svg validado con `tools/png_check.py`.
- **CI**: quality/conformance/MSRV en verde; workflow instala `libwayland-dev`, `libxkbcommon*`, `libx11*`, `libxcb*` para `winit`/`softbuffer`.

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO: arena generacional, API de navegador | 28 |
| `html` | COMPLETO: parser 3.904/3.904, serializador §13.3, hook `ScriptExecutor` | 60 |
| `css` | COMPLETO + `@font-face`/`@import`/media/query API | 61 |
| `layout` | COMPLETO Fase 4: tablas §17, `Arc<ComputedStyle>` (LayoutBox 176 B) | 135 |
| `fonts` | COMPLETO LTR + **fuentes web @font-face/WOFF2** | 17 + 1 doctest |
| `paint` | COMPLETO Fase 4 + backgrounds + hit-testing + encoder PNG −30,5 % invariante | 109 |
| `net` | Fase 5: TLS/caché/data:/cookies/CORS/gzip | 180 |
| `image` | Decoder PNG + inflate propios | 45 + 1 doctest |
| `js` | Fase 6 WP-A/B/6.4 + eventos avanzados | 58 |
| `svg` | **NUEVO**: parser+modelo+rasterizador AA desde cero; cableado E2E en render (`<img>`/background) | 41 |
| `render` | Pipeline URL→PNG + imágenes/backgrounds/SVG + scripts + event loop + **webfonts** | 104 |
| `shell` | Fase 7: scroll/entrada/HiDPI/worker + navegación + **zoom/auto-refresh/estado** | 100 |
| `bench` | Benchmarks reproducibles | 8 |
| `test_harness` | Gate 4 suites (árbol 3.904/3.904 sin allowlist) + `JsScriptExecutor` | 11 |

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 959 tests, doc y gate
cargo test --workspace                              # 959 tests (con JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo test -p svg                                   # 41 tests del crate SVG
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/simple.svg /tmp/simple.png
cargo run -p render -- tests/fixtures/render/svg_img.html /tmp/svg_img.png --json   # svgs_rendered=1
cargo test -p render --test svg_tests               # 11 tests SVG E2E

# Fuentes web @font-face/WOFF2 (ronda 13)
cargo run -p render -- tests/fixtures/render/webfont.html /tmp/webfont.png --json          # f38aed03…
cargo run -p render -- tests/fixtures/render/webfont-fallback.html /tmp/fallback.png      # e2d76dee…
cargo test -p fonts -p css -p render

# Shell: zoom / auto-refresh
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/zoom2.png --width 640 --height 400 --zoom 2  # c11903cf…
cargo run -p shell -- tests/fixtures/render/showcase/index.html --zoom 1.5 --refresh-ms 250
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/shot.png --width 640 --height 400            # 7324e851…

# Rustdoc / MSRV
cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets
```

## Deuda priorizada

1. **Ampliar SVG**: `<text>`/fuentes en el crate, gradientes/patterns y `stroke-dasharray`; cachear los SVG renderizados por URL entre renders y `background-size/position` para SVG.
2. **Layout**: IFC ~34 ms (`format_line`/tokenización/line breaking), propuesta `layout_document_with_owned_styles` (~2,5 ms) + caché de familias.
3. **Web fonts**: weight/style para elegir cara, WOFF1, `unicode-range`/`font-display` (hoy bloqueante), subsetting, DB cacheada entre renders.
4. **Fase 6**: `document.onerror`, módulos ES, contabilidad de memoria real, provider de `getComputedStyle` cableado y event loop vivo en el shell.
5. **Fase 7**: formularios, pestañas, event loop real en el host, verificación de ventana con Xvfb; refresh con reloj virtual repite el primer frame de animaciones.
6. **Fase 5**: HTTP/2 y streaming, SameSite/PSL, `br`.
7. **Fase 4**: `background-repeat/size/position`, colores de `border-collapse`, block-in-inline.
8. **Fuzzing/bench**: targets `render`/`js`/**`svg`**, bench E2E de webfonts/SVG, cachear Boa en CI (+25 s) y prever el rebuild del cache de CI por `woff2-patched`.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust) para WOFF2**; `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image`/`svg` std (+`image` interno en `svg`); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js` para el corpus.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo (ADR JS aceptado 2026-11-10).
