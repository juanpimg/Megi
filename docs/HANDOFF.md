# Handoff de sesión — ronda 12 (layout −14,6 %, encoder PNG −30,5 % byte-idéntico, eventos JS avanzados, CI en verde)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `7d2ddde` (árbol limpio salvo `website/` untracked del dueño)
**Estado**: **877 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %) y sin allowlist de tree**; Fase 4 funcionalmente completa, **Fase 5 ≈95 %**, **Fase 6 ≈80 %** (eventos avanzados HECHOS), **Fase 7 ≈65-70 %**.
**CI/Repo**: **GitHub Actions en verde** (quality gate, conformance gate y MSRV); `main` y `sprint/robustez-fase4` sincronizadas (81+ commits). El workflow instala los paquetes de sistema Wayland/X11/xcb que necesitan `winit`/`softbuffer` (ver §CI).
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md) · [`FUZZING.md`](FUZZING.md) · [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 12 (5 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `77b8114`+`39e5609` `ci` | **CI en verde** | Se instalan en el runner de GitHub Actions los paquetes de sistema que requieren `winit` 0.30/`softbuffer` 0.4: `libwayland-dev`, `libxkbcommon-dev`, `libxkbcommon-x11-dev`, `libx11-dev`, `libx11-xcb-dev`, `libxcursor-dev`, `libxi-dev`, `libxrandr-dev`, `libxcb1-dev`, `libxcb-render0-dev`, `libxcb-shape0-dev`, `libxcb-xfixes0-dev`, `libxcb-shm0-dev`, `libxcb-randr0-dev` (+ `pkg-config`). Los **3 jobs pasan en verde**. |
| `66a2e3b` `feat(js)` | **Eventos JS avanzados** | `currentTarget` real y `eventPhase` 1/2/3 (fase de captura en el target incluida), `once`/`passive`, `removeEventListener` por identidad, handlers inline `onclick`/`onload`/`onerror` en elemento/document/window, `document.close()` no-op, `estimated_heap_bytes()` aproximado, dispatch de algo que no es `Event` ⇒ `TypeError`, y excepciones de listeners capturadas (`last_event_error()`) sin abortar el resto del dispatch. **58 tests js** (+12). |
| `178290d` `perf(layout)` | **`Arc<ComputedStyle>` en `LayoutBox`** | `LayoutBox.style: Option<Arc<ComputedStyle>>` (+ `style()`/`with_shared_style`; `new` mantiene compat con `Option<ComputedStyle>`); estilo anónimo derivado **una vez por padre** y compartido entre text/anónimos; `inline.rs` mueve/clona Arcs en vez de deep-clones por fragmento/atomic-inline; tokens `Space` con `char` (sin ~150k `String`). **LayoutBox 496→176 B (−65 %)**. `layout/realistic-1mb` **60,4-62,2 → 51,3-53,5 ms (−14,6 %)**, pipeline 141-147 → **134-137 ms**. |
| `7d2ddde` `perf(paint)` | **Encoder PNG −30,5 %** | Filtros en una pasada fusionada, LZ77 con hashes precalculados y match a 8 bytes, `png::encode_pixels` sin buffer RGBA intermedio y Adler-32 vectorizable: render-to-PNG **13,30 → 9,24 ms**, showcase wall **34,1 → 29,5 ms**, con **bitstream byte a byte idéntico** (PNG fixture 42.853 B `md5 9e957177…`, showcase 80.220 B `md5 c95c7eff…`) ⇒ goldens **sin regenerar**. Huffman dinámico queda como follow-up de ratio (cambiaría bytes). |

## Evidencia verificada

- `cargo test --workspace` → **877 tests, 0 fallos**: bench 8, css 57, dom 28, fonts 13+1, html 60, image 45+1, **js 58**, **layout 135**, net 180, **paint 109**, render 88, shell 83, test_harness 11. fmt, clippy `-D warnings`, `cargo doc` y MSRV 1.88 OK.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, **árbol 3.904/3.904 (100 %)**, serializer **230/230**, CSS Syntax **30/30**; allowlists de tokenizer y tree **vacías**.
- **Píxeles sin cambios**: showcase 1024×768 **80.220 B `md5 c95c7eff83e7bd08f815631aa42afdb0`**; render basic 15.498 B **`md5 7324e851…`**; Phase-4 `basic.rgba` byte-idéntico (**`md5 60f8984e…`**); PNG del fixture 42.853 B **`md5 9e957177…`**.
- **Rendimiento (A/B con binarios intercalados)**: layout −8,9 ms (−14,6 %, min 50,4); pipeline −9,0 ms (−6,3 %, ~134,5 ms de 160 de presupuesto); encoder PNG −30,5 %; `html/tag-40k-attrs` ~12,4 ms.
- JS: `js_text.html` 15.876 B `md5 e1da51d4…` con scripting vs 15.499 B `md5 53f67e77…` con `--no-scripting`.

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO: arena generacional, API de navegador | 28 |
| `html` | COMPLETO: parser 3.904/3.904, serializador §13.3, hook `ScriptExecutor` (WP-6.4) | 60 |
| `css` | COMPLETO: selectores/cascada/@media/query API, background-image parsing | 57 |
| `layout` | COMPLETO Fase 4 + **`Arc<ComputedStyle>`/LayoutBox 176 B** (ronda 12), tablas §17 | 135 |
| `fonts` | COMPLETO (base LTR) | 13 + 1 doctest |
| `paint` | COMPLETO Fase 4 + backgrounds + hit-testing + **encoder PNG −30,5 % invariante** | 109 |
| `net` | Fase 5: TLS/caché/data:/cookies/CORS/gzip | 180 |
| `image` | Decoder PNG + inflate propios | 45 + 1 doctest |
| `js` | **Fase 6 WP-A/B + eventos avanzados**: Boa 0.21.1, currentTarget/eventPhase/once/passive/onclick | 58 |
| `render` | Pipeline URL→PNG + imágenes/backgrounds + scripts + event loop presupuestado | 88 |
| `shell` | Fase 7 usable: scroll/entrada/HiDPI/worker + navegación/historial/título + `find_link_at` | 83 |
| `bench` | Benchmarks reproducibles (parseo/estilos/layout/paint/PNG) | 8 |
| `test_harness` | Gate 4 suites (árbol 3.904/3.904 **sin allowlist**) + `JsScriptExecutor` (WP-6.4) | 11 |

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 877 tests, doc y gate
cargo test --workspace                              # 877 tests (con JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo test -p layout -p paint -p render -p js

# Rendimiento (release; baseline en docs/PERFORMANCE.md)
./tools/bench.sh --filter layout                   # ronda 12: ~51-53 ms
./tools/bench.sh --filter paint                    # render-to-png: encoder −30,5 %
./tools/bench.sh --filter pipeline                 # ronda 12: ~134-137 ms

# Eventos JS avanzados (ronda 12)
cargo test -p js
cargo run -p render -- tests/fixtures/render/js_event_timeout.html /tmp/timer.png --json
cargo run -p render -- tests/fixtures/render/js_event_domcontentloaded.html /tmp/dcl.png
cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768
python3 tools/png_check.py docs/images/showcase.png   # 80.220 B, md5 c95c7eff…
```

## CI y repositorio

- `.github/workflows/ci.yml`: jobs **quality** (fmt/clippy/tests/doc), **conformance** (`test_harness --summary`) y **MSRV 1.88**; en Linux instala los paquetes de sistema Wayland/X11/xcb listados arriba para que `winit`/`softbuffer` compilen en el runner. Los 3 jobs están **en verde** en `main`.
- `main` y `sprint/robustez-fase4` sincronizadas en GitHub (81+ commits). `website/` es un directorio **untracked del dueño** (página oficial en construcción): no debe tocarse, commitearse ni enlazarse desde docs.
- Fuzz nocturno (`fuzz-nightly.yml`) se mantiene; candidato: cachear la compilación de Boa en CI (+25 s de build actual).

## Deuda priorizada

1. **Layout/IFC**: ~34 ms de IFC (`format_line` 14,5 / tokenización 11,3 / line breaking 4,5); propuesta `layout_document_with_owned_styles` (mover cada `ComputedStyle` a su caja elimina el último clon por elemento, ~2,5 ms) + caché de familias en `FontTextMeasurer`.
2. **Parser**: `massive-end-p` (bloque de `</p>` masivos) sigue como optimización pendiente.
3. **PNG**: Huffman dinámico solo si se prioriza ratio (cambiaría el bitstream y exigiría regenerar goldens); el encoder actual es invariante.
4. **JS**: `document.onerror` para errores de scripts, módulos ES, contabilidad de memoria real, provider de `getComputedStyle` cableado al pipeline, event loop vivo en el shell.
5. **Shell**: zoom, formularios, pestañas y pruebas de ventana con Xvfb.
6. **Fase 5**: HTTP/2 y streaming, SameSite/PSL, `br`.
7. **Fase 4**: `background-repeat/size/position`, colores de `border-collapse`, block-in-inline.
8. **Fuzzing/CI**: targets `render`/`js`; cachear Boa en CI.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` es la excepción tipográfica autorizada (§3.1); `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image` std puro; `js` usa **Boa 0.21.1** (MIT, feature `js` default ON, stub sin feature); `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js` para ejecutar los scripts del corpus (follow-up de feature-gate).
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/` y `.cache/` ignorados; `website/` no se toca.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo; el ADR JS ya está aceptado (2026-11-10).
