# Handoff de sesión — ronda 25 (CSS Color 4/5, ES modules, WebP VP8L completo, providers render y save-png directo)

**Fecha**: 2026-11-11 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `4718408`
**Estado**: **1594 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; F4 ≈99 %, F5 ≈99 %, **F6 ≈95 %**, **F7 ≈93 %**. Manifiesto `docs/status/progress.toml` actualizado (F4/F6/F7 + `next_up`); `PROGRESS.md`/SVG se regeneran con `python3 tools/update_progress.py --write`.
**CI/Repo**: GitHub Actions en verde, con **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `4718408`; fuzz PR build-only; suite determinista.
**Estado de fases**: fuente editable [`docs/status/progress.toml`](status/progress.toml), página generada [`PROGRESS.md`](../PROGRESS.md); **no editar a mano** los artefactos generados.

## Qué se hizo en la ronda 25 (5 commits + refresco de status)

| Commit | Bloque | Resultado |
|---|---|---|
| `bd3b317` `feat(css,paint)` | **CSS Color 4/5** | `crates/css/src/color.rs` con `color(<space> …)` (srgb/srgb-linear/display-p3/a98-rgb/prophoto-rgb/rec2020/xyz/xyz-d50/xyz-d65), `lab()/lch()/oklab()/oklch()`, `hwb()` y `color-mix()` (srgb/srgb-linear/oklab/oklch; pesos, alpha premultiplicado y hue por arco corto), matrices CSS Color 4 documentadas y clamp por canal; todas las propiedades de color lo heredan. Fixture `color_spaces`; css **230**/paint **208**. |
| `9522cb1` `feat(js)` | **ES modules subset** | `JsRuntime::eval_module(source, source_name, base_url, budget) -> ModuleOutcome` con **resolver/loader inyectables** (el host hace toda la E/S); default/named/namespace/side-effect imports, `export const/let/var/function/class/default/{as}`, `import.meta.url`, orden topológico en un realm; ciclos ⇒ `ModuleCycle`, y errores tipados `ModuleNotFound`/`ModuleExport`/`ModuleSyntax`. js **96** (+13). Límites: subset line-based, sin `import()` dinámico, top-level await, `export … from` ni live bindings reales. |
| `6d80b20` `feat(image,render)` | **WebP VP8L completo** | Transformadas inversas (predictor 14 modos, color transform 3.5 fixed, subtract-green), color indexing con paleta delta e índices 1/2/4/8, Huffman complejo/meta-Huffman y LZ77 con overlap/mapa de distancias. Fixtures reales PIL/libwebp validadas **píxel-exactas** contra `.rgba`; `render` decodifica VP8L sin warning y mantiene warning para VP8 lossy/corrupto (`webp_real.html`). image **69**/render **145**. |
| `c90aab4` `feat(render)` | **Providers JS cableados** | `RenderOutput.history_actions`/`location_actions` (+stats, URLs resueltas; render **no navega**, canal para shell), `RenderOptions.storage` compartido para `localStorage` persistente, `sessionStorage` por render, `RenderOptions.host_events` (hashchange/popstate) y checkpoints de microtasks tras scripts/timers/host events y antes del re-render, cerrando el loop de `MutationObserver` con E2E DOM→PNG (`7411a495…`). Fixtures js_provider_actions/js_storage_*/js_observer_chain/js_host_events; render **145**. |
| `560f036` `perf(shell)` | **save-png directo** | `--save-png`/Ctrl+S escriben los **bytes PNG del render** sin re-encode y el headless renderiza una sola vez: antes +200,1 ms ⇒ ahora **+1,7 ms**; escritura atómica con validación de firma y errores tipados; Ctrl+S guarda la página completa. shell **183** (+3); golden basic `7324e851…` intacto. |
| `4718408` `chore(status)` | **Refresco del status** | Regenera `PROGRESS.md`, `docs/status/progress.svg` y badges; manifiesto editable actualizado en esta ronda (r25). |

## Evidencia verificada

- `cargo test --workspace` → **1594 tests, 0 fallos**: css **230**, paint **208**, js **96**, image **69**, render **145**, shell **183**, layout **215**, net **237**, svg **85**, html **60**, dom **28**, fonts 17 + 1 doctest, bench **9**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **Fixtures**: color_spaces, webp_real (VP8L real), js_provider_actions/js_storage_*/js_observer_chain/js_host_events; observer chain E2E `7411a495…`; básic `7324e851…` y goldens antiguos intactos.
- **Status**: `docs/status/progress.toml` es la fuente editable; `python3 tools/update_progress.py --write` regenera `PROGRESS.md` + `docs/status/*`; CI ejecuta `--check --no-gate --no-count`.
- **Fuzz**: matriz sin cambios (10 targets, corpora 11/9/21, CI 300 s + PR build-only).

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1594 tests, doc y gate
cargo test --workspace                              # 1594 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 25
cargo test -p css --test color4
cargo run -p render -- tests/fixtures/render/color_spaces.html /tmp/colors.png --json
cargo test -p js --test es_modules
cargo run -p render -- tests/fixtures/render/webp_real.html /tmp/webp_real.png --json
cargo run -p render -- tests/fixtures/render/js_observer_chain.html /tmp/observer.png --json
cargo run -p shell -- tests/fixtures/render/basic.html --save-png /tmp/page.png --width 640 --height 400

# Progress status
python3 tools/update_progress.py --write
python3 tools/update_progress.py --check --no-gate --no-count
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS (arena + parser 3.904/3.904) | 28 / 60 |
| `css` | F2+F4 + var/calc/pseudos + grid completo + filtros/blend + **Color 4/5** | **230** |
| `layout` | F4 completa + flex/grid + calc mixto/sticky/tipografía + interning + aspect-ratio | **215** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + EffectGroup + **Color 4/5** | **208** |
| `image` | PNG + JPEG baseline/progresivo color + GIF + EXIF 1–8 + **WebP VP8L completo** | **69** |
| `net` | F5: TLS/caché/cookies/PSL/Referrer/Range + HTTP/2 (HPACK/framing/flow/mux/SETTINGS) | **237** |
| `js` | F6: DOM/eventos/timers/APIs + **ES modules (resolver/loader)** | **96** |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text + clipPath/mask/filtros | **85** |
| `render` | Pipeline URL→PNG + imágenes PNG/JPEG/GIF/SVG/WebP-VP8L + **providers JS cableados** | **145** |
| `shell` | F7 + chrome/tabs/find/forms/zoom + sesión persistente/**save-png directo** | **183** |
| `bench` / `test_harness` | Tooling / gate 4 suites | 9 / 11 |
| `fuzz` (workspace aparte) | 10 targets + corpus curado + PR build-only | build 10/10 |

## Deuda priorizada

1. **JS**: `<script type=module>` + loader real en render, dynamic `import()`/top-level await/live bindings, StorageEvent/proxy/requestIdleCallback, fetch/XHR/workers, memoria y conformance JS/DOM.
2. **Imágenes**: VP8 lossy, rendimiento de decode en páginas reales y regiones PNG.
3. **HTTP/2**: push/prioridades, h2c, trailers, request bodies/100-continue y streaming.
4. **SVG/CSS**: animación SVG, text-on-path/feColorMatrix/feBlend/feMerge, efectos por capa y color avanzado más allá de Color 4/5.
5. **Shell**: save-as/prompt visual, descarga de recursos, cookies/passwords, autocompletar/favicons, multi-select/native widgets e aislamiento.
6. **Fuzz/bench**: campañas más largas (imagen/HTTP2), bench E2E de las features r25 y mantener el status al cerrar cada ronda.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*.svg` son generados (no editar a mano); `docs/status/progress.toml` es la fuente editable.
