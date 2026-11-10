# Handoff de sesión — ronda 3 (Fase 4 casi completa + Fase 5 arrancada)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `14500e6` (+ `532bb6a`, captura de documentación)
**Estado**: árbol limpio; **301 tests, 0 fallos**; gate de conformidad con 4 suites en PASS; Fase 4 ≈90 % y Fase 5 ≈10 %.
**Matriz de soporte**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · **Rendimiento**: [`PERFORMANCE.md`](PERFORMANCE.md) · **Plan**: [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 3 (5 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `4613ff0` `fix(layout)` | Conformidad IFC | Saltos solo en oportunidades reales (nodos inline adyacentes sin espacio ya no se separan), soft hyphen invisible de ancho 0 (el `-` solo se materializa al romper), `line-height` numérico heredado como factor, `vertical-align: top/bottom` con *aligned subtree* §10.8.1 y cálculo de line-height unificado, `text-align: justify` en líneas soft-wrap. `layout`: **80 tests**. |
| `5c511ee` `feat(paint)` | Rasterizado de calidad + bordes | Rasterizador con **culling** y cachés por pasada (shaped runs, caras, máscaras de glifo), **AA exacta** de rectángulos fraccionarios, alfa straight, **bordes por lado** (color/ancho/estilo `solid`/`dashed`/`dotted`/`double`/`none|hidden`, esquinas a mitre) y fondo bajo el *border box*; golden regenerado determinista. `paint`: **52 tests**. |
| `dc8aef0` `feat(html)` | Serializador público §13.3 | `html::serialize_html`, `serialize_fragment` (`innerHTML`), `serialize_outer_html` y `*_with_options(HtmlSerializeOptions { scripting_enabled })`; las 5 suites `html5lib-tests/serializer` entran en el gate (**230/230**; 18 alternativas XHTML registradas, no ejercitadas). Tokenizer 7050/7050 con allowlist vacía. `html`: 14 unit + 6 rendimiento + 13 robustez. |
| `14500e6` `feat(net)` | Fase 5 bootstrap | Crate `net` **std-only** y `#![forbid(unsafe_code)]`: URL WHATWG subset (`http`/`https`/`file` + `join`), HTTP/1.1 `GET` sobre `std::net` (timeouts, `Content-Length`, *chunked*, redirects 301/302/303/307/308 con límite, caps 16 MiB/64 KiB), `file://`, MIME sniffing WHATWG subset y `ResourceLoader` con caché. HTTPS ⇒ `TlsUnsupported` (TLS = siguiente WP). **57 tests offline**. |
| `532bb6a` `docs` | Captura de Fase 4 | `docs/images/phase4-basic.png` (22,9 KB, 800×600), render comprimido del pipeline con tipografía real, fondo propagado y bordes por lado; usada en el README. |

Las rondas anteriores (robustez anti-DoS, pipeline PNG inicial, fonts, gate reproducible, CSS/layout de Fase 4, benchmarks) siguen vigentes; ver `CHANGELOG.md` §Unreleased.

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO (arena, mutaciones O(1), robustez de API) | 9 |
| `html` | COMPLETO declarativo + serializador §13.3 + anti-DoS | 14 unit + 6 rendimiento + 13 robustez |
| `css` | COMPLETO (Fase 2) + bases/selectores/`@media`/índice de Fase 4 | 41 |
| `layout` | BFC/IFC completos; faltan posicionados, floats, tablas e `inline-block` | 80 |
| `fonts` | COMPLETO (base LTR) e **integrado** en layout/paint | 13 + 1 doctest |
| `paint` | Fase 4 ≈90 %: pipeline PNG real determinista, AA, bordes por lado; faltan stacking/clip/compresión | 52 |
| `net` | FASE 5 BOOTSTRAP: URL/HTTP1.1/file/MIME/loader; sin TLS | 57 (56 + 1 doctest) |
| `bench` | Arnés de benchmarks determinista y baseline publicada | 8 |
| `test_harness` | Gate de 4 suites + allowlists | 7 |

**Total workspace: 301 tests, 0 fallos.** MSRV declarada y verificada: **Rust 1.88** (`let-chains`).

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
cargo test --workspace                                   # 301 tests
cargo doc --workspace --no-deps
cargo run -p test_harness -- --summary                   # gate 4 suites: PASS, exit 0
cargo run -p test_harness -- --summary --strict          # exit 1 por fallos conocidos

# Pipeline HTML → PNG (release) y validación
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
cargo test -p paint --test end_to_end_html --test png_format --test raster_quality

# Rendimiento (release; baseline en docs/PERFORMANCE.md)
./tools/bench.sh
./tools/bench.sh --scale 0.25 --filter paint

# Red (offline)
cargo test -p net
```

Evidencia medida en esta ronda: PNG del pipeline 800×600, 1.920.813 B, `md5 d97c67dc107313749a6cd28f161e0f63`; golden `basic.rgba` `md5 4139fdd8fec9364b03139d8d7cf32a8b`; gate PASS exit 0 (`--strict` exit 1); raster 800×600 1598,8 → 0,9 ms, render-to-PNG 1628 → 7,7 ms, pipeline 1842 → 170,7 ms (**10,8×**).

## Deuda técnica priorizada (siguiente ronda)

1. **WP-4.6 — `position`/`z-index`/stacking/`opacity`** *(máxima prioridad de fidelidad)*: hoy un `position: absolute` se pinta en el flujo normal; requiere contenedores de posicionados, posición estática y árbol de stacking con las fases del Apéndice E; `opacity` necesita grupos offscreen.
2. **WP-4.8 — tablas CSS 2.1**: `display: table*` se apila como bloques; faltan columnas/filas automáticas, `colspan`/`rowspan`, `border-collapse` y pintado de tablas; añadir pruebas de fecha/tablas al corpus.
3. **WP-4.10 — clipping desde `overflow`**: el rasterizador ya tiene `ClipStack`/`PushClip`/`PopClip`, pero el painter no genera clips; falta propagar `overflow` en layout y emitir clips balanceados.
4. **WP-4.7 / WP-4.9 — floats/`clear` e `inline-block`/listas/decoraciones/reemplazados**: `float`/`clear` se parsean pero no se aplican; `inline-block` sigue siendo inline normal; marcadores y `text-decoration` no se pintan; `<img>` depende de Fase 5.
5. **Fase 5 — TLS y caché HTTP**: integrar `rustls`+`webpki-roots` (WP-5.3), política de caché HTTP (`Cache-Control`/ETag, WP-5.4) y Fetch/CORS + carga de subrecursos (WP-5.5). `net` es std-only hoy.
6. **Rendimiento/robustez**: DOM identidad/memoria (auditoría **F8**), recursión de layout/paint en documentos muy profundos (**F6**), presupuestos de tiempo y `Result` en la API de layout; evitar el `Vec<ComputedStyle>` clonado por caja.
7. **PNG y raster**: compresión PNG dentro del codificador (hoy *stored*, ~1,92 MB a 800×600) o enmienda de política para `flate2`/`png`; evaluar premultiplicado interno; **glyph cache global** (hoy caché por pasada) y atlas de glifos.
8. **Cobertura**: ampliar fixtures de fecha/tablas, `real_page.html`/`torture.html` + goldens cuando existan posicionamiento/tablas; subir la cobertura An+B del gate (8 casos curados de 67 upstream); pruebas de fecha/zona horaria solo si el motor acaba exponiendo `Date`.
9. **Documentación**: `docs/images/` versionadas y referenciadas; mantener `PERFORMANCE.md` (otra tarea) y `PHASE4-SUPPORT.md` sincronizados por ronda; matriz de soporte de Fase 5 al cerrar TLS.
10. **CI**: añadir el job de benchmarks como *smoke* opcional (`--scale 0.1`) para detectar regresiones de complejidad sin alargar el pipeline principal.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` es la excepción tipográfica autorizada (§3.1) y `net`/`bench` son std-only. `test_harness` sólo usa `serde`/`serde_json`.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Todo cambio funcional necesita tests en el mismo commit; *Conventional Commits*.
- No commitear/pushear desde agentes: el Lead integra y revisa.
- `tests/html5lib-tests` es upstream versionado; de `tests/wpt/` sólo se versiona `tests/wpt/css/css-syntax/`; `.agents/` y `.cache/` están ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo; el PNG propio sigue la decisión D3 (§3.2).
