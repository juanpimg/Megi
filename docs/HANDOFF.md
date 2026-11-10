# Handoff de sesión — ronda 17 (flexbox MVP, radius/sombras, formularios editables, SVG pattern/text)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `67f7c57`
**Estado**: **1107 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; Fase 4 prácticamente completa (flex/fl/decoraciones/backgrounds/SVG), **Fase 5 ≈95 %**, **Fase 6 ≈90 %**, **Fase 7 ≈75 %** (formularios editables).
**CI/Repo**: GitHub Actions **en verde** (quality, conformance y MSRV; deps Wayland/X11/xcb), `main` y `sprint/robustez-fase4` sincronizadas en `67f7c57`; suite determinista (reloj virtual, epoch fijo de rAF).
**Docs**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md) · [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) · [`PERFORMANCE.md`](PERFORMANCE.md) · [`FUZZING.md`](FUZZING.md) · [`ROADMAP.md`](ROADMAP.md).

## Qué se hizo en la ronda 17 (3 commits)

| Commit | Bloque | Resultado |
|---|---|---|
| `8f60cb8` `feat(svg)` | **`<pattern>` + `<text>` básico** | `<pattern>` con tiling (patternUnits/contentUnits/transform/contenido arbitrario, fill/stroke, **cap 10k tiles**, ciclos sin panic) y `<text>` básico (x/y/dx/dy, `text-anchor`, `font-family`/weight vía **nueva dep interna `crates/fonts`** — DejaVu bundled, bold; italic→normal; font-size, letter-spacing, fill/stroke/opacity y transform translate/scale; sin kerning/ligaduras/bidi; caps 1.000 texts/10.000 chars). svg 60→**73 tests** (+13), fixtures `pattern.svg`/`text.svg`. |
| `082f52d` `feat(css,layout,paint)` | **Flexbox MVP + decoraciones + UA forms** | **Flexbox** (`layout/flex.rs` + `css/flex.rs`): `display:flex/inline-flex` (inline-flex = `Display::InlineBlock` + flag `is_inline_flex`, sin variant nueva), direction ±reverse, justify 5 incl. `space-evenly`, align stretch/flex-*/center/baseline, wrap nowrap/wrap, gap/row-gap/column-gap, grow/shrink/basis + shorthand `flex`, min/max clamp, sin margin collapsing entre items (layout 135→**146**, css 76→**94**; 3 fixtures E2E `flex_row` `c28ce538…`, `flex_column` `8d06d66c…`, `flex_wrap` `7fbf998e…`). **Decoraciones** (`border-radius` 1-4/slash/longhands, `box-shadow`, `text-shadow`): DisplayItems `RoundedRect`/`RoundedBorder`/`Push-PopRoundedGroup`/`BoxShadow`/`TextShadow`, **blur determinista de 3 pasadas separables** (radio=blur/2, caps blur ≤64 px y ≤32 sombras); sin radius/sombras el display list es byte-idéntico (paint 130→**141**, +11 effects). **UA styles de formularios** aproximados (inputs/textarea/select/checkbox/radio/button/label/fieldset + `[checked]`/`[disabled]`/`[data-megi-focused]`). |
| `67f7c57` `feat(shell)` | **Formularios visibles y editables** | `src/forms.rs`: extracción layout+DOM con **`data-megi-form-id` estable**, `FocusState`, `TextEdit` UTF-8 con caret, patch value/checked/focus/active y **snapshot `data:` URL** + re-render; ventana: clic enfoca (precedencia sobre links), teclado edita value, Backspace, **Tab**, Esc blur, **checkbox/radio toggle**, botón flash, **Enter nunca envía**; scripts desactivados al editar hasta reload. Fixture `forms.html` (8 controles) y 3 tests integración + 5 unit ⇒ shell 100→**108**; screenshot 640×480 md5 `b8b9a2047ddae43a6f0a329e7c2e396e`, golden basic `7324e851…` intacto. |

## Evidencia verificada

- `cargo test --workspace` → **1107 tests, 0 fallos**: bench 8, **css 94**, dom 28, **fonts 17 + 1 doctest**, html 60, image 45+1, js 71, **layout 146**, net 180, **paint 141**, render 123, **shell 108**, **svg 73**, test_harness 11.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer 7.050/7.050, **árbol 3.904/3.904**, serializer 230/230, CSS Syntax 30/30; allowlists vacías. fmt/clippy `-D warnings`/doc/MSRV 1.88 OK.
- **Píxeles/evidencias**: showcase `c95c7eff…` y phase4 PNG `9e957177…`/`basic.rgba 60f8984e…` intactos; flex E2E con bboxes (row: red x10-49/y40-59, green x140-179/y30-69, blue x270-309/y20-79; column stretch 120/grow 110; wrap 2 líneas) y md5s `c28ce538…`, `8d06d66c…`, `7fbf998e…`; forms 640×480 `b8b9a2047ddae43a6f0a329e7c2e396e`; `paint --test effects` 11/11; svg `pattern.svg`/`text.svg` renderizados con `examples/svg_to_png`.
- **Rendimiento**: layout ~36,3 ms y pipeline ~120,5 ms (ronda 15, vigente); ronda 17 no altera el pipeline sin flex/decoraciones/forms.

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS (arena + parser 3.904/3.904) | 28 / 60 |
| `css` | COMPLETO + backgrounds + flex + radius/sombras + UA forms | 94 |
| `layout` | COMPLETO Fase 4 + **Flexbox MVP** + IFC optimizado | 146 |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | COMPLETO + backgrounds multicapa + **radius/sombras** | 141 |
| `net` / `image` | Fase 5 (TLS/caché) / decoder PNG propio | 180 / 45 + 1 doctest |
| `js` | Fase 6: DOM/eventos/timers + APIs de navegador | 71 |
| `svg` | Parser/modelo/rasterizador AA + CSS/gradientes + **pattern/text** | 73 |
| `render` | Pipeline URL→PNG + imágenes/backgrounds/SVG + event loop + webfonts | 123 |
| `shell` | Fase 7 + zoom/refresh + **formularios editables** | 108 |
| `bench` / `test_harness` | Tooling / gate 4 suites | 8 / 11 |

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1107 tests, doc y gate
cargo test --workspace                              # 1107 tests (con JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0

# Ronda 17
cargo run -p render -- tests/fixtures/render/flex_row.html /tmp/flex_row.png
cargo run -p render -- tests/fixtures/render/flex_column.html /tmp/flex_col.png
cargo run -p render -- tests/fixtures/render/flex_wrap.html /tmp/flex_wrap.png
cargo run -p shell -- tests/fixtures/render/forms.html --screenshot /tmp/forms.png --width 640 --height 480
cargo test -p paint --test effects                  # radius/sombras (11 tests)
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/pattern.svg /tmp/pattern.png
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/text.svg /tmp/text.png

# Rendimiento y goldens
cargo run --release -p bench -- --filter layout --iterations 5     # ~36 ms
cargo run --release -p bench -- --filter pipeline --iterations 3   # ~120 ms
cargo run -p render -- tests/fixtures/render/showcase/index.html /tmp/showcase.png --width 1024 --height 768  # c95c7eff…
cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets
```

## Deuda priorizada

1. **Layout**: grid, `order`/`flex-flow`/`align-content`, `overflow:hidden`+radius, block-in-inline y colores de `border-collapse`; IFC otra vuelta (<35 ms).
2. **Formularios**: widgets nativos (texto/caret del input pintado por el motor), caret blink/selección, `select` dropdown, validación/submit, `:focus`/`:checked` reales y JS del host durante la edición.
3. **SVG**: shaping avanzado (`kerning`/italic real/bidi), patterns rotados/escalados complejos, filtros.
4. **Paint/CSS**: `border-image`, filtros, `background-clip:text`, inset shadows exactas.
5. **Fase 6**: módulos ES, memoria real, eventos `resize`/`scroll`/`change`.
6. **Fase 5**: HTTP/2 y streaming, SameSite/PSL, `br`.
7. **Fuzz/bench**: targets render/js/svg, bench E2E de flex/SVG/forms y cachear Boa en CI (+25 s).

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- Al tocar dependencias, actualizar `ARCHITECTURE.md` §3 y justificarlo (ADR JS aceptado 2026-11-10).
