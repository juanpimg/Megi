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
- Los crates de parsing/layout fundamentales (`crates/dom`, `crates/html`, `crates/css`, `crates/layout`) se desarrollan exclusivamente con la **biblioteca estándar de Rust (`std`)**, sin librerías externas de parsing en crates.io. La única excepción autorizada son las dependencias tipográficas de bajo nivel de `crates/fonts` (`fontdb`, `ttf-parser`, `rustybuzz`, `ab_glyph`), listadas en `docs/ARCHITECTURE.md` §3.1; el rasterizador y el codificador PNG de `crates/paint` son propios y no usan dependencias externas (decisión D3, §3.2).

### 1.3 Auto-Testeo Continuo en Bucle Cerrado (Closed-Loop Testing)
- Ninguna funcionalidad se considera completada sin una prueba de conformidad que la valide.
- Trabajamos con las suites de prueba de referencia de la industria:
  - **`html5lib-tests`**: Más de 7.000 pruebas de tokenizador, ~1.900 de construcción del DOM y las suites oficiales de **serialización HTML** (`serializer/*.test`).
  - **Web Platform Tests (WPT)**: Pruebas oficiales compartidas por todos los motores de navegación modernos (fixtures de CSS Syntax vendorizadas).
- El arnés interno ([`crates/test_harness`](crates/test_harness)) permite ejecutar las pruebas con `cargo test` o `cargo run -p test_harness -- --summary`, diagnosticar discrepancias y corregir el código iterativamente. El gate cubre **4 suites**: tokenizer **7.050/7.050** (allowlist vacía), **árbol 3.904/3.904 (100 %, sin allowlist; las 4 suites `scripted_*` ejecutan JS real vía WP-6.4)**, **serializador HTML 230/230** y **CSS Syntax 30/30** con fixtures WPT vendorizadas en `tests/wpt/css/css-syntax/` (reproducibles sin red). `--summary` actúa como **gate** (exit 0 si todos los fallos están en la allowlist documentada; `--strict` hace exit 1 ante cualquier fallo). El workspace tiene **1449 tests**.

### 1.4 Modelo de Memoria en Arena (Sin `Rc<RefCell<Node>>`)
- Uno de los mayores cuellos de botella y fuentes de *memory leaks* en motores web en Rust es el uso de `Rc<RefCell<Node>>` o recolectores de basura pesados.
- Megi utiliza una **arena contigua indexada** ([`crates/dom/src/lib.rs`](crates/dom/src/lib.rs)):
  - Todos los nodos residen en un vector plano `Document::nodes: Vec<Node>`.
  - Los nodos se referencian mediante manejadores compactos de 4 bytes: `NodeId(NonZeroU32)` (aprovechando la optimización de nicho para que `Option<NodeId>` ocupe solo 4 bytes).
  - Los 5 punteros del árbol (`parent`, `first_child`, `last_child`, `prev_sibling`, `next_sibling`) son intrusivos y permiten mutaciones y reparentados en tiempo constante $O(1)$ con máxima localidad de caché.

### 1.5 Cero Advertencias y Calidad Estricta (Zero-Warning Policy)
- El proyecto mantiene una política estricta de **cero advertencias**:
  - `cargo fmt --check`: Formateo canónico con `rustfmt`.
  - `cargo clippy --workspace --all-targets -- -D warnings`: Cero advertencias de linter permitidas.
  - `cargo +1.88 check --workspace --all-targets`: verificación contra la MSRV declarada (`rust-version = "1.88"`).
  - `./tools/dev.sh all`: script integral (fmt, clippy, tests, doc y **gate de conformidad**) que debe pasar al 100% verde antes de cualquier commit o Pull Request.
- **Progress status generado**: si tocas docs, tests, fases o `docs/status/progress.toml`, regenera el snapshot antes de commitear con `python3 tools/update_progress.py --write` (CI ejecuta el check de frescura); **no edites a mano** `PROGRESS.md` ni `docs/status/`.
  - `cd fuzz && cargo +nightly fuzz build`: compila los 7 targets de fuzz (nightly; también lo hace un job build-only en PRs que tocan `fuzz/**`).

---

## 2. Mapa de Crates del Proyecto

El espacio de trabajo se organiza como un **Grafo Acíclico Dirigido (DAG)** de dependencias:

```text
crates/
├── dom/           # Núcleo: Arena DOM, NodeId(NonZeroU32), mutaciones O(1) y robustez de API. [0 dependencias]
├── html/          # HTML5: Tokenizador de 84 estados, TreeBuilder de 21 modos (3.904/3.904), hook ScriptExecutor (WP-6.4), ParseOptions anti-DoS. [Depende solo de dom]
├── css/           # CSS: Sintaxis L3, Selectores L4, Cascada L4, ComputedStyle, Hoja UA §15. [Depende solo de dom]
├── layout/        # Layout: Box Tree, BFC/IFC, posicionamiento/floats, tablas §17, inline-block y reemplazados. [Depende de dom y css]
├── fonts/         # Tipografía (Fase 4): fontdb + ttf-parser + rustybuzz + ab_glyph + woff2-patched (WOFF2, §3.1); fuentes web @font-face E2E, integrado en layout/paint.
├── paint/         # Paint (Fase 4): Display Lists, raster con AA/culling, bordes por lado, canvas §14.2, PNG propio y hit-testing aditivo. [Depende de dom, css, layout y fonts]
├── net/           # Red (Fase 5): URL, HTTP/1.1+TLS (rustls/webpki-roots), caché, data:, file://, MIME y loader.
├── image/         # Decoder PNG + inflate zlib/DEFLATE propios (std puro, sin deps).
├── svg/           # SVG desde cero (Fase 4): parser XML subset + modelo SVG + path/arcos + rasterizador AA, **CSS interno, gradientes, `<pattern>` y `<text>`** (rondas 16-17); `std` + `image` + `fonts`; cableado E2E en render; 73 tests.
├── render/        # Pipeline de navegador (Fase 5/6): URL → net → HTML → CSS externo → layout → PNG (+ <img>, backgrounds geometry/SVG inline, scripts, event loop determinista por reloj virtual, hit_regions, cookies/CORS/gzip y webfonts).
├── js/            # JS (Fase 6, WP-A/B/6.4 + eventos avanzados): Boa 0.21.1 + bindings DOM + EventTarget/timers con currentTarget/eventPhase/once/passive/onclick + CSSOM/NodeList/identidad; consumido por render y por el harness; 71 tests.
├── shell/         # Ventana (Fase 7): winit+softbuffer, scroll/entrada/HiDPI, worker, navegación/historial/título con hit_regions/find_link_at, zoom, auto-refresh opt-in, formularios editables, find-in-page, pestañas y chrome (address bar/tab strip/select); screenshot headless. [Depende de render/paint/net]; 160 tests.
├── bench/         # Benchmarks reproducibles (parseo, estilos, layout, paint, PNG). [Solo crates internos]
└── test_harness/  # Gate de conformidad: tokenizer, árbol 3.904/3.904 (sin allowlist), serializer y CSS Syntax; ejecuta los scripts del corpus vía js (WP-6.4).
```

---

## 3. Cómo Encontrar Tareas para Colaborar

La hoja de ruta maestra del proyecto se encuentra en [**`docs/ROADMAP.md`**](docs/ROADMAP.md). Allí se desglosa el trabajo en **micro-paquetes atómicos** (`WP-*`) de 1 a 2 días de duración máxima cada uno, vinculados a:
1. La sección exacta de la especificación WHATWG/W3C.
2. El archivo de prueba de `html5lib-tests` o WPT que lo valida.
3. El crate y módulo responsable.
4. El criterio de aceptación verificable por test.

### Áreas activas donde puedes aportar hoy (Fases 4–7 en curso; **hito ronda 11: árbol HTML5 3.904/3.904, 100 %**):
- **Layout (`crates/layout`) — Fase 4 funcionalmente cerrada**:
  - **Ronda 17**: **Flexbox MVP** (`layout/flex.rs`: display flex/inline-flex, direction/justify/align/wrap/gap/grow-shrink-basis).
  - **Ronda 18**: **Grid MVP** (`layout/grid.rs`: display grid/inline-grid, tracks fixed/%/fr/auto/repeat, gaps, auto-placement row-major, justify/align items/self y auto-rows).
  - **Ronda 19**: **Grid avanzado** (placement explícito con líneas 1-based/negativas y `span N`, auto-placer que salta ocupadas y crece tracks implícitos, `minmax()`, `repeat(auto-fill|auto-fit)`, `order` estable; solapes explícitos/named lines/zonas/subgrid fuera de alcance).
  - Pendiente menor: block-in-inline, colores de conflictos de `border-collapse` y auto-colapso de bloques vacíos.
  - Ya hechos (ronda 8): BFC/IFC, posicionamiento, floats, overflow, tablas §17 (rowspan/fixed/inline-table/collapse + fast path), inline-block, reemplazados, árbol de celdas expuesto y verificado (133 tests).
- **Paint (`crates/paint`)**:
  - Gradientes ya HECHOS (rondas 19-20): `DisplayItem::Gradient` por tile con AA/sRGB, size/position/repeat/origin/clip/radius/clip:text y **conic/hints/OKLab** (ronda 20); siguientes: `display-p3`/espacios avanzados, `border-image`, filtros y pulido de `border-collapse` (colores).
  - **Ronda 18 HECHO**: `overflow:hidden`+`border-radius` recorta contenido real y `background-clip:text` compone por máscara de glifos (`cargo test -p paint --test clipping`, 9 tests); display list byte-idéntica sin clipping.
  - **Ronda 17**: **`border-radius`/`box-shadow`/`text-shadow`** (DisplayItems redondeados, blur determinista 3 pasadas; `cargo test -p paint --test effects`).
  - Ya hechos (rondas 15-16): **`background-repeat`/`size`/`position`/`origin`/`clip`/`attachment`** con tiling/clip (cap 4096), **multi-capa `background_layers`** (`space`/`round` reales) y legacy flag para goldens.
  - Glyph cache global, presupuestos de memoria/tiempo y capas de opacidad.
  - Ya hecho (rondas 9–10): **hit-testing aditivo** (`HitRegion`/`hit_test`/`find_link_at`, 11 tests) y su cableado en `render` (`RenderOutput.hit_regions`, +0,6 µs) sin cambiar golden.
- **CSS (`crates/css`)**:
  - **Ronda 20 HECHO**: `conic-gradient`/`repeating-conic` + color hints + espacios `srgb|srgb-linear|oklab`, `calc()` mixto (`CalcLengths`), tipografía (`text-transform`/spacing/white-space) y **fix de custom properties** lazy/COW (`Option<Arc<HashMap>>`, fast path `needs_resolution`; A/B −14,9 % css resolve quiet).
- **Red y render (`crates/net`, `crates/render`, `crates/image`) — Fase 5 (≈99 %)**:
  - **Ronda 22 HECHO**: **HTTP/2 MVP** (HPACK/framing/ALPN h2 + fallback h1; caps 16 KiB frame/64 KiB headers/2 streams), **JPEG baseline y GIF87a/89a desde cero** y sniffing PNG|JPEG|GIF en render (PNG decode 6,65 ms p50). Pendientes: multiplexado/flow-control/push/prioridades/h2c/trailers, streaming, PSL completa y multi-range.
  - **Ronda 19 HECHO**: `ReferrerConfig` instalado en el loader de render (header>meta>default) con `Referer`/SameSite reales y `RenderStats.referrer_policy`; estado CSS real (`css::state::with_state` pre/post-script) con `css_state.html`; 136 tests render.
  - **Ronda 18 HECHO**: SameSite+PSL reducida, Referrer-Policy, Range/206 y cache refinada (`no-cache`/`must-revalidate`/`s-maxage`/`Age`/`requires_revalidation`); net 207 tests.
  - Endurecer tests HTTP/TLS, ampliar fixtures de backgrounds y cubrir **red avanzada E2E** (cookies por render/sesión, CORS/credentials con denegación no fatal, gzip/deflate en HTML/CSS; `--cookies`/`--origin` ya hechos en la ronda 10).
  - Ya hechos: TLS, caché, `data:`, pipeline URL→PNG, `<img>` E2E, `@import`+media, **backgrounds E2E**, **cookies RFC 6265 + CORS básico + gzip/deflate con caps**, **ejecución JS (WP-6.2 host)**, **fuentes web @font-face/WOFF2 E2E** y decoder PNG/inflate.
- **Tipografía y fuentes web (`crates/fonts`)**:
  - Siguientes: weight/style para elegir cara, WOFF1, `unicode-range`/`font-display` (hoy bloqueante), subsetting y DB de fuentes cacheada entre renders.
  - Ya hecho (ronda 13): **`@font-face`+WOFF2 E2E** (dep autorizada `woff2-patched` 0.4, DB por render en layout+paint, caps 8/8 MiB/32 MiB, stats/timing y warnings no fatales).
  - Mejoras de shaping para más scripts, fallback entre familias y métricas de fuentes variables.
- **SVG (`crates/svg` + `render`, rondas 13-17)**: crate propio completo (parser XML+modelo+rasterizador AA, **73 tests**) con **CSS interno y gradientes lineales/radiales** (ronda 16) y **cableado E2E**: `<img src=*.svg>`, `background-image` **e inline `<svg>`** (caps 256/1 MiB/8 MiB, `svgs_rendered`/`inline_svgs`; 11+10 tests). **Ronda 17**: **`<pattern>`** (cap 10k tiles) y **`<text>` básico** con `crates/fonts` (caps 1.000 texts/10.000 chars). Siguientes: shaping avanzado (kerning/italic/bidi), patterns rotados y cache por URL.
- **Rendimiento y features (`crates/html`, `crates/dom`, `layout`, `paint`, `css`, `svg`, `shell`) — los presupuestos son gates que NO se suben**: ronda 21 recuperó pipeline ≤140 ms y layout ≤43 ms (html −18/−20 %, css resolve 33,91 ms, layout 36,4 ms, build-display −9,1 %).
  - DOM identidad/memoria (auditoría F8), recursión de layout/paint en documentos profundos (F6), presupuestos de tiempo y API `Result`.
  - Benchmarks reproducibles (`./tools/bench.sh`) y regresiones de complejidad.
- **Fuzzing (`fuzz/`)**:
  - Ampliar corpus y targets; mejorar la señal de la CI nocturna. Ver [`docs/FUZZING.md`](docs/FUZZING.md).
- **Fase 7 — shell (`crates/shell`, ≈75 %)**: ya hechos ventana/worker/scroll/entrada/HiDPI, **navegación con historial/título** (WP-7.5), **enlaces con hit regions nativos**, **zoom** (`--zoom`/`Ctrl+=/-/0`) y **auto-refresh opt-in** (`--refresh-ms`). **Ronda 17**: **formularios editables** (UA styles, `src/forms.rs` con FocusState/TextEdit UTF-8/snapshot re-render, clic/teclado/Tab/checkbox). Siguientes: pestañas, widgets nativos/select dropdown, validación/submit, **event loop vivo en el host** y pruebas con Xvfb.
- **Fase 6 — JS (`crates/js` + `crates/render` + parser, WP-6.1/6.2/6.4 + WP-A/WP-B + eventos + APIs de navegador HECHOS, ≈90 %)**: Boa 0.21.1 con feature `js` ON por defecto (build mínimo `--no-default-features`), bindings `NodeId↔handle` generacionales, eventos/timers con reloj inyectable, **WP-B** (CSSStyleDeclaration, `getComputedStyle` con provider, NodeList, matches/closest, insertAdjacent, identidad), **event loop presupuestado en `render`** y **WP-6.4** (scripts parser-inserted + `document.write` ⇒ gate 3.904/3.904). **Eventos avanzados (ronda 12)**: `currentTarget`/`eventPhase`, `once`/`passive`, handlers inline `onclick`/`onload`/`onerror`, excepciones capturadas. **APIs de navegador (ronda 16)**: `matchMedia`, viewport/DPR/scroll, `requestAnimationFrame` 16 ms, `onerror`, `readyState`, consola y `getComputedStyle` real pre-script, cableadas en `render` (rAF epoch fijo, errores ⇒ warnings). Siguientes: módulos ES, memoria real, eventos `resize`/`scroll`/`change` y event loop vivo en el shell. Ver [`docs/JAVASCRIPT-STRATEGY.md`](docs/JAVASCRIPT-STRATEGY.md).
- **Rendimiento (rondas 10-12)**: pipeline 167,6→**134-137 ms**, `html/realistic-1mb` ~19-20 ms, `tag-40k-attr` ~12,4 ms; ronda 12: **layout −14,6 %**; ronda 15: **IFC −28 %** (layout 50,5→36,3 ms) y **pipeline 132,9→120,5 ms** con goldens byte-idénticos; encoder PNG −30,5 % invariante y `massive-end-p` ~19 ms. Siguiente techo: IFC (~34 ms; owned-styles API + caché de familias), Huffman dinámico para ratio PNG y bench E2E con JS. Medidor: `./tools/bench.sh` (baseline en [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md)).
- **Harness & Tooling (`crates/test_harness`, `tools/`)**:
  - Más fixtures vendorizadas de WPT, ampliación de An+B y comparación visual/golden images.

La matriz de soporte por propiedad CSS, el estado de cada paquete de Fase 4 y el plan de Fase 5 están en [`docs/PHASE4-SUPPORT.md`](docs/PHASE4-SUPPORT.md) y [`docs/ROADMAP.md`](docs/ROADMAP.md) (secciones «FASE 4» y «FASE 5»); el rendimiento en [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md) y el fuzzing/seguridad en [`docs/FUZZING.md`](docs/FUZZING.md) y [`SECURITY.md`](SECURITY.md).

---

## 4. Flujo de Trabajo para Desarrolladores

### 4.1 Clonar y Preparar el Repositorio

```bash
git clone https://github.com/juanpimg/Megi.git
cd Megi

# Verificar que tu entorno de Rust cumple la MSRV declarada (Edition 2024 / Rust 1.88+)
rustup toolchain install 1.88
rustc --version
cargo +1.88 check --workspace --all-targets
```

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
cargo test --workspace                         # 1449 tests (con JS por defecto)
cargo test --workspace --no-default-features   # build mínimo sin JS (CI)
cargo +1.88 check --workspace --all-targets
cargo run -p test_harness -- --summary          # gate 4 suites: PASS, exit 0
cargo run -p test_harness -- --summary --strict # exit 1 si hay fallos no aceptados

# Pipeline HTML → PNG (Fase 4) y validación de la imagen
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png

# Rendimiento (release; baseline en docs/PERFORMANCE.md)
./tools/bench.sh --scale 0.25
./tools/bench.sh --filter layout    # ronda 12: ~51-53 ms, LayoutBox 176 B
./tools/bench.sh --filter paint     # ronda 12: encoder PNG −30,5 % invariante
./tools/bench.sh --filter pipeline  # ronda 12: ~134-137 ms

# Red, render y JS (Fases 5–6; tests offline)
cargo test -p net -p render -p image
cargo test -p js
cargo test -p js --no-default-features   # stub mínimo sin Boa
cargo test -p js                                # 58 tests (eventos avanzados)
cargo test -p svg                               # crate SVG (73 tests)
cargo test -p render --test svg_tests            # SVG E2E en render (11 tests)
cargo run -p render -- tests/fixtures/render/svg_inline_simple.html /tmp/inline.png --json   # inline_svgs>0
cargo run -p svg --example svg_to_png -- crates/svg/tests/fixtures/gradient.svg /tmp/gradient.png
cargo run -p render -- tests/fixtures/render/flex_row.html /tmp/flex_row.png
cargo run -p shell -- tests/fixtures/render/forms.html --screenshot /tmp/forms.png --width 640 --height 480
cargo test -p paint --test effects
cargo run -p render -- tests/fixtures/render/background_layers.html /tmp/layers.png --width 240 --height 260
cargo run -p render -- tests/fixtures/render/browser_api.html /tmp/api.png --json
cargo run -p render -- tests/fixtures/render/backgrounds.html /tmp/bg.png
cargo run --release -p bench -- --filter layout --iterations 5   # ~36 ms
cargo run -p render -- tests/fixtures/render/svg_img.html /tmp/svg_img.png --json   # svgs_rendered=1
cargo test -p fonts -p css -p render             # fuentes web @font-face/WOFF2
cargo run -p render -- tests/fixtures/render/webfont.html /tmp/webfont.png --json   # md5 f38aed03…
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/zoom2.png --width 640 --height 400 --zoom 2  # md5 c11903cf…
cargo test -p html --lib scripting             # WP-6.4: hook parser-inserted
cargo test -p render --test scripting_tests   # JS ejecutado y visible en el PNG (7 fixtures js_*)
cargo test -p render --test event_loop_tests  # event loop presupuestado (6 fixtures js_event_*)
cargo test -p paint --test hit_testing        # hit-testing aditivo (topmost, links, clip)
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_on.png   # con JS
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_off.png --no-scripting
cargo run -p render -- https://example.com/ /tmp/page.png --cookies --origin https://example.com --json   # red avanzada E2E
cargo run -p render -- tests/fixtures/render/showcase/index.html /tmp/showcase.png --width 1024 --height 768
cargo run -p shell -- tests/fixtures/render/showcase/index.html              # navegación: clic, Alt+←/→, b/n, r
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/shot.png   # Fase 7 headless
tools/megi.sh tests/fixtures/render/showcase/index.html                             # ventana usable (X11/Wayland)

# Fuzzing (workspace aparte; nightly + ASan)
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
```

Si `./tools/dev.sh all` reporta `==> All checks passed!`, tu código está listo para ser revisado. La CI remota (`.github/workflows/ci.yml`) ejecuta los mismos pasos: `quality` (fmt, clippy, tests, doc), `conformance` (gate `--summary`) y `msrv` (`cargo check` con Rust 1.88).
- **CI (ronda 12)**: los jobs instalan en el runner los paquetes de sistema Wayland/X11/xcb (`libwayland-dev`, `libxkbcommon*`, `libx11*`, `libxcb*`) que necesitan `winit`/`softbuffer`; los 3 jobs están en verde y `main`/`feature` sincronizadas.

---

## 5. Convenciones de Código y Commits

### 5.1 Convención de Commits (Conventional Commits)
Utilizamos commits atómicos, descriptivos y en formato semántico:

- `feat(layout): implement inline formatting context line breaking`
- `feat(css): implement functional Selectors L4 (:is/:where/:not/:has)`
- `feat(net): bootstrap Phase 5 with URL, HTTP/1.1, file and resource loading`
- `feat(render,image): browser pipeline and dependency-free PNG decoding`
- `feat(shell): first browser window with winit, softbuffer and headless screenshots`
- `feat(render): recursive @import, real media evaluation and a showcase golden`
- `feat(paint): DEFLATE PNG compression and fully iterative painter`
- `fix(html): pop implied end tags when encountering hr inside select scope`
- `test(test_harness): add WPT selector matching suite runner`
- `docs: update ROADMAP.md with Phase 3 completion`

### 5.2 Estilo de Código en Rust
- Respeta la Edición 2024 de Rust y las sugerencias de Clippy.
- Prefiere nombres explícitos y acordes a los términos normativos de la especificación (por ejemplo, `generate_implied_end_tags`, `foster_parenting`, `compute_edges`).
- Cita siempre la referencia de la especificación:
  ```rust
  // WHATWG HTML §13.2.6.4.7: When an end tag whose tag name is "p" is received:
  // If the stack of open elements does not have a p element in button scope,
  // this is a parse error; act as if a start tag with tag name "p" had been seen.
  ```

---

## 6. Proceso de Pull Request (PR)

1. **Crea una rama descriptiva**:
   ```bash
   git checkout -b feat/inline-formatting-context
   ```
2. **Haz commits atómicos y claros** vinculando tus cambios al paquete de trabajo correspondiente.
3. **Verifica que `./tools/dev.sh all` pase al 100%** (incluye el gate de conformidad) y que la MSRV siga verde con `cargo +1.88 check --workspace --all-targets`.
4. **Abre el Pull Request en GitHub** detallando:
   - Resumen del cambio y motivación.
   - Enlace a la sección de la especificación WHATWG o W3C implementada.
   - Comando de prueba que valida la funcionalidad (`cargo test -p ...`).

## 7. Licencias

El proyecto se distribuye bajo doble licencia **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE`). Al enviar una contribución aceptas que se publique bajo estos términos. Las fuentes empaquetadas para tests (`crates/fonts/tests/fixtures/`) conservan su licencia original, incluida junto a los ficheros (`LICENSE-DejaVu.txt`).

---

¡Esperamos con entusiasmo tus contribuciones para hacer de **Megi** un motor web independiente de referencia! 🚀
