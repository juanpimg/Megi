# Handoff de sesión — ronda 15 (SVG inline, background-* geometry, IFC −28 %, determinismo)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `b808d7b` (árbol limpio salvo untracked ajenos al repo versionado)
**Estado**: **990 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; Fase 4 (layout/paint/tipografía/imágenes/backgrounds/SVG) prácticamente completa, **Fase 5 ≈95 %**, **Fase 6 ≈80-85 %**, **Fase 7 ≈70 %**. Rendimiento: **layout/realistic-1mb 50,5 → 36,3 ms (−28 %)**, **pipeline 132,9 → 120,5 ms (−9,3 %)**; goldens byte-idénticos.
**CI/Repo**: GitHub Actions **en verde** (quality + conformance + MSRV; deps Wayland/X11/xcb en el runner). `main` y `sprint/robustez-fase4` sincronizadas en `b808d7b`. La suite es **determinista**: CI cazó un uso de reloj de pared en el event loop y quedó corregido.
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md) · [`FUZZING.md`](FUZZING.md) · [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 15 (4 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `b5b07b3` `feat(css,paint)` | **Background geometry** | `background-repeat` (repeat/repeat-x/repeat-y/no-repeat/space/round y pares), `background-size` (`auto`/`cover`/`contain`/longitudes/%), `background-position` (keywords/%/px, 1-2) y shorthand `background:` con position/size; paint con `background_tile_size`/`background_tiles` (cover/contain centrado, tiling con **clip al padding box**, **cap 4096**; space/round pintan como repeat, documentado) y **`background_geometry_explicit`** que mantiene el modo legado ⇒ goldens intactos (showcase `c95c7eff…`, `basic.rgba` `60f8984e…`, PNG `9e957177…`). CSS 61→**69**, paint 109→**122** tests. |
| `c267d45` `feat(render)` | **SVG inline** | Pre-proceso tras los scripts y antes de styles/layout: serializa cada `<svg>…</svg>` (con `xmlns` reparado), lo convierte a `data:` URL base64 y lo sustituye por un `<img>` sintético copiando `class/id/style/width/height/alt/aria/data-*`; **límites 256 SVG/documento, 1 MiB/SVG, 8 MiB total, profundidad 256** (warning no fatal), stat **`inline_svgs`**; fix del sniffing de doctype (HTML directo ≠ SVG). 7 fixtures `svg_inline_*` y **10 tests** ⇒ render 104→**114**. Límites: CSS interno/`<style>` del SVG, gradientes, filtros, `<text>`, animación y refs externas fuera del crate `svg`; selectores CSS sobre el `svg` interno no alcanzan al `<img>` sintético; enlaces `<a>` internos se pierden; redondeo ceil <1 px. |
| `9090175` `perf(layout)` | **IFC −28 %** | Tokenización con **pool de texto con rangos** (sin `String` por palabra), **tabla de estilos con ids** (1 `Arc` por nodo), métricas una vez por nodo, buffers pre-dimensionados; breaking **incremental** (sin O(n²)); **fast path de `format_line`** para líneas de un nodo de texto; memoización en `FontTextMeasurer` (familias/métricas/avances, tope 4 MiB). A/B intercalado: **layout 50,5→36,3 ms (−28 %)**, **pipeline 132,9→120,5 ms (−9,3 %)**; geometría/texto byte-idénticos. Layout sigue en **135 tests**. |
| `b808d7b` `fix(render)` | **Determinismo del event loop** | CI detectó que `run_event_loop` usaba `run_tasks` (reloj de pared): en máquinas lentas ejecutaba timers antes de tiempo (**10 vs 13 tasks**). Ahora usa **sólo `run_until` con reloj virtual** en pasos de 1 ms y `virtual_time_ms` por conteo de pasos; fixture `js_event_timeout` determinista (tasks=1, iterations=10, virtual=10 ms, **md5 `0768208824102e94a6f32574991b7692`**). Suite render **114** verde y reproducible. |

## Evidencia verificada

- `cargo test --workspace` → **990 tests, 0 fallos**: bench 8, **css 69**, dom 28, **fonts 17 + 1 doctest**, html 60, image 45+1, js 58, layout 135, net 180, **paint 122**, **render 114**, shell 100, svg 41, test_harness 11. fmt, clippy `-D warnings`, `cargo doc` y MSRV 1.88 OK.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, **árbol 3.904/3.904 (100 %)**, serializer **230/230**, CSS Syntax **30/30**; allowlists vacías.
- **Píxeles/evidencias**: showcase 1024×768 80.220 B `md5 c95c7eff…`; render basic 15.498 B `md5 7324e851…`; `basic.rgba` `md5 60f8984e…`; PNG phase4 42.853 B `md5 9e957177…` (legacy flag); webfont `f38aed03…` vs fallback `e2d76dee…`; event loop determinista `js_event_timeout` `md5 07682088…`.
- **Rendimiento**: layout 36,3 ms (mediana A/B 50,5→36,3; muestras 35,7-36,9) y pipeline 120,5 ms (132,9→120,5); `html/massive-end-p` ~19 ms; encoder PNG −30,5 % invariante.

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO: arena generacional, API de navegador | 28 |
| `html` | COMPLETO: parser 3.904/3.904, serializador §13.3, hook `ScriptExecutor` | 60 |
| `css` | COMPLETO + `@font-face`/`@import`/media/query API + **background-* geometry** | 69 |
| `layout` | COMPLETO Fase 4: tablas §17, `Arc<ComputedStyle>`, **IFC −28 %** | 135 |
| `fonts` | COMPLETO LTR + fuentes web @font-face/WOFF2 | 17 + 1 doctest |
| `paint` | COMPLETO Fase 4 + hit-testing + encoder PNG invariante + **backgrounds con tiling** | 122 |
| `net` | Fase 5: TLS/caché/data:/cookies/CORS/gzip | 180 |
| `image` | Decoder PNG + inflate propios | 45 + 1 doctest |
| `js` | Fase 6 WP-A/B/6.4 + eventos avanzados | 58 |
| `svg` | Parser+modelo+rasterizador AA; `<img>`/background e **inline** vía render | 41 |
| `render` | Pipeline URL→PNG + imágenes/backgrounds/scripts + event loop determinista + webfonts + SVG | **114** |
| `shell` | Fase 7: scroll/entrada/HiDPI/worker + navegación + zoom/auto-refresh/estado | 100 |
| `bench` | Benchmarks reproducibles | 8 |
| `test_harness` | Gate 4 suites (árbol 3.904/3.904 sin allowlist) + `JsScriptExecutor` | 11 |

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 990 tests, doc y gate
cargo test --workspace                              # 990 tests (con JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0

# Ronda 15: SVG inline y backgrounds
cargo run -p render -- tests/fixtures/render/svg_inline_simple.html /tmp/inline.png --json   # inline_svgs>0
cargo run -p render -- tests/fixtures/render/backgrounds.html /tmp/bg.png
cargo test -p render --test svg_tests -p render --test event_loop_tests

# Rendimiento (A/B intercalado)
cargo run --release -p bench -- --filter layout --iterations 5     # ~36 ms
cargo run --release -p bench -- --filter pipeline --iterations 3   # ~120 ms

# Determinismo y goldens
cargo test -p paint --test end_to_end_html      # basic.rgba byte-idéntico
cargo run -p render -- tests/fixtures/render/showcase/index.html /tmp/showcase.png --width 1024 --height 768   # c95c7eff…

# Rustdoc / MSRV
cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets
```

## Deuda priorizada

1. **IFC**: otra vuelta de perfil si se quiere bajar de ~35 ms (resto en tree build/drop y bloque); no tocar geometría.
2. **Backgrounds**: `background-origin`/`clip`/`attachment`, múltiples capas y gradientes (fuera de la ronda 15); `background-repeat: space/round` hoy pintan como repeat.
3. **SVG inline**: CSS interno del SVG (requiere ampliar `crates/svg` o aplicar estilos del documento al subárbol), gradientes/filtros/`<text>`, enlaces internos y cache por URL.
4. **Layout/DOM**: block-in-inline, colores de `border-collapse`, owned-styles/provider de `getComputedStyle`.
5. **Fase 6**: `document.onerror`, módulos ES, memoria real, provider cableado y event loop vivo en el shell.
6. **Fase 7**: formularios, pestañas, event loop real en el host y verificación de ventana con Xvfb.
7. **Fase 5**: HTTP/2 y streaming, SameSite/PSL, `br`.
8. **Fuzzing/bench**: targets `render`/`js`/`svg`, bench E2E de webfonts/SVG/inline, cachear Boa en CI (+25 s).

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust) para WOFF2**; `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image`/`svg` std (+`image` interno en `svg`); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js` para el corpus.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo (ADR JS aceptado 2026-11-10).
