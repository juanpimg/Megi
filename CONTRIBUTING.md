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
  - **`html5lib-tests`**: Más de 7.000 pruebas para el tokenizador y más de 1.900 árboles para la construcción del DOM.
  - **Web Platform Tests (WPT)**: Pruebas oficiales compartidas por todos los motores de navegación modernos.
- El arnés interno ([`crates/test_harness`](crates/test_harness)) permite ejecutar las pruebas con `cargo test` o `cargo run -p test_harness -- --summary`, diagnosticar discrepancias y corregir el código iterativamente. Las cifras de referencia son **7.047/7.050** (tokenizador, allowlist `xmlViolation.test`: 3), **3.898/3.904** (construcción de árbol, allowlist 4 suites `scripted_*`: 6 mode-runs) y **30/30** en las fixtures WPT de CSS Syntax **vendorizadas** en `tests/wpt/css/css-syntax/` (reproducibles sin red). `--summary` actúa como **gate** (exit 0 si todos los fallos están en la allowlist documentada; `--strict` hace exit 1 ante cualquier fallo).

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

---

## 2. Mapa de Crates del Proyecto

El espacio de trabajo se organiza como un **Grafo Acíclico Dirigido (DAG)** de dependencias:

```text
crates/
├── dom/           # Núcleo: Arena DOM, NodeId(NonZeroU32), mutaciones O(1) y robustez de API. [0 dependencias]
├── html/          # HTML5: Tokenizador de 84 estados, TreeBuilder de 21 modos, ParseOptions anti-DoS. [Depende solo de dom]
├── css/           # CSS: Sintaxis L3, Selectores L4, Cascada L4, ComputedStyle, Hoja UA §15. [Depende solo de dom]
├── layout/        # Layout: Box Tree, Anonymous Blocks §9.2.1.1, BFC (+ colapso de márgenes) e IFC. [Depende de dom y css]
├── fonts/         # Tipografía (Fase 4): fontdb + ttf-parser + rustybuzz + ab_glyph. [0 deps internas]
├── paint/         # Paint (Fase 4): Display Lists, raster 2D, canvas §14.2 y PNG propio. [Depende de dom, css y layout]
└── test_harness/  # Gate de conformidad: WPT (fixtures vendorizadas) y html5lib-tests.
```

---

## 3. Cómo Encontrar Tareas para Colaborar

La hoja de ruta maestra del proyecto se encuentra en [**`docs/ROADMAP.md`**](docs/ROADMAP.md). Allí se desglosa el trabajo en **micro-paquetes atómicos** (`WP-*`) de 1 a 2 días de duración máxima cada uno, vinculados a:
1. La sección exacta de la especificación WHATWG/W3C.
2. El archivo de prueba de `html5lib-tests` o WPT que lo valida.
3. El crate y módulo responsable.
4. El criterio de aceptación verificable por test.

### Áreas activas donde puedes aportar hoy (Fase 4: paint + tipografía):
- **Tipografía (`crates/fonts`)**:
  - Integración del motor tipográfico en `layout`/`paint` (WP-4.4 fase 2): `TextMeasurer` real, `ShapedRun`/glyph runs, baseline y `line-height: normal`.
  - Cobertura de shaping para más scripts, fallback entre familias y métricas de fuentes variables.
- **Paint (`crates/paint`)**:
  - Pintado de bordes por lado/estilo y fondos en *border box* (WP-4.3); `background-clip`.
  - Contextos de apilamiento, `position`/`z-index`/`opacity` (WP-4.6) y `overflow`/clip (WP-4.10).
  - Calidad del rasterizador: cobertura por píxel, antialiasing y compresión PNG (WP-4.11).
- **Layout (`crates/layout`)**:
  - Posicionamiento absoluto/relativo y floats; tablas CSS 2.1; `inline-block` y marcadores de listas (WP-4.6–4.9).
  - IFC pendiente: oportunidades de salto UAX #14, soft hyphens y `vertical-align` §10.8.1 (WP-4.5).
- **CSS (`crates/css`)**:
  - Propiedades de presentación de Fase 4 y ampliación de Selectors L4 (`:is()`, `:where()`, `:not()`).
- **Harness & Tooling (`crates/test_harness`, `tools/`)**:
  - Más fixtures vendorizadas de WPT y ampliación de la cobertura An+B del gate.
  - Comparación visual de renderizado y nuevas golden images.
- **Rendimiento/robustez (`crates/html`, `crates/dom`)**:
  - Eliminar la CPU cuadrática restante del tree builder (hallazgo F2 de la auditoría) y presupuestos de tiempo.

La matriz de soporte por propiedad CSS y el estado exacto de cada paquete de Fase 4 están en [`docs/PHASE4-SUPPORT.md`](docs/PHASE4-SUPPORT.md) y [`docs/ROADMAP.md`](docs/ROADMAP.md) (sección «FASE 4»).

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
cargo test --workspace
cargo +1.88 check --workspace --all-targets
cargo run -p test_harness -- --summary          # gate: PASS, exit 0
cargo run -p test_harness -- --summary --strict # exit 1 si hay fallos no aceptados

# Pipeline HTML → PNG (Fase 4) y validación de la imagen
cargo run -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
```

Si `./tools/dev.sh all` reporta `==> All checks passed!`, tu código está listo para ser revisado. La CI remota (`.github/workflows/ci.yml`) ejecuta los mismos pasos: `quality` (fmt, clippy, tests, doc), `conformance` (gate `--summary`) y `msrv` (`cargo check` con Rust 1.88).

---

## 5. Convenciones de Código y Commits

### 5.1 Convención de Commits (Conventional Commits)
Utilizamos commits atómicos, descriptivos y en formato semántico:

- `feat(layout): implement inline formatting context line breaking`
- `feat(css): add support for :is() and :where() pseudo-classes`
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
