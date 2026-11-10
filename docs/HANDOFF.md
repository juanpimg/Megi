# Handoff de sesión — ronda 6 (imágenes E2E, @import/media, DOM de navegador, shell/Fase 7, layout −43 %)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `13e9945` (árbol limpio)
**Estado**: **546 tests, 0 fallos**; gate de 4 suites PASS; Fase 4 funcionalmente cerrada, Fase 5 ≈70 %, Fase 6 planificada (DOM API lista), Fase 7 ≈20 % (ventana + headless).
**Matriz**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · **Rendimiento**: [`PERFORMANCE.md`](PERFORMANCE.md) · **Fuzzing**: [`FUZZING.md`](FUZZING.md) · **Plan**: [`ROADMAP.md`](ROADMAP.md) · **ADR JS (Fase 6)**: [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md).

## Qué se hizo en la ronda 6 (5 commits + docs)

| Commit | Bloque | Resultado |
|---|---|---|
| `960d84a` `feat(css)` | `@import` + Media Queries | `Stylesheet.imports: Vec<ImportRule{url,media}>`; `parse_media_query_list` y `media::evaluate` (and/or/not/only, listas, tipos all/screen/print, width/height/min/max en px/em/rem/pt/pc/in/cm/mm, orientation); `StyleCollection::add_import_stylesheet` con gate de media. **51 tests** css. El `render` aún no consume `@import`. |
| `78aee66` `perf(layout)` | Optimización y drop iterativo | **−43 %** en `layout/realistic-1mb` (100,6→57,2 ms) vía `Arc<ComputedStyle>` en tokens IFC, line breaking en streaming, `format_line` de una pasada, post-pase omitido sin overflow y sin lowercase por elemento; `LayoutBox` con **`Drop` iterativo** (50.000 niveles, pila 256 KiB; la construcción profunda sigue recursiva). **121 tests** layout. |
| `deb2cdd` `feat(dom,html)` | DOM de navegador | `document_element/head/body`, `get_element_by_id`, `get_elements_by_tag_name/class_name`, factorías, atributos, `children_ids/parent_element/text_content/set_text_content/remove_child/insert_before_child/append_text_node/replace_all_children/clone_node_into`, `DomError`; html `parse_fragment_into`, `TreeBuilder::with_document`, `set_inner_html`/`set_outer_html`/`document_write` (contextos table/select/template/rawtext, límites, sin fugas de slots). **81 tests dom+html**. Sin `querySelector`/eventos/CSSOM/JS. |
| `c1e5e14` `feat(paint,render)` | Imágenes `<img>` E2E | Paint: `ImageStore = HashMap<NodeId, Arc<image::RgbaImage>>`, `DisplayItem::Image`, `build_display_list_with_images`, placeholder gris y raster **bilineal clamp-to-edge** con AA/alfa/clip. Render: carga `<img src>` por `net` (respeta `--no-network`), decodifica PNG (`crates/image`; otros formatos ⇒ warning + placeholder no fatal), inyecta tamaños intrínsecos y pinta (caps 64 imágenes / 8 MiB / 32 MiB). Fixture `tests/fixtures/render/images.html` + `assets/px.png` (82 B) → 800×600, **15.625 B `md5 1de31699…`**; `basic.rgba` intacto (`60f8984e…`). **+14 tests** (paint 84, render 28). |
| `13e9945` `feat(shell)` | Fase 7: ventana + headless | Nuevo crate `crates/shell`: `winit` 0.30.13 (X11/Wayland dlopen) + `softbuffer` 0.4.8, binario `megi <entrada> [--width --height --screenshot PATH --title]`; ventana `0RGB`, cierre/q/Esc/r, resize con debounce 150 ms, errores a stderr+frame blanco; **`--screenshot` headless sin EventLoop** (CI) = PNG determinista 640×400 `md5 7324e851…` idéntico al pipeline `render`; `tools/megi.sh`. **20 tests**. Ventana no verificada visualmente en este host (sin servidor gráfico); compila y entra en el event loop. |
| `fa98f64` `docs(perf)` | Baseline de rendimiento | Refresco de `docs/PERFORMANCE.md` tras la ronda 5 e investigación del coste de layout (la causa era el IFC, no las tablas: **eliminado el WP candidato de "optimización de tablas"**; el trabajo de layout de la ronda 6 lo cubre). |

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO + API de navegador (búsquedas, mutaciones, `DomError`) | 28 |
| `html` | COMPLETO + `innerHTML`/`outerHTML`/`document.write` | 53 |
| `css` | COMPLETO + `@import`/Media Queries completas | 51 |
| `layout` | COMPLETO Fase 4; rendimiento −43 %, `Drop` iterativo | 121 |
| `fonts` | COMPLETO (base LTR) | 13 + 1 doctest |
| `paint` | COMPLETO Fase 4 + imágenes (`ImageStore`/bilineal) | 84 |
| `net` | Fase 5: URL/HTTP1.1/TLS/caché/`data:`/file/MIME/loader | 85 |
| `image` | Decoder PNG + inflate propios | 45 + 1 doctest |
| `render` | Pipeline URL→PNG + CSS externo + **imágenes E2E** | 28 |
| `shell` | Fase 7: ventana winit+softbuffer + headless screenshot | 20 |
| `bench` | Benchmarks deterministas + baseline | 8 |
| `test_harness` | Gate de 4 suites + allowlists | 7 |

**Total workspace: 546 tests, 0 fallos.** MSRV Rust 1.88; `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (4 targets en CI nocturna).

## Comandos de verificación exactos

```bash
export CARGO_HOME=/home/jp/Escritorio/Proyectos/.cargo-home   # sólo en el sandbox del equipo
cd /home/jp/Escritorio/Proyectos/Megi

# Puerta única
./tools/dev.sh all

# Equivalente paso a paso
cargo +1.88 check --workspace --all-targets     # incluye shell/winit
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                          # 546 tests
cargo doc --workspace --no-deps
cargo run -p test_harness -- --summary          # gate 4 suites: PASS, exit 0
cargo run -p test_harness -- --summary --strict # exit 1 por fallos conocidos

# Imágenes E2E (paint+image+render)
cargo test -p paint -p image -p render
cargo run -p render -- tests/fixtures/render/images.html /tmp/images.png
ls -l /tmp/images.png                         # 15.625 B; md5 1de31699…
python3 tools/png_check.py /tmp/images.png

# Shell: ventana nativa o screenshot headless
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/megi-shot.png --width 640 --height 400
md5sum /tmp/megi-shot.png                     # 7324e851… (idéntico a render)
tools/megi.sh https://example.com/            # requiere X11/Wayland

# Pipeline de Fase 4 + red + rendimiento + fuzz
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
cargo test -p net
./tools/bench.sh
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
```

## Deuda técnica priorizada (siguiente ronda)

1. **Integrar `@import` (y media completa) en `render`** *(WP-5.8)*: el parser/evaluador CSS ya existe; falta que el pipeline resuelva imports (dedup, ciclos, caps) y evalúe `media` con el viewport real por hoja.
2. **`background-image` y `border-image`** *(WP-4.9b)*: conectar `crates/image`/`net` a `url(...)` con `background-repeat/position/size` básicos y `border-image` fuera de alcance.
3. **Tablas avanzadas** *(WP-4.8b)*: conflictos de `border-collapse`, `rowspan`, `table-layout: fixed`, `inline-table` real y optimización del auto layout.
4. **Fase 7**: scroll, **hit-testing** (`NodeId`/`LayoutBox` ↔ `DisplayItem`), navegación/historial, inputs de ratón/teclado, zoom/HiDPI (device-scale), render en worker con cancelación y pruebas de ventana reales en CI con Xvfb.
5. **Robustez de layout**: construcción profunda sigue recursiva (solo el `Drop` es iterativo); presupuestos de tiempo y coherencia `ParseOptions::max_nodes` (1M) vs `MAX_SLOTS` (1.048.575).
6. **Fase 6 (JS) — decisión pendiente del dueño**: el ADR [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) recomienda **Boa 0.21.x** (pure Rust, MSRV 1.88, Unlicense OR MIT) con feature `js` opcional; alternativas QuickJS/rquickjs (MIT, C/FFI) o diferir. Preguntas abiertas: motor, feature on/off por defecto y alcance inicial (`document.write`+bindings ≈14 días vs ES/DOM más amplio ≈25). No crear `crates/js` ni tocar `Cargo.toml` hasta la decisión; el DOM ya expone `innerHTML`/`document.write` y handles generacionales para los bindings.
7. **Rendimiento**: coste de compresión PNG en pintados grandes, glyph cache global, presupuesto del pipeline HTML→PNG y benchmarks de tablas/imágenes.
8. **Fuzzing/seguridad**: añadir targets `image` (PNG/inflate) y `render`; mantener límites de imagen (caps) y de red; revisar `winit`/`softbuffer` como superficie de ventana.
9. **Fixtures/goldens**: `real_page.html`/`torture.html` con tablas, imágenes y CSS externo; goldens con tolerancia para AA; capturas nuevas en `docs/images/`.
10. **Documentación**: mantener README/ROADMAP/ARCHITECTURE/HANDOFF/PHASE4-SUPPORT/TOOLING/SECURITY/CONTRIBUTING/CHANGELOG sincronizados por ronda (esta tarea).

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` es la excepción tipográfica autorizada (§3.1); `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image` es std puro; `render` no añade deps externas; `shell` usa `winit`/`softbuffer` autorizados (§3.1) y el resto de crates internos.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/` y `.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3; codificador (paint) y decoder (image) PNG propios por decisión D3.
