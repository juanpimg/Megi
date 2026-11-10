# Handoff de sesión — ronda 4 (Fase 4 ≈97 % + Fase 5 ≈35 % + fuzzing en CI)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `3cc8449` (árbol limpio)
**Estado**: **362 tests, 0 fallos**; gate de conformidad de **4 suites** en PASS; Fase 4 ≈97 % y Fase 5 ≈35 %.
**Matriz de soporte**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · **Rendimiento**: [`PERFORMANCE.md`](PERFORMANCE.md) · **Fuzzing**: [`FUZZING.md`](FUZZING.md) · **Plan**: [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 4 (5 commits + 1 chore)

| Commit | Bloque | Resultado |
|---|---|---|
| `d4dd2be` `feat(layout)` | Posicionamiento, floats, overflow | `position: relative/absolute/fixed` con *containing blocks*, posición estática y shrink-to-fit (`sticky` documentado como relative sin scroll); `float: left/right` + `clear` §9.5 (fuerte en BFC §10.6.7, *blockification*, shrink-to-fit); `overflow` crea BFC y propaga un **clip rect**; `LayoutBox` gana `position`, `z_index`, `opacity`, `establishes_stacking_context`, `clip`, `is_float`, `float_side`. **+22 tests** (10 posicionamiento + 12 floats/clip) → layout **102**. |
| `3cc8449` `feat(paint)` | Apéndice E, opacity, clip, decoraciones, marcadores | Orden completo de CSS 2.2 Ap. E con stacking contexts y `z-index` (positivo/negativo/auto), `DisplayItem::OpacityGroup` (offscreen + composición única), `PushClip`/`PopClip` desde `overflow`, `text-decoration` (underline/overline/line-through), `visibility: hidden` y marcadores `disc`/`circle`/`square`/`decimal`. Golden regenerado: `basic.rgba` `md5 60f8984e…`, PNG `md5 34152025…`, **26 display items**. **+11 tests** (6 stacking + 5 decoraciones) → paint **63**. |
| `4015950` `feat(net)` | TLS + caché HTTP + `data:` | **TLS 1.2/1.3 con `rustls` 0.23 (backend ring) + `webpki-roots`** (feature `tls` default; SNI, validación de cadena/hostname, `FetchOptions::tls_config` para CA privada; `rcgen` sólo dev-dependency), **caché HTTP RFC 9111 subset** (`max-age`/`Expires`/`ETag`/`Last-Modified`/`304`/`no-store`/`Vary`, `parse_http_date`), **`data:` URLs** con Base64 propio y dispatch en `ResourceLoader`. **+22 tests** → net **85**. |
| `83dc90f` `test(fuzz)` | Fuzzing + CI nocturna | Workspace `fuzz/` autocontenido (cargo-fuzz/libFuzzer, target nightly+ASan): 4 targets (`html_parse`, `css_parse`, `selectors`, `net_url`), corpus semilla versionado (17 ficheros), `.github/workflows/fuzz-nightly.yml` (cron diario 03:17 UTC + dispatch, 300 s/target) y [`FUZZING.md`](FUZZING.md). Campaña local: **~1,06 M ejecuciones ASan, 0 crashes**. |
| `fc16bab` `chore` | `forbid(unsafe_code)` | `#![forbid(unsafe_code)]` en **todos** los crates del motor (`dom`, `html`, `css`, `fonts`, `layout`, `paint`, `net`, `test_harness`, `bench`). |

Las rondas anteriores (robustez anti-DoS, pipeline PNG, fonts, gate, CSS/layout de Fase 4, benchmarks, serializer, bootstrap de red) siguen vigentes; ver `CHANGELOG.md` §Unreleased.

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO (arena, mutaciones O(1), robustez de API) | 9 |
| `html` | COMPLETO declarativo + serializador §13.3 + anti-DoS | 14 unit + 6 rendimiento + 13 robustez |
| `css` | COMPLETO (Fase 2) + selectores L4/`@media`/índice de reglas | 41 |
| `layout` | BFC/IFC + posicionamiento/floats/overflow; faltan tablas e `inline-block` | 102 |
| `fonts` | COMPLETO (base LTR) e integrado en layout/paint | 13 + 1 doctest |
| `paint` | Fase 4 ≈95 %: Ap. E completo, opacity, clip, decoraciones, marcadores; falta compresión PNG | 63 |
| `net` | Fase 5 ≈35 %: URL/HTTP1.1/TLS/caché/`data:`/file/MIME/loader; faltan Fetch/CORS e integración | 85 |
| `bench` | Arnés de benchmarks determinista y baseline publicada | 8 |
| `test_harness` | Gate de 4 suites + allowlists | 7 |

**Total workspace: 362 tests, 0 fallos.** MSRV verificada: **Rust 1.88**. Sin `unsafe` en el motor; el tooling de fuzzing vive en un workspace aparte.

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
cargo test --workspace                                   # 362 tests
cargo doc --workspace --no-deps
cargo run -p test_harness -- --summary                   # gate 4 suites: PASS, exit 0
cargo run -p test_harness -- --summary --strict          # exit 1 por fallos conocidos

# Pipeline HTML → PNG (release) y validación
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
cargo test -p paint --test end_to_end_html --test png_format --test raster_quality \
    --test stacking_order --test decorations

# Red (offline, TLS incluido por defecto)
cargo test -p net
cargo test -p net --no-default-features   # sin feature tls

# Rendimiento
./tools/bench.sh
./tools/bench.sh --scale 0.25 --filter paint

# Fuzzing (workspace aparte; nightly + ASan)
rustup toolchain install nightly --profile minimal && cargo install cargo-fuzz --locked
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
```

Evidencia medida: PNG del pipeline 800×600, 1.920.813 B, `md5 3415202548ee0d8f6af2c076cbca030d`, 26 display items; golden `basic.rgba` `md5 60f8984ecf01e0db67976867172c0eee`; gate PASS exit 0 (`--strict` exit 1); fuzzing ~1,06 M ejecuciones ASan con 0 crashes.

## Deuda técnica priorizada (siguiente ronda)

1. **WP-4.8 — tablas CSS 2.1** *(único WP de layout pendiente)*: `display: table*` sigue apilándose como bloques; faltan columnas/filas automáticas, `colspan`/`rowspan`, `border-collapse` y pintado de tablas. Añadir fixtures/goldens de tabla.
2. **WP-4.9b — `inline-block`/block-in-inline y reemplazados en layout**: `inline-block` sigue siendo inline normal (sin shrink-to-fit ni baseline propia); block-in-inline no se normaliza; `<img>`/reemplazados dependen de la Fase 5 (decodificación) y de un modelo de tamaño intrínseco.
3. **WP-4.11b — compresión PNG** dentro del codificador propio (hoy *stored*, ~1,92 MB a 800×600) o enmienda de política para `flate2`/`png`; opcional premultiplicado interno.
4. **WP-4.12b — presupuestos/iteratividad**: eliminar la recursión de layout y paint en documentos muy profundos, presupuestos de tiempo y API de layout con `Result` en vez de `Option`.
5. **DOM identidad/memoria (auditoría F8)**: memoria por caja (clon completo de `ComputedStyle`), identidad de nodos para hit-testing futuro y reciclaje/reutilización en layout/paint.
6. **Fase 5**: Fetch/CORS y `Referrer-Policy` (WP-5.5), **integración de recursos en el pipeline de render** (`<link rel=stylesheet>`, `@import`, después imágenes; WP-5.6), cookies/streaming/compresión/HTTP2 (WP-5.7). El caché y los `data:` ya están; TLS y trust roots también.
7. **Fase 4 cierre documental**: `real_page.html`/`torture.html` + goldens de tablas/inline-block; comparación con tolerancia si se añade más AA; matriz de Fase 5 al cerrar Fetch.
8. **Fase 7 — shell**: ventana/eventos/scroll/hit-testing reutilizando `Pixmap`; diseñar el mapeo `NodeId↔DisplayItem` para hit-testing.
9. **Glyph cache global**: la caché de glifos es por pasada; evaluar caché global/atlas para páginas grandes.
10. **CI**: job de benchmarks *smoke* opcional (`--scale 0.1`) y mantener el workflow de fuzz con presupuesto de 5 min/target.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` es la excepción tipográfica autorizada (§3.1), `net` usa sólo `rustls`/`webpki-roots` (producción) y `rcgen` (tests), y `bench`/`test_harness` usan únicamente crates internos + `serde`/`serde_json`.
- `#![forbid(unsafe_code)]` en todos los crates del motor. El fuzzing (`fuzz/`) es un workspace **aparte** y no contamina el build estable ni el lockfile del motor.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Todo cambio funcional necesita tests en el mismo commit; *Conventional Commits*.
- No commitear/pushear desde agentes: el Lead integra y revisa.
- `tests/html5lib-tests` es upstream versionado; de `tests/wpt/` sólo se versiona `tests/wpt/css/css-syntax/`; `.agents/` y `.cache/` están ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo; el PNG propio sigue la decisión D3 (§3.2).
