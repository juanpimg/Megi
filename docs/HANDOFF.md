# Handoff de sesión — ronda 10 (JS con eventos/timers, pipeline 131 ms, hit regions y red avanzada E2E)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `f10635c` (árbol limpio)
**Estado**: **820 tests, 0 fallos**; gate de 4 suites PASS; Fase 4 funcionalmente completa, **Fase 5 ≈95 %** (red avanzada E2E), **Fase 6 ≈40 % (WP-6.1 + WP-6.2 host + WP-A HECHOS)**, **Fase 7 ≈65 %** (navegación + hit regions).
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md) · [`FUZZING.md`](FUZZING.md) · [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 10 (3 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `7faa431` `feat(js)` | **Fase 6 WP-A — eventos y timers** | `EventTarget` (`addEventListener`/`removeEventListener`/`dispatchEvent`) en nodos/document/window con **captura window→document→ancestros→target y burbuja de vuelta**; `Event`/`CustomEvent` (`bubbles`/`cancelable`/`detail`/`defaultPrevented`, `preventDefault`/`stopPropagation`/`stopImmediatePropagation`, `dispatchEvent` devuelve false si `preventDefault`); **timers** en scheduler Rust con **reloj inyectable**: `setTimeout(args)`/`setInterval`/`clear`, orden `(due, id)`, `advance(Duration)`, `run_until(deadline_ms)`, `now_ms()`, `pending_timers()`; `run_tasks` ahora ejecuta solo los vencidos por reloj real (cambio de semántica documentado). 14 tests nuevos → **26 js**. Límites: sin `onclick`/`once`/`passive`, `currentTarget` null en listener, `removeEventListener` sin comparar identidad. |
| `6a41551` `perf(html,layout)` | **Rendimiento parse+layout** | Pipeline 800×600 **167,6 → 131,2 ms (−21,7 %)**; `html/realistic-1mb` **55,0 → 19,2 ms (−65 %)**; layout 56,1→54,5 ms (−2,8 %). Causas: batching de caracteres in-body (una inserción por run con `append_text(&str)`), `consume_data_run` en el tokenizer, preprocesado en una pasada, hasher interno, caché de `allow_cdata`, contador foreign y buffer de tokens reutilizado; en layout, menos clones de `ComputedStyle` en rutas calientes e iteración directa de tablas. **Gate intacto** (tokenizer 7.050/7.050, árbol 3.898/3.904) y **golden byte-idéntico**. El pipeline consume ahora el 65,6 % del presupuesto de 200 ms. |
| `f10635c` `feat(render,shell)` | **hit regions en el pipeline + red avanzada** | `RenderOutput.hit_regions` (+ `RenderStats.hit_regions`) construido con `paint::build_display_list_with_hit_regions`: **píxeles sin cambios** (`basic` `7324e851…`, showcase `c95c7eff…`) y coste **+0,6 µs** en paint (80 items/121 regiones). El **shell usa `output.hit_regions` + `paint::find_link_at`** (fallback layout+DOM documentado) con fixture `file://` de enlaces relativos/absolutos; un anchor sin `href` no genera región. **Red avanzada**: `RenderOptions.cookies` (jar por render) o `cookie_jar` compartido de sesión propagando `Set-Cookie` entre documento/redirects/subrecursos y renders posteriores; `RenderOptions.origin`/`--origin` (o Origin derivado del document URL) con validación **CORS/credentials** cuando hay jar; denegaciones CORS ⇒ warning **no fatal**; gzip/deflate verificados E2E en loopback (HTML y CSS gzip aplican estilos). `RenderStats` gana `hit_regions`/`cookies_stored`/`content_encoding`; CLI render/shell `--cookies`/`--origin`. **render 77 tests (+10), shell 83 (+3)**. |

## Evidencia verificada

- `cargo test --workspace` → **820 tests, 0 fallos** (css 57, dom 28, fonts 13+1 doctest, html 53, image 45+1, **js 26**, layout 133, net 180, paint 108, **render 77**, **shell 83**, bench 8, test_harness 7); fmt, clippy `-D warnings`, `cargo doc` y MSRV 1.88 OK.
- Gate de conformidad: tokenizer **7.050/7.050**, árbol **3.898/3.904** (allowlist `scripted_*` hasta WP-6.4), serializador **230/230**, CSS Syntax **30/30**; `--summary` exit 0.
- **Rendimiento (release, mediana)**: `pipeline/html-to-png-800x600` 167,6 → **131,2 ms**; `html/realistic-1mb` 55,0 → **19,2 ms**; `layout/realistic-1mb` 56,1 → **54,5 ms**.
- **Píxeles sin cambios**: showcase 1024×768 **80.220 B `md5 c95c7eff83e7bd08f815631aa42afdb0`**; render basic 15.498 B **`md5 7324e8512e831de132b1172965332a73`**; Phase-4 `basic.rgba` byte-idéntico (`md5 60f8984e…`).
- **JS**: `js_text.html` con scripting 15.876 B `md5 e1da51d4…` vs `--no-scripting` 15.499 B `md5 53f67e77…`; 26 tests js (14 nuevos de eventos/timers).
- **hit regions**: 121 regiones/80 items en el showcase; shell resuelve enlaces vía `find_link_at` con fallback documentado.

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO: arena generacional, API de navegador | 28 |
| `html` | COMPLETO: parser, serializador §13.3, `innerHTML`/`document.write`; **perf ronda 10** | 53 |
| `css` | COMPLETO: selectores/cascada/@media/query API, background-image parsing | 57 |
| `layout` | COMPLETO Fase 4: tablas §17, árbol de celdas fijado; perf ronda 10 | 133 |
| `fonts` | COMPLETO (base LTR) | 13 + 1 doctest |
| `paint` | COMPLETO Fase 4 + backgrounds + hit-testing | 108 |
| `net` | Fase 5: TLS/caché/data:/cookies/CORS/gzip | 180 |
| `image` | Decoder PNG + inflate propios | 45 + 1 doctest |
| `js` | **Fase 6 WP-6.1 + WP-A**: Boa 0.21.1, bindings DOM, eventos y timers con reloj inyectable | 26 |
| `render` | Pipeline URL→PNG + imágenes/backgrounds + scripts + **hit_regions** + red avanzada E2E | 77 |
| `shell` | Fase 7 usable: scroll/entrada/HiDPI/worker + navegación/historial/título + **find_link_at** | 83 |
| `bench` | Benchmarks reproducibles (parseo/estilos/layout/paint/PNG) | 8 |
| `test_harness` | Gate de conformidad 4 suites + allowlist | 7 |

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 820 tests, doc y gate
cargo test --workspace                              # 820 tests (con JS por defecto)
cargo test --workspace --no-default-features        # stub sin Boa (build mínimo)
cargo test -p js -p render -p shell -p paint

# JS del documento + eventos/timers (WP-A)
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_on.png
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_off.png --no-scripting
cargo test -p js                                    # 26 tests (eventos/timers/reloj)

# hit regions + red avanzada (cookies/CORS/gzip)
cargo run -p render -- https://example.com/ /tmp/page.png --cookies --origin https://example.com --json
cargo run -p shell -- tests/fixtures/render/showcase/index.html --cookies --origin https://localhost

# Rendimiento (release; baseline en docs/PERFORMANCE.md)
./tools/bench.sh --filter pipeline
./tools/bench.sh --filter html
./tools/bench.sh --filter layout

# Showcase / fase 4
cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/phase4.png
python3 tools/png_check.py /tmp/phase4.png
```

## Deuda priorizada

1. **WP-B (js)**: `CSSStyleDeclaration` completa + `getComputedStyle` provider, `NodeList`, `matches`/`closest`, `insertAdjacent*` y caché de identidad wrapper↔nodo; completar `onclick`/`once`/`passive` y `removeEventListener` con identidad.
2. **WP-6.4 (render+js)**: ejecución **durante el parsing** (`parser-inserted` + `document.write` con re-tokenización) para sacar los `scripted_*` de la allowlist → árbol **3.904/3.904**; después `async`/`defer` y módulos ES.
3. **Integrar eventos/timers en el host** (shell): `run_until` + re-render periódico, hoy el bucle del host no ejecuta JS.
4. **Rendimiento**: techo de layout → pasar `LayoutBox.style` a `Arc<ComputedStyle>` (toca paint); bench E2E con JS y presupuesto global del pipeline.
5. **Fase 5**: HTTP/2 y streaming, SameSite/PSL; `br`.
6. **Fase 7**: formularios, pestañas, zoom, eventos DOM/foco y tests de ventana con Xvfb.
7. **Fase 4**: `background-repeat/size/position`, colores de conflictos `border-collapse`, block-in-inline.
8. **Fuzzing**: targets `render`/`js` además de los 4 actuales (`html_parse`, `css_parse`, `selectors`, `net_url`).

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` es la excepción tipográfica autorizada (§3.1); `net` usa `rustls`/`webpki-roots` (producción) y `rcgen` (tests); `image` std puro; `js` usa **Boa 0.21.1** (MIT, feature `js` default ON, stub sin feature); `render` no añade deps externas; `shell` usa `winit`/`softbuffer`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/` y `.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo; el ADR JS ya está aceptado (2026-11-10).
