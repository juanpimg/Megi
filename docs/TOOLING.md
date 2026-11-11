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
| **Integración de Suites Oficiales** | `wptrunner` completo vs. fixtures vendorizadas + `tools/test_inspector.py` | `git` + `python3` | `wptrunner` completo está diseñado para navegadores con WebDriver/automatización de ventana completa. `html5lib-tests` está versionado íntegro en `tests/html5lib-tests/`; de WPT sólo se **vendorizan** las 3 fixtures de CSS Syntax en `tests/wpt/css/css-syntax/` (con SHA-256 en su README), de modo que el gate de conformidad es reproducible sin red desde un clon limpio. El gate cubre **4 suites**: tokenizer (7.050/7.050), **árbol (3.904/3.904, 100 %, sin allowlist; WP-6.4 ejecuta los `scripted_*`)**, **serializador HTML** (230/230, `html5lib-tests/serializer/*.test`) y CSS Syntax (30/30). `tools/test_inspector.py` queda como herramienta de inspección de casos `.test`/`.dat`. |

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

`megi` presenta el `Pixmap` como framebuffer `0RGB` con `softbuffer` (winit 0.30, backends X11/Wayland por dlopen, sin GPU). `--screenshot` usa el mismo pipeline `render` sin `EventLoop` y se testea en CI. **Navegación (rondas 9–10)**: clic izquierdo en enlaces (`Url::join`), historial `NavigationState` (back/forward/replace/reload, cap 256, truncado forward; fallos conservan la entrada), **Alt+←/Backspace/b** atrás, **Alt+→/n** adelante, **r** recargar, **q/Esc** cerrar, scroll completo, HiDPI y **título dinámico** `<title>`+URL+% scroll+`[recortado]` (errores «Megi — error: …»); hit regions y título del mismo render que los píxeles; desde la ronda 10 los enlaces se resuelven con **`RenderOutput.hit_regions` + `paint::find_link_at`** (fallback layout+DOM), el shell acepta `--cookies`/`--origin` y la **ronda 13** añade **zoom** (`Ctrl+=/-/0`, `--zoom` 0.5–3.0) y **auto-refresh opt-in** (`--refresh-ms`). 100 tests. Limitaciones: formularios/pestañas/descargas, fragmentos = nueva navegación, >8192 CSS px recortado, JS/eventos no en el bucle del host y ventana no testeada en CI. Ver `crates/shell/src/lib.rs`.

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

**Eventos avanzados (ronda 12)**: `currentTarget`/`eventPhase` (1/2/3) reales, `once`/`passive`, `removeEventListener` por identidad, handlers inline `onclick`/`onload`/`onerror`, `document.close()`, `estimated_heap_bytes()` y excepciones de listeners capturadas. **WP-B (ronda 11)**: `CSSStyleDeclaration` completa, `getComputedStyle` con provider inyectable, `NodeList`, `matches`/`closest`, `insertAdjacentHTML/Text`, identidad de wrappers y `document` como `EventTarget`. **WP-A (ronda 10)**: `EventTarget` con captura+burbuja (`Event`/`CustomEvent`) y **timers con reloj inyectable** (`setTimeout`/`setInterval`/clear, `advance`/`run_until`/`now_ms`/`pending_timers`; `run_tasks` solo vencidos por reloj real).

`render` integra el runtime (WP-6.2 host): descubre los `<script>` clásicos (inline/`src`, en orden; módulos avisados y saltados), los ejecuta con `JsRuntime::with_document` bajo `ScriptingLimits`/`ScriptBudget` (budget por script + total), drena microtasks y hasta 10.000 timers, captura `console` (`scripts_executed/failed/bytes/console`, `timings.scripting`) y **re-renderiza estilos→layout→paint sobre el DOM mutado** (mutaciones visibles en el PNG). Caps: 8 MiB/script, 32 MiB total, 64 scripts; `--no-scripting` desactiva todo y `--no-network` bloquea `src` externos.

```bash
cargo test -p js                        # 58 tests (feature js por defecto; eventos avanzados)
cargo test -p js --no-default-features  # stub mínimo sin Boa (CI)

# JS del documento ejecutado de extremo a extremo (7 fixtures js_*)
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_on.png
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_off.png --no-scripting
md5sum /tmp/js_on.png /tmp/js_off.png   # e1da51d4… (15.876 B) vs 53f67e77… (15.499 B)
cargo test -p render --test scripting_tests

# Event loop presupuestado (ronda 11): reloj virtual, DOMContentLoaded/load, timers, re-render único
cargo run -p render -- tests/fixtures/render/js_event_timeout.html /tmp/timer.png --json
cargo run -p render -- tests/fixtures/render/js_event_domcontentloaded.html /tmp/dcl.png
cargo test -p render --test event_loop_tests        # 6 fixtures js_event_*, 11 tests

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

### 2.16d Rendimiento ronda 12: `Arc<ComputedStyle>` en layout y encoder PNG invariante

`LayoutBox.style` es `Option<Arc<ComputedStyle>>` (`style()`, `with_shared_style`, estilo anónimo compartido por padre, tokens `Space` con `char`): `layout/realistic-1mb` **60,4-62,2 → 51,3-53,5 ms (−14,6 %)** y LayoutBox **496→176 B**. El encoder PNG (−30,5 %) fusiona filtros/LZ77/Adler en una pasada y evita el buffer RGBA intermedio, con **bitstream byte a byte idéntico** (goldens sin regenerar).

```bash
cargo run --release -p bench -- --filter layout    # ~51-53 ms
cargo run --release -p bench -- --filter paint     # render-to-png 13,30→9,24 ms
cargo run --release -p bench -- --filter pipeline  # ~134-137 ms
python3 tools/png_check.py docs/images/showcase.png  # md5 c95c7eff… sin cambios
```

### 2.16c Event loop presupuestado y fixtures `js_event_*` (ronda 11)

`crates/render` drena la cola de microtasks/timers con **reloj virtual** (`max_tasks` 10.000, `max_virtual_time` 1000 ms), despacha `DOMContentLoaded` y `load` por el `EventTarget` del documento y hace **un solo re-render** tras el loop; stats `script_*`/`timings.event_loop`. Fixtures: `js_event_timeout.html`, `js_event_timeout_zero.html`, `js_event_interval.html`, `js_event_load.html`, `js_event_domcontentloaded.html`, `js_event_loop.html`.

```bash
cargo run -p render -- tests/fixtures/render/js_event_timeout.html /tmp/timer.png --json
cargo test -p render --test event_loop_tests
```

### 2.16b `paint::hit_test` — hit-testing aditivo (ronda 9)

La API aditiva de `crates/paint` mapea píxeles a cajas/DOM en **orden de pintado**:
`HitRegion { rect, node_id, kind }`, `hit_test`/`hit_test_all` (gana el topmost), `HitLink`/`find_link_at` (sube al `<a href>` HTML y devuelve el href crudo) y los builders `build_display_list_with_hit_regions`/`build_display_list_and_hit_regions`. Las entradas clásicas (`build_display_list`) no cambian ni un byte de golden y el coste es opt-in. Límites: granularidad caja/run, sin `pointer-events`/transforms, coordenadas CSS de página (el host ajusta scroll/scale).

```bash
cargo test -p paint --test hit_testing   # 11 tests nuevos (108 paint)
# ronda 10: render construye RenderOutput.hit_regions (+0,6 µs) y el shell usa find_link_at
```

### 2.16e Fuentes web @font-face + WOFF2 (ronda 13)

`crates/css` parsea `@font-face` (`Stylesheet.font_faces`), `crates/fonts` decodifica **WOFF2→sfnt** con la dependencia autorizada **`woff2-patched` 0.4** (Apache-2.0, pure Rust, sin C/FFI; **solo en `fonts`**) y `render` recoge las reglas de todas las hojas (incluido `@import`), deduplica, descarga con `ResourceLoader` (`--no-network`, caps **8 fuentes/8 MiB/32 MiB**), construye una **DB por render** para layout+paint y reporta `fonts_loaded/fonts_failed/font_bytes` + `timings.fonts` (fallos ⇒ warning no fatal). Fixtures OFL Open Sans (18.640 B) en `crates/fonts/tests/fixtures/` y `tests/fixtures/render/`.

```bash
cargo test -p fonts -p css -p render
cargo run -p render -- tests/fixtures/render/webfont.html /tmp/webfont.png --json      # md5 f38aed03…; fonts_loaded=1
cargo run -p render -- tests/fixtures/render/webfont-fallback.html /tmp/fallback.png  # md5 e2d76dee…
```

Límites: sin WOFF1, primer `src` soportado, weight/style no eligen cara, sin unicode-range/font-display (bloqueante), sin subsetting/color fonts y DB no cacheada. La dep nueva hará rebuild del cache de CI en la próxima ejecución.

### 2.16f Crate `svg` + SVG E2E en render (ronda 13)

`crates/svg` implementa XML subset + modelo SVG + path data con arcos + rasterizador **scanline AA determinista** (4 subfilas + cobertura analítica; strokes 4×4), con límites anti-DoS (8 MiB/256 profundidad/100k elementos/1M segmentos/16k lado/64 Mpx/`<use>` 16) y `#![forbid(unsafe_code)]`. No soporta `<text>`, CSS class/style sheets, gradientes/patterns, dasharray, markers, filtros/máscaras/clips, script ni refs externas. **Cableado E2E HECHO** en `render`: `<img src=*.svg>` y `background-image` (detección por Content-Type/sniffing, tamaño intrínseco width/height/viewBox con 300×150 por defecto, cap **16 Mpx/64 MiB RGBA**, corrupto ⇒ warning no fatal, navegación directa en página sintética) con stat `svgs_rendered`; 5 fixtures `svg_*` + 6 assets y 11 tests.

```bash
cargo test -p svg
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/simple.svg /tmp/simple.png
python3 tools/png_check.py /tmp/simple.png
cargo test -p render --test svg_tests     # 11 tests SVG E2E
cargo run -p render -- tests/fixtures/render/svg_img.html /tmp/svg_img.png --json   # svgs_rendered=1
```

### 2.16g Shell: zoom y auto-refresh (ronda 13)

```bash
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/zoom2.png --width 640 --height 400 --zoom 2  # 320×200, md5 c11903cf…
cargo run -p shell -- tests/fixtures/render/showcase/index.html --zoom 1.5 --refresh-ms 250
```

Zoom `Ctrl+=/-/0` y `--zoom` 0.5–3.0 (escalones 50–300 %, viewport CSS = físico/(scale·zoom)); `Ctrl+L` imprime la URL; auto-refresh opt-in solo cuando `scripts_executed>0 && script_timers_pending>0` (worker generacional, scroll conservado, sin bucles; repite el primer frame de animaciones con reloj virtual).

### 2.16h Background geometry: repeat/size/position (ronda 15)

CSS parsea `background-repeat`/`size`/`position` y el shorthand; paint calcula `background_tile_size`/`background_tiles` (cover/contain centrado, tiling con clip al padding box, **cap 4096**; space/round como repeat). **`background_geometry_explicit`** mantiene el modo legado para que los goldens (`c95c7eff…`, `60f8984e…`, `9e957177…`) sigan byte-idénticos.

```bash
cargo test -p css -p paint
cargo run -p render -- tests/fixtures/render/backgrounds.html /tmp/bg.png
```

### 2.16i SVG inline, determinismo y rendimiento IFC (ronda 15)

`render` sustituye los `<svg>…</svg>` inline por un `<img>` sintético antes de styles/layout (base64 + `xmlns`; caps **256/1 MiB/8 MiB/profundidad 256**, stat `inline_svgs`, 7 fixtures `svg_inline_*`). El event loop usa **sólo reloj virtual** (`run_until` en pasos de 1 ms; fixture `js_event_timeout` md5 `07682088…`). IFC: pool de texto/ids/breaking incremental/fast path ⇒ **layout 50,5→36,3 ms** y **pipeline 132,9→120,5 ms**.

```bash
cargo run -p render -- tests/fixtures/render/svg_inline_simple.html /tmp/inline.png --json   # inline_svgs>0
cargo test -p render --test svg_tests -p render --test event_loop_tests
cargo run --release -p bench -- --filter layout --iterations 5     # ~36 ms
cargo run --release -p bench -- --filter pipeline --iterations 3   # ~120 ms
```

### 2.16j Ronda 16: SVG real, backgrounds multicapa y APIs de navegador

- **SVG**: `<style>`/class/id/descendientes con cascada (CDATA incluido) y gradientes `linearGradient`/`radialGradient` (stops/offset/opacity, units, spread, gradientTransform, href); `fill`/`stroke`; `<pattern>` pendiente; 60 tests.
- **Backgrounds**: `background-origin`/`clip`/`attachment` + **multi-capa** (`background_layers` top-first), `space`/`round` reales; goldens intactos via `background_geometry_explicit`; css 76/paint 130 tests.
- **APIs de navegador**: `matchMedia`, viewport/DPR/scroll, rAF 16 ms, `onerror`, `readyState`, consola ampliada; cableadas en `render` (getComputedStyle pre-script, MediaContext real, rAF epoch fijo 48 ms, onerror/console ⇒ warnings `console_errors`); js 71/render 123 tests.

```bash
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/gradient.svg /tmp/gradient.png
cargo run -p render -- tests/fixtures/render/background_layers.html /tmp/layers.png --width 240 --height 260  # c357cc2c…
cargo run -p render -- tests/fixtures/render/browser_api.html /tmp/api.png --json
cargo run -p render -- tests/fixtures/render/raf_animation.html /tmp/raf.png --json  # script_virtual_time_ms=48
```

### 2.16k Ronda 17: flexbox, decoraciones, formularios y SVG pattern/text

- **Flexbox MVP**: `display:flex/inline-flex`, direction 4, justify 5, align 5, wrap, gap, grow/shrink/basis + shorthand; layout 146 / css 94 tests; fixtures `flex_row`/`flex_column`/`flex_wrap` (md5s `c28ce538…`/`8d06d66c…`/`7fbf998e…`).
- **Decoraciones**: `border-radius`/`box-shadow`/`text-shadow` con blur determinista de 3 pasadas (caps 64 px/32 sombras), `paint --test effects` (11) y goldens intactos.
- **Formularios editables**: UA styles + shell `src/forms.rs` (FocusState/TextEdit UTF-8/snapshot `data:` URL y re-render); fixture `forms.html` 640×480 md5 `b8b9a20…`; shell 108 tests.
- **SVG**: `<pattern>` (cap 10k tiles) y `<text>` básico con `crates/fonts` (caps 1.000 texts/10.000 chars); svg 73 tests.

```bash
cargo run -p render -- tests/fixtures/render/flex_row.html /tmp/flex_row.png
cargo run -p shell -- tests/fixtures/render/forms.html --screenshot /tmp/forms.png --width 640 --height 480
cargo test -p paint --test effects
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/pattern.svg /tmp/pattern.png
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/text.svg /tmp/text.png
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
./tools/dev.sh all                     # fmt, clippy, 1107 tests, doc y gate de conformidad
cargo run -p test_harness -- --summary # gate de 4 suites: PASS, exit 0 (árbol 3904/3904, 100 %)
cargo run -p test_harness -- --summary --strict # exit 1 por fallos conocidos

cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
cargo run -p render -- tests/fixtures/render/basic.html /tmp/render.png --width 640 --height 400 --json
bash tools/render_fixture.sh
cargo test -p net -p render -p image -p shell
cargo test -p js && cargo test -p js --no-default-features
cargo test -p paint --test hit_testing
cargo test -p html --lib scripting      # WP-6.4: hook de scripts parser-inserted
cargo test -p render --test event_loop_tests
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/shot.png
./tools/bench.sh --scale 0.1
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
```

### 2.16l Ronda 18: fuzz, grid, clipping/clip:text, CSS var/calc/pseudo-clases y red avanzada

La ronda 18 cierra la robustez y el CSS moderno de la Fase 4 y la red de la Fase 5:

```bash
# Fuzz: 7 targets (html_parse, css_parse, selectors, net_url, render_pipeline, js_eval, svg_parse)
cd fuzz && cargo +nightly fuzz build                  # build de los 7 bins (nightly)
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
cd fuzz && cargo +nightly fuzz run js_eval -- -detect_leaks=0 -runs=300

# Grid MVP (layout + render E2E)
cargo test -p layout --test grid                      # 12 tests de layout grid
cargo run -p render -- tests/fixtures/render/grid_columns.html /tmp/grid.png --json
cargo run -p render -- tests/fixtures/render/grid_rows.html /tmp/grid_rows.png --json
cargo run -p render -- tests/fixtures/render/grid_alignment.html /tmp/grid_align.png --json

# Clipping + background-clip:text
cargo test -p paint --test clipping                   # 9 tests (overflow+radius, clip:text)
cargo test -p css --test background_clip_prefixed     # alias -webkit-background-clip (5 tests)

# CSS moderno: custom properties/var(), calc() y pseudo-clases de estado
cargo test -p css --test state_vars_calc              # 26 tests (var/calc/pseudos)
cargo test -p css                                    # 128 tests

# Red avanzada: SameSite/PSL, Referrer-Policy, Range/206 y cache
cargo test -p net referrer                            # Referrer-Policy
cargo test -p net range                               # Range/206 y Content-Range
cargo test -p net cookies                             # SameSite/PSL
cargo test -p net cache                               # no-cache/must-revalidate/s-maxage/Age/304
```

Límites documentados de la ronda: `calc()` mixto `% + px` no es representable en `Length` (⇒ invalid/inherit); `var()` cae a fallback/initial en ciclos y a los 32 niveles; el clipping y `background-clip:text` tienen cap 16,7 Mpx/4096 runs; grid no soporta named lines/subgrid/zonas/`dense`/auto-fit real (ronda 19 añade spans, placement, minmax y auto-fill); el `ReferrerConfig` se cablea en `render` en la ronda 19 (ya no pendiente); grid soporta spans/placement/minmax/auto-fill desde la ronda 19.

### 2.16m Ronda 19: gradientes CSS, grid avanzado, render referrer/state y find-in-page

```bash
# Gradientes CSS E2E (linear/radial/repeating, capas de background y clip:text)
cargo test -p css --test gradients
cargo test -p paint --test gradients
cargo run -p render -- tests/fixtures/render/gradient_linear.html /tmp/grad_linear.png --width 240 --height 220  # md5 6a04fdc6…
cargo run -p render -- tests/fixtures/render/gradient_radial.html /tmp/grad_radial.png --json

# Grid avanzado: placement con spans/negativas, minmax y auto-fill
cargo test -p layout --test grid                       # 26 tests
cargo run -p render -- tests/fixtures/render/grid_span.html /tmp/grid_span.png --json       # md5 0c376406…
cargo run -p render -- tests/fixtures/render/grid_autofill.html /tmp/grid_auto.png --json   # md5 d2255ca3…

# Render: Referrer-Policy/SameSite en subrecursos y estado CSS real
cargo test -p render --test referrer_policy_tests
cargo test -p render --test css_state_tests
cargo run -p render -- tests/fixtures/render/css_state.html /tmp/css_state.png --json

# Shell find-in-page (headless) y navegación extra
cargo test -p shell --test shell_tests                 # find-in-page (+17)
cargo run -p shell -- tests/fixtures/render/find.html --find "link" --screenshot /tmp/find.png --width 640 --height 400
```

Límites de la ronda: gradientes cap **64 stops / 256 por estilo**, sin `conic-gradient`/color hints/espacios avanzados/`image-set`/`border-image`; grid avanzado cap **1.024 tracks**, sin dense/zonas/auto-fit real/solapes explícitos; find-in-page sólo texto literal case-insensitive (sin regex/imágenes/barra visual). Referrer/SameSite ya cableados en render (snapshot por render; SameSite top-level aproximado).

Cifras de referencia en la ronda 19: **1260 tests** (bench 8, css 142, dom 28, fonts 17+1 doctest, html 60, image 45+1 doctest, js 71, layout 173, net 207, paint 162, **render 136**, shell 125, svg 73, harness 11); gate tokenizer 7.050/7.050, **árbol 3.904/3.904 (100 %)**, serializer 230/230, CSS 30/30; allowlists vacías; rendimiento: **layout ~36,3 ms**, **pipeline ~120,5 ms** (`html/massive-end-p` ~19 ms), encoder PNG −30,5 % invariante; fuzz 7 targets; suite determinista; todos los crates `#![forbid(unsafe_code)]`; MSRV Rust 1.88.

### 2.16n Ronda 21: recuperación de rendimiento y progress status

La ronda 21 recupera los presupuestos de rendimiento **sin subirlos** (pipeline ≤140 ms y layout ≤43 ms son gates). A/B con binarios pre/post del mismo árbol, `taskset` y pasadas intercaladas:

```bash
cargo run --release -p bench -- --filter html/realistic-1mb        # 20,0 → 16,4 ms (−18/−20 %)
cargo run --release -p bench -- --filter resolve-styles-10k-nodes  # 38,84 → 33,91 ms (−12,7 %; ≤35 ✅)
cargo run --release -p bench -- --filter layout/realistic-1mb      # 46,6 → 36,4 ms (−21,8 %; ≤43 ✅)
cargo run --release -p bench -- --filter build-display             # 8,62 → 7,84 ms (−9,1 %)
cargo run --release -p bench -- --filter pipeline/html-to-png      # 154,13 → 132,39 ms (−14,1 %; ≤140 ✅)
```

Resumen de técnicas: html fast paths bulk del tokenizer + runs UTF-8 byte a byte + `has_wide` + dedupe híbrido; css hasher FNV/claves prestadas + caché de `ComputedStyle` 128-bit (umbral 2, sin inline, cap 1024); layout interning de `ComputedStyle` (fingerprint + `eq` exacta antes de compartir `Arc`) + estilos anónimos perezosos + spacing cero en IFC; paint fast path por caja + `DisplayList::with_capacity(16384)` + baseline sin `Vec`. Goldens y gate byte-idénticos; **no se subieron presupuestos**.

Cifras de referencia en la ronda 21: **1345 tests** (bench 8, css 175, dom 28, fonts 17+1 doctest, html 60, image 45+1 doctest, js 71, layout 197, net 207, paint 175, render 136, shell 140, svg 73, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializer 230/230, CSS 30/30; allowlists vacías; status generado overall 95 %, ~108.972 líneas Rust, 14 crates, 12/14 `#![forbid(unsafe_code)]`.

**Progress status**: vive en §4; se regenera con `python3 tools/update_progress.py --write` y CI ejecuta `--check --no-gate --no-count` para frescura. No editar a mano `PROGRESS.md` ni `docs/status/`.

### 2.16o Ronda 22: JPEG/GIF, efectos SVG, HTTP/2, chrome, grid completo y filtros CSS

```bash
# JPEG/GIF desde cero (image + render por sniffing)
cargo test -p image --test decode                    # PNG/JPEG/GIF (54 tests)
cargo run -p render -- tests/fixtures/render/jpeg_gif.html /tmp/jpeg_gif.png --json

# Efectos SVG (clipPath/mask/filtros)
cargo test -p svg --test effects                     # 12 tests + fixtures clip/mask/filter

# HTTP/2 MVP (HPACK/framing/ALPN)
cargo test -p net --test http2_tests                 # 18 tests

# Chrome del shell (address bar, tab strip, `<select>`) headless
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/chrome.png --chrome --width 640 --height 400

# Grid completo y filtros CSS/mix-blend/image-set
cargo run -p render -- tests/fixtures/render/grid_areas.html /tmp/grid_areas.png --json     # md5 8f9a9eba…
cargo run -p render -- tests/fixtures/render/grid_dense.html /tmp/grid_dense.png --json     # md5 1789daf8…
cargo run -p render -- tests/fixtures/render/grid_content.html /tmp/grid_content.png --json # md5 123d1a2b…
cargo run -p render -- tests/fixtures/render/filter_effects.html /tmp/filter.png --json     # md5 ffe06406…
cargo run -p render -- tests/fixtures/render/blend_modes.html /tmp/blend.png --json         # md5 32788e6d…
cargo run -p render -- tests/fixtures/render/image_set.html /tmp/image_set.png --json       # md5 dc638211…
```

Límites de la ronda: JPEG sin progressive/CMYK/EXIF orientation; GIF animado solo primer frame; WebP sigue aviso no fatal. HTTP/2 sin multiplexado/flow-control/push/prioridades/h2c/trailers/request bodies (caps 16 KiB frame/64 KiB headers/2 streams, headers minúsculas). SVG effects: 4096 shapes/16 primitivas/blur 64/64M muestras, sin textPath/feColorMatrix/feBlend/feMerge. Grid: overlaps en orden de documento, auto-fit solo rango repetido, sin subgrid/masonry. CSS filters/blend: caps área 16,7 Mpx/blur 64/funciones 16, sin backdrop-filter/espacios de color avanzados.

Cifras de referencia en la ronda 22: **1449 tests** (bench 9, css 195, dom 28, fonts 17+1 doctest, html 60, image 54, js 71, layout 206, net 225, paint 190, render 137, shell 160, svg 85, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; decode PNG 800×600 **6,65 ms p50 (~290 MB/s)**; status generado desde `docs/status/progress.toml` (regenerar con `python3 tools/update_progress.py --write`).

### 2.16p Ronda 23: perf CSS/paint, EXIF/JPEG progresivo, WebP detectado y HTTP/2 avanzado

```bash
# Perf recuperada (A/B intercalado contra el binario pre-r23)
cargo run --release -p bench -- --filter resolve-styles-10k-nodes   # 35,81 → 26,54 ms
cargo run --release -p bench -- --filter build-display              # 7,34–7,72 ms (≤8)
cargo run --release -p bench -- --filter pipeline/html-to-png      # 142,64 → 105,47 ms

# EXIF orientation, JPEG progresivo y detección WebP
cargo test -p image --test exif                                     # orientaciones 1–8
cargo test -p image --test decode                                   # progresivo (gris exacto)
cargo run -p render -- tests/fixtures/render/image_formats.html /tmp/image_formats.png --json

# HTTP/2 flow control, multiplexado y SETTINGS
cargo test -p net --test h2_flow_multiplex_tests                    # 12 tests loopback
```

Límites de la ronda: JPEG progresivo solo subset (DC/AC first+refine, spectral selection, successive approximation; color refinement ⇒ `UnsupportedFeature`), sin CMYK/12-bit; EXIF solo orientación (sin thumbnails); WebP **detectado sin decoder** (aviso no fatal); HTTP/2 con ventanas conexión/stream y SETTINGS, pero sin push/prioridades/h2c/trailers/100-continue/request bodies; paint fast path no altera semántica; caché CSS 128-bit/umbral 2/cap 1024 con StyleCollection inmutable.

Cifras de referencia en la ronda 23: **1468 tests** (bench 9, css 195, dom 28, fonts 17+1 doctest, html 60, image 60, js 71, layout 206, net 237, paint 190, render 138, shell 160, svg 85, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; css resolve 26,54 ms, pipeline 105,47 ms (A/B), build-display ≤8 ms; status generado desde `docs/status/progress.toml` (`python3 tools/update_progress.py --write`).

### 2.16q Ronda 24: JS browser APIs, aspect-ratio/object-fit/backdrop, imagen, sesión/descargas y fuzz 10 targets

```bash
# JS: History/location/Web Storage/MutationObserver
cargo test -p js --test browser_features_r24
# CSS/paint/layout: aspect-ratio, object-fit/object-position y backdrop-filter
cargo test -p css
cargo run -p render -- tests/fixtures/render/aspect_ratio.html /tmp/aspect.png --json    # a1e4d950…
cargo run -p render -- tests/fixtures/render/object_fit.html /tmp/object_fit.png --json  # b54ddebe…
cargo run -p render -- tests/fixtures/render/backdrop_filter.html /tmp/backdrop.png --json # ff04fac9…
cargo test -p layout --test aspect_ratio                                                 # layout 215
# Imagen: progresivo color completo + WebP VP8L subset
cargo test -p image --test decode
cargo run -p render -- tests/fixtures/render/webp.html /tmp/webp.png --json
# Shell: sesión persistente y descargas (headless)
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/shot.png --width 640 --height 400
# Fuzz: 10 targets
cd fuzz && cargo +nightly fuzz build
```

Límites r24: History/location sin provider ⇒ no-op; MutationObserver subset (sin attributeFilter/StorageEvent); WebP solo VP8L subset (transforms/meta-Huffman/LZ77/VP8 lossy ⇒ `UnsupportedFeature`); backdrop-filter caps 64 blur/16 funciones/16,7 Mpx; sesión caps 32 tabs/64 historial y escritura atómica best-effort; descargas PNG/HTML sanizited; fuzz 10 targets con smoke 300 runs.

Cifras de referencia en la ronda 24: **1547 tests** (bench 9, css 210, dom 28, fonts 17+1 doctest, html 60, image 67, js 83, layout 215, net 237, paint 205, render 139, shell 180, svg 85, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; status generado desde `docs/status/progress.toml`.

### 2.16r Ronda 25: CSS Color 4/5, ES modules, WebP VP8L, providers render y save-png

```bash
cargo test -p css --test color4                        # color()/lab/lch/oklab/oklch/hwb/color-mix
cargo run -p render -- tests/fixtures/render/color_spaces.html /tmp/colors.png --json
cargo test -p js --test es_modules                     # resolver/loader, import/export, ciclos
cargo run -p render -- tests/fixtures/render/webp_real.html /tmp/webp_real.png --json
cargo run -p render -- tests/fixtures/render/js_observer_chain.html /tmp/observer.png --json
cargo run -p shell -- tests/fixtures/render/basic.html --save-png /tmp/page.png --width 640 --height 400
```

Límites r25: Color 4/5 con matrices CSS Color 4 y clamp por canal (sin HDR/color spaces arbitrarios); ES modules subset line-based (sin import() dinámico/top-level await/export-from/live bindings; ciclos ⇒ `ModuleCycle`); VP8L completo pero VP8 lossy/corrupto ⇒ warning; providers de render son canales (`history_actions`/`location_actions`) — render no navega; save-png directo valida firma y escribe atómico.

Cifras de referencia en la ronda 25: **1594 tests** (bench 9, css 230, dom 28, fonts 17+1 doctest, html 60, image 69, js 96, layout 215, net 237, paint 208, render 145, shell 183, svg 85, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; status generado desde `docs/status/progress.toml`.

### 2.16s Ronda 26: module scripts, decode escalado, h2 bodies, DOM bug-bash, SVG filters/SMIL y bench aislado

```bash
cargo test -p render --test module_scripts_tests      # <script type=module> inline/externo
cargo test -p image --test scaled                     # decode_scaled
cargo run -p render -- tests/fixtures/render/module_chain.html /tmp/modules.png --json
cargo test -p net                                      # h2 bodies/trailers/100-continue + h1 POST
cargo test -p dom && cargo test -p js                  # bug-bash DOM/JS
cargo test -p svg --test filters_smil                  # feColorMatrix/Blend/Merge/Composite + SMIL
cargo run --release -p bench -- --filter shell/save-png-encode-800x600
```

Límites r26: subset ESM estático (sin dynamic import/TLA/import maps; una raíz por script; loader http con caché propia); decode escalado PNG exacto por unfilter/Adam7, JPEG/GIF/WebP post-subsample; h2 sin push/priorities/h2c y cuerpos buffered; 100-continue sin timer; bug-bash deja gaps documentados (fetch/dataset/innerText/Range/MouseEvent/URLSearchParams/queueMicrotask…); SMIL sin keyTimes/sync/CSS animations/textPath; bench `--save-png-isolated` reutiliza bytes.

Cifras de referencia en la ronda 26: **1653 tests** (bench 9, css 230, dom 33, fonts 17+1 doctest, html 60, image 76, js 111, layout 215, net 249, paint 208, render 153, shell 183, svg 97, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; status generado desde `docs/status/progress.toml`.

### 2.16t Ronda 27: fix PNG, h2 push/priorities/h2c, gaps DOM/JS, perf efectos y chrome

```bash
cargo test -p image                                   # regresión PNG 6,54 ms y escalado
cargo test -p net --test h2_advanced                  # PUSH_PROMISE/priorities/h2c
cargo test -p js --test gap_apis && cargo test -p dom --test gap_api
cargo run --release -p bench -- --filter pipeline/filter-fixture-300x260
cargo run -p shell -- tests/fixtures/render/basic.html --chrome --screenshot /tmp/chrome.png --width 640 --height 400
```

Límites r27: push h2 descartado (render no lo consume aún), h2c GET-only con reconexión y prioridades MVP; gaps DOM restantes (getBoundingClientRect/offset sin layout provider, fetch/XHR, crypto, dataset escritura, innerText con layout, Range, NodeList live); chrome con portapapeles interno documentado y caret sólido determinista; backdrop sigue pagando blur+PNG entrópico (byte-safe no toca el encoder de goldens).

Cifras de referencia en la ronda 27: **1695 tests** (bench 9, css 230, dom 36, fonts 17+1 doctest, html 60, image 76, js 128, layout 215, net 259, paint 208, render 153, shell 195, svg 97, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; status generado desde `docs/status/progress.toml`.

### 2.16u Ronda 28: reduced IDCT/EXIF, push+uploads streaming, geometría JS, blur byte-safe y menú contextual

```bash
cargo test -p image                                      # IDCT reducida y EXIF thumbnails
cargo test -p net --test h2_push_tests && cargo test -p net --test upload_streaming
cargo test -p js --test gap_apis && cargo test -p render --test js_geometry_tests
cargo run -p render -- tests/fixtures/render/js_geometry_fixed.html /tmp/geo.png --json
cargo run -p shell -- tests/fixtures/render/basic.html --chrome --chrome-menu --screenshot /tmp/menu.png --width 640 --height 400
```

Límites r28: reduced IDCT solo en `decode_scaled` (progresivo fallback documentado) y EXIF thumbnails con caps; push h2 solo misma conexión, preload cache por carga y sin push h1/trailers/sendfile; streaming uploads single-shot con cap 16 MiB; geometría sin scroll offsets reales/transforms/zoom y primer script sin layout ⇒ 0; blur/PNG byte-safe con objetivo 1,65 ms no alcanzable (floor ~1,54 ms); menú contextual con portapapeles interno, sin submenús/nativo y Save image re-descarga solo `<img>`.

Cifras de referencia en la ronda 28: **1738 tests** (bench 9, css 230, dom 36, fonts 17+1 doctest, html 60, image 84, js 130, layout 215, net 279, paint 208, render 157, shell 204, svg 97, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; status generado desde `docs/status/progress.toml`.

### 2.16v Ronda 29: streaming/keep-alive, textPath/SMIL, import maps, WebP header, shell pages y fuzz 14

```bash
cargo test -p net --test streaming_responses && cargo test -p net --test connection_reuse
cargo test -p svg --test textpath_smil
cargo test -p js --test es_modules
cargo test -p image --test webp_lossy
cargo run -p shell -- tests/fixtures/render/basic.html --chrome --screenshot /tmp/chrome.png --width 640 --height 400
cd fuzz && cargo +nightly fuzz build   # 14/14
```

Límites r29: h1 streaming con fallback buffered 16 KiB y pool solo http plano (sin pool h2 entre loads); textPath sin auto-rotate/side right/textLength y solo hijo directo; SMIL keyTimes/keySplines subset; import maps solo primer map, import() solo literales eager, top-level await gap tipado; WebP lossy solo cabecera (pixel decoder pendiente, warning/placeholder); shell directorios file:// deterministas y favicon cap 64 KiB; fuzz 14 targets/corpus 37.

Cifras de referencia en la ronda 29: **1796 tests** (bench 9, css 230, dom 36, fonts 17+1 doctest, html 60, image 90, js 137, layout 215, net 287, paint 208, render 162, shell 224, svg 109, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; status generado desde `docs/status/progress.toml`.

### 2.16w Ronda 30: VP8 lossy, container queries, PSL/trailers, workers, historial y chrome fast path

```bash
cargo test -p image --test vp8
cargo test -p css && cargo test -p layout
cargo test -p net --test psl && cargo test -p net --test trailers
cargo test -p js --test workers && cargo test -p render --test worker_tests
cargo test -p shell --test history
```

Límites r30: VP8 keyframe only, simple filter luma-only y fixtures de 1 partición (multi-partición implementada); container queries sin style/scroll-state ni cqi/cqw completas y predicción de tamaño de una pasada; PSL IDN no implementado y trailers requieren body; workers cooperativos sin SharedWorker/importScripts y loader file://; historial cap 64/dedupe y fast path chrome con PNG final dominante (floor 22,9 ms a 1024).

Cifras de referencia en la ronda 30: **1853 tests** (bench 9, css 241, dom 36, fonts 17+1 doctest, html 60, image 90, js 147, layout 224, net 299, paint 208, render 165, shell 236, svg 109, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; status generado desde `docs/status/progress.toml`.

### 2.16x Ronda 31: cascade layers, PNG single-encode, fetch, proxies y perf VP8

```bash
cargo test -p css --test cascade_layers
cargo test -p render --test encode_png_tests
cargo test -p js --test fetch_tests && cargo test -p render --test fetch_tests
cargo test -p net --test proxy
cargo run --release -p bench -- --filter image/decode-webp-lossy
```

Límites r31: layers sin `layer()` en imports y sublayers aproximadas; `encode_png=false` solo útil si el caller muta el pixmap y los overlays requieren un encode final; fetch sin streaming/abort/timeout, GET/POST y CORS same-origin; proxies sin PAC/WPAD/SOCKS, Basic solo y sin pooling; VP8 mantiene caps y tolerancias (keyframe-only).

Cifras de referencia en la ronda 31: **1907 tests** (bench 10, css 260, dom 36, fonts 17+1 doctest, html 60, image 90, js 158, layout 224, net 315, paint 208, render 170, shell 238, svg 109, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; status generado desde `docs/status/progress.toml`.

### 2.16y Ronda 32: perf CSS/layout/PNG, TLA/live bindings, h1 streaming y push priority

```bash
cargo run --release -p bench -- --filter css/resolve-styles
cargo run --release -p bench -- --filter image/decode-png
cargo run --release -p bench -- --filter paint/encode-png-1024
cargo test -p js --test tla_live_bindings && cargo test -p render --test tla_tests
cargo test -p net --test h1_streaming && cargo test -p net --test push_priority
```

Límites r32: Bloom pruning solo en relaciones descendiente/hijo (no rechaza matches válidos); PNG decode sin SIMD/unsafe y raw md5 idéntico a PIL; encode byte-safe mantiene goldens; backgrounds/inline_svg fast paths solo si el documento no puede necesitar imágenes/SVG; TLA/live bindings en la ruta de módulos nativos (import maps no se consultan ahí); h1 streaming con gzip/deflate buffered y 16 KiB slices, sin HTTP/1.0; push prioritization sin grafo completo de prioridades.

Cifras de referencia en la ronda 32: **1929 tests** (bench 10, css 260, dom 36, fonts 17+1 doctest, html 60, image 90, js 168, layout 224, net 325, paint 208, render 172, shell 238, svg 109, harness 11); gate tokenizer 7.050/7.050, árbol 3.904/3.904, serializador 230/230, CSS 30/30; allowlists vacías; status generado desde `docs/status/progress.toml`.

## 4. Progress status generado (`PROGRESS.md`, `docs/status/*.svg`)

El estado de avance del proyecto se genera **desde datos reales del repositorio**, sin
servicios externos ni CI de terceros (inspirado en el *progress map* de AnyPS5). El
manifiesto editable es `docs/status/progress.toml` (fases F1–F7 con `items`/`pending`,
`next_up` y áreas transversales); el script `tools/update_progress.py` (Python 3.11+,
**solo stdlib**, determinista) mide el repo y regenera:

| Fichero generado | Contenido |
|---|---|
| `PROGRESS.md` | Página de progreso: badges, mapa de fases con barras, hechos/pendientes, stats, gate de conformidad, rondas recientes y “next up”. |
| `docs/status/progress.svg` | Mapa visual 900 px: % global, una fila por fase con color (verde ≥95 %, azul ≥85 %, ámbar ≥70 %, rojo <70 %) y pie con commit/tests/gate. |
| `docs/status/badge-tests.svg` | Badge shields-like con el nº de tests del workspace. |
| `docs/status/badge-conformance.svg` | Badge del gate de conformidad (PASS/FAIL + 4 suites). |
| `docs/status/badge-crates.svg` | Badge del nº de crates del workspace. |
| `docs/status/badge-unsafe.svg` | Badge de crates con `#![forbid(unsafe_code)]`. |
| `docs/status/progress.toml` | Los bloques `[stats]`/`[gate]` se reescriben con las medidas reales. |

Datos medidos: HEAD corto y fecha del último commit (`git log -1 --format=%cs`, nunca la
hora actual), nº de crates (`crates/*/Cargo.toml`), líneas Rust (`git ls-files '*.rs'`),
tests (`cargo test --workspace -- --list`, cacheado por HEAD en `target/progress-stats.json`),
gate de conformidad (`cargo run -q -p test_harness -- --summary`: tokenizer/árbol/serializer/
CSS + PASS) y crates con `#![forbid(unsafe_code)]`.

```bash
# Regenerar todo (cuenta tests y ejecuta el gate de conformidad)
python3 tools/update_progress.py --write

# Regeneraciones rápidas (reutilizan el último conteo / gate del manifiesto)
python3 tools/update_progress.py --write --no-gate
python3 tools/update_progress.py --write --no-count --no-gate

# Verificación de frescura (CI): exit 0 si está al día, 1 si hay que regenerar
python3 tools/update_progress.py --check --no-gate
```

**Idempotencia**: dos ejecuciones sobre el mismo árbol escriben bytes idénticos (no hay
timestamps de reloj; la fecha es la del commit). `--check` regenera en memoria y compara
bytes, mostrando la primera línea distinta. `--no-count` reutiliza `[stats].tests` del
manifiesto (útil en local); `--no-gate` reutiliza el último gate verificado. En CI, el job
`quality` ejecuta `--check --no-gate` tras “Workspace tests” (el gate ya corre en el job
`conformance`), de modo que un PR que cambie tests/fases sin regenerar falla.

**Auto-referencia del snapshot**: los ficheros generados incluyen el commit/fecha del
snapshot, pero no pueden contener el hash del commit que los contiene. Por eso `--check`
**normaliza el bloque de snapshot** (hash y fecha en la cabecera/tabla de `PROGRESS.md`, el
`<desc>`/pie del SVG y `tests_commit`/`tests_date`/`verified_commit` del manifiesto) a
placeholders `<commit>`/`<date>` en ambos lados antes de comparar; el resto (fases, items,
stats, LoC, tests, gate, histórico) se compara byte a byte. Resultado: tras `--write` y
commitear los ficheros de status, `--check` sigue en verde; un cambio real sin regenerar
sigue fallando. Probado simulando ficheros estampados con otro commit/fecha (exit 0) y
cambiando un `percent` del manifiesto (exit 1).

Flujo recomendado al cerrar una ronda: actualizar `docs/status/progress.toml` (percent,
`items`/`pending`, `next_up`), ejecutar `--write`, y commitear juntos el manifiesto y los
ficheros generados.

