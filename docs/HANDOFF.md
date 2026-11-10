# Handoff de sesión — ronda 8 (Fase 6 arrancada con Boa, red avanzada, backgrounds E2E, texto de tablas)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `31e5139` (árbol limpio)
**Estado**: **734 tests, 0 fallos**; gate de 4 suites PASS; Fase 4 funcionalmente completa, Fase 5 ≈90 %, **Fase 6 ≈10 % (WP-6.1 HECHO)**, Fase 7 ≈40 %.
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md) · [`FUZZING.md`](FUZZING.md) · [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 8 (5 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `f3d9eef` `feat(js)` | **Fase 6 WP-6.1** | Nuevo crate `js` con **Boa 0.21.1** (`#![forbid(unsafe_code)]`, feature `js` **ON por defecto** y stub `--no-default-features`): `JsRuntime` (eval persistente, console capture, `Budget`/`RuntimeLimits` anti-bucle, `JsError` tipado), **bindings DOM** (`document.write`, `getElementById`, `getElementsByTagName/ClassName`, `querySelector(All)`, `createElement/TextNode`, `documentElement/body/head/title/URL`; wrappers con `tagName/id/className/textContent/innerHTML/outerHTML`, atributos, `appendChild/insertBefore/removeChild/replaceChild`, `children/parentNode/firstChild/nextSibling`, `style` y `classList`; **NodeId stale ⇒ TypeError**, nunca panic) y scheduler base (`setTimeout/Interval` encolan, `run_tasks`/`run_microtasks`). **12 tests**. Aún **no integrado en `render`**. |
| `be72a25` `feat(paint)` | Backgrounds + texto de tablas | `BackgroundImages` y `build_display_list_with_images_and_backgrounds`; background-image estirada al *padding box* (color debajo) y `list-style-image` como marcador (~1em); **fix del texto de celdas de tabla** (`paint_inline_content` no descendía en cajas de tabla; ahora sí). 9 tests nuevos → **97** paint; `basic.rgba` sin cambios. |
| `0f556bd` `test(layout)` | Modelo de tablas | Documentado y fijado por 4 tests el árbol `table > rowgroup > row > TableCell > LineBox > TextNode` → **133** tests layout. |
| `debc1e8` `feat(render)` | Backgrounds E2E + golden | Resolución de `url(...)` relativa al documento, dedupe y caps (32 recursos / 8 MiB / 16 MiB), stats y warnings no fatales; fixtures `backgrounds.html` + assets; **golden showcase regenerado: 1024×768, 80.220 B, `md5 c95c7eff83e7bd08f815631aa42afdb0`** (antes `c95c7eff…`), 80 display items, determinista file://==HTTP. `basic` sigue 15.498 B `md5 7324e851…`. **53** tests render. |
| `31e5139` `feat(net)` | Red avanzada (Fase 5) | **Cookies RFC 6265** (subset: dominio/path/secure/httpOnly/expiry, `Set-Cookie`, `Cookie` header, `FetchOptions::cookie_jar` por hop), **CORS básico** (Origin, ACAO exacto/`*`, credentials, preflight OPTIONS, `FetchError::Cors`) y **gzip/deflate** (`Accept-Encoding: gzip` por defecto, wrapper gzip RFC 1952 con CRC32/ISIZE, inflate compartido de `crates/image`, **caps anti-bomba** 64 MiB y ratio 1000:1, `max_response_bytes` sobre descomprimido). +95 tests → **180** net. |

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO: arena generacional, API de navegador | 28 |
| `html` | COMPLETO: parser, serializador §13.3, `innerHTML`/`document.write` | 53 |
| `css` | COMPLETO + query API + `@import`/media + `background-image` parsing | 57 |
| `layout` | COMPLETO Fase 4: tablas avanzadas y modelo de celdas fijado | 133 |
| `fonts` | COMPLETO (base LTR) | 13 + 1 doctest |
| `paint` | COMPLETO Fase 4 + backgrounds + texto de tabla | 97 |
| `net` | Fase 5: TLS/caché/data:/cookies/CORS/gzip | 180 |
| `image` | Decoder PNG + inflate propios | 45 + 1 doctest |
| `js` | **Fase 6 WP-6.1**: Boa 0.21.1 + bindings DOM + budgets | 12 |
| `render` | Pipeline URL→PNG + imágenes/backgrounds E2E + `@import`/media | 53 |
| `shell` | Fase 7 usable: scroll/entrada/HiDPI/worker + headless | 46 |
| `bench` | Benchmarks deterministas + baseline | 8 |
| `test_harness` | Gate de 4 suites + allowlists | 7 |

**Total workspace: 734 tests, 0 fallos.** MSRV Rust 1.88 (incluye `boa_engine 0.21.1`); `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.

## Comandos de verificación exactos

```bash
export CARGO_HOME=/home/jp/Escritorio/Proyectos/.cargo-home   # sólo en el sandbox del equipo
cd /home/jp/Escritorio/Proyectos/Megi

./tools/dev.sh all                                  # fmt, clippy, 734 tests, doc y gate
cargo +1.88 check --workspace --all-targets         # MSRV (incluye boa_engine 0.21.1)
cargo test --workspace                              # 734 tests (con JS por defecto)
cargo test --workspace --no-default-features        # build mínimo sin JS (stub)
cargo run -p test_harness -- --summary              # gate 4 suites: PASS, exit 0

cargo test -p js                                    # 12 tests (Boa, feature js ON)
cargo test -p js --no-default-features              # stub sin motor

# Backgrounds + showcase (1024×768, golden determinista)
cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768
md5sum docs/images/showcase.png                     # c95c7eff… (80.220 B, 80 items)
python3 tools/png_check.py docs/images/showcase.png

# Shell, red avanzada, rendimiento y fuzz
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/megi-shot.png --width 640 --height 400
cargo test -p net -p render -p paint -p layout
./tools/bench.sh
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
```

Evidencia: showcase **80.220 B `md5 c95c7eff83e7bd08f815631aa42afdb0`** (80 display items, `background_images_loaded: 2`); basic shell/render **15.498 B `md5 7324e851…`**; Phase-4 golden `basic.rgba` `md5 60f8984e…`; gate PASS.

## Deuda técnica priorizada (siguiente ronda)

1. **Integrar JS en `render` (WP-6.2/6.3)**: ejecutar `<script>` parser-inserted y `document.write` durante el parseo; hito: cerrar los `scripted_*` (árbol 3.904/3.904) y ejecutar JS en `render`/`shell`. Después WP-6.4–6.7: eventos/timers de reloj real, CSSOM/`getComputedStyle`, `MutationObserver`, fuzz de bindings.
2. **Fase 5 restante**: HTTP/2, CORS completo (preflight real, `SameSite`/PSL), streaming de cuerpos, `Referrer-Policy`; `background-repeat/size/position` y colores de `border-collapse` (Fase 4).
3. **Fase 7**: hit-testing (`NodeId`/`LayoutBox` ↔ `DisplayItem`), navegación/historial, eventos DOM/foco, zoom y pulido de HiDPI; tests de ventana en CI con Xvfb.
4. **Rendimiento**: perfil de parseo+layout (según baseline, ~65 % del pipeline), tablas grandes, compresión PNG en páginas grandes y glyph cache global; evaluar memoria de Boa por realm.
5. **Robustez/seguridad**: budgets JS (ya hay `RuntimeLimits`), fuzz targets nuevos (`render`, `js` host bridge), caps de backgrounds y anti-bomba gzip; límites de memoria del runtime JS.
6. **Fixtures/goldens**: `real_page.html`/`torture.html`, goldens con tolerancia para AA y nuevos casos de tablas/backgrounds.
7. **Documentación**: mantener README/ROADMAP/ARCHITECTURE/HANDOFF/PHASE4-SUPPORT/TOOLING/SECURITY/CONTRIBUTING/CHANGELOG y ADR sincronizados por ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` es la excepción tipográfica autorizada (§3.1); `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image` std puro; `js` usa **Boa 0.21.1** (MIT, feature `js` default ON, stub sin feature); `render` no añade deps externas; `shell` usa `winit`/`softbuffer`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/` y `.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo; el ADR JS ya está aceptado (2026-11-10).
