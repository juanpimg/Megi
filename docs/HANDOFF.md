# Handoff de sesión — ronda 5 (Fase 4 ≈99 % + pipeline de navegador + PNG comprimido + DOM generacional)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `49a2792` (árbol limpio)
**Estado**: **475 tests, 0 fallos**; gate de 4 suites PASS; Fase 4 ≈99 % y Fase 5 ≈60 % (pipeline de navegador real).
**Matriz**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · **Rendimiento**: [`PERFORMANCE.md`](PERFORMANCE.md) · **Fuzzing**: [`FUZZING.md`](FUZZING.md) · **Plan**: [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 5 (6 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `6c80517` `feat(css)` | Tablas CSS 2.1 | `Display::{InlineTable, TableRowGroup, TableHeaderGroup, TableFooterGroup, TableColumnGroup, TableColumn, TableCaption}`; `ComputedStyle::{border_spacing, border_collapse, table_layout, caption_side, empty_cells}`; hoja UA §15 con reglas de tabla. **+3 tests** → css **44**. |
| `2e1ce10` `feat(layout)` | Tablas/inline-block/reemplazados | Tablas §17 (auto layout min/max-content, `border-spacing`, `colspan`, fixups anónimos, `vertical-align` de celdas, caption/row-groups); `inline-block` (shrink-to-fit, baseline, vertical-align); `BoxType::Replaced` con tamaños intrínsecos (`img` 300×150, `input` 20×20, `textarea` 160×40) y APIs `*_with_replaced_sizes`. **+16 tests** → layout **118**. |
| `dd6c952` `feat(dom)` | DOM generacional | `NodeId` 4 B = **20 bits slot + 12 bits generación**, free list LIFO con reutilización de slots, `remove_node`/`remove_subtree`, rechazo de handles stale/foráneos (`owns`, `try_node`), `descendants`/`ancestors` iterativos con guardas de ciclo (100k), `validate_invariants`, `estimate_bytes/allocated/free_slots`. **+11 tests** → dom **20**. |
| `cd1e55b` `feat(paint)` | Compresión PNG + painter iterativo | **DEFLATE propio** (LZ77 hash-chains 32 KiB + Huffman fijo BTYPE=01, zlib/Adler) + filtros PNG adaptativos: **1.920.813 → 42.853 B (44,8×)**, `basic.rgba` idéntico (`md5 60f8984e…`), PNG `md5 9e957177…`; `build_display_list` **totalmente iterativo** (20.000 niveles; caps stacking 256/floats 64/items 5M); render-to-PNG 8,6 → 14,2 ms por compresión. **+14 tests** → paint **77**. |
| `49a2792` `feat(render,image)` | Pipeline de navegador + decoder PNG | **`crates/render`**: `render_url` URL→PNG con `net` (file/data/http/https TLS+caché), charset UTF-8/Windows-1252, `<base>`, `<style>`+`<link rel=stylesheet>` externos (dedup, `media` screen/all, caps 16 MiB/32 hojas/1 MiB/hoja/4 MiB total), cascada UA+autor, layout con fuentes bundled, stats por etapa y CLI `render <entrada> <salida.png> [--width/--height/--base/--no-network/--json]`; fixture 640×400 → **15.498 B `md5 7324e851…`**. **`crates/image`**: decoder PNG propio + **inflate zlib/DEFLATE completo** (stored/fixed/dynamic, 32 KiB, Adler), color types 0/2/3/4/6, bit depth 1–16, filtros 0–4, Adam7, CRC, `PngLimits`; decodifica el PNG del pipeline byte a byte contra `basic.rgba`. **+46 tests** → render 22, image 46. |
| `8f273f0` `docs` | Docs ronda 4 | Sincronización previa (commit del Lead con la documentación de R24). |

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO: arena generacional, free list, identidad, invariantes | 20 |
| `html` | COMPLETO declarativo + serializador §13.3 + anti-DoS | 14 + 6 + 13 |
| `css` | COMPLETO (Fase 2) + tablas CSS 2.1 + selectores/`@media`/índice | 44 |
| `layout` | BFC/IFC, posicionamiento, floats, overflow, **tablas**, `inline-block`, reemplazados | 118 |
| `fonts` | COMPLETO (base LTR) e integrado | 13 + 1 doctest |
| `paint` | Fase 4 completa: Ap. E, opacity, clip, decoraciones, marcadores, **PNG comprimido propio**, painter iterativo | 77 |
| `net` | Fase 5: URL/HTTP1.1/TLS/caché/`data:`/file/MIME/loader | 85 |
| `image` | NUEVO: decoder PNG + inflate propios (std puro) | 45 + 1 doctest |
| `render` | NUEVO: pipeline de navegador URL→PNG con CSS externo | 21 + 1 doctest |
| `bench` | Benchmarks deterministas y baseline publicada | 8 |
| `test_harness` | Gate de 4 suites + allowlists | 7 |

**Total workspace: 475 tests, 0 fallos.** MSRV verificada: **Rust 1.88**; `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` es un workspace aparte (4 targets en CI nocturna, ~1,06 M ejecuciones ASan / 0 crashes).

## Comandos de verificación exactos

```bash
export CARGO_HOME=/home/jp/Escritorio/Proyectos/.cargo-home   # sólo en el sandbox del equipo
cd /home/jp/Escritorio/Proyectos/Megi

# Puerta única: fmt, clippy -D warnings, tests, doc y gate de conformidad
./tools/dev.sh all

# Equivalente paso a paso
cargo +1.88 check --workspace --all-targets
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                   # 475 tests
cargo doc --workspace --no-deps
cargo run -p test_harness -- --summary                   # gate 4 suites: PASS, exit 0
cargo run -p test_harness -- --summary --strict          # exit 1 por fallos conocidos

# Pipeline de navegador (Fase 5): fixture offline con CSS externo
cargo run -p render -- tests/fixtures/render/basic.html /tmp/render.png --width 640 --height 400 --json
python3 tools/png_check.py /tmp/render.png
bash tools/render_fixture.sh /tmp/megi-render-fixture.png    # + md5
cargo test -p render -p image

# Pipeline de Fase 4 (paint) y compresión PNG
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
ls -l /tmp/basic.png        # 42.853 B; md5 9e957177…
python3 tools/png_check.py /tmp/basic.png
cargo test -p paint

# Red (offline, TLS por defecto)
cargo test -p net

# Rendimiento
./tools/bench.sh
./tools/bench.sh --scale 0.25 --filter paint

# Fuzzing (workspace aparte; nightly + ASan)
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
```

Evidencia: PNG paint **42.853 B** `md5 9e95717795744335bc0dfc7cade782ee`; golden `basic.rgba` `md5 60f8984ecf01e0db67976867172c0eee`; render fixture **15.498 B** `md5 7324e8512e831de132b1172965332a73` (canvas verde por CSS externo resuelto con `<base>`); gate PASS exit 0.

## Deuda técnica priorizada (siguiente ronda)

1. **Pintar imágenes `<img>` y `background-image`** *(última pieza de presentación de Fase 4)*: conectar `crates/image` al pipeline (fetch con caché/caps, tamaño intrínseco, pintado, `alt`, `background-repeat/position/size` básicos) → WP-4.9b / WP-5.7.
2. **Tablas avanzadas (WP-4.8b)**: resolución de conflictos de `border-collapse`, `rowspan`, `table-layout: fixed`, `inline-table` real y optimización del auto layout (coste min/max-content).
3. **Robustez de layout (WP-4.12b)**: drop iterativo de `LayoutBox` en árboles muy profundos, presupuestos de tiempo, y coherencia `ParseOptions::max_nodes` (1M) vs `MAX_SLOTS` (1.048.575).
4. **CSS externo avanzado (WP-5.8)**: `@import` encadenado con dedup/ciclos, media queries completas, `@font-face` si la política lo permite.
5. **Red avanzada (WP-5.9)**: CORS/`Referrer-Policy`, cookies, `Content-Encoding` gzip/br, HTTP/2, streaming y proxies.
6. **DOM/identidad para JS (Fase 6)**: API pública de mutación/traversal, wrappers JS ↔ `NodeId` generacional, tabla lateral por mundo; el DOM ya está preparado (generaciones/free list).
7. **Fase 7 (`crates/shell`)**: ventana/eventos/scroll/hit-testing reutilizando `Pixmap`; mapear `NodeId`/`LayoutBox` ↔ `DisplayItem` para hit-testing.
8. **Rendimiento**: medir layout con tablas grandes y el coste de compresión PNG; caché de glifos global; optimizar reintentos/huffman adatativo en el DEFLATE si hace falta.
9. **Cobertura**: fixtures `real_page.html`/`torture.html` con tablas e imágenes, goldens con tolerancia, ampliar corpus de fuzz a `render`/`image` (decoder PNG es superficie no confiable).
10. **CI**: job de benchmarks smoke (`--scale 0.1`), fuzz de `image` (PNG/inflate) y mantener docs sincronizadas por ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` es la excepción tipográfica autorizada (§3.1); `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image` es std puro sin deps; `render` no añade deps externas. `bench`/`test_harness` usan internos + `serde`.
- `#![forbid(unsafe_code)]` en todos los crates del motor; `fuzz/` es un workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Todo cambio funcional necesita tests en el mismo commit; *Conventional Commits*.
- No commitear/pushear desde agentes: el Lead integra y revisa.
- `tests/html5lib-tests` es upstream versionado; de `tests/wpt/` sólo se versiona `tests/wpt/css/css-syntax/`; `.agents/` y `.cache/` están ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3; codificador (paint) y decoder (image) PNG son propios por decisión D3 (§3.2).
