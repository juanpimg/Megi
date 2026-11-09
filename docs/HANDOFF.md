# Handoff de sesión — sprint de robustez y Fase 4

**Fecha**: 2026-11-09 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` (basada en `main` @ `ac2b79b`)
**Estado**: trabajo del sprint completo y verificado; la documentación de este handoff ya refleja la realidad.
**Matriz de soporte de Fase 4**: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md).

## Resumen del sprint (R1–R6)

| Bloque | Qué se hizo | Evidencia |
|---|---|---|
| **R1 — Conformidad como gate** | El harness suma resultados y aplica una allowlist documentada; `--summary` devuelve exit 0 sólo si no hay regresiones y `--strict` devuelve 1. Las 3 fixtures WPT de CSS Syntax quedan **vendorizadas** en `tests/wpt/css/css-syntax/` (con SHA-256 y README), por lo que `30/30` es **reproducible desde un clon limpio sin red**. `./tools/dev.sh all` ejecuta el gate como paso `[5/5]`. | tokenizer **7047/7050** (allowlist `xmlViolation.test`: 3); árbol **3898/3904** (allowlist 4 suites `scripted_*`: 6 mode-runs); CSS **30/30**; `Conformance gate: PASS`. |
| **R2 — Robustez anti-DoS** | `ParseOptions` (profundidad 4.096, 1 M nodos, 4.096 atributos/elemento, 16 MiB de input) + `parse_document_with_options`, `LimitViolations`, serializador **iterativo**, DOM con validación de ciclos (`append_child`/`insert_before` → `bool`), `try_node` y `clone_node` de `<template>`; 13 tests de robustez con hilos de pila pequeña. | 100k/200k `<div>` anidados **no abortan** (~70 ms; antes SIGABRT); 40k atributos en un start-tag: de ~1.276 ms a ~10 ms (medido en esta máquina, 2 vCPU). |
| **R3 — Correcciones CSS** | Bordes por lado/`shorthands`, `border-style: none` ⇒ ancho computado 0, `{1,4}` de `border-width/color/style`, campos `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type` en `ComputedStyle`, y correcciones de selectores (pseudo-clase desconocida invalida, SVG/MathML, comas en atributos). | `cargo test -p css`: **23/23** + 30/30 CSS Syntax. |
| **R4 — Correcciones layout** | Cajas anónimas *inherit-only* (fin de márgenes/bordes/fondos fantasma y de los huecos por whitespace entre bloques), `<br>` como salto forzado, colapso de márgenes CSS 2.2 §8.3.1, ICB con altura de viewport, `box-sizing`, `min/max`, alturas porcentuales y sobre-restringido. | `cargo test -p layout`: **61** tests (4 unit + 10 adversarial + 18 BFC + 22 IFC adversarial + 7 IFC). |
| **R5 — Pipeline HTML → PNG** | Driver `html_to_png` (HTML→estilos→layout→display list→`Pixmap`→PNG), codificador **PNG propio sin dependencias** (DEFLATE *stored*), `RenderConfig`, canvas blanco con propagación de fondo raíz/`body` (CSS 2.2 §14.2), extracción de `<style>` (`collect_style_text`/`resolve_embedded_styles`), golden determinista. | `cargo run -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png` → 800×600, 1.920.813 B, 9 colores, **0 %** de píxeles transparentes (antes 88,7 %); `python3 tools/png_check.py /tmp/basic.png` → `OK`; `cargo test -p paint`: **33**. |
| **R6 — Tipografía, infra y docs** | Nuevo crate `crates/fonts` (Fase 4, fase 1): `fontdb`+`ttf-parser`+`rustybuzz`+`ab_glyph`, métricas, shaping LTR (kern/ligaduras), rasterización de glifos y fallback; fuente DejaVu determinista empaquetada con licencia. CI, licencias MIT/Apache y MSRV real. | `cargo test -p fonts`: **13 + 1 doctest**; `cargo test --workspace`: **165 tests, 0 fallos**; `./tools/dev.sh all` → exit 0. |

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` | COMPLETO (arena, mutaciones O(1), robustez de API) | 9 |
| `html` | COMPLETO declarativo + endurecido anti-DoS | 7 unit + 13 robustez |
| `css` | COMPLETO (Fase 2) + bases de Fase 4 | 23 |
| `layout` | Fase 3 completa + correcciones de Fase 4; faltan posicionados/floats/tablas/inline-block | 61 |
| `fonts` | **NUEVO** — Fase 4 fase 1 (motor tipográfico), aún **no integrado** en layout/paint | 13 + 1 doctest |
| `paint` | Fase 4 en curso (~55 %): pipeline PNG operativo; faltan fuentes reales integradas, bordes por lado en el raster, stacking/opacity, clip y AA | 33 |
| `test_harness` | COMPLETO: gate de conformidad con allowlist | 5 |

**Total workspace: 165 tests, 0 fallos.** MSRV declarada y verificada: **Rust 1.88** (`rust-version = "1.88"`; el código usa `let-chains`, estabilizadas en 1.88).

## Comandos de verificación exactos

```bash
export CARGO_HOME=/home/jp/Escritorio/Proyectos/.cargo-home   # sólo en el sandbox del equipo
cd /home/jp/Escritorio/Proyectos/Megi

# Puerta única de calidad: fmt, clippy -D warnings, tests, doc y gate de conformidad
./tools/dev.sh all

# Equivalente paso a paso
cargo +1.88 check --workspace --all-targets
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
cargo run -p test_harness -- --summary          # gate: PASS, exit 0
cargo run -p test_harness -- --summary --strict # exit 1: no tolera fallos de la allowlist

# Pipeline HTML → PNG y validación
cargo run -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png

# Motor tipográfico
cargo run -p fonts --example report
```

## Deuda técnica y follow-ups priorizados (Fase 4)

1. **WP-4.4 fase 2 — integrar `crates/fonts` en layout/paint** *(prioridad máxima)*: sustituir `DefaultTextMeasurer` y los glifos 8×8 *stub* por `FontSpec`/`ShapedRun`, `line-height: normal` real, baseline en la line box y máscaras de glifo en el raster. Requiere cambiar la firma de `layout_document`/`build_display_list` (hoy sin contexto tipográfico).
2. **WP-4.3 — bordes por lado en el raster**: `DisplayItem::Border` con 4 colores/estilos y esquinas a inglete; fondos en *border box* con `background-clip`; `dashed`/`dotted`/`double`/`groove`/`ridge`/`inset`/`outset` (hoy degradan a sólido o no se pintan). El CSS ya entrega los datos.
3. **WP-4.5 — IFC pendiente**: no romper palabras entre nodos inline adyacentes sin espacio (hallazgo F-09), soft hyphen de ancho cero que sólo se convierte en guion al romper, `vertical-align: top/bottom` conforme al *aligned subtree* de §10.8.1, y `line-height` con semántica de `<number>` recalibrada por elemento (hoy se hereda congelada en px).
4. **WP-4.6 — posición y stacking**: `position: relative/absolute/fixed`, contenedores de posicionados, `z-index` y `opacity` por grupos offscreen. Hoy `position: absolute` se pinta en el flujo normal; es el mayor bloqueador de fidelidad en páginas reales.
5. **WP-4.7/4.8/4.9/4.10 — features de layout**: floats/`clear`, tablas CSS 2.1 (`colspan`/`rowspan`, `border-collapse`), `inline-block`/marcadores de lista/`text-decoration` y `overflow`/clip (`PushClip`/`PopClip` ya existen en la display list pero el painter no los emite).
6. **WP-4.11 — calidad de imagen**: antialiasing/cobertura por píxel (`fill_rect` aún redondea hacia fuera), alfa premultiplicado y **compresión PNG** (hoy ~1,92 MB por 800×600). Cualquier compresión debe respetar `ARCHITECTURE.md` §3.2 (decisión D3).
7. **WP-4.12 — CPU cuadrática restante (auditoría F2)**: `has_element_in_scope`, `push_active_formatting` y `reconstruct_active_formatting_elements` del tree builder siguen siendo lineales por token. Adicionalmente, límites de profundidad/tiempo en layout y paint (el parseo ya está acotado).
8. **Margen de auto-colapso pendiente**: bloques de altura cero con márgenes superior e inferior (*self-collapsing margins*, §8.3.1); el colapso general ya está implementado y testeado.
9. **WP-4.11/4.13 — fixtures**: `real_page.html` y `torture.html` + goldens cuando existan posicionamiento/tablas/fuentes reales; hoy sólo hay golden de `basic.html`.
10. **Cobertura WPT An+B**: el gate usa 8 casos curados de 67 aserciones upstream; ampliar error cases y roundtrip de `selectorText`.
11. **API de layout**: `layout_document` devuelve `Option`; migrar a errores explícitos (`Result`) para no ocultar subárboles sin estilo.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout de terceros. Los crates de núcleo (`dom`, `html`, `css`, `layout`) sólo usan `std`; `crates/fonts` es la excepción autorizada de §3.1 y `test_harness` sólo usa `serde`/`serde_json`.
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings`.
- Todo cambio funcional necesita tests en el mismo commit; los mensajes siguen *Conventional Commits*.
- No commitear/pushear desde agentes: el Lead integra y revisa.
- `tests/html5lib-tests` es upstream versionado; de `tests/wpt/` sólo se versiona `tests/wpt/css/css-syntax/` (fixtures vendorizadas); `.agents/` y `.cache/` están ignorados.
- Al tocar la matriz de dependencias, actualizar `docs/ARCHITECTURE.md` §3 y justificarlo en el PR.
