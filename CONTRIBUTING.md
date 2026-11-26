# Guía de Contribución al Motor Megi (`CONTRIBUTING.md`)

¡Gracias por tu interés en contribuir a **Megi**! 🌐🦀

Megi es un proyecto ambicioso cuyo objetivo es construir un **motor de navegación web moderno, open source, independiente y de alto rendimiento**, desarrollado en **Rust** (Edición 2024), guiado estrictamente por las especificaciones formales de la web (**WHATWG** y **W3C**) y auto-verificado continuamente mediante pruebas oficiales de conformidad.

Este documento explica en detalle el **enfoque y la filosofía del proyecto**, la arquitectura técnica que debes respetar, y el flujo de trabajo para colaborar eficazmente.

---

## 1. El Enfoque y Filosofía de Megi

Para construir un motor web robusto sin perderse en la complejidad de la web moderna, Megi se apoya en **cinco principios inquebrantables**:

### 1.1 Desarrollo Guiado por Especificaciones (Spec-Driven Development)
- **Queda terminantemente prohibido inferir o inventar algoritmos por intuición**: Todo el comportamiento de Megi debe seguir al pie de la letra el estándar formal vivo (*Living Standard*) de WHATWG o la recomendación W3C correspondiente.
- **Trazabilidad directa en el código**: Cada función, enumerador o rama condicional debe documentar la sección, ancla (`#id`) y paso exacto de la especificación que implementa (por ejemplo: `// WHATWG HTML §13.2.6.4.7 The "in body" insertion mode, step 12`).

### 1.2 Independencia Absoluta (Zero Engine Forks & Pure Safe Rust)
- Megi **no es una bifurcación (*fork*)** ni un envoltorio de Blink (Chromium), WebKit (Safari), Gecko (Firefox), ni de los motores de layout de Servo o Blitz.
- Los crates de parsing/layout fundamentales (`crates/core`, `crates/html`, `crates/css`, `crates/layout`) se desarrollan exclusivamente con la **biblioteca estándar de Rust (`std`)**, sin librerías externas de parsing en crates.io. La única excepción autorizada son las dependencias tipográficas de bajo nivel de `crates/fonts` (`fontdb`, `ttf-parser`, `rustybuzz`, `ab_glyph`), listadas en `docs/ARCHITECTURE.md` §3.1; el rasterizador y el codificador PNG son propios de `crates/raster` (`paint` construye la display list y depende de `raster`, spec §2.3) y no usan dependencias externas (decisión D3, §3.2).

### 1.3 Auto-Testeo Continuo en Bucle Cerrado (Closed-Loop Testing)
- Ninguna funcionalidad se considera completada sin una prueba de conformidad que la valide.
- Trabajamos con las suites de prueba de referencia de la industria:
  - **`html5lib-tests`**: Más de 7.000 pruebas de tokenizador, **3.904/3.904** de construcción del árbol (último gate publicado, sin allowlist) y las suites oficiales de **serialización HTML** (`serializer/*.test`).
  - **Web Platform Tests (WPT)**: Pruebas oficiales compartidas por todos los motores de navegación modernos (fixtures de CSS Syntax vendorizadas).
- El arnés interno ([`crates/test_harness`](crates/test_harness)) permite ejecutar las pruebas con `cargo test` o `cargo run -p megi-test-harness -- --summary`, diagnosticar discrepancias y corregir el código iterativamente. El gate cubre **4 suites**: tokenizer **7.050/7.050** (allowlist vacía), **árbol 3.904/3.904 (100 %, sin allowlist; las 4 suites `scripted_*` ejecutan JS real vía WP-6.4)**, **serializador HTML 230/230** y **CSS Syntax 30/30** con fixtures WPT vendorizadas en `tests/wpt/css/css-syntax/` (reproducibles sin red). `--summary` actúa como **gate** (exit 0 si todos los fallos están en la allowlist documentada; `--strict` hace exit 1 ante cualquier fallo). El total vigente de tests se publica en [PROGRESS.md](PROGRESS.md) (regenerado por `tools/update_progress.py`).

### 1.4 Modelo de Memoria en Arena (Sin `Rc<RefCell<Node>>`)
- Uno de los mayores cuellos de botella y fuentes de *memory leaks* en motores web en Rust es el uso de `Rc<RefCell<Node>>` o recolectores de basura pesados.
- Megi utiliza una **arena contigua indexada** ([`crates/core/src/lib.rs`](crates/core/src/lib.rs)):
  - Todos los nodos residen en un vector plano `Document::nodes: Vec<Node>`.
  - Los nodos se referencian mediante manejadores compactos de 4 bytes: `NodeId(NonZeroU32)` (aprovechando la optimización de nicho para que `Option<NodeId>` ocupe solo 4 bytes).
  - Los 5 punteros del árbol (`parent`, `first_child`, `last_child`, `prev_sibling`, `next_sibling`) son intrusivos y permiten mutaciones y reparentados en tiempo constante $O(1)$ con máxima localidad de caché.

### 1.5 Cero Advertencias y Calidad Estricta (Zero-Warning Policy)
- El proyecto mantiene una política estricta de **cero advertencias**:
  - `cargo fmt --check`: Formateo canónico con `rustfmt`.
  - `cargo clippy --workspace --all-targets -- -D warnings`: Cero advertencias de linter permitidas.
  - `cargo +1.91 check --workspace --all-targets`: verificación contra la MSRV declarada (`rust-version = "1.91"`).
  - `./tools/dev.sh all`: script integral (fmt, clippy, tests, doc y **gate de conformidad**) que debe pasar al 100% verde antes de cualquier commit o Pull Request.
- **Progress status generado**: si tocas docs, tests, fases o `docs/status/progress.toml`, regenera el snapshot antes de commitear con `python3 tools/update_progress.py --write` (CI ejecuta el check de frescura); **no edites a mano** `PROGRESS.md` ni `docs/status/`.
  - `cd fuzz && cargo +nightly fuzz build`: compila los 34 targets de fuzz (nightly; también lo hace un job build-only en PRs y pushes a `main` que tocan `fuzz/**`, crates del motor o manifests raíz).

### 1.6 Política de MSRV

- La MSRV vive **solo** en `Cargo.toml` (`rust-version`); los crates la heredan con `rust-version.workspace = true`. No se declara una MSRV distinta por crate.
- Hoy es **1.91**, y no es una elección libre: `boa_engine` 0.22 exige 1.91 (`cargo +1.88 check` falla con *"rustc 1.88.0 is not supported"*). Si se sube Boa, la MSRV sube con él.
- El job `msrv` de CI instala un toolchain **fijado** (`1.91.0`, no `stable`) y ejecuta `cargo check --workspace --all-targets --locked` **y** el build sin JS (`-p megi-js`, `-p megi-render` con `--no-default-features`).
- Se usa `--locked` a propósito: la MSRV se valida contra el lockfile versionado, que es exactamente lo que compilan los contribuyentes y los usuarios de `cargo install --locked`.
- **No** se ejecuta `-Z minimal-versions` en CI: exigiría un árbol de dependencias mínimo irreal y produce rojos que nadie reproduce. La política es la contraria: el lockfile es la fuente de verdad y, si una actualización de dependencia rompe la MSRV, el propio job falla con el nombre del crate culpable; se resuelve fijando la versión anterior o subiendo `rust-version` de forma explícita (con su nota en `CHANGELOG.md`).
- El toolchain de desarrollo puede ser más nuevo (`stable`); el job semanal `toolchain-drift` compila con el `stable` más reciente para detectar lints/novedades que rompan la build antes de que lo haga un usuario.

---

## 2. Mapa de Crates del Proyecto

El espacio de trabajo se organiza como un **Grafo Acíclico Dirigido (DAG)** de dependencias:

```text
crates/
├── core/          # Núcleo: arena DOM, NodeId(NonZeroU32), mutaciones O(1). [0 dependencias]
├── html/          # HTML5: tokenizer 84 estados, tree builder 21 modos, serializer, scripting.
├── css/           # CSS Syntax L3, Selectors L4, Cascade L4, ComputedStyle, hoja UA.
├── layout/        # Box Tree, BFC/IFC, flex, grid, tablas, bidi.
├── fonts/         # Tipografía: fontdb, ttf-parser, rustybuzz, ab_glyph, WOFF2.
├── display/       # Contrato puro de DisplayList compartido por paint y raster.
├── paint/         # Construcción de DisplayList y orden de pintado CSS 2.2.
├── raster/        # Rasterizado 2D AA y codificación PNG determinista.
├── image/         # Decodificadores PNG/JPEG/GIF/WebP e inflate propios.
├── svg/           # Parser SVG, modelo, path y rasterizador vectorial AA.
├── net/           # URL, HTTP/1.1+HTTP/2, TLS, caché, cookies, CORS, loader, WebSocket.
├── js/            # Boa 0.22 + bindings DOM, providers Arc y event loop; estado en Context (sin TLS).
├── render/        # Pipeline URL → PNG con scripts, event loop, imágenes y fuentes.
├── webview/       # API embebible sin ventana (load/frame/eventos/hit-test). Depende de render.
├── browser/       # Shell nativo: winit+softbuffer, chrome, pestañas, sesión. Solo depende de webview y sandbox.
├── sandbox/       # Landlock/no_new_privs aplicados en el worker (Unsupported en macOS/Windows).
├── bench/         # Benchmarks reproducibles.
└── test_harness/  # Gate de conformidad html5lib-tests/WPT.

Reglas: DAG unidireccional verificado por tools/check_arch.py; .rs <= 500 líneas
(objetivo 400); #![forbid(unsafe_code)] en los 18 crates; sin Rc<RefCell> salvo el
ContextHost de Boa; TDD y gates antes de cada commit (ver AGENTS.md).
```

---

> **Trabajas con un agente de IA?** Empieza por [`docs/AI-QUICKSTART.md`](docs/AI-QUICKSTART.md): contiene un prompt autocontenido con el flujo de orientación, TDD, verificación y entrega específico de este repo.

## 3. Cómo Encontrar Tareas para Colaborar

La **hoja de ruta vigente** es la del roadmap aprobado (PR #2290): [`docs/superpowers/specs/2026-11-24-megi-ambition-design.md`](docs/superpowers/specs/2026-11-24-megi-ambition-design.md) y el índice [`docs/superpowers/plans/2026-11-24-megi-ambition-master-plan.md`](docs/superpowers/plans/2026-11-24-megi-ambition-master-plan.md). El trabajo se organiza en **work packages (WP)** con esquema en [`docs/work-packages/SCHEMA.md`](docs/work-packages/SCHEMA.md), cola en `docs/work-packages/BOARD.md` y punto de entrada en [`docs/AGENT-INDEX.md`](docs/AGENT-INDEX.md). `docs/ROADMAP.md` es histórico. Los WPs se vinculan a:
1. La sección exacta de la especificación WHATWG/W3C.
2. El archivo de prueba de `html5lib-tests` o WPT que lo valida.
3. El crate y módulo responsable.
4. El criterio de aceptación verificable por test.

### Estado por áreas (hitos M0–M12; **hito histórico ronda 11: árbol HTML5 3.904/3.904, 100 %**):
- **Layout (`crates/layout`) — trabajo acumulado (la etiqueta «Fase 4» es histórica)**:
  - **Ronda 17**: **Flexbox MVP** (`layout/flex.rs`: display flex/inline-flex, direction/justify/align/wrap/gap/grow-shrink-basis).
  - **Ronda 18**: **Grid MVP** (`layout/grid.rs`: display grid/inline-grid, tracks fixed/%/fr/auto/repeat, gaps, auto-placement row-major, justify/align items/self y auto-rows).
  - **Ronda 19**: **Grid avanzado** (placement explícito con líneas 1-based/negativas y `span N`, auto-placer que salta ocupadas y crece tracks implícitos, `minmax()`, `repeat(auto-fill|auto-fit)`, `order` estable; solapes explícitos/named lines/zonas/subgrid fuera de alcance).
  - Pendiente menor: block-in-inline, colores de conflictos de `border-collapse` y auto-colapso de bloques vacíos.
  - Ya hechos (ronda 8): BFC/IFC, posicionamiento, floats, overflow, tablas §17 (rowspan/fixed/inline-table/collapse + fast path), inline-block, reemplazados, árbol de celdas expuesto y verificado (133 tests).
- **Paint (`crates/paint`)**:
  - Gradientes ya HECHOS (rondas 19-20): `DisplayItem::Gradient` por tile con AA/sRGB, size/position/repeat/origin/clip/radius/clip:text y **conic/hints/OKLab** (ronda 20); siguientes: `display-p3`/espacios avanzados, `border-image`, filtros y pulido de `border-collapse` (colores).
  - **Ronda 18 HECHO**: `overflow:hidden`+`border-radius` recorta contenido real y `background-clip:text` compone por máscara de glifos (`cargo test -p megi-paint --test clipping`, 9 tests); display list byte-idéntica sin clipping.
  - **Ronda 17**: **`border-radius`/`box-shadow`/`text-shadow`** (DisplayItems redondeados, blur determinista 3 pasadas; `cargo test -p megi-paint --test effects`).
  - Ya hechos (rondas 15-16): **`background-repeat`/`size`/`position`/`origin`/`clip`/`attachment`** con tiling/clip (cap 4096), **multi-capa `background_layers`** (`space`/`round` reales) y legacy flag para goldens.
  - Glyph cache global, presupuestos de memoria/tiempo y capas de opacidad.
  - Ya hecho (rondas 9–10): **hit-testing aditivo** (`HitRegion`/`hit_test`/`find_link_at`, 11 tests) y su cableado en `render` (`RenderOutput.hit_regions`, +0,6 µs) sin cambiar golden.
- **CSS (`crates/css`)**:
  - **Ronda 20 HECHO**: `conic-gradient`/`repeating-conic` + color hints + espacios `srgb|srgb-linear|oklab`, `calc()` mixto (`CalcLengths`), tipografía (`text-transform`/spacing/white-space) y **fix de custom properties** lazy/COW (`Option<Arc<HashMap>>`, fast path `needs_resolution`; A/B −14,9 % css resolve quiet).
- **Estado por áreas**: el detalle vivo de capacidades está en `PROGRESS.md` y `docs/status/coverage.html`. La descripción por hitos **M0–M12** y sus barras de aceptación está en el master plan (sección «Barras de cierre por hito»). Los antiguos bloques «Fase 5/6/7» de este documento son históricos y no deben usarse para decidir qué hacer a continuación.

Para decidir el siguiente trabajo usa siempre [`docs/AGENT-INDEX.md`](docs/AGENT-INDEX.md) y el master plan. El documento `docs/ROADMAP.md` es un resumen histórico: consulta el código y `PROGRESS.md` para el estado real. Rendimiento en [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md) y fuzzing/seguridad en [`docs/FUZZING.md`](docs/FUZZING.md) y [`SECURITY.md`](SECURITY.md).

---

## 4. Flujo de Trabajo para Desarrolladores

### 4.1 Clonar y Preparar el Repositorio

```bash
git clone https://github.com/juanpimg/Megi.git
cd Megi

# Verificar que tu entorno de Rust cumple la MSRV declarada (Edition 2024 / Rust 1.91+)
rustup toolchain install 1.91
rustc --version
cargo +1.91 check --workspace --all-targets
```

#### Dependencias del sistema (Linux)

Solo `crates/browser` necesita bibliotecas nativas (`winit`/`softbuffer` con X11 y Wayland); el resto del workspace no necesita bibliotecas de sistema. En Debian/Ubuntu, los mismos paquetes que instala CI:

```bash
sudo apt-get install -y --no-install-recommends \
  pkg-config libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libx11-dev libx11-xcb-dev libxcursor-dev libxi-dev libxrandr-dev \
  libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxcb-shm0-dev libxcb-randr0-dev
```

Sin ellas `cargo check --workspace` falla al compilar `wayland-sys`/`x11` y `cargo test --workspace` no llega a ejecutar los tests. Si no quieres instalarlas, añade `--exclude megi-browser` a `check`/`clippy`/`test`/`doc` y verifica el crate de ventana solo en CI.

### 4.2 Utilizar las Herramientas de Inspección

Megi incluye scripts CLI dedicados en `tools/` para acelerar el desarrollo:

```bash
# 1. Consultar la especificación WHATWG HTML en vivo por ancla
python3 tools/whatwg_spec.py html "#parsing-main-inbody"

# 2. Consultar la especificación DOM
python3 tools/whatwg_spec.py dom "#concept-node-tree"

# 3. Inspeccionar un caso de prueba de html5lib-tests
python3 tools/test_inspector.py tree template.dat 0
```

### 4.3 Ejecutar el Ciclo de Verificación Local

Antes de enviar cualquier cambio, ejecuta:

```bash
# Formatear el código
cargo fmt

# Ejecutar el script maestro: fmt, clippy, tests, doc y gate de conformidad
./tools/dev.sh all

# Comprobaciones individuales útiles
cargo test --workspace                         # suite completa (con JS por defecto; recuento en docs/status/badge-tests.svg)
cargo test -p megi-js --no-default-features         # stub del motor sin Boa (CI)
cargo test -p megi-render --no-default-features     # render sin scripts (CI)
cargo +1.91 check --workspace --all-targets
cargo run -p megi-test-harness -- --summary          # gate 4 suites: PASS, exit 0
cargo run -p megi-test-harness -- --summary --strict # exit 1 si hay fallos no aceptados

# Si tu CPU es más lenta que la de CI, escala los presupuestos de complejidad (CI usa 1.0)
MEGI_PERF_BUDGET_SCALE=4 cargo test -p megi-html --test perf

# Pipeline HTML → PNG (comando histórico de la antigua Fase 4; ver PROGRESS.md) y validación de la imagen
cargo run --release -p megi-paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png

# Rendimiento (release; baseline en docs/PERFORMANCE.md)
./tools/bench.sh --scale 0.25
./tools/bench.sh --filter layout    # ronda 12: ~51-53 ms, LayoutBox 176 B
./tools/bench.sh --filter paint     # ronda 12: encoder PNG −30,5 % invariante
./tools/bench.sh --filter pipeline  # ronda 12: ~134-137 ms

# Red, render y JS (comandos históricos de las antiguas Fases 5–6; tests offline)
cargo test -p megi-net -p megi-render -p megi-image
cargo test -p megi-js
cargo test -p megi-js --no-default-features   # stub mínimo sin Boa
cargo test -p megi-js                                # 58 tests (eventos avanzados)
cargo test -p megi-svg                               # crate SVG (73 tests)
cargo test -p megi-render --test svg_tests            # SVG E2E en render (11 tests)
cargo run -p megi-render -- tests/fixtures/render/svg_inline_simple.html /tmp/inline.png --json   # inline_svgs>0
cargo run -p megi-svg --example svg_to_png -- crates/svg/tests/fixtures/gradient.svg /tmp/gradient.png
cargo run -p megi-render -- tests/fixtures/render/flex_row.html /tmp/flex_row.png
cargo run -p megi-browser -- tests/fixtures/render/forms.html --screenshot /tmp/forms.png --width 640 --height 480
cargo test -p megi-paint --test effects
cargo run -p megi-render -- tests/fixtures/render/background_layers.html /tmp/layers.png --width 240 --height 260
cargo run -p megi-render -- tests/fixtures/render/browser_api.html /tmp/api.png --json
cargo run -p megi-render -- tests/fixtures/render/backgrounds.html /tmp/bg.png
cargo run --release -p megi-bench -- --filter layout --iterations 5   # ~36 ms
cargo run -p megi-render -- tests/fixtures/render/svg_img.html /tmp/svg_img.png --json   # svgs_rendered=1
cargo test -p megi-fonts -p megi-css -p megi-render             # fuentes web @font-face/WOFF2
cargo run -p megi-render -- tests/fixtures/render/webfont.html /tmp/webfont.png --json   # md5 f38aed03…
cargo run -p megi-browser -- tests/fixtures/render/basic.html --screenshot /tmp/zoom2.png --width 640 --height 400 --zoom 2  # md5 c11903cf…
cargo test -p megi-html --lib scripting             # WP-6.4: hook parser-inserted
cargo test -p megi-render --test scripting_tests   # JS ejecutado y visible en el PNG (7 fixtures js_*)
cargo test -p megi-render --test event_loop_tests  # event loop presupuestado (6 fixtures js_event_*)
cargo test -p megi-paint --test hit_testing        # hit-testing aditivo (topmost, links, clip)
cargo run -p megi-render -- tests/fixtures/render/js_text.html /tmp/js_on.png   # con JS
cargo run -p megi-render -- tests/fixtures/render/js_text.html /tmp/js_off.png --no-scripting
cargo run -p megi-render -- https://example.com/ /tmp/page.png --cookies --origin https://example.com --json   # red avanzada E2E
cargo run -p megi-render -- tests/fixtures/render/showcase/index.html /tmp/showcase.png --width 1024 --height 768
cargo run -p megi-browser -- tests/fixtures/render/showcase/index.html              # navegación: clic, Alt+←/→, b/n, r
cargo run -p megi-browser -- tests/fixtures/render/basic.html --screenshot /tmp/shot.png   # headless (antigua Fase 7; histórico)
tools/megi.sh tests/fixtures/render/showcase/index.html                             # ventana usable (X11/Wayland)

# Fuzzing (workspace aparte; nightly + ASan)
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
```

Si `./tools/dev.sh all` reporta `==> All checks passed!`, tu código está listo para ser revisado. La CI remota (`.github/workflows/ci.yml`) ejecuta los mismos pasos y además `docs`, `wpt`, `package`, `security`, `unsafe-audit` y `toolchain-drift`: `quality` (fmt, clippy, tests, doc), `conformance` (gate `--summary`), `wpt` (conformidad WPT), `msrv` (`cargo check` con Rust 1.91 en el build por defecto y en el JS-free), `minimal` (clippy y tests de `js`/`render` con `--no-default-features`), `package`, `security`, `unsafe-audit`, `workflow-lint` (actionlint + shellcheck), `secret-scan` (gitleaks con `.gitleaks.toml`), `docs-links` (`python3 tools/check_docs_links.py`) y `toolchain-drift`; `release`/`check` y el workflow `bench` comparan cada fila contra `docs/status/bench-baseline.json` con `tools/bench_compare.py` (#963). El build sin JS se comprueba **por crate**: `shell` y `test_harness` enlazan Boa aunque se pase `--no-default-features` al workspace, así que ese comando no produce un build sin motor.
- **CI (ronda 12)**: los jobs instalan en el runner los paquetes de sistema Wayland/X11/xcb (`libwayland-dev`, `libxkbcommon*`, `libx11*`, `libxcb*`) que necesitan `winit`/`softbuffer`; los 3 jobs están en verde y `main`/`feature` sincronizadas.

---

## 5. Convenciones de Código y Commits

### 5.1 Mensajes de commit (Conventional Commits)
Usa commits atómicos, descriptivos y en formato semántico:

```
<type>(<scope>): <summary>
```

Tipos comunes: `feat`, `fix`, `perf`, `docs`, `test`, `refactor`, `chore`, `ci`, `build`.

Ejemplos:
- `feat(layout): implement inline formatting context line breaking`
- `fix(image): accept VP8X+VP8L alpha carried in bitstream (#984)`
- `perf(css): bloom-prune descendant selectors`
- `docs: update ROADMAP.md with Phase 3 completion`

Incluye el issue cuando aplique (p. ej. `(#123)`). Mantén el resumen en imperativo y en inglés.

### 5.2 DCO (Developer Certificate of Origin)
Al contribuir aceptas el [DCO](https://developercertificate.org/). Firma cada commit con:

```bash
git commit -s -m "feat(css): add container query support"
```

Esto añade `Signed-off-by: Nombre <email>` al mensaje. Si olvidas firmar:

```bash
git commit --amend -s
```

### 5.3 Estilo de código (Rust)
- Edición 2024, MSRV (`rust-version = 1.91`) declarada solo en `Cargo.toml`; `rust-toolchain.toml` fija el toolchain de desarrollo 1.99.1 (no subir la MSRV sin necesidad).
- Formato canónico: `cargo fmt --all`.
- Lints: `cargo clippy --workspace --all-targets -- -D warnings`.
- **`unsafe`**: evita código inseguro; si es imprescindible, cada bloque debe llevar `// SAFETY:` explicando invariantes, precondiciones y por qué es correcto. Respeta `#![forbid(unsafe_code)]` donde aplique.
- Prefiere nombres explícitos alineados con la especificación (`generate_implied_end_tags`, `foster_parenting`, etc.).
- Cita la referencia normativa en el código:
  ```rust
  // WHATWG HTML §13.2.6.4.7: When an end tag whose tag name is "p" is received...
  ```
- Sin reformateos ni refactors ajenos al cambio.

### 5.4 Convención de ramas
Trabaja en una rama dedicada (desde `main` o tu fork):

- `feat/<descripcion-corta>` o `feat/issue-123`
- `fix/<descripcion-corta>` o `fix/issue-123`
- `docs/<tema>`, `perf/<tema>`, `test/<tema>`, `chore/<tema>`

---

## 6. Pruebas requeridas

Antes de abrir o actualizar un PR, ejecuta y verifica:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p megi-test-harness -- --summary   # gate de conformidad: PASS
```

Cuando aplique: `cargo doc --workspace --no-deps`, regenerar `PROGRESS.md`/`docs/status/*` con `python3 tools/update_progress.py --write` si cambian métricas/fases, y comprobar goldens/benchmarks afectados.

### 6.1 Fallos de entorno: GC del fork de Cargo

El fork de Cargo instalado (`huacnlee/cargo` 0.103.0-1) tiene GC automático configurable en `~/.cargo/config.toml`. En corridas largas (`cargo test --workspace`, `cargo test -p megi-render --no-fail-fast`) aparecieron `could not execute process .../out/<test-bin> (never executed) / No such file or directory` y picos de tiempo (p. ej. `images` 669 ms > 500 ms): los binarios recién compilados desaparecían durante la corrida. **No lo confundas con una regresión**: antes de investigar el código, repite el subconjunto con el GC desactivado:

```bash
cargo --config build.gc.enabled=false test -p megi-render --test image_scaled_tests --test js_geometry_tests
```

Ese diagnóstico terminó con EXIT 0 (2/2 y 4/4), pero al repetir el crate completo con GC desactivado el `--lib` pasó (78 tests) y los binarios de integración volvieron a desaparecer: otro proceso del sistema también puede eliminarlos. `cargo test --workspace` exige vigilar el disco (con GC desactivado agotó el espacio al enlazar, EXIT 101 con 74 M libres; con GC activo el propio GC recuperó 6 G). Repite siempre con `--config build.gc.enabled=false` antes de atribuir un `never executed` al código.

---

## 7. Proceso de Pull Request (PR)

1. **Rama y commits**: rama descriptiva, commits atómicos y firmados con DCO.
2. **Verificación local**: `./tools/dev.sh all` en verde y `cargo +1.91 check --workspace --all-targets` si aplica a MSRV.
3. **Abre el PR** con:
   - Resumen del cambio y motivación (y enlace al issue).
   - Sección/ancla de la especificación WHATWG/W3C implementada o corregida.
   - Comandos de prueba y resultados relevantes.
4. **Revisión**:
   - Cambios de seguridad o sensibles: se recomienda **2 aprobaciones**.
   - Resto de features/fixes/docs: al menos **1 aprobación**.
   - Atiende comentarios, mantén el PR enfocado y evita mezclar cambios no relacionados.
5. **Merge**: el mantenedor integra cuando CI y revisiones estén en verde.

## 7. Licencias

El proyecto se distribuye bajo doble licencia **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE`). Al enviar una contribución aceptas que se publique bajo estos términos. Las fuentes empaquetadas para tests (`crates/fonts/tests/fixtures/`) conservan su licencia original, incluida junto a los ficheros (`LICENSE-DejaVu.txt`).

---

¡Esperamos con entusiasmo tus contribuciones para hacer de **Megi** un motor web independiente de referencia! 🚀
