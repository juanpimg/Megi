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
- Los crates de parsing fundamentales (`crates/dom`, `crates/html`, `crates/css`, `crates/layout`) se desarrollan exclusivamente con la **biblioteca estándar de Rust (`std`)**, sin librerías externas de parsing en crates.io. Esto asegura control milimétrico sobre el rendimiento, la memoria y la exactitud formal.

### 1.3 Auto-Testeo Continuo en Bucle Cerrado (Closed-Loop Testing)
- Ninguna funcionalidad se considera completada sin una prueba de conformidad que la valide.
- Trabajamos con las suites de prueba de referencia de la industria:
  - **`html5lib-tests`**: Más de 7.000 pruebas para el tokenizador y más de 1.900 árboles para la construcción del DOM.
  - **Web Platform Tests (WPT)**: Pruebas oficiales compartidas por todos los motores de navegación modernos.
- El arnés interno ([`crates/test_harness`](crates/test_harness)) permite ejecutar las pruebas con `cargo test` o `cargo run -p test_harness -- --summary`, diagnosticar discrepancias y corregir el código iterativamente.

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
  - `./tools/dev.sh all`: Script integral que debe pasar al 100% verde antes de cualquier commit o Pull Request.

---

## 2. Mapa de Crates del Proyecto

El espacio de trabajo se organiza como un **Grafo Acíclico Dirigido (DAG)** de dependencias:

```text
crates/
├── dom/           # Núcleo: Arena DOM, NodeId(NonZeroU32), mutaciones O(1). [0 dependencias]
├── html/          # HTML5: Tokenizador de 84 estados, TreeBuilder de 21 modos, InputStream. [Depende solo de dom]
├── css/           # CSS: Sintaxis L3, Selectores L4, Cascada L4, ComputedStyle, Hoja UA §15. [Depende solo de dom]
├── layout/        # Layout: Box Tree, Anonymous Blocks §9.2.1.1, Block Formatting Context (BFC). [Depende de dom y css]
└── test_harness/  # Runner de WPT y html5lib-tests para verificación en bucle cerrado.
```

---

## 3. Cómo Encontrar Tareas para Colaborar

La hoja de ruta maestra del proyecto se encuentra en [**`docs/ROADMAP.md`**](docs/ROADMAP.md). Allí se desglosa el trabajo en **micro-paquetes atómicos** (`WP-*`) de 1 a 2 días de duración máxima cada uno, vinculados a:
1. La sección exacta de la especificación WHATWG/W3C.
2. El archivo de prueba de `html5lib-tests` o WPT que lo valida.
3. El crate y módulo responsable.
4. El criterio de aceptación verificable por test.

### Áreas activas donde puedes aportar hoy:
- **Layout Engine (`crates/layout`)**:
  - Implementación del Contexto de Formato en Línea (*Inline Formatting Context / IFC*) y cálculo de cajas de línea (*line boxes*).
  - Algoritmo de división y flujo de texto (*line breaking*).
  - Soporte de Contextos de Apilamiento (*Stacking Contexts*) y `z-index`.
- **CSS Engine (`crates/css`)**:
  - Ampliación de pseudoclases en Selectors L4 (`:is()`, `:where()`, `:not()`, `:has()`).
  - Expansión de propiedades de caja flexibles (*Flexbox*) o tablas.
- **Harness & Tooling (`crates/test_harness`, `tools/`)**:
  - Integración de más suites WPT de `tests/wpt/css/`.
  - Herramientas visuales de comparación de renderizado.
- **Display Lists & Paint (`crates/paint`)**:
  - Scaffolding de comandos de pintura desacoplados (`DrawRect`, `DrawText`, `FillColor`).

---

## 4. Flujo de Trabajo para Desarrolladores

### 4.1 Clonar y Preparar el Repositorio

```bash
git clone https://github.com/juanpimg/Megi.git
cd Megi

# Verificar que tu entorno de Rust esté actualizado (Edition 2024 / Rust 1.85+)
rustc --version
cargo --version
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

# Ejecutar el script maestro que valida formato, clippy, tests y documentación
./tools/dev.sh all
```

Si `./tools/dev.sh all` reporta `==> All checks passed!`, tu código está listo para ser revisado.

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
3. **Verifica que `./tools/dev.sh all` pase al 100%**.
4. **Abre el Pull Request en GitHub** detallando:
   - Resumen del cambio y motivación.
   - Enlace a la sección de la especificación WHATWG o W3C implementada.
   - Comando de prueba que valida la funcionalidad (`cargo test -p ...`).

¡Esperamos con entusiasmo tus contribuciones para hacer de **Megi** un motor web independiente de referencia! 🚀
