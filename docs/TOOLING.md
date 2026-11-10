# Documentación de Tooling, Skills y Automatización (FASES 2–4)

Este documento registra la investigación, evaluación, instalación y configuración de herramientas CLI, scripts de extracción de especificaciones y skills locales realizadas durante las **FASES 2–6** para optimizar el desarrollo autónomo guiado por especificaciones (WHATWG / W3C), el auto-testeo continuo (`html5lib-tests` y Web Platform Tests), el pipeline de renderizado a PNG, el navegador real (render + red + imágenes + JavaScript embebido) y el fuzzing continuo en **Megi**.

---

## 1. Auditoría del Entorno y Ecosistema Evaluado

Antes de instalar o construir herramientas, se auditó el entorno de ejecución local y la disponibilidad de herramientas del ecosistema Rust, Python y MCP:

| Categoría | Herramienta Evaluada | Estado en Entorno | Decisión Técnica y Justificación |
|---|---|---|---|
| **Compilador y Gestor Rust** | `rustc 1.99.1`, `cargo 1.99.1`, `clippy`, `rustfmt` | Preinstalado (`rustup 1.29.3`) | Base oficial para el Cargo Workspace (`edition = "2024"`, `resolver = "2"`). Configurado mediante `.cargo/config.toml`. |
| **Inspección de Código / LSP** | `rust-analyzer`, `ripgrep 13.0.0` (`rg`), `fdfind` | Preinstalado | Utilizado para diagnóstico estático, verificación de tipos y búsqueda rápida en el monorepo. |
| **Test Runners Externos** | `cargo-nextest`, `cargo-insta` | No preinstalado | Se descartó compilar `cargo-nextest` desde código fuente (alto coste de CPU en máquina de 2 vCPUs) a favor de `cargo test --workspace` nativo combinando un binario y librería interna `test_harness` con reportes detallados. |
| **Extracción de Specs (WHATWG/W3C)** | Servidores MCP externos vs. `python3 3.11.2` + `lxml 4.9.2` | `python3` + `lxml` preinstalados | En lugar de depender de scraping manual o lectura web no estructurada, se construyó `tools/whatwg_spec.py` sobre `lxml.html` con caché local en `.cache/specs/` para extraer pasos normativos exactos (`<ol>`, `<dl class="switch">`, códigos de error y `entities.json`) en milisegundos. |
| **Integración de Suites Oficiales** | `wptrunner` completo vs. fixtures vendorizadas + `tools/test_inspector.py` | `git` + `python3` | `wptrunner` completo está diseñado para navegadores con WebDriver/automatización de ventana completa. `html5lib-tests` está versionado íntegro en `tests/html5lib-tests/`; de WPT sólo se **vendorizan** las 3 fixtures de CSS Syntax en `tests/wpt/css/css-syntax/` (con SHA-256 en su README), de modo que el gate de conformidad es reproducible sin red desde un clon limpio. El gate cubre **4 suites**: tokenizer (7.050/7.050), árbol (3.898/3.904), **serializador HTML** (230/230, `html5lib-tests/serializer/*.test`) y CSS Syntax (30/30). `tools/test_inspector.py` queda como herramienta de inspección de casos `.test`/`.dat`. |

---

## 2. Herramientas y Skills Añadidas al Proyecto

### 2.1 `tools/whatwg_spec.py` — Extractor y Caché de Especificaciones Oficiales (WHATWG / W3C)

Script CLI ejecutable en Python 3 (`lxml.html` + `urllib.request`) que descarga, almacena en caché local (`.cache/specs/`, ignorado por Git) y consulta de forma estructurada las especificaciones formales:
- **WHATWG HTML Living Standard**:
  - `parsing.html` (`https://html.spec.whatwg.org/multipage/parsing.html` — §13.2 Parsing HTML documents, §13.3 Serializing HTML fragments, §13.4 Parsing HTML fragments).
  - `syntax.html` (`https://html.spec.whatwg.org/multipage/syntax.html` — §13.1 Writing HTML documents).
  - `dom.html` (`https://html.spec.whatwg.org/multipage/dom.html` — §3 Semantics, structure, and APIs of HTML documents).
  - `entities.json` (`https://html.spec.whatwg.org/entities.json` — Tabla oficial de las 2.231 referencias de caracteres con nombre de §13.5).
- **WHATWG DOM Living Standard**: `https://dom.spec.whatwg.org/`.
- **W3C CSS Syntax Module Level 3**: `https://drafts.csswg.org/css-syntax-3/`.

**Subcomandos disponibles**:
```bash
# 1. Descargar y actualizar la caché local en .cache/specs/
./tools/whatwg_spec.py fetch [--all] [--force]

# 2. Listar el índice completo de secciones, números y anclas (#id) de parsing.html
./tools/whatwg_spec.py toc [--filter 13.2.5]

# 3. Extraer los pasos normativos exactos de un estado del tokenizador o modo de inserción
./tools/whatwg_spec.py section 13.2.5.1
./tools/whatwg_spec.py section "#tag-open-state"
./tools/whatwg_spec.py section "#adoption-agency-algorithm"

# 4. Listar los 52 códigos normativos de error de parseo de §13.2.2
./tools/whatwg_spec.py errors

# 5. Consultar la tabla oficial entities.json
./tools/whatwg_spec.py entities --query amp
```

---

### 2.2 `tools/test_inspector.py` — Inspector y Conversor de Suites `html5lib-tests` y WPT

En junio de 2026 (commit `224991ec10db04f056a89eed8b0bd8695fd2950e`), el repositorio oficial `html5lib/html5lib-tests` migró sus tests de `tree-construction/*.dat` a `web-platform-tests/wpt/html/syntax/parsing/resources/*.dat`. Además, mientras que `tokenizer/*.test` utiliza archivos JSON, `tree-construction/*.dat` utiliza el formato basado en directivas de línea (`#data`, `#errors`, `#new-errors`, `#document-fragment`, `#script-on`, `#script-off`, `#document`).

`tools/test_inspector.py` resuelve esta dualidad:
1. Implementa en Python la máquina de estados exacta de `parseDat()` de WPT (`html/syntax/parsing/resources/test.js`), respetando nodos de texto multilínea con líneas en blanco internas (`\n\n`).
2. Permite inspeccionar resúmenes globales y casos individuales tanto de `tokenizer/*.test` como de `tree-construction/*.dat`.
3. Exporta los 62 archivos `.dat` de construcción de árboles (1.973 casos de prueba) a fixtures JSON estructurados en `tests/html5lib-tests/tree-construction-json/*.json`, permitiendo que `crates/test_harness` consuma tanto `.dat` nativos como `.json` preprocesados.

**Subcomandos disponibles**:
```bash
# Resumen estadístico de los 14 archivos tokenizer/*.test (6.824 tests) y 62 archivos tree-construction/*.dat (1.973 tests)
./tools/test_inspector.py summary

# Mostrar la entrada, estado inicial, errores y salida esperada de un test específico
./tools/test_inspector.py show test1.test 0
./tools/test_inspector.py show doctype01.dat 0

# Exportar los 62 archivos tree-construction/*.dat a tests/html5lib-tests/tree-construction-json/*.json
./tools/test_inspector.py export-json
```

---

### 2.3 `tools/dev.sh` y Alias en `.cargo/config.toml` — Automatización del Ciclo de Verificación Rust

Para acelerar la inspección de código, el formateo, el análisis estático (`clippy`), la generación de documentación (`rustdoc`) y la ejecución del bucle cerrado de tests:

- **Alias de Cargo (`.cargo/config.toml`)**:
  - `cargo test-all` → `cargo test --workspace`
  - `cargo test-harness` → `cargo test -p test_harness`
  - `cargo lint` → `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo doc-check` → `cargo doc --workspace --no-deps`
- **Script unificado (`tools/dev.sh`)**:
  - `./tools/dev.sh check`: Compilación rápida de todos los targets del workspace.
  - `./tools/dev.sh test`: Ejecución del bucle de tests unitarios e integración.
  - `./tools/dev.sh lint`: Verificación estricta con `rustfmt` y `clippy` (`-D warnings`).
  - `./tools/dev.sh doc`: Verificación de documentación de API (`rustdoc`).
  - `./tools/dev.sh report`: Ejecución del reporte completo de conformidad del `test_harness` (gate `--summary`; exit 0 sólo si todos los fallos están en la allowlist documentada).
  - `./tools/dev.sh all`: Ejecuta secuencialmente `fmt`, `clippy`, `test`, `doc` y el **gate de conformidad** (pasos `[1/5]`…`[5/5]`). Es la puerta única de calidad local y replica los jobs de CI.

### 2.4 `tools/png_check.py` — Validador de PNG sin dependencias

Script Python 3 (sólo biblioteca estándar: `struct`, `zlib`, `binascii`) que valida la salida del pipeline de Fase 4 y es el contrato de aceptación de WP-4.1:

- Firma PNG de 8 bytes y layout de *chunks* (longitud / tipo / payload / **CRC-32 IEEE**).
- `IHDR` primero y único, con coherencia de *bit depth* vs. *color type* y métodos de compresión/filtro/entrelazado.
- `IDAT` contiguos y concatenación que infla a exactamente `height * (1 + stride)` bytes.
- Bytes de filtro de cada scanline válidos (0..=4), `IEND` último y sin bytes posteriores.
- Para imágenes de 8 bits, des-filtra y reporta estadísticas: dimensiones, colores distintos y porcentaje de píxeles transparentes. Es la comprobación que evita la regresión del hallazgo F-08 (canvas 88,7 % transparente) y del PNG inválido.

```bash
python3 tools/png_check.py /tmp/basic.png
# OK: /tmp/basic.png
#   IHDR: 800x600, bit_depth=8, color_type=6 (RGBA)
#   chunks: IHDR IDAT IEND
#   IDAT: zlib ok, ... -> ... raw bytes
#   pixels: 480000, distinct_colors=997, transparent=0 (0.00%), white=42032 (8.76%)
```

(El número de colores distintos subió al integrar tipografía real y bordes por lado con antialiasing; sigue siendo determinista.)

Exit status: 0 si todos los ficheros son válidos, 1 si alguno falla. Se ejecuta manualmente y en la documentación de aceptación del pipeline; los tests de `crates/paint/tests/png_format.rs` cubren la misma estructura desde Rust.

### 2.5 `tests/fixtures/phase4/` — Fixtures y golden images del pipeline

Directorio versionado con documentos HTML autocontenidos (sin red ni JavaScript) y sus imágenes de referencia:

- `basic.html`: documento 800×600 con `h1`, `p`, `br`, `span` con color/negrita, `ul/li`, `div` con padding + borde + fondo, `hr` y enlace.
- `basic.rgba`: golden de 800×600 en RGBA crudo (4 bytes/píxel), comparado **byte a byte** por `crates/paint/tests/end_to_end_html.rs` (determinismo total del pipeline).
- `README.md`: procedimiento de regeneración del golden, validación y limitaciones (fuente DejaVu determinista para `crates/fonts`; el PNG se comprime con el DEFLATE propio de `paint`).

```bash
# Render + validación + tests
cargo run -p paint --example html_to_png --     tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
cargo test -p paint --test end_to_end_html
```

El golden sólo debe regenerarse ante un cambio intencionado de renderizado y revisarse en el mismo commit (ver `tests/fixtures/phase4/README.md`). Las capturas para documentación (versión comprimida del render) se guardan en `docs/images/` — p. ej. `docs/images/phase4-basic.png`, usada en el README.

### 2.6 `crates/fonts/examples/report.rs` — Inspección del motor tipográfico

Ejemplo del crate `fonts` (Fase 4) que carga la fuente DejaVu Sans empaquetada, imprime métricas a 16 px, mide con y sin shaping, y rasteriza un glifo para que los integradores de layout/paint comprueben sus números:

```bash
cargo run -p fonts --example report
cargo test -p fonts
```

---

### 2.7 `tools/bench.sh` + `crates/bench` + `docs/PERFORMANCE.md` — Rendimiento reproducible

Arnés de benchmarks **sin dependencias** (`criterion`/`hyperfine` descartados) para detectar regresiones de complejidad con números medibles:

- `crates/bench`: cargas sintéticas deterministas generadas en memoria (sin reloj/red/`rand`), grupos de parseo HTML, resolución de estilos, layout, rasterizado, render-to-PNG y pipeline completo; `--scale`, `--filter`, `--iterations`, `--warmup` y salida JSON. 8 tests.
- `tools/bench.sh`: compila en release (`cargo build --release -p bench`), ejecuta el binario y guarda `target/bench-latest.txt` + metadatos (`target/bench-latest-meta.txt`: fecha, commit, rustc, args). Argumentos reenviados al binario.
- `docs/PERFORMANCE.md`: metodología, cargas, máquina, baseline medida y limitaciones; **otro agente lo actualiza** con la baseline de esta ronda (no editar desde tareas de documentación general).

```bash
./tools/bench.sh                    # suite completa -> target/bench-latest.txt
./tools/bench.sh --scale 0.25       # smoke test (~10 s)
./tools/bench.sh --filter paint     # un grupo
BENCH_FEATURES=fonts ./tools/bench.sh   # benchmarks de fuentes (opt-in)
```

Baseline de referencia (release, i7-12700H): raster 800×600 **1598,8 → 0,9 ms** (culling), render-to-PNG **1628 → 7,7 ms**, pipeline HTML→PNG **1842 → 170,7 ms** (**10,8×**). Detalle en `docs/PERFORMANCE.md`.

### 2.8 Fuzzing — `fuzz/`, `docs/FUZZING.md` y CI nocturna

Workspace Cargo **independiente** (`fuzz/`, con `[workspace]` vacío) que alberga los targets de [libFuzzer](https://llvm.org/docs/LibFuzzer.html) vía [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz). No forma parte del workspace del motor ni de su grafo de dependencias: el tooling de fuzzing (`libfuzzer-sys`, nightly + ASan) no contamina el build estable.

- **Targets** (4): `html_parse` (tokenizer + árbol + serializer), `css_parse` (tokenizer/parser de hojas y declaraciones), `selectors` (Selectors L4 + cascade sobre un DOM sintético) y `net_url` (URL WHATWG + `join`, sin abrir sockets).
- **Corpus semilla versionado** en `fuzz/corpus/<target>/` (17 ficheros); `fuzz/target/`, `fuzz/artifacts/`, `fuzz/coverage/` y `fuzz/Cargo.lock` están ignorados.
- **CI**: `.github/workflows/fuzz-nightly.yml` (`schedule` diario 03:17 UTC + `workflow_dispatch`), 300 s por target, sube artefactos de crash si falla.
- **Campaña local documentada**: ~1,06 M de ejecuciones ASan con **0 crashes**; política de triaje (reproducir → minimizar → clasificar → añadir regresión) en [`FUZZING.md`](FUZZING.md).

```bash
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
cd fuzz
cargo +nightly fuzz run html_parse -- -max_total_time=30
cargo +nightly fuzz run css_parse  -- -max_total_time=30
cargo +nightly fuzz run selectors  -- -max_total_time=30
cargo +nightly fuzz run net_url    -- -max_total_time=30
```

### 2.9 `crates/render` + `tools/render_fixture.sh` — Pipeline de navegador URL → PNG

`crates/render` es el punto de integración del motor (Fase 5): `URL → net → HTML → CSS externo → layout → display list → PNG`.

```bash
# Fixture offline con CSS externo (resuelto por <base>) + validación + md5
bash tools/render_fixture.sh /tmp/render-fixture.png

# Equivalente manual, con JSON de stats por etapa
cargo run -p render -- tests/fixtures/render/basic.html /tmp/render.png --width 640 --height 400 --json

# URL real (requiere red/TLS); --no-network para offline
cargo run -p render -- https://example.com/ /tmp/example.png
cargo run -p render -- file:///ruta/pagina.html /tmp/local.png --no-network
```

`tools/render_fixture.sh` renderiza `tests/fixtures/render/basic.html` (640×400), valida el PNG con `png_check.py` e imprime su md5. El fixture comprueba `<base>`, hoja externa `screen.css` (canvas verde), hoja `media="print"` ignorada y `<style>` inline; resultado de referencia: **15.498 B**, `md5 7324e851…`.

### 2.10 `crates/image` — Decoder PNG y oráculo del codificador

`crates/image` (std puro, sin dependencias) decodifica PNG a RGBA8 straight y expone un inflate zlib/DEFLATE reutilizable:

```bash
cargo test -p image                 # 45 tests + 1 doctest (decode + inflate + PNG del pipeline)
cargo run -p render -- tests/fixtures/phase4/basic.html /tmp/basic.png 2>/dev/null || true
# El test de integración decodifica el PNG generado por paint y lo compara con basic.rgba
cargo test -p paint --test end_to_end_html
```

En tests actúa como **oráculo** del codificador de `paint` (ida y vuelta PNG → RGBA = golden `basic.rgba`). Límites anti-DoS (`PngLimits`: 16384 px/lado, 16 Mpx, 64 MiB/chunk, 256 MiB IDAT) documentados en el crate.

### 2.11 `crates/shell` + `tools/megi.sh` — Ventana nativa y screenshots headless (Fase 7)

```bash
# Ventana nativa (requiere X11/Wayland)
tools/megi.sh https://example.com/
cargo run -p shell -- tests/fixtures/render/basic.html

# Screenshot headless (CI-friendly; no crea EventLoop)
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/megi-shot.png --width 640 --height 400
tools/megi.sh tests/fixtures/render/basic.html --screenshot /tmp/megi-shot.png
md5sum /tmp/megi-shot.png     # 7324e851… (idéntico al pipeline render)
```

`megi` presenta el `Pixmap` como framebuffer `0RGB` con `softbuffer` (winit 0.30, backends X11/Wayland por dlopen, sin GPU). `--screenshot` usa el mismo pipeline `render` sin `EventLoop` y se testea en CI. **Navegación (rondas 9–10)**: clic izquierdo en enlaces (`Url::join`), historial `NavigationState` (back/forward/replace/reload, cap 256, truncado forward; fallos conservan la entrada), **Alt+←/Backspace/b** atrás, **Alt+→/n** adelante, **r** recargar, **q/Esc** cerrar, scroll completo, HiDPI y **título dinámico** `<title>`+URL+% scroll+`[recortado]` (errores «Megi — error: …»); hit regions y título del mismo render que los píxeles; desde la ronda 10 los enlaces se resuelven con **`RenderOutput.hit_regions` + `paint::find_link_at`** (fallback layout+DOM) y el shell acepta `--cookies`/`--origin`. 83 tests. Limitaciones: formularios/pestañas/zoom/descargas, fragmentos = nueva navegación, >8192 CSS px recortado, JS/eventos no en el bucle del host y ventana no testeada en CI. Ver `crates/shell/src/lib.rs`.

### 2.12 Fixtures de imágenes y captura E2E

`tests/fixtures/render/images.html` + `assets/px.png` (PNG 4×4 de 82 B) ejercitan la carga/decodificación/pintado de `<img>` con una imagen válida escalada bilinealmente y una ausente que debe convertirse en placeholder gris con warning no fatal. Captura de referencia versionada en `docs/images/images-e2e.png` (15.625 B, `md5 1de31699…` a 800×600); el golden `basic.rgba` no cambia.

```bash
cargo run -p render -- tests/fixtures/render/images.html docs/images/images-e2e.png --width 640 --height 400
cargo test -p paint -p image -p render
```

### 2.13 Showcase de navegador real y controles del shell (rondas 7–8)

```bash
# Showcase 1024×768: CSS externo + @import + media + 2 PNG + tabla + stage posicionado
cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768
python3 tools/png_check.py docs/images/showcase.png     # 80.220 B, md5 c95c7eff…
cargo test -p render

# Shell usable: ventana con scroll/entrada/HiDPI y render en worker
tools/megi.sh tests/fixtures/render/showcase/index.html
# controles: rueda/touchpad/↑↓/PageUp/PageDown/Home/End = scroll; r = re-render;
#            q/Esc/cerrar = salir; clic izquierdo = imprime la URL actual.
# Páginas > 8192 CSS px se recortan (título con "[recortado]").
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/megi-shot.png --width 640 --height 400
```

El golden del showcase compara byte a byte (1024×768, **80.220 B**, `md5 c95c7eff…`) y los tests de render lo verifican tanto por `file://` como por HTTP local. En la ronda 8 se regeneró tras el **fix del texto de celdas de tabla** (paint) y el soporte de **`background-image`/`list-style-image`** E2E (WP-5.9): ahora se ven el texto y las cabeceras en negrita de la tabla, el patrón de fondo de la cabecera/escenario y los marcadores con imagen. Limitaciones visibles restantes: `background-repeat`/`size`/`position`, colores de `border-collapse` y block-in-inline.

### 2.14 `crates/js` — Runtime JavaScript Boa (Fase 6, WP-6.1)

`crates/js` embebe **Boa 0.21.1** detrás de la feature `js`, **ON por defecto**; con `--no-default-features` queda un stub std-only (sin Boa) para CI y auditoría. Expone `JsRuntime::eval` sobre un realm persistente, `console.*` capturado, bindings DOM (`document.write`, `getElementById`, `querySelector(All)`, `innerHTML`, `style`, `classList`, árbol/atributos) y un scheduler base (`setTimeout`/microtasks). Anti-DoS: `Budget` mapeado a Boa `RuntimeLimits` (5.000.000 iteraciones de bucle, recursión 512, pila VM 20.480; `Budget::SMALL` para páginas/tests) y side-table `NodeId↔handle` capada a 1.000.000; sin APIs de red/FS. Aún **no integrado en `render`**.

**WP-A (ronda 10)**: `EventTarget` con captura+burbuja (`Event`/`CustomEvent`) y **timers con reloj inyectable** (`setTimeout`/`setInterval`/clear, `advance`/`run_until`/`now_ms`/`pending_timers`; `run_tasks` solo vencidos por reloj real).

`render` integra el runtime (WP-6.2 host): descubre los `<script>` clásicos (inline/`src`, en orden; módulos avisados y saltados), los ejecuta con `JsRuntime::with_document` bajo `ScriptingLimits`/`ScriptBudget` (budget por script + total), drena microtasks y hasta 10.000 timers, captura `console` (`scripts_executed/failed/bytes/console`, `timings.scripting`) y **re-renderiza estilos→layout→paint sobre el DOM mutado** (mutaciones visibles en el PNG). Caps: 8 MiB/script, 32 MiB total, 64 scripts; `--no-scripting` desactiva todo y `--no-network` bloquea `src` externos.

```bash
cargo test -p js                        # 26 tests (feature js por defecto; incluye WP-A)
cargo test -p js --no-default-features  # stub mínimo sin Boa (CI)

# JS del documento ejecutado de extremo a extremo (7 fixtures js_*)
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_on.png
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_off.png --no-scripting
md5sum /tmp/js_on.png /tmp/js_off.png   # e1da51d4… (15.876 B) vs 53f67e77… (15.499 B)
cargo test -p render --test scripting_tests

# Red avanzada en el pipeline (rondas 10): cookies + CORS/credentials, gzip E2E
cargo run -p render -- https://example.com/ /tmp/page.png --cookies --origin https://example.com --json
cargo run -p shell -- tests/fixtures/render/showcase/index.html --cookies --origin https://localhost
```

### 2.15 `crates/net` — cookies, CORS y compresión (Fase 5, WP-5.9)

Además de URL/HTTP/TLS/caché: **cookies RFC 6265** (`CookieJar`, cap 4.096, `FetchOptions.cookie_jar`), **CORS básico** (`Origin`, `validate_response`/`validate_preflight_response`, credentials) y **gzip/deflate** (`Accept-Encoding: gzip` por defecto, CRC32/ISIZE, `CompressionLimits`: 64 MiB descomprimidos y ratio máximo 1000:1). Límites: sin SameSite/PSL, CORS parcial (sin caché de preflight), sin `br`/HTTP2/streaming.

```bash
cargo test -p net        # 180 tests offline (listeners en 127.0.0.1)
```

### 2.16 Fixtures de `background-image`/`list-style-image` (E2E, ronda 8)

`tests/fixtures/render/backgrounds.html` + `assets/pattern.png` (16×16 checker) y `assets/bullet.png` (8×8 rombo) ejercitan `background-image` estirada al *padding box* y `list-style-image` como marcador, incluido fallback a color + warning con recurso ausente y caps **32 imágenes / 8 MiB por imagen / 16 MiB totales**.

```bash
cargo run -p render -- tests/fixtures/render/backgrounds.html /tmp/backgrounds.png
python3 tools/png_check.py /tmp/backgrounds.png
cargo test -p render --test backgrounds_tests
```

### 2.16b `paint::hit_test` — hit-testing aditivo (ronda 9)

La API aditiva de `crates/paint` mapea píxeles a cajas/DOM en **orden de pintado**:
`HitRegion { rect, node_id, kind }`, `hit_test`/`hit_test_all` (gana el topmost), `HitLink`/`find_link_at` (sube al `<a href>` HTML y devuelve el href crudo) y los builders `build_display_list_with_hit_regions`/`build_display_list_and_hit_regions`. Las entradas clásicas (`build_display_list`) no cambian ni un byte de golden y el coste es opt-in. Límites: granularidad caja/run, sin `pointer-events`/transforms, coordenadas CSS de página (el host ajusta scroll/scale).

```bash
cargo test -p paint --test hit_testing   # 11 tests nuevos (108 paint)
# ronda 10: render construye RenderOutput.hit_regions (+0,6 µs) y el shell usa find_link_at
```

### 2.17 Skill Local para Agentes (`.agents/skills/whatwg-spec-driven-dev/SKILL.md`)

Se instaló la skill local `whatwg-spec-driven-dev` en `.agents/skills/whatwg-spec-driven-dev/SKILL.md` (directorio ignorado en `.gitignore` para mantener limpio el árbol Git de metadatos de agentes, mientras su documentación permanente reside en este archivo `docs/TOOLING.md`). Esta skill estandariza el protocolo operativo para cualquier agente que trabaje en la Fase 1 y posteriores:
1. Consultar primero la sección normativa con `./tools/whatwg_spec.py section <id>`.
2. Inspeccionar los casos de prueba objetivo con `./tools/test_inspector.py show <file> <idx>`.
3. Implementar la transición de estado o regla del árbol DOM en `crates/html` o `crates/dom`.
4. Verificar en bucle cerrado con `cargo test -p test_harness`.

---

## 3. Verificación rápida de la documentación y del estado

```bash
./tools/dev.sh all                     # fmt, clippy, 820 tests, doc y gate de conformidad
cargo run -p test_harness -- --summary # gate de 4 suites: PASS, exit 0
cargo run -p test_harness -- --summary --strict # exit 1 por fallos conocidos

cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
cargo run -p render -- tests/fixtures/render/basic.html /tmp/render.png --width 640 --height 400 --json
bash tools/render_fixture.sh
cargo test -p net -p render -p image -p shell
cargo test -p js && cargo test -p js --no-default-features
cargo test -p paint --test hit_testing
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/shot.png
./tools/bench.sh --scale 0.1
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
```

Cifras de referencia de esta ronda: **820 tests** (bench 8, css 57, dom 28, fonts 13+1 doctest, html 53, image 45+1 doctest, js 26, layout 133, net 180, paint 108, render 77, shell 83, harness 7); gate tokenizer 7.050/7.050, árbol 3.898/3.904, serializer 230/230, CSS 30/30; todos los crates `#![forbid(unsafe_code)]`; MSRV Rust 1.88.
