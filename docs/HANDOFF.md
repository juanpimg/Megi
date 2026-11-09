# Handoff de sesión

## Estado (commit b2c86f4 + docs)
- Crates: dom, html, css, layout (100%), paint (~40%, Fase 4). Tests ≈71, todo verde.
- Conformance: tokenizer 7047/7050, tree construction 3898/3904, WPT CSS syntax 30/30.
- `.agents/` y `.cache/` están en gitignore (metadatos de teamwork no versionados).

## Cómo ejecutar
- `./tools/dev.sh all` (fmt, clippy -D warnings, tests, doc)
- `cargo run -p test_harness -- --summary`
- `tools/whatwg_spec.py`, `tools/test_inspector.py` (ver docs/TOOLING.md)

## Reglas
Sin forks de motores; crates core solo con std; cero warnings; cada cambio con tests; push a `origin main`.

## Paint: estado y límites
Fuente bitmap stub (pocas glifos, resto caja); `DefaultTextMeasurer` es métrica falsa; paint lee `border_color.top` como único color.

## Backlog
- **Paint**: stacking contexts/z-index (CSS 2.1 App. E), bordes por lado, fuentes reales (ttf-parser/rustybuzz), clip/overflow, PNG, tests en `crates/paint/tests/`, hit-testing.
- **Layout**: inline-block, floats, position, flexbox, tablas, margin collapsing, vertical-align top/bottom en línea.
- **HTML**: 6 tests `scripted_*.dat` (requieren document.write).
- **Crates futuros**: net (F5), js (F6), shell (F7).
- **Teamwork**: relanzar con `/teamwork` cuando haya cuota.
